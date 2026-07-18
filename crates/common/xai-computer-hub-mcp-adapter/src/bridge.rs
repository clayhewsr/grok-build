//! Core bridge that connects an MCP server to the computer hub.
//!
//! [`McpBridge`] discovers tools from an [`McpTransport`] and registers
//! them with a hub `ToolServer` via one `ToolServerHandler` per
//! tool. Incoming hub calls are translated to MCP `tools/call` and the
//! response is mapped back to [`ToolOutputWire`].

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use tracing::{debug, info, warn};
use xai_tool_protocol::{McpBlock, SessionId, ToolId, ToolOutputWire};
use xai_tool_runtime::{ToolCallContext, ToolError, ToolStream, TypedToolOutput, terminal_only};
use xai_tool_types::ToolDescription;

use crate::transport::McpTransport;
use crate::types::{McpCallResult, McpContent, McpError, McpServerInfo, McpToolDefinition};

/// Hard backstop for a single MCP `tools/call` round-trip through the adapter.
///
/// Private and intentionally not configurable in this layer: it bounds a hung
/// MCP server/tool invocation without changing the transport or hub APIs.
const MCP_TOOL_CALL_BACKSTOP_TIMEOUT: Duration = Duration::from_secs(30);

/// Configuration for an [`McpBridge`] instance.
#[derive(Debug, Clone)]
pub struct McpBridgeConfig {
    /// Hub session to bind tools to.
    pub session_id: SessionId,
    /// Optional namespace prefix for tool descriptions.
    pub namespace: Option<String>,
}

/// Result of a successful [`McpBridge::connect`] call.
///
/// Contains the bridge handle and the server info returned during the
/// MCP initialize handshake.
pub struct McpBridgeHandle {
    /// The bridge managing the MCP-to-hub tool registrations.
    pub bridge: McpBridge,
    /// Server metadata from the MCP `initialize` response.
    pub server_info: McpServerInfo,
}

impl std::fmt::Debug for McpBridgeHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpBridgeHandle")
            .field("server_info", &self.server_info.name)
            .field("tool_count", &self.bridge.tool_count())
            .finish_non_exhaustive()
    }
}

/// Bridges an MCP server's tools into the computer hub.
///
/// On construction the bridge performs the MCP `initialize` handshake,
/// discovers tools via `tools/list`, and builds a handler
/// for each one. Callers wire these handlers into a
/// [`xai_computer_hub_sdk::ToolServerBuilder`] to register them
/// with the hub.
///
/// Callers **must** call [`McpBridge::shutdown`] before dropping to
/// close the underlying MCP transport cleanly. If the bridge is dropped
/// without an explicit shutdown, a best-effort `close()` is spawned on
/// the tokio runtime (mirroring `ToolServer`'s drop behavior).
pub struct McpBridge {
    transport: Arc<dyn McpTransport>,
    handlers: Vec<Arc<McpToolHandler>>,
    server_info: McpServerInfo,
}

impl std::fmt::Debug for McpBridge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpBridge")
            .field("server", &self.server_info.name)
            .field("tool_count", &self.handlers.len())
            .finish_non_exhaustive()
    }
}

impl McpBridge {
    /// Initialize the MCP server, discover its tools, and build handlers.
    ///
    /// Returns `Err` if the MCP handshake or tool discovery fails.
    pub async fn connect(
        transport: Arc<dyn McpTransport>,
        config: &McpBridgeConfig,
    ) -> Result<McpBridgeHandle, McpError> {
        let server_info = match transport.initialize().await {
            Ok(info) => info,
            Err(e) => {
                crate::metrics::mcp_error();
                return Err(e);
            }
        };
        info!(
            server_name = %server_info.name,
            version = %server_info.version,
            "MCP server initialized"
        );

        let tools = match transport.list_tools().await {
            Ok(t) => t,
            Err(e) => {
                // Close the transport so the initialized connection is not leaked
                // when list_tools fails after a successful initialize.
                if let Err(close_err) = transport.close().await {
                    warn!(
                        ?close_err,
                        "failed to close transport after list_tools error"
                    );
                }
                crate::metrics::mcp_error();
                return Err(e);
            }
        };
        debug!(
            server_name = %server_info.name,
            tool_count = tools.len(),
            "discovered MCP tools"
        );

        let handlers: Vec<Arc<McpToolHandler>> = tools
            .into_iter()
            .filter_map(|def| {
                let tool_id = match ToolId::new(&def.name) {
                    Ok(id) => id,
                    Err(err) => {
                        warn!(
                            tool_name = %def.name,
                            %err,
                            "skipping MCP tool with invalid name"
                        );
                        return None;
                    }
                };
                Some(Arc::new(McpToolHandler {
                    tool_id,
                    definition: def,
                    transport: Arc::clone(&transport),
                    namespace: config.namespace.clone(),
                }))
            })
            .collect();

        crate::metrics::mcp_tools_bridged_set(handlers.len() as i64);

        let bridge = McpBridge {
            transport,
            handlers,
            server_info: server_info.clone(),
        };

        Ok(McpBridgeHandle {
            bridge,
            server_info,
        })
    }

    /// Handlers to register with a [`xai_computer_hub_sdk::ToolServerBuilder`].
    ///
    /// Each handler implements `ToolServerHandler` for one MCP tool.
    pub fn handlers(&self) -> &[Arc<McpToolHandler>] {
        &self.handlers
    }

    /// Server metadata from the MCP `initialize` response.
    pub fn server_info(&self) -> &McpServerInfo {
        &self.server_info
    }

    /// Number of tools discovered and registered.
    pub fn tool_count(&self) -> usize {
        self.handlers.len()
    }

    /// Close the underlying MCP transport.
    pub async fn shutdown(&self) -> Result<(), McpError> {
        crate::metrics::mcp_tools_bridged_set(0);
        self.transport.close().await
    }
}

impl Drop for McpBridge {
    fn drop(&mut self) {
        crate::metrics::mcp_tools_bridged_set(0);
        let transport = Arc::clone(&self.transport);
        if tokio::runtime::Handle::try_current().is_ok() {
            tokio::spawn(async move {
                if let Err(err) = transport.close().await {
                    warn!(?err, "best-effort transport close on drop failed");
                }
            });
        }
    }
}

/// Hub-facing handler for a single MCP tool.
///
/// Translates hub `tool_call_request` frames into MCP `tools/call`
/// invocations and maps the result back to [`ToolOutputWire`].
pub struct McpToolHandler {
    tool_id: ToolId,
    definition: McpToolDefinition,
    transport: Arc<dyn McpTransport>,
    namespace: Option<String>,
}

impl std::fmt::Debug for McpToolHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpToolHandler")
            .field("tool_id", &self.tool_id)
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl xai_computer_hub_sdk::ToolServerHandler for McpToolHandler {
    fn tool_id(&self) -> ToolId {
        self.tool_id.clone()
    }

    fn description(&self) -> ToolDescription {
        let desc = ToolDescription::new(
            self.definition.name.clone(),
            self.definition.description.clone().unwrap_or_default(),
        );
        match self.namespace {
            Some(ref ns) => desc.with_namespace(ns.clone()),
            None => desc,
        }
    }

    fn input_schema(&self) -> Option<Value> {
        self.definition.input_schema.clone()
    }

    async fn handle_call(&self, _ctx: ToolCallContext, args: Value) -> ToolStream<TypedToolOutput> {
        let _start = std::time::Instant::now();
        let tool_id = self.tool_id.clone();
        let result = tokio::time::timeout(
            MCP_TOOL_CALL_BACKSTOP_TIMEOUT,
            self.transport
                .call_tool(self.definition.name.as_str(), args),
        )
        .await;
        crate::metrics::mcp_call_duration_observe(_start.elapsed().as_secs_f64());

        let terminal = match result {
            Ok(Ok(call_result)) => {
                let output = translate_mcp_result(&call_result);
                serde_json::to_value(output)
                    .map(|value| TypedToolOutput::from_value(tool_id, value))
                    .map_err(|e| {
                        crate::metrics::mcp_error();
                        ToolError::execution(self.tool_id.clone(), e.to_string()).with_source(e)
                    })
            }
            Ok(Err(mcp_err)) => {
                crate::metrics::mcp_error();
                Err(ToolError::execution(
                    self.tool_id.clone(),
                    format!("{mcp_err}"),
                ))
            }
            Err(_elapsed) => {
                crate::metrics::mcp_call_timed_out();
                Err(ToolError::timeout(
                    self.tool_id.clone(),
                    format!(
                        "MCP tool call timed out after {:?}",
                        MCP_TOOL_CALL_BACKSTOP_TIMEOUT
                    ),
                ))
            }
        };

        terminal_only(terminal)
    }
}

/// Convert an [`McpCallResult`] into the wire output format.
///
/// - **Error responses** (`is_error: true`): concatenates text-only blocks
///   into a single [`ToolOutputWire::Text`], discarding non-text content
///   (with a warning when content is dropped).
/// - **Empty content**: returns `ToolOutputWire::Text("")` regardless of
///   `is_error` — matches side-effect-only MCP tools.
/// - **Single text block**: returns [`ToolOutputWire::Text`] directly.
/// - **Multi-block / non-text**: returns [`ToolOutputWire::Mcp`] with
///   structured blocks.
fn translate_mcp_result(result: &McpCallResult) -> ToolOutputWire {
    if result.content.is_empty() {
        return ToolOutputWire::Text(String::new());
    }

    if result.is_error {
        let error_text = result
            .content
            .iter()
            .filter_map(|c| match c {
                McpContent::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        if error_text.is_empty() {
            warn!(
                content_count = result.content.len(),
                "MCP error response contained only non-text blocks; content dropped"
            );
        }
        return ToolOutputWire::Text(error_text);
    }

    // Single text block → flat text output.
    if result.content.len() == 1
        && let Some(McpContent::Text { text }) = result.content.first()
    {
        return ToolOutputWire::Text(text.clone());
    }

    let blocks: Vec<McpBlock> = result
        .content
        .iter()
        .map(|c| match c {
            McpContent::Text { text } => McpBlock::Text { text: text.clone() },
            McpContent::Image { mime_type, data } => McpBlock::Image {
                mime_type: mime_type.clone(),
                data: data.clone(),
            },
            McpContent::Resource {
                uri,
                mime_type,
                text,
            } => McpBlock::Resource {
                uri: uri.clone(),
                mime_type: mime_type.clone(),
                text: text.clone(),
            },
        })
        .collect();

    ToolOutputWire::Mcp { blocks }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{McpCallResult, McpContent, McpServerInfo, McpToolDefinition};
    use std::future::pending;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use tokio::sync::Mutex;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum CallBehavior {
        ImmediateOk,
        ImmediateErr,
        Hang,
    }

    struct ActiveCallGuard {
        active_calls: Arc<AtomicUsize>,
    }

    impl Drop for ActiveCallGuard {
        fn drop(&mut self) {
            self.active_calls.fetch_sub(1, Ordering::SeqCst);
        }
    }

    struct MockTransport {
        server_info: McpServerInfo,
        tools: Vec<McpToolDefinition>,
        call_response: Mutex<Option<McpCallResult>>,
        call_error: Mutex<Option<McpError>>,
        closed: AtomicBool,
        last_call: Mutex<Option<(String, Value)>>,
        call_behavior: CallBehavior,
        active_calls: Arc<AtomicUsize>,
    }

    impl MockTransport {
        fn new(server_info: McpServerInfo, tools: Vec<McpToolDefinition>) -> Self {
            Self {
                server_info,
                tools,
                call_response: Mutex::new(None),
                call_error: Mutex::new(None),
                closed: AtomicBool::new(false),
                last_call: Mutex::new(None),
                call_behavior: CallBehavior::ImmediateOk,
                active_calls: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn with_call_response(self, response: McpCallResult) -> Self {
            Self {
                call_response: Mutex::new(Some(response)),
                ..self
            }
        }

        fn with_call_error(self, error: McpError) -> Self {
            Self {
                call_error: Mutex::new(Some(error)),
                call_behavior: CallBehavior::ImmediateErr,
                ..self
            }
        }

        fn with_hung_call(self) -> Self {
            Self {
                call_behavior: CallBehavior::Hang,
                ..self
            }
        }

        fn active_calls(&self) -> Arc<AtomicUsize> {
            self.active_calls.clone()
        }
    }

    #[async_trait]
    impl McpTransport for MockTransport {
        async fn initialize(&self) -> Result<McpServerInfo, McpError> {
            Ok(self.server_info.clone())
        }

        async fn list_tools(&self) -> Result<Vec<McpToolDefinition>, McpError> {
            Ok(self.tools.clone())
        }

        async fn call_tool(&self, name: &str, arguments: Value) -> Result<McpCallResult, McpError> {
            *self.last_call.lock().await = Some((name.to_string(), arguments));
            self.active_calls.fetch_add(1, Ordering::SeqCst);
            let _guard = ActiveCallGuard {
                active_calls: self.active_calls.clone(),
            };

            match self.call_behavior {
                CallBehavior::ImmediateOk => self
                    .call_response
                    .lock()
                    .await
                    .clone()
                    .ok_or_else(|| McpError::Transport("no canned response".into())),
                CallBehavior::ImmediateErr => {
                    if let Some(err) = self.call_error.lock().await.take() {
                        Err(err)
                    } else {
                        Err(McpError::Transport("no canned error".into()))
                    }
                }
                CallBehavior::Hang => {
                    pending::<()>().await;
                    unreachable!("hung call future should be cancelled by timeout")
                }
            }
        }

        async fn close(&self) -> Result<(), McpError> {
            self.closed.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    fn sample_server_info() -> McpServerInfo {
        McpServerInfo {
            name: "test-server".into(),
            version: "1.0.0".into(),
            capabilities: Value::Null,
        }
    }

    fn sample_tools() -> Vec<McpToolDefinition> {
        vec![
            McpToolDefinition {
                name: "search".into(),
                description: Some("Search for items".into()),
                input_schema: Some(serde_json::json!({
                    "type": "object",
                    "properties": { "query": { "type": "string" } }
                })),
            },
            McpToolDefinition {
                name: "create".into(),
                description: Some("Create an item".into()),
                input_schema: None,
            },
        ]
    }

    fn make_transport(mock: MockTransport) -> Arc<dyn McpTransport> {
        Arc::new(mock) as Arc<dyn McpTransport>
    }

    async fn collect_terminal(
        handler: &Arc<McpToolHandler>,
        args: Value,
    ) -> xai_tool_runtime::ToolStreamItem<TypedToolOutput> {
        use futures::StreamExt;
        use xai_computer_hub_sdk::ToolServerHandler;

        let ctx = ToolCallContext::default();
        let mut stream = handler.handle_call(ctx, args).await;
        stream.next().await.expect("terminal item")
    }

    #[tokio::test]
    async fn bridge_discovers_and_builds_handlers() {
        let transport = make_transport(MockTransport::new(sample_server_info(), sample_tools()));
        let config = McpBridgeConfig {
            session_id: SessionId::new("test-session").unwrap(),
            namespace: None,
        };

        let handle = McpBridge::connect(transport, &config).await.unwrap();
        assert_eq!(handle.server_info.name, "test-server");
        assert_eq!(handle.bridge.tool_count(), 2);

        let ids: Vec<String> = handle
            .bridge
            .handlers()
            .iter()
            .map(|h| h.tool_id.as_str().to_string())
            .collect();
        assert!(ids.contains(&"search".to_string()));
        assert!(ids.contains(&"create".to_string()));
    }

    #[tokio::test]
    async fn bridge_handler_descriptions() {
        let transport = make_transport(MockTransport::new(sample_server_info(), sample_tools()));
        let config = McpBridgeConfig {
            session_id: SessionId::new("test-session").unwrap(),
            namespace: Some("mcp".into()),
        };

        let handle = McpBridge::connect(transport, &config).await.unwrap();
        let handler = handle
            .bridge
            .handlers()
            .iter()
            .find(|h| h.tool_id.as_str() == "search")
            .unwrap();

        use xai_computer_hub_sdk::ToolServerHandler;
        let desc = handler.description();
        assert_eq!(desc.name, "search");
        assert_eq!(desc.description, "Search for items");
        assert_eq!(desc.namespace.as_deref(), Some("mcp"));
        assert!(handler.input_schema().is_some());
    }

    #[tokio::test]
    async fn bridge_forwards_call_text_response() {
        let call_result = McpCallResult {
            content: vec![McpContent::Text {
                text: "found 3 results".into(),
            }],
            is_error: false,
        };
        let transport = make_transport(
            MockTransport::new(sample_server_info(), sample_tools())
                .with_call_response(call_result),
        );
        let config = McpBridgeConfig {
            session_id: SessionId::new("test-session").unwrap(),
            namespace: None,
        };

        let handle = McpBridge::connect(Arc::clone(&transport), &config)
            .await
            .unwrap();
        let handler = handle
            .bridge
            .handlers()
            .iter()
            .find(|h| h.tool_id.as_str() == "search")
            .unwrap();

        use futures::StreamExt;
        use xai_computer_hub_sdk::ToolServerHandler;

        let ctx = ToolCallContext::default();
        let args = serde_json::json!({"query": "test"});
        let mut stream = handler.handle_call(ctx, args).await;

        let item = stream.next().await.unwrap();
        match item {
            xai_tool_runtime::ToolStreamItem::Terminal(Ok(typed)) => {
                let output: ToolOutputWire = serde_json::from_value(typed.value).unwrap();
                assert_eq!(output, ToolOutputWire::Text("found 3 results".into()));
            }
            other => panic!("expected Terminal(Ok(_)), got {other:?}"),
        }
    }

    #[tokio::test]
    async fn bridge_forwards_call_mcp_blocks_response() {
        let call_result = McpCallResult {
            content: vec![
                McpContent::Text {
                    text: "result text".into(),
                },
                McpContent::Image {
                    mime_type: "image/png".into(),
                    data: "base64data".into(),
                },
            ],
            is_error: false,
        };
        let transport = make_transport(
            MockTransport::new(sample_server_info(), sample_tools())
                .with_call_response(call_result),
        );
        let config = McpBridgeConfig {
            session_id: SessionId::new("test-session").unwrap(),
            namespace: None,
        };

        let handle = McpBridge::connect(transport, &config).await.unwrap();
        let handler = &handle.bridge.handlers()[0];

        use futures::StreamExt;
        use xai_computer_hub_sdk::ToolServerHandler;

        let ctx = ToolCallContext::default();
        let mut stream = handler
            .handle_call(ctx, Value::Object(Default::default()))
            .await;

        let item = stream.next().await.unwrap();
        match item {
            xai_tool_runtime::ToolStreamItem::Terminal(Ok(typed)) => {
                let output: ToolOutputWire = serde_json::from_value(typed.value).unwrap();
                match output {
                    ToolOutputWire::Mcp { blocks } => {
                        assert_eq!(blocks.len(), 2);
                        assert!(
                            matches!(&blocks[0], McpBlock::Text { text } if text == "result text")
                        );
                        assert!(
                            matches!(&blocks[1], McpBlock::Image { mime_type, .. } if mime_type == "image/png")
                        );
                    }
                    other => panic!("expected Mcp blocks, got {other:?}"),
                }
            }
            other => panic!("expected Terminal(Ok(_)), got {other:?}"),
        }
    }

    #[tokio::test]
    async fn bridge_handles_mcp_error_response() {
        let call_result = McpCallResult {
            content: vec![McpContent::Text {
                text: "permission denied".into(),
            }],
            is_error: true,
        };
        let transport = make_transport(
            MockTransport::new(sample_server_info(), sample_tools())
                .with_call_response(call_result),
        );
        let config = McpBridgeConfig {
            session_id: SessionId::new("test-session").unwrap(),
            namespace: None,
        };

        let handle = McpBridge::connect(transport, &config).await.unwrap();
        let handler = &handle.bridge.handlers()[0];

        use futures::StreamExt;
        use xai_computer_hub_sdk::ToolServerHandler;

        let ctx = ToolCallContext::default();
        let mut stream = handler.handle_call(ctx, Value::Null).await;

        let item = stream.next().await.unwrap();
        match item {
            xai_tool_runtime::ToolStreamItem::Terminal(Ok(typed)) => {
                let output: ToolOutputWire = serde_json::from_value(typed.value).unwrap();
                assert_eq!(output, ToolOutputWire::Text("permission denied".into()));
            }
            other => panic!("expected Terminal(Ok(_)), got {other:?}"),
        }
    }

    #[tokio::test]
    async fn bridge_handles_transport_error() {
        let transport = make_transport(
            MockTransport::new(sample_server_info(), sample_tools())
                .with_call_error(McpError::Transport("connection reset".into())),
        );
        let config = McpBridgeConfig {
            session_id: SessionId::new("test-session").unwrap(),
            namespace: None,
        };

        let handle = McpBridge::connect(transport, &config).await.unwrap();
        let handler = &handle.bridge.handlers()[0];

        use futures::StreamExt;
        use xai_computer_hub_sdk::ToolServerHandler;

        let ctx = ToolCallContext::default();
        let mut stream = handler.handle_call(ctx, Value::Null).await;

        let item = stream.next().await.unwrap();
        match item {
            xai_tool_runtime::ToolStreamItem::Terminal(Err(ref e))
                if e.kind == xai_tool_runtime::ToolErrorKind::Execution =>
            {
                assert!(
                    e.detail.contains("connection reset"),
                    "expected 'connection reset' in: {}",
                    e.detail
                );
            }
            other => panic!("expected Terminal(Err(Execution)), got {other:?}"),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn bridge_call_timeout_returns_within_bounded_test_window() {
        let mock =
            Arc::new(MockTransport::new(sample_server_info(), sample_tools()).with_hung_call());
        let active_calls = mock.active_calls();
        let transport: Arc<dyn McpTransport> = mock.clone();
        let config = McpBridgeConfig {
            session_id: SessionId::new("test-session").unwrap(),
            namespace: None,
        };

        let handle = McpBridge::connect(transport, &config).await.unwrap();
        let handler = handle.bridge.handlers()[0].clone();

        let task = tokio::spawn(async move { collect_terminal(&handler, Value::Null).await });
        tokio::task::yield_now().await;
        assert_eq!(active_calls.load(Ordering::SeqCst), 1);
        tokio::time::advance(MCP_TOOL_CALL_BACKSTOP_TIMEOUT - Duration::from_secs(1)).await;
        assert!(
            !task.is_finished(),
            "call must still be pending before timeout"
        );
        tokio::time::advance(Duration::from_secs(1)).await;

        let item = task.await.expect("join timeout task");
        match item {
            xai_tool_runtime::ToolStreamItem::Terminal(Err(ref e)) => {
                assert_eq!(e.kind, xai_tool_runtime::ToolErrorKind::Timeout);
            }
            other => panic!("expected Terminal(Err(Timeout)), got {other:?}"),
        }
        assert_eq!(active_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn bridge_call_timeout_uses_safe_timeout_message_and_classification() {
        crate::metrics::reset_mcp_timeout_metric_hooks_for_tests();
        let transport = make_transport(
            MockTransport::new(sample_server_info(), sample_tools()).with_hung_call(),
        );
        let config = McpBridgeConfig {
            session_id: SessionId::new("test-session").unwrap(),
            namespace: None,
        };

        let handle = McpBridge::connect(transport, &config).await.unwrap();
        let handler = handle.bridge.handlers()[0].clone();

        let task = tokio::spawn(async move { collect_terminal(&handler, Value::Null).await });
        tokio::task::yield_now().await;
        tokio::time::advance(MCP_TOOL_CALL_BACKSTOP_TIMEOUT).await;

        let item = task.await.expect("join timeout task");
        match item {
            xai_tool_runtime::ToolStreamItem::Terminal(Err(ref e)) => {
                assert_eq!(e.kind, xai_tool_runtime::ToolErrorKind::Timeout);
                assert!(e.detail.contains("MCP tool call timed out"));
                assert!(!e.detail.contains("test-token"));
                assert!(!e.detail.contains("query"));
            }
            other => panic!("expected Terminal(Err(Timeout)), got {other:?}"),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn bridge_timeout_metric_increments_exactly_once() {
        crate::metrics::reset_mcp_timeout_metric_hooks_for_tests();
        let before = crate::metrics::mcp_timeout_metric_hooks_snapshot_for_tests();
        let transport = make_transport(
            MockTransport::new(sample_server_info(), sample_tools()).with_hung_call(),
        );
        let config = McpBridgeConfig {
            session_id: SessionId::new("test-session").unwrap(),
            namespace: None,
        };

        let handle = McpBridge::connect(transport, &config).await.unwrap();
        let handler = handle.bridge.handlers()[0].clone();
        let task = tokio::spawn(async move { collect_terminal(&handler, Value::Null).await });
        tokio::task::yield_now().await;
        tokio::time::advance(MCP_TOOL_CALL_BACKSTOP_TIMEOUT).await;
        let _ = task.await.expect("join timeout task");

        let after = crate::metrics::mcp_timeout_metric_hooks_snapshot_for_tests();
        assert_eq!(after.timeouts, before.timeouts + 1);
    }

    #[tokio::test]
    async fn bridge_non_timeout_transport_error_keeps_existing_mapping() {
        crate::metrics::reset_mcp_timeout_metric_hooks_for_tests();
        let before = crate::metrics::mcp_timeout_metric_hooks_snapshot_for_tests();
        let transport = make_transport(
            MockTransport::new(sample_server_info(), sample_tools())
                .with_call_error(McpError::Transport("connection reset".into())),
        );
        let config = McpBridgeConfig {
            session_id: SessionId::new("test-session").unwrap(),
            namespace: None,
        };

        let handle = McpBridge::connect(transport, &config).await.unwrap();
        let handler = handle.bridge.handlers()[0].clone();
        let item = collect_terminal(&handler, Value::Null).await;
        match item {
            xai_tool_runtime::ToolStreamItem::Terminal(Err(ref e)) => {
                assert_eq!(e.kind, xai_tool_runtime::ToolErrorKind::Execution);
                assert!(e.detail.contains("connection reset"));
            }
            other => panic!("expected Terminal(Err(Execution)), got {other:?}"),
        }
        let after = crate::metrics::mcp_timeout_metric_hooks_snapshot_for_tests();
        assert_eq!(after.timeouts, before.timeouts);
    }

    #[tokio::test]
    async fn bridge_success_does_not_increment_timeout_metric() {
        crate::metrics::reset_mcp_timeout_metric_hooks_for_tests();
        let before = crate::metrics::mcp_timeout_metric_hooks_snapshot_for_tests();
        let call_result = McpCallResult {
            content: vec![McpContent::Text {
                text: "found 3 results".into(),
            }],
            is_error: false,
        };
        let transport = make_transport(
            MockTransport::new(sample_server_info(), sample_tools())
                .with_call_response(call_result),
        );
        let config = McpBridgeConfig {
            session_id: SessionId::new("test-session").unwrap(),
            namespace: None,
        };

        let handle = McpBridge::connect(transport, &config).await.unwrap();
        let handler = handle.bridge.handlers()[0].clone();
        let item = collect_terminal(&handler, serde_json::json!({"query": "test"})).await;
        match item {
            xai_tool_runtime::ToolStreamItem::Terminal(Ok(typed)) => {
                let output: ToolOutputWire = serde_json::from_value(typed.value).unwrap();
                assert_eq!(output, ToolOutputWire::Text("found 3 results".into()));
            }
            other => panic!("expected Terminal(Ok(_)), got {other:?}"),
        }
        let after = crate::metrics::mcp_timeout_metric_hooks_snapshot_for_tests();
        assert_eq!(after.timeouts, before.timeouts);
    }

    #[tokio::test(start_paused = true)]
    async fn repeated_timeouts_do_not_leave_accumulating_background_work() {
        crate::metrics::reset_mcp_timeout_metric_hooks_for_tests();
        let mock =
            Arc::new(MockTransport::new(sample_server_info(), sample_tools()).with_hung_call());
        let active_calls = mock.active_calls();
        let transport: Arc<dyn McpTransport> = mock.clone();
        let config = McpBridgeConfig {
            session_id: SessionId::new("test-session").unwrap(),
            namespace: None,
        };
        let handle = McpBridge::connect(transport, &config).await.unwrap();

        for _ in 0..3 {
            let handler = handle.bridge.handlers()[0].clone();
            let task = tokio::spawn(async move { collect_terminal(&handler, Value::Null).await });
            tokio::task::yield_now().await;
            assert_eq!(active_calls.load(Ordering::SeqCst), 1);
            tokio::time::advance(MCP_TOOL_CALL_BACKSTOP_TIMEOUT).await;
            let item = task.await.expect("join repeated timeout task");
            match item {
                xai_tool_runtime::ToolStreamItem::Terminal(Err(ref e)) => {
                    assert_eq!(e.kind, xai_tool_runtime::ToolErrorKind::Timeout);
                }
                other => panic!("expected Terminal(Err(Timeout)), got {other:?}"),
            }
            assert_eq!(active_calls.load(Ordering::SeqCst), 0);
        }
    }

    #[tokio::test]
    async fn bridge_shutdown_closes_transport() {
        let mock = Arc::new(MockTransport::new(sample_server_info(), sample_tools()));
        let transport: Arc<dyn McpTransport> = Arc::clone(&mock) as Arc<dyn McpTransport>;
        let config = McpBridgeConfig {
            session_id: SessionId::new("test-session").unwrap(),
            namespace: None,
        };

        let handle = McpBridge::connect(transport, &config).await.unwrap();
        assert!(!mock.closed.load(Ordering::SeqCst));

        handle.bridge.shutdown().await.unwrap();
        assert!(mock.closed.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn bridge_skips_tools_with_invalid_names() {
        let tools = vec![
            McpToolDefinition {
                name: "valid_tool".into(),
                description: Some("a valid tool".into()),
                input_schema: None,
            },
            McpToolDefinition {
                name: "".into(),
                description: Some("empty name".into()),
                input_schema: None,
            },
        ];
        let transport = make_transport(MockTransport::new(sample_server_info(), tools));
        let config = McpBridgeConfig {
            session_id: SessionId::new("test-session").unwrap(),
            namespace: None,
        };

        let handle = McpBridge::connect(transport, &config).await.unwrap();
        assert_eq!(handle.bridge.tool_count(), 1);
        assert_eq!(handle.bridge.handlers()[0].tool_id.as_str(), "valid_tool");
    }

    #[test]
    fn translate_mcp_result_single_text() {
        let result = McpCallResult {
            content: vec![McpContent::Text {
                text: "hello".into(),
            }],
            is_error: false,
        };
        assert_eq!(
            translate_mcp_result(&result),
            ToolOutputWire::Text("hello".into())
        );
    }

    #[test]
    fn translate_mcp_result_error_concatenates_text() {
        let result = McpCallResult {
            content: vec![
                McpContent::Text {
                    text: "line 1".into(),
                },
                McpContent::Text {
                    text: "line 2".into(),
                },
            ],
            is_error: true,
        };
        assert_eq!(
            translate_mcp_result(&result),
            ToolOutputWire::Text("line 1\nline 2".into())
        );
    }

    #[test]
    fn translate_mcp_result_mixed_content_uses_blocks() {
        let result = McpCallResult {
            content: vec![
                McpContent::Text {
                    text: "hello".into(),
                },
                McpContent::Resource {
                    uri: "file:///test".into(),
                    mime_type: Some("text/plain".into()),
                    text: Some("content".into()),
                },
            ],
            is_error: false,
        };
        match translate_mcp_result(&result) {
            ToolOutputWire::Mcp { blocks } => assert_eq!(blocks.len(), 2),
            other => panic!("expected Mcp, got {other:?}"),
        }
    }

    #[test]
    fn translate_mcp_result_empty_content_returns_empty_text() {
        let result = McpCallResult {
            content: vec![],
            is_error: false,
        };
        assert_eq!(
            translate_mcp_result(&result),
            ToolOutputWire::Text(String::new())
        );
    }

    #[test]
    fn translate_mcp_result_empty_error_content_returns_empty_text() {
        let result = McpCallResult {
            content: vec![],
            is_error: true,
        };
        assert_eq!(
            translate_mcp_result(&result),
            ToolOutputWire::Text(String::new())
        );
    }

    #[test]
    fn translate_mcp_result_error_with_only_image_drops_content() {
        let result = McpCallResult {
            content: vec![McpContent::Image {
                mime_type: "image/png".into(),
                data: "base64data".into(),
            }],
            is_error: true,
        };
        assert_eq!(
            translate_mcp_result(&result),
            ToolOutputWire::Text(String::new())
        );
    }
}
