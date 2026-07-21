//! MCP protocol types used by the adapter.
//!
//! These mirror the MCP specification's JSON-RPC shapes for server
//! metadata, tool definitions, and call results. They are intentionally
//! decoupled from any specific transport implementation so the bridge
//! stays testable with in-memory mocks.

use serde::{Deserialize, Serialize};

/// Metadata returned by a successful MCP `initialize` handshake.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerInfo {
    /// Human-readable server name (e.g. `"linear"`, `"github"`).
    pub name: String,
    /// Semver-ish version reported by the server.
    pub version: String,
    /// Free-form capability flags advertised during init.
    #[serde(default)]
    pub capabilities: serde_json::Value,
}

/// A single tool definition from MCP `tools/list`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolDefinition {
    /// Unqualified tool name (e.g. `"create_issue"`).
    pub name: String,
    /// Model-facing description of the tool.
    #[serde(default)]
    pub description: Option<String>,
    /// JSON Schema for the tool's input arguments.
    #[serde(default)]
    pub input_schema: Option<serde_json::Value>,
}

impl McpToolDefinition {
    /// Adapter-local retry safety hint.
    ///
    /// The marker is explicit and opt-in: `"x-retry-safe": true` at the top
    /// level of `input_schema`.
    pub fn is_retry_safe(&self) -> bool {
        self.input_schema
            .as_ref()
            .and_then(|schema| schema.get("x-retry-safe"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }
}

/// Result of an MCP `tools/call` invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpCallResult {
    /// Content blocks returned by the tool.
    #[serde(default)]
    pub content: Vec<McpContent>,
    /// When `true`, the tool signalled an application-level error.
    #[serde(default)]
    pub is_error: bool,
}

/// A single content block inside an [`McpCallResult`].
///
/// Covers the three content types defined by the MCP specification:
/// text, image (base64-encoded), and embedded resource.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum McpContent {
    /// Plain text content.
    #[serde(rename = "text")]
    Text {
        /// The text payload.
        text: String,
    },
    /// Base64-encoded image content.
    #[serde(rename = "image")]
    Image {
        /// MIME type (e.g. `"image/png"`).
        #[serde(rename = "mimeType")]
        mime_type: String,
        /// Base64-encoded image bytes.
        data: String,
    },
    /// Embedded resource content.
    #[serde(rename = "resource")]
    Resource {
        /// Resource URI.
        uri: String,
        /// Optional MIME type.
        #[serde(default, rename = "mimeType")]
        mime_type: Option<String>,
        /// Optional text body.
        #[serde(default)]
        text: Option<String>,
    },
}

/// Errors originating from MCP transport or protocol handling.
#[derive(Debug, Clone, thiserror::Error)]
pub enum McpError {
    /// The underlying transport failed (connection refused, pipe broken, etc.).
    #[error("transport error: {0}")]
    Transport(String),

    /// A transport failure that happened before the remote tool call could
    /// execute (e.g., send/connect failure before request dispatch).
    #[error("pre-call transport error: {0}")]
    TransportPreCall(String),

    /// The server returned a JSON-RPC error response.
    #[error("protocol error (code {code}): {message}")]
    Protocol {
        /// JSON-RPC error code.
        code: i64,
        /// Human-readable error message.
        message: String,
    },

    /// Timeout waiting for MCP server response.
    #[error("timeout: {0}")]
    Timeout(String),

    /// The response could not be decoded.
    #[error("decode error: {0}")]
    Decode(String),
}

impl McpError {
    /// True when this failure class is transient enough to consider retrying.
    pub fn is_transient(&self) -> bool {
        match self {
            Self::Transport(_) | Self::TransportPreCall(_) | Self::Timeout(_) => true,
            Self::Protocol { code, .. } => protocol_code_is_transient(*code),
            Self::Decode(_) => false,
        }
    }

    /// True when transport failed before the remote tool body could run.
    pub fn happened_before_remote_execution(&self) -> bool {
        matches!(self, Self::TransportPreCall(_))
    }
}

fn protocol_code_is_transient(code: i64) -> bool {
    matches!(code, 408 | 409 | 425 | 429) || (500..=599).contains(&code)
}
