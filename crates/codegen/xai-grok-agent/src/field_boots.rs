use std::path::Path;

use xai_grok_tools::types::tool::ToolKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExecutionReadinessState {
    Ready,
    Limited,
    PermissionRequired,
    EnvironmentBlocked,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionRoute {
    Local,
    Remote,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct KnownPrerequisite {
    pub name: String,
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TerrainAwareness {
    pub operating_system: String,
    pub architecture: String,
    pub workspace_context: WorkspaceContext,
    pub execution_context: ExecutionContext,
    pub shell_available: bool,
    pub filesystem_available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_available: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub known_prerequisites: Vec<KnownPrerequisite>,
    pub resource_health: ResourceHealth,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorkspaceContext {
    pub workspace_label: String,
    pub repo_detected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExecutionContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_detected: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ResourceHealth {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_healthy: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_healthy: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CapabilityReadiness {
    pub action_class: String,
    pub state: ExecutionReadinessState,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remediation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SafeStepMode {
    pub preview_supported: bool,
    pub preview_recommended: bool,
    pub preview_used: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RouteDecision {
    pub chosen_route: ExecutionRoute,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RecoveryGrip {
    pub checkpoint_available: bool,
    pub resume_available: bool,
    pub rollback_available: bool,
    pub irreversible_action_requires_review: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EvidenceFootprint {
    pub requested_action_class: String,
    pub chosen_route: ExecutionRoute,
    pub readiness_state: ExecutionReadinessState,
    pub preview_used: bool,
    pub execution_started: bool,
    pub execution_completed: bool,
    pub result_state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FieldBootsDecision {
    pub terrain_awareness: TerrainAwareness,
    pub readiness: CapabilityReadiness,
    pub traction_allowed: bool,
    pub safe_step_mode: SafeStepMode,
    pub pathfinder: RouteDecision,
    pub recovery_grip: RecoveryGrip,
    pub evidence_footprint: EvidenceFootprint,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_tools: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_prerequisites: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerrainInput {
    pub workspace_hint: Option<String>,
    pub repo_detected: bool,
    pub remote_context_detected: Option<bool>,
    pub shell_available: bool,
    pub filesystem_available: bool,
    pub network_available: Option<bool>,
    pub known_prerequisites: Vec<KnownPrerequisite>,
    pub disk_healthy: Option<bool>,
    pub memory_healthy: Option<bool>,
}

impl Default for TerrainInput {
    fn default() -> Self {
        Self {
            workspace_hint: None,
            repo_detected: true,
            remote_context_detected: None,
            shell_available: true,
            filesystem_available: true,
            network_available: None,
            known_prerequisites: Vec::new(),
            disk_healthy: None,
            memory_healthy: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldBootsRequest {
    pub action_class: String,
    pub required_tool_kinds: Vec<ToolKind>,
    pub missing_prerequisites: Vec<String>,
    pub requires_permission: bool,
    pub permission_granted: bool,
    pub resource_constrained: bool,
    pub expensive_action: bool,
    pub irreversible_action: bool,
    pub preview_supported: bool,
    pub preview_used: bool,
    pub remote_route_available: bool,
    pub checkpoint_available: bool,
    pub resume_available: bool,
    pub rollback_available: bool,
    pub execution_started: bool,
    pub execution_completed: bool,
    pub result_state: Option<String>,
}

impl FieldBootsRequest {
    pub fn new(action_class: impl Into<String>) -> Self {
        Self {
            action_class: action_class.into(),
            required_tool_kinds: vec![ToolKind::Execute],
            missing_prerequisites: Vec::new(),
            requires_permission: true,
            permission_granted: true,
            resource_constrained: false,
            expensive_action: false,
            irreversible_action: false,
            preview_supported: false,
            preview_used: false,
            remote_route_available: false,
            checkpoint_available: false,
            resume_available: false,
            rollback_available: false,
            execution_started: false,
            execution_completed: false,
            result_state: None,
        }
    }
}

pub fn evaluate_field_boots(
    terrain_input: TerrainInput,
    request: FieldBootsRequest,
    available_tool_kinds: &[ToolKind],
) -> FieldBootsDecision {
    let terrain = build_terrain(terrain_input);

    let mut required_tool_kinds = request.required_tool_kinds.clone();
    required_tool_kinds.sort_by_key(|kind| kind.as_key());
    required_tool_kinds.dedup();

    let required_tools = required_tool_kinds
        .iter()
        .map(|kind| kind.as_key().to_string())
        .collect::<Vec<_>>();

    let local_route_available = required_tool_kinds
        .iter()
        .all(|required| available_tool_kinds.contains(required));

    let route = choose_route(local_route_available, request.remote_route_available);

    let mut missing_prerequisites = request
        .missing_prerequisites
        .iter()
        .map(|p| sanitize_public_text(p))
        .collect::<Vec<_>>();
    missing_prerequisites.sort();
    missing_prerequisites.dedup();

    let action_class = sanitize_public_text(&request.action_class);

    let (readiness_state, reason, remediation) = determine_readiness(
        &terrain,
        &request,
        local_route_available,
        route,
        &missing_prerequisites,
    );

    let preview_recommended = request.preview_supported
        && (request.irreversible_action
            || request.expensive_action
            || request.resource_constrained);
    let irreversible_requires_review = request.irreversible_action && !request.preview_used;

    let result_state = match (
        request.execution_started,
        request.execution_completed,
        request.result_state.as_deref(),
    ) {
        (false, _, _) => "not_started".to_string(),
        (true, false, _) => "in_progress".to_string(),
        (true, true, Some(state)) => sanitize_public_text(state),
        (true, true, None) => "completed".to_string(),
    };

    let readiness = CapabilityReadiness {
        action_class: action_class.clone(),
        state: readiness_state,
        reason,
        remediation,
    };

    let traction_allowed = matches!(
        readiness.state,
        ExecutionReadinessState::Ready | ExecutionReadinessState::Limited
    );

    let pathfinder = RouteDecision {
        chosen_route: route,
        reason: route_reason(route),
    };

    let evidence_footprint = EvidenceFootprint {
        requested_action_class: action_class,
        chosen_route: route,
        readiness_state: readiness.state,
        preview_used: request.preview_used,
        execution_started: request.execution_started,
        execution_completed: request.execution_completed,
        result_state,
    };

    FieldBootsDecision {
        terrain_awareness: terrain,
        readiness,
        traction_allowed,
        safe_step_mode: SafeStepMode {
            preview_supported: request.preview_supported,
            preview_recommended,
            preview_used: request.preview_used,
        },
        pathfinder,
        recovery_grip: RecoveryGrip {
            checkpoint_available: request.checkpoint_available,
            resume_available: request.resume_available,
            rollback_available: request.rollback_available,
            irreversible_action_requires_review: irreversible_requires_review,
        },
        evidence_footprint,
        required_tools,
        missing_prerequisites,
    }
}

fn determine_readiness(
    terrain: &TerrainAwareness,
    request: &FieldBootsRequest,
    local_route_available: bool,
    route: ExecutionRoute,
    missing_prerequisites: &[String],
) -> (ExecutionReadinessState, String, Option<String>) {
    if !terrain.shell_available {
        return (
            ExecutionReadinessState::EnvironmentBlocked,
            "Shell execution is unavailable in the current environment.".to_string(),
            Some(
                "Use an environment with a supported shell route before executing commands."
                    .to_string(),
            ),
        );
    }

    if !terrain.filesystem_available {
        return (
            ExecutionReadinessState::EnvironmentBlocked,
            "Filesystem access is unavailable in the current environment.".to_string(),
            Some(
                "Enable filesystem access or run in an environment that exposes workspace files."
                    .to_string(),
            ),
        );
    }

    if !missing_prerequisites.is_empty() {
        return (
            ExecutionReadinessState::EnvironmentBlocked,
            "Required runtime prerequisites are missing for this action.".to_string(),
            Some(
                "Install or enable the missing prerequisites before executing this action."
                    .to_string(),
            ),
        );
    }

    if request.requires_permission && !request.permission_granted {
        return (
            ExecutionReadinessState::PermissionRequired,
            "Execution requires explicit permission in the current session mode.".to_string(),
            Some("Grant permission for execution, then retry.".to_string()),
        );
    }

    if request.resource_constrained {
        return (
            ExecutionReadinessState::Limited,
            "Resource pressure detected; execution should proceed with caution.".to_string(),
            Some("Run a preview first or reduce resource usage before full execution.".to_string()),
        );
    }

    if !local_route_available && route != ExecutionRoute::Remote {
        return (
            ExecutionReadinessState::Unavailable,
            "No available execution route supports this action right now.".to_string(),
            Some(
                "Enable the required execution tool or provide an existing alternate route."
                    .to_string(),
            ),
        );
    }

    (
        ExecutionReadinessState::Ready,
        "Execution prerequisites are satisfied for this action.".to_string(),
        None,
    )
}

fn choose_route(local_route_available: bool, remote_route_available: bool) -> ExecutionRoute {
    if local_route_available {
        return ExecutionRoute::Local;
    }
    if remote_route_available {
        return ExecutionRoute::Remote;
    }
    ExecutionRoute::Blocked
}

fn route_reason(route: ExecutionRoute) -> String {
    match route {
        ExecutionRoute::Local => {
            "Selected local route because it is available and deterministic.".to_string()
        }
        ExecutionRoute::Remote => {
            "Selected remote route because local route is unavailable and a remote route exists."
                .to_string()
        }
        ExecutionRoute::Blocked => "No valid execution route is available.".to_string(),
    }
}

fn build_terrain(input: TerrainInput) -> TerrainAwareness {
    let mut known_prerequisites = input
        .known_prerequisites
        .into_iter()
        .map(|item| KnownPrerequisite {
            name: sanitize_public_text(&item.name),
            available: item.available,
            source: item.source.map(|value| sanitize_public_text(&value)),
        })
        .collect::<Vec<_>>();
    known_prerequisites.sort_by(|a, b| a.name.cmp(&b.name));

    TerrainAwareness {
        operating_system: std::env::consts::OS.to_string(),
        architecture: std::env::consts::ARCH.to_string(),
        workspace_context: WorkspaceContext {
            workspace_label: sanitize_workspace_label(input.workspace_hint.as_deref()),
            repo_detected: input.repo_detected,
        },
        execution_context: ExecutionContext {
            remote_detected: input.remote_context_detected,
        },
        shell_available: input.shell_available,
        filesystem_available: input.filesystem_available,
        network_available: input.network_available,
        known_prerequisites,
        resource_health: ResourceHealth {
            disk_healthy: input.disk_healthy,
            memory_healthy: input.memory_healthy,
        },
    }
}

fn sanitize_workspace_label(workspace_hint: Option<&str>) -> String {
    let Some(workspace_hint) = workspace_hint else {
        return "workspace".to_string();
    };

    let path = Path::new(workspace_hint);
    let leaf = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("workspace");

    sanitize_public_text(leaf)
}

fn sanitize_public_text(input: &str) -> String {
    let lowered = input.to_ascii_lowercase();
    let blocked = [
        "operator-research-private",
        "nine linked rings",
        "delta/353",
        "private benchmark",
        "secret",
        "token",
        "password",
        "api_key",
        "apikey",
    ];
    if blocked.iter().any(|needle| lowered.contains(needle)) || looks_like_private_path(input) {
        return "[redacted]".to_string();
    }
    input.trim().to_string()
}

fn looks_like_private_path(input: &str) -> bool {
    let slashy = input.contains('\\') || input.contains('/');
    let windows_drive = input.as_bytes().get(1).is_some_and(|value| *value == b':');
    slashy && windows_drive
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PermissionMode;
    use crate::core_utility_belt::{UtilityBeltFacts, build_registry};

    fn healthy_terrain() -> TerrainInput {
        TerrainInput {
            workspace_hint: Some("C:/Users/example/project".to_string()),
            repo_detected: true,
            remote_context_detected: Some(false),
            shell_available: true,
            filesystem_available: true,
            network_available: Some(true),
            known_prerequisites: vec![KnownPrerequisite {
                name: "rustc".to_string(),
                available: true,
                source: Some("doctor".to_string()),
            }],
            disk_healthy: Some(true),
            memory_healthy: Some(true),
        }
    }

    fn ready_request() -> FieldBootsRequest {
        let mut request = FieldBootsRequest::new("terminal_command");
        request.preview_supported = true;
        request.expensive_action = true;
        request
    }

    #[test]
    fn healthy_local_environment_is_ready() {
        let decision =
            evaluate_field_boots(healthy_terrain(), ready_request(), &[ToolKind::Execute]);
        assert_eq!(decision.readiness.state, ExecutionReadinessState::Ready);
    }

    #[test]
    fn missing_execution_tool_is_unavailable() {
        let decision = evaluate_field_boots(healthy_terrain(), ready_request(), &[]);
        assert_eq!(
            decision.readiness.state,
            ExecutionReadinessState::Unavailable
        );
    }

    #[test]
    fn permission_gate_reports_permission_required() {
        let mut request = ready_request();
        request.permission_granted = false;
        let decision = evaluate_field_boots(healthy_terrain(), request, &[ToolKind::Execute]);
        assert_eq!(
            decision.readiness.state,
            ExecutionReadinessState::PermissionRequired
        );
    }

    #[test]
    fn missing_required_prerequisite_is_environment_blocked() {
        let mut request = ready_request();
        request.missing_prerequisites = vec!["nasm".to_string()];
        let decision = evaluate_field_boots(healthy_terrain(), request, &[ToolKind::Execute]);
        assert_eq!(
            decision.readiness.state,
            ExecutionReadinessState::EnvironmentBlocked
        );
    }

    #[test]
    fn constrained_resource_state_is_limited() {
        let mut request = ready_request();
        request.resource_constrained = true;
        let decision = evaluate_field_boots(healthy_terrain(), request, &[ToolKind::Execute]);
        assert_eq!(decision.readiness.state, ExecutionReadinessState::Limited);
    }

    #[test]
    fn local_route_is_selected_deterministically() {
        let decision = evaluate_field_boots(
            healthy_terrain(),
            ready_request(),
            &[ToolKind::Execute, ToolKind::Read],
        );
        assert_eq!(decision.pathfinder.chosen_route, ExecutionRoute::Local);
    }

    #[test]
    fn unavailable_local_route_does_not_claim_execution() {
        let decision = evaluate_field_boots(healthy_terrain(), ready_request(), &[]);
        assert_eq!(decision.evidence_footprint.execution_started, false);
        assert_eq!(decision.pathfinder.chosen_route, ExecutionRoute::Blocked);
    }

    #[test]
    fn alternate_route_is_selected_only_when_it_exists() {
        let mut request = ready_request();
        request.remote_route_available = true;
        let decision = evaluate_field_boots(healthy_terrain(), request, &[]);
        assert_eq!(decision.pathfinder.chosen_route, ExecutionRoute::Remote);
        assert_eq!(decision.readiness.state, ExecutionReadinessState::Ready);
    }

    #[test]
    fn no_available_route_is_blocked() {
        let decision = evaluate_field_boots(healthy_terrain(), ready_request(), &[]);
        assert_eq!(decision.pathfinder.chosen_route, ExecutionRoute::Blocked);
        assert_eq!(
            decision.readiness.state,
            ExecutionReadinessState::Unavailable
        );
    }

    #[test]
    fn preview_capability_is_represented_correctly() {
        let decision =
            evaluate_field_boots(healthy_terrain(), ready_request(), &[ToolKind::Execute]);
        assert!(decision.safe_step_mode.preview_supported);
        assert!(decision.safe_step_mode.preview_recommended);
        assert!(!decision.safe_step_mode.preview_used);
    }

    #[test]
    fn irreversible_review_required_state_is_preserved() {
        let mut request = ready_request();
        request.irreversible_action = true;
        request.preview_used = false;
        let decision = evaluate_field_boots(healthy_terrain(), request, &[ToolKind::Execute]);
        assert!(decision.recovery_grip.irreversible_action_requires_review);
    }

    #[test]
    fn checkpoint_and_recovery_availability_is_surfaced() {
        let mut request = ready_request();
        request.checkpoint_available = true;
        request.resume_available = true;
        request.rollback_available = true;
        let decision = evaluate_field_boots(healthy_terrain(), request, &[ToolKind::Execute]);
        assert!(decision.recovery_grip.checkpoint_available);
        assert!(decision.recovery_grip.resume_available);
        assert!(decision.recovery_grip.rollback_available);
    }

    #[test]
    fn execution_footprint_distinguishes_readiness_and_execution_progress() {
        let mut request = ready_request();
        request.execution_started = true;
        request.execution_completed = false;
        let in_progress = evaluate_field_boots(healthy_terrain(), request, &[ToolKind::Execute]);
        assert_eq!(in_progress.evidence_footprint.result_state, "in_progress");

        let complete = evaluate_field_boots(
            healthy_terrain(),
            FieldBootsRequest {
                execution_started: true,
                execution_completed: true,
                result_state: Some("succeeded".to_string()),
                ..ready_request()
            },
            &[ToolKind::Execute],
        );
        assert_eq!(complete.evidence_footprint.result_state, "succeeded");
    }

    #[test]
    fn output_does_not_leak_secrets() {
        let mut request = ready_request();
        request.action_class = "run with TOKEN=abcd".to_string();
        request.result_state = Some("api_key leaked".to_string());
        let decision = evaluate_field_boots(healthy_terrain(), request, &[ToolKind::Execute]);
        let json = serde_json::to_string(&decision).expect("decision should serialize");
        assert!(!json.to_ascii_lowercase().contains("token"));
        assert!(!json.to_ascii_lowercase().contains("api_key"));
    }

    #[test]
    fn output_does_not_leak_private_paths() {
        let terrain = TerrainInput {
            workspace_hint: Some("C:/Users/chris/operator-research-private".to_string()),
            ..healthy_terrain()
        };
        let decision = evaluate_field_boots(terrain, ready_request(), &[ToolKind::Execute]);
        let json = serde_json::to_string(&decision).expect("decision should serialize");
        assert!(!json.contains("operator-research-private"));
        assert!(!json.contains("C:/Users/chris/operator-research-private"));
    }

    #[test]
    fn output_ordering_is_deterministic() {
        let mut request = ready_request();
        request.required_tool_kinds = vec![ToolKind::Read, ToolKind::Execute, ToolKind::Read];
        request.missing_prerequisites = vec!["zlib".to_string(), "cmake".to_string()];
        let first = evaluate_field_boots(healthy_terrain(), request.clone(), &[ToolKind::Execute]);
        let second = evaluate_field_boots(healthy_terrain(), request, &[ToolKind::Execute]);
        assert_eq!(first, second);
        assert_eq!(first.required_tools, vec!["execute", "read"]);
    }

    #[test]
    fn utility_belt_behavior_remains_unchanged() {
        let registry = build_registry(UtilityBeltFacts {
            has_web_search: true,
            has_x_search: true,
            has_execute: true,
            has_read: true,
            has_web_fetch: true,
            has_search_tool: true,
            has_use_tool: true,
            has_image_generation: true,
            permission_mode: PermissionMode::BypassPermissions,
        });
        assert_eq!(registry.capabilities.len(), 7);
        assert!(
            registry
                .capabilities
                .iter()
                .any(|c| c.capability == "tool_availability_registry")
        );
    }
}
