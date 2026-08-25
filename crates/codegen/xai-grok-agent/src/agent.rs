//! Agent — a fully built agent: definition + session context.

use std::sync::Arc;

use xai_grok_sampling_types::HostedTool;
use xai_grok_tools::bridge::ToolBridge;
use xai_grok_tools::types::definition::ToolDefinition;
use xai_grok_tools::types::tool::ToolKind;

use crate::compaction::CompactionPolicy;
use crate::config::{AgentDefinition, CompletionRequirement, PermissionMode};
use crate::core_utility_belt::{
    CoreUtilityBeltRegistry, FallbackDecision, UtilityBeltFacts, build_registry,
    fallback_for_request,
};
use crate::prompt::context::PromptContext;
use crate::system_reminder::ReminderPolicy;

/// A fully built agent: definition + session context.
///
/// NOT portable — tied to a specific session via its ToolBridge,
/// rendered system prompt, and session-level policies.
///
/// Created by AgentBuilder from an AgentDefinition + session context.
///
/// The Agent is effectively immutable after construction. It holds
/// Arc<ToolBridge> — mutations to tool state (MCP registration,
/// completion tracking, retry config) go through ToolBridge's
/// internal locks.
pub struct Agent {
    /// The definition this agent was built from.
    definition: AgentDefinition,

    /// The context that produced the current system prompt.
    /// Stored for inspection, re-rendering, and serialization.
    prompt_context: PromptContext,

    /// The rendered system prompt (cached from prompt_context.render()).
    system_prompt: String,

    /// The tool bridge — owns ToolRegistry + ToolState + SessionContext.
    tool_bridge: Arc<ToolBridge>,

    /// Session-level policies.
    reminder_policy: ReminderPolicy,
    compaction_policy: CompactionPolicy,

    /// Backend-hosted tools to include in API requests.
    /// These are sent as native Responses API types (e.g., `WebSearch`)
    /// and executed server-side by the agentic sampler.
    hosted_tools: Vec<HostedTool>,

    /// Build-time toggle for server-side search tools. ANDed at request
    /// time with the per-model `SessionActor::supports_backend_search`.
    backend_search_enabled: bool,
}

impl Agent {
    /// Create a new Agent.
    ///
    /// Normally called by `AgentBuilder::build()`. Exposed publicly for
    /// test helpers that need to construct an Agent with a pre-built ToolBridge.
    pub fn new(
        definition: AgentDefinition,
        prompt_context: PromptContext,
        system_prompt: String,
        tool_bridge: Arc<ToolBridge>,
        reminder_policy: ReminderPolicy,
        compaction_policy: CompactionPolicy,
        hosted_tools: Vec<HostedTool>,
        backend_search_enabled: bool,
    ) -> Self {
        Self {
            definition,
            prompt_context,
            system_prompt,
            tool_bridge,
            reminder_policy,
            compaction_policy,
            hosted_tools,
            backend_search_enabled,
        }
    }

    // ── From definition ──────────────────────────────────────────────

    /// Agent name (unique identifier).
    pub fn name(&self) -> &str {
        &self.definition.name
    }

    /// Agent description.
    pub fn description(&self) -> &str {
        &self.definition.description
    }

    /// The full agent definition.
    pub fn definition(&self) -> &AgentDefinition {
        &self.definition
    }

    /// Permission mode for this agent.
    pub fn permission_mode(&self) -> &PermissionMode {
        &self.definition.permission_mode
    }

    /// Completion requirement, if any.
    pub fn completion_requirement(&self) -> Option<&CompletionRequirement> {
        self.definition.completion_requirement.as_ref()
    }

    // ── Session-level ────────────────────────────────────────────────

    /// The rendered system prompt.
    pub fn system_prompt(&self) -> &str {
        &self.system_prompt
    }

    /// Compact system prompt for post-compaction use.
    ///
    /// Returns a static string — the compact prompt never changes at runtime.
    pub fn compact_system_prompt(&self) -> &str {
        crate::prompt::template::COMPACT_SYSTEM_PROMPT
    }

    /// The tool bridge for this agent.
    pub fn tool_bridge(&self) -> &Arc<ToolBridge> {
        &self.tool_bridge
    }

    /// Compaction policy.
    pub fn compaction_policy(&self) -> &CompactionPolicy {
        &self.compaction_policy
    }

    /// Reminder policy.
    pub fn reminder_policy(&self) -> &ReminderPolicy {
        &self.reminder_policy
    }

    /// Cached AGENTS.md section (derived from prompt_context).
    pub fn agents_md_section(&self) -> Option<String> {
        self.prompt_context.format_agents_md_section()
    }

    /// AGENTS.md content formatted for user-message injection.
    ///
    /// Returns the `<system-reminder>` block to prepend as a user message,
    /// respecting audience (compacted for subagents) and template.
    pub fn agents_md_user_reminder(&self) -> Option<String> {
        self.prompt_context.agents_md_user_reminder()
    }

    /// Personas content formatted for user-message injection.
    ///
    /// Returns the `<system-reminder>` block to prepend as a user message,
    /// respecting audience (suppressed for subagents) and template.
    pub fn personas_user_reminder(&self) -> Option<String> {
        self.prompt_context.personas_user_reminder()
    }

    /// The structured prompt context for inspection and re-rendering.
    pub fn prompt_context(&self) -> &PromptContext {
        &self.prompt_context
    }

    /// Audience this agent's prompt was rendered for (Primary or Subagent).
    ///
    /// Used by the runtime turn-end TodoGate together with
    /// [`crate::AgentDefinition::carries_task_completion_discipline`] to
    /// decide whether the active prompt actually carries the discipline
    /// rules the gate's reminder text invokes.
    pub fn prompt_audience(&self) -> crate::prompt::context::PromptAudience {
        self.prompt_context.audience
    }

    /// Tool definitions for the sampling API — delegates to ToolBridge.
    pub async fn tool_definitions(&self) -> Vec<ToolDefinition> {
        self.tool_bridge.tool_definitions().await
    }

    /// Backend-hosted tools that should be included in API requests.
    /// These are sent as native types (e.g., `rs::Tool::WebSearch`) and
    /// executed server-side by the agentic sampler.
    pub fn hosted_tools(&self) -> &[HostedTool] {
        &self.hosted_tools
    }

    /// Build-time toggle for server-side search tools. Callers should
    /// AND this with the per-model `supports_backend_search` flag to
    /// decide whether to ship `hosted_tools` on a request. Do not use
    /// `hosted_tools().is_empty()` as a proxy — the list also depends
    /// on web-search config.
    pub fn backend_search_enabled(&self) -> bool {
        self.backend_search_enabled
    }

    /// Built-in tool definitions only (excludes MCP tools).
    pub async fn tool_definitions_builtins_only(&self) -> Vec<ToolDefinition> {
        self.tool_bridge.tool_definitions_builtins_only().await
    }

    /// Deterministic runtime capability snapshot for the focused core utility belt.
    pub async fn core_utility_belt_registry(&self) -> CoreUtilityBeltRegistry {
        let has_web_search = self
            .tool_bridge
            .tool_for_kind(ToolKind::WebSearch)
            .await
            .is_some()
            || self
                .hosted_tools
                .iter()
                .any(|t| matches!(t, HostedTool::WebSearch { .. }));
        let has_x_search = self
            .hosted_tools
            .iter()
            .any(|t| matches!(t, HostedTool::XSearch { .. }));
        let has_execute = self
            .tool_bridge
            .tool_for_kind(ToolKind::Execute)
            .await
            .is_some();
        let has_read = self
            .tool_bridge
            .tool_for_kind(ToolKind::Read)
            .await
            .is_some();
        let has_web_fetch = self
            .tool_bridge
            .tool_for_kind(ToolKind::WebFetch)
            .await
            .is_some();
        let has_search_tool = self
            .tool_bridge
            .tool_for_kind(ToolKind::SearchTool)
            .await
            .is_some();
        let has_use_tool = self
            .tool_bridge
            .tool_for_kind(ToolKind::UseTool)
            .await
            .is_some();
        let has_image_generation = self
            .tool_bridge
            .tool_for_kind(ToolKind::ImageGen)
            .await
            .is_some()
            || self
                .tool_bridge
                .tool_for_kind(ToolKind::ImageToVideo)
                .await
                .is_some()
            || self
                .tool_bridge
                .tool_for_kind(ToolKind::ReferenceToVideo)
                .await
                .is_some();

        build_registry(UtilityBeltFacts {
            has_web_search,
            has_x_search,
            has_execute,
            has_read,
            has_web_fetch,
            has_search_tool,
            has_use_tool,
            has_image_generation,
            permission_mode: self.definition.permission_mode.clone(),
        })
    }

    /// Resolve a safe fallback plan when a requested utility-belt capability is
    /// unavailable or degraded.
    pub async fn core_utility_belt_fallback(
        &self,
        requested_capability: &str,
        requested_action: &str,
        alternatives: &[&str],
    ) -> FallbackDecision {
        let registry = self.core_utility_belt_registry().await;
        fallback_for_request(
            &registry,
            requested_capability,
            requested_action,
            alternatives,
        )
    }

    /// Whether auto-compact should trigger given current token usage.
    ///
    /// `context_window` comes from the session's SamplingConfig (model-provided).
    pub fn should_auto_compact(
        &self,
        total_tokens: u64,
        context_window: std::num::NonZeroU64,
    ) -> bool {
        let cw = context_window.get();
        xai_token_estimation::exceeds_threshold(
            total_tokens,
            cw,
            self.compaction_policy.auto_compact_threshold_percent as u8,
        )
    }

    /// Update completion and retry policies from a new definition.
    ///
    /// Does NOT rebuild the tool registry or re-render prompts.
    /// Used for mid-session mode switching.
    pub async fn update_policies_from_definition(&self, _def: &AgentDefinition) {
        // TODO: completion requirements and retry configs are now part of
        // ToolServerConfig and handled at registry finalization time.
        // Mid-session policy updates are not yet supported in the new architecture.
    }

    /// Re-render the system prompt from current ToolBridge state
    /// (tool name overrides, disabled tools). Called by hosts after
    /// mid-session tool-override updates.
    pub async fn finalize_prompt(&mut self) {
        self.prompt_context.build_timestamp_utc = chrono::Utc::now().to_rfc3339();

        self.system_prompt = self
            .prompt_context
            .render(&self.tool_bridge)
            .await
            .unwrap_or_default();
    }

    /// Re-render the system prompt for a different definition, reusing
    /// the existing ToolBridge. Used for mid-session mode switching.
    pub async fn render_prompt_for_definition(&self, definition: &AgentDefinition) -> String {
        let mut ctx = self.prompt_context.clone();
        ctx.prompt_mode = definition.prompt_mode.clone();
        ctx.prompt_body = definition.prompt_body.clone();
        ctx.system_prompt = definition.system_prompt.clone();
        ctx.include_browser_verification = definition.include_browser_verification();
        ctx.build_timestamp_utc = chrono::Utc::now().to_rfc3339();

        // Clear agents_md if the new definition doesn't want it
        if !definition.agents_md {
            ctx.agents_md_files.clear();
        }

        ctx.render(&self.tool_bridge).await.unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;

    /// Standalone function testing the same logic as Agent::should_auto_compact
    fn should_auto_compact_check(total_tokens: u64, context_window: u64, threshold: u32) -> bool {
        let cw = NonZeroU64::new(context_window).expect("test context_window must be non-zero");
        let usage_percent = (total_tokens * 100) / cw.get();
        usage_percent >= threshold as u64
    }

    #[test]
    fn test_should_auto_compact_below_threshold() {
        // 80% of 100K window with 85% threshold → false
        assert!(!should_auto_compact_check(80_000, 100_000, 85));
    }

    #[test]
    fn test_should_auto_compact_above_threshold() {
        // 90% of 100K window with 85% threshold → true
        assert!(should_auto_compact_check(90_000, 100_000, 85));
    }

    #[test]
    fn test_should_auto_compact_at_threshold() {
        // Exactly 85% of 100K window with 85% threshold → true
        assert!(should_auto_compact_check(85_000, 100_000, 85));
    }

    #[test]
    fn test_should_auto_compact_empty_usage() {
        // 0 tokens used → false
        assert!(!should_auto_compact_check(0, 100_000, 85));
    }

    #[test]
    fn test_should_auto_compact_100_percent_threshold() {
        // 100% threshold → only triggers when fully used
        assert!(!should_auto_compact_check(99_999, 100_000, 100));
        assert!(should_auto_compact_check(100_000, 100_000, 100));
    }
}
