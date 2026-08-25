use crate::config::PermissionMode;

/// Public-safe capability state for utility-belt components.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CapabilityState {
    Available,
    Unavailable,
    Degraded,
    PermissionRequired,
}

/// One capability entry in the utility-belt registry.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CapabilityReport {
    pub capability: String,
    pub state: CapabilityState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remediation: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provenance: Vec<String>,
    pub freshness_aware: bool,
}

/// Deterministic status snapshot for the focused core utility belt.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CoreUtilityBeltRegistry {
    pub capabilities: Vec<CapabilityReport>,
}

/// Runtime facts used to derive the utility-belt status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UtilityBeltFacts {
    pub has_web_search: bool,
    pub has_x_search: bool,
    pub has_execute: bool,
    pub has_read: bool,
    pub has_web_fetch: bool,
    pub has_search_tool: bool,
    pub has_use_tool: bool,
    pub has_image_generation: bool,
    pub permission_mode: PermissionMode,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FallbackDecision {
    pub requested_capability: String,
    pub requested_action: String,
    pub executed: bool,
    pub blocked: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used_alternative: Option<String>,
    pub message: String,
}

fn requires_permission(mode: PermissionMode) -> bool {
    mode != PermissionMode::BypassPermissions
}

fn report(
    capability: &str,
    state: CapabilityState,
    provider: Option<&str>,
    reason: Option<&str>,
    remediation: Option<&str>,
    provenance: &[&str],
    freshness_aware: bool,
) -> CapabilityReport {
    CapabilityReport {
        capability: capability.to_string(),
        state,
        provider: provider.map(str::to_string),
        reason: reason.map(str::to_string),
        remediation: remediation.map(str::to_string),
        provenance: provenance.iter().map(|v| (*v).to_string()).collect(),
        freshness_aware,
    }
}

pub fn build_registry(facts: UtilityBeltFacts) -> CoreUtilityBeltRegistry {
    let permission_gate = requires_permission(facts.permission_mode);

    let web = if facts.has_web_search {
        report(
            "web_live_search",
            CapabilityState::Available,
            Some("responses_web_search"),
            None,
            None,
            &["source_url", "provider", "timestamp"],
            true,
        )
    } else {
        report(
            "web_live_search",
            CapabilityState::Unavailable,
            None,
            Some("No live web search provider is configured for this runtime."),
            Some("Enable a web_search provider in the active tool configuration."),
            &[],
            true,
        )
    };

    let x = if facts.has_x_search {
        report(
            "x_live_search",
            CapabilityState::Available,
            Some("hosted_x_search"),
            None,
            None,
            &["source_url", "provider", "timestamp"],
            true,
        )
    } else if facts.has_web_search {
        report(
            "x_live_search",
            CapabilityState::Degraded,
            None,
            Some("Real-time X search is unavailable; web search fallback remains available."),
            Some("Enable hosted x_search support for this model/runtime."),
            &["source_url", "provider", "timestamp"],
            true,
        )
    } else {
        report(
            "x_live_search",
            CapabilityState::Unavailable,
            None,
            Some("Real-time X search is not available in this runtime."),
            Some("Enable backend search and x_search support."),
            &[],
            true,
        )
    };

    let python = if facts.has_execute {
        if permission_gate {
            report(
                "python_execution",
                CapabilityState::PermissionRequired,
                Some("shell_python"),
                Some("Python execution requires runtime permission in this session mode."),
                Some("Approve command execution or switch to a permission mode that allows it."),
                &["tool_call", "stdout", "stderr", "exit_code"],
                false,
            )
        } else {
            report(
                "python_execution",
                CapabilityState::Available,
                Some("shell_python"),
                None,
                None,
                &["tool_call", "stdout", "stderr", "exit_code"],
                false,
            )
        }
    } else {
        report(
            "python_execution",
            CapabilityState::Unavailable,
            None,
            Some("No safe execution tool is available for Python code."),
            Some("Enable command execution tooling for this agent/session."),
            &[],
            false,
        )
    };

    let research = if facts.has_web_fetch && facts.has_search_tool && facts.has_use_tool {
        report(
            "research_multitool",
            CapabilityState::Available,
            Some("web_fetch+mcp"),
            None,
            None,
            &["url", "source", "citation"],
            true,
        )
    } else if facts.has_web_fetch || (facts.has_search_tool && facts.has_use_tool) {
        report(
            "research_multitool",
            CapabilityState::Degraded,
            Some("partial_research_stack"),
            Some("Research stack is partially available; some evidence workflows are limited."),
            Some("Enable both web_fetch and MCP search/dispatch tooling for full coverage."),
            &["url", "source", "citation"],
            true,
        )
    } else if facts.has_search_tool ^ facts.has_use_tool {
        report(
            "research_multitool",
            CapabilityState::Degraded,
            Some("mcp_partial"),
            Some("MCP research is partially configured (discovery/dispatch mismatch)."),
            Some("Enable both search_tool and use_tool together."),
            &["source"],
            true,
        )
    } else {
        report(
            "research_multitool",
            CapabilityState::Unavailable,
            None,
            Some("No research-capable web or integration tooling is available."),
            Some("Enable web_fetch or MCP search+dispatch capabilities."),
            &[],
            true,
        )
    };

    let vision = if facts.has_read {
        report(
            "vision_analysis",
            CapabilityState::Available,
            Some("read_file_multimodal"),
            None,
            None,
            &["image_uri", "mime_type"],
            false,
        )
    } else if facts.has_image_generation {
        report(
            "vision_analysis",
            CapabilityState::Degraded,
            Some("generation_only"),
            Some("Image generation is available, but image analysis input is not."),
            Some("Enable multimodal file/image read capability."),
            &["image_uri", "mime_type"],
            false,
        )
    } else {
        report(
            "vision_analysis",
            CapabilityState::Unavailable,
            None,
            Some("No image-analysis capability is available in this runtime."),
            Some("Enable multimodal file/image read capability."),
            &[],
            false,
        )
    };

    let registry = report(
        "tool_availability_registry",
        CapabilityState::Available,
        Some("core_utility_belt_v1"),
        None,
        None,
        &["capability", "state", "provider", "reason"],
        false,
    );

    let fallback = report(
        "graceful_fallback",
        CapabilityState::Available,
        Some("core_utility_belt_v1"),
        None,
        None,
        &[
            "requested_capability",
            "executed",
            "blocked",
            "used_alternative",
        ],
        false,
    );

    CoreUtilityBeltRegistry {
        capabilities: vec![web, x, python, research, vision, registry, fallback],
    }
}

pub fn fallback_for_request(
    registry: &CoreUtilityBeltRegistry,
    requested_capability: &str,
    requested_action: &str,
    alternatives: &[&str],
) -> FallbackDecision {
    let state = registry
        .capabilities
        .iter()
        .find(|c| c.capability == requested_capability)
        .map(|c| c.state)
        .unwrap_or(CapabilityState::Unavailable);

    if state == CapabilityState::Available {
        return FallbackDecision {
            requested_capability: requested_capability.to_string(),
            requested_action: requested_action.to_string(),
            executed: true,
            blocked: false,
            used_alternative: None,
            message: "Capability is available; execute the requested action normally.".to_string(),
        };
    }

    if let Some(alternative) = alternatives.first() {
        return FallbackDecision {
            requested_capability: requested_capability.to_string(),
            requested_action: requested_action.to_string(),
            executed: false,
            blocked: false,
            used_alternative: Some((*alternative).to_string()),
            message: format!(
                "Requested capability '{requested_capability}' is {state:?}; primary action was not executed. Use '{alternative}' instead."
            ),
        };
    }

    FallbackDecision {
        requested_capability: requested_capability.to_string(),
        requested_action: requested_action.to_string(),
        executed: false,
        blocked: true,
        used_alternative: None,
        message: format!(
            "Requested capability '{requested_capability}' is {state:?}; no valid fallback is available."
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find<'a>(reg: &'a CoreUtilityBeltRegistry, name: &str) -> &'a CapabilityReport {
        reg.capabilities
            .iter()
            .find(|c| c.capability == name)
            .expect("capability report must exist")
    }

    fn baseline() -> UtilityBeltFacts {
        UtilityBeltFacts {
            has_web_search: true,
            has_x_search: true,
            has_execute: true,
            has_read: true,
            has_web_fetch: true,
            has_search_tool: true,
            has_use_tool: true,
            has_image_generation: true,
            permission_mode: PermissionMode::BypassPermissions,
        }
    }

    #[test]
    fn available_web_capability_reports_available() {
        let reg = build_registry(baseline());
        assert_eq!(
            find(&reg, "web_live_search").state,
            CapabilityState::Available
        );
    }

    #[test]
    fn unavailable_live_search_reports_unavailable_not_success() {
        let mut facts = baseline();
        facts.has_web_search = false;
        let reg = build_registry(facts);
        assert_eq!(
            find(&reg, "web_live_search").state,
            CapabilityState::Unavailable
        );
        let fallback = fallback_for_request(&reg, "web_live_search", "search latest", &[]);
        assert!(!fallback.executed);
        assert!(fallback.blocked);
    }

    #[test]
    fn python_execution_availability_is_reported() {
        let reg = build_registry(baseline());
        assert_eq!(
            find(&reg, "python_execution").state,
            CapabilityState::Available
        );
    }

    #[test]
    fn research_open_page_capability_is_represented() {
        let reg = build_registry(baseline());
        let report = find(&reg, "research_multitool");
        assert!(matches!(
            report.state,
            CapabilityState::Available | CapabilityState::Degraded
        ));
    }

    #[test]
    fn vision_capability_is_represented() {
        let reg = build_registry(baseline());
        assert_eq!(
            find(&reg, "vision_analysis").state,
            CapabilityState::Available
        );
    }

    #[test]
    fn permission_required_is_distinct_from_unavailable() {
        let mut facts = baseline();
        facts.permission_mode = PermissionMode::Default;
        let reg = build_registry(facts);
        assert_eq!(
            find(&reg, "python_execution").state,
            CapabilityState::PermissionRequired
        );
        assert_ne!(
            find(&reg, "python_execution").state,
            CapabilityState::Unavailable
        );
    }

    #[test]
    fn degraded_is_distinct_from_unavailable() {
        let mut facts = baseline();
        facts.has_x_search = false;
        let reg = build_registry(facts);
        assert_eq!(find(&reg, "x_live_search").state, CapabilityState::Degraded);
        assert_ne!(
            find(&reg, "x_live_search").state,
            CapabilityState::Unavailable
        );
    }

    #[test]
    fn fallback_never_claims_execution_when_primary_unavailable() {
        let mut facts = baseline();
        facts.has_web_search = false;
        let reg = build_registry(facts);
        let decision = fallback_for_request(&reg, "web_live_search", "live query", &["web_fetch"]);
        assert!(!decision.executed);
        assert_eq!(decision.used_alternative.as_deref(), Some("web_fetch"));
    }

    #[test]
    fn registry_output_is_deterministic() {
        let facts = baseline();
        let a = build_registry(facts.clone());
        let b = build_registry(facts);
        assert_eq!(a, b);
    }

    #[test]
    fn reports_do_not_leak_secrets_or_private_paths() {
        let reg = build_registry(baseline());
        let serialized = serde_json::to_string(&reg).expect("registry serializes");
        for forbidden in [
            "api_key",
            "secret",
            "token",
            "operator-research-private",
            "C:\\\\Users\\\\",
        ] {
            assert!(
                !serialized
                    .to_ascii_lowercase()
                    .contains(&forbidden.to_ascii_lowercase()),
                "registry leaked forbidden marker: {forbidden}"
            );
        }
    }

    #[test]
    fn available_capabilities_do_not_require_fallback_interception() {
        let reg = build_registry(baseline());
        let decision = fallback_for_request(&reg, "web_live_search", "search", &["web_fetch"]);
        assert!(decision.executed);
        assert!(!decision.blocked);
        assert!(decision.used_alternative.is_none());
    }
}
