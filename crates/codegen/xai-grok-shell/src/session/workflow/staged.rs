use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageStatus {
    Pending,
    Ready,
    Running,
    Succeeded,
    Failed,
    Blocked,
    Compensating,
    Compensated,
    Cancelled,
    ReviewRequired,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageExecutionClass {
    Reversible,
    Irreversible,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StageDefinition {
    pub stage_id: String,
    #[serde(default)]
    pub prerequisites: Vec<String>,
    pub execution_class: StageExecutionClass,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compensation_action: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StageRuntime {
    pub status: StageStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked_reason: Option<String>,
    #[serde(default)]
    pub attempt_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_details: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StageSnapshot {
    pub operation_id: String,
    pub stage_id: String,
    pub status: StageStatus,
    pub prerequisites: Vec<String>,
    pub blocked_reason: Option<String>,
    pub attempt_count: u32,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub execution_class: StageExecutionClass,
    pub checkpoint_ref: Option<String>,
    pub compensation_action: Option<String>,
    pub failure_details: Option<String>,
    pub owner_session_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CheckpointSnapshot {
    pub operation_id: String,
    pub owner_session_id: String,
    pub checkpoint_id: String,
    pub recorded_at: String,
    pub completed_stages: Vec<String>,
    pub remaining_stages: Vec<String>,
    pub running_stage: Option<String>,
    pub recovery_actions_available: Vec<String>,
    pub resume_safe: bool,
    pub stages: Vec<StageSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DryRunPlan {
    pub operation_id: String,
    pub ordered_stages: Vec<String>,
    pub ready_stages: Vec<String>,
    pub blocked_stages: Vec<(String, String)>,
    pub reversible_stages: Vec<String>,
    pub irreversible_stages: Vec<String>,
    pub checkpoint_candidates: Vec<String>,
    pub recovery_sequence: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StagedOperationError {
    #[error("duplicate stage id: {0}")]
    DuplicateStage(String),
    #[error("missing prerequisite '{prerequisite}' for stage '{stage}'")]
    MissingPrerequisite { prerequisite: String, stage: String },
    #[error("dependency cycle detected")]
    DependencyCycle,
    #[error("stage not found: {0}")]
    StageNotFound(String),
    #[error("stage is not currently executable: {0}")]
    StageNotExecutable(String),
    #[error("stage is already running: {0}")]
    StageAlreadyRunning(String),
    #[error("invalid stage transition for '{stage}': {from:?} -> {to:?}")]
    InvalidTransition {
        stage: String,
        from: StageStatus,
        to: StageStatus,
    },
    #[error("checkpoint cannot be restored because it no longer matches stage definitions")]
    CheckpointMismatch,
}

#[derive(Debug, Clone)]
pub struct StagedOperation {
    operation_id: String,
    owner_session_id: String,
    definitions: HashMap<String, StageDefinition>,
    runtime: HashMap<String, StageRuntime>,
    stage_order: Vec<String>,
    recovery_queue: VecDeque<String>,
}

impl StagedOperation {
    pub fn new(
        operation_id: String,
        owner_session_id: String,
        definitions: Vec<StageDefinition>,
    ) -> Result<Self, StagedOperationError> {
        let mut by_id = HashMap::new();
        let mut stage_order = Vec::new();
        for def in definitions {
            if by_id.contains_key(&def.stage_id) {
                return Err(StagedOperationError::DuplicateStage(def.stage_id));
            }
            stage_order.push(def.stage_id.clone());
            by_id.insert(def.stage_id.clone(), def);
        }

        for stage_id in &stage_order {
            let def = by_id
                .get(stage_id)
                .expect("stage id in order must exist in definition map");
            for prereq in &def.prerequisites {
                if !by_id.contains_key(prereq) {
                    return Err(StagedOperationError::MissingPrerequisite {
                        prerequisite: prereq.clone(),
                        stage: stage_id.clone(),
                    });
                }
            }
        }

        ensure_acyclic(&by_id, &stage_order)?;

        let mut runtime = HashMap::new();
        for stage_id in &stage_order {
            runtime.insert(
                stage_id.clone(),
                StageRuntime {
                    status: StageStatus::Pending,
                    blocked_reason: None,
                    attempt_count: 0,
                    started_at: None,
                    completed_at: None,
                    owner_session_id: None,
                    failure_details: None,
                },
            );
        }

        let mut op = Self {
            operation_id,
            owner_session_id,
            definitions: by_id,
            runtime,
            stage_order,
            recovery_queue: VecDeque::new(),
        };
        op.refresh_blocked_and_ready();

        tracing::info!(operation_id = %op.operation_id, "staged operation created");
        Ok(op)
    }

    pub fn operation_id(&self) -> &str {
        &self.operation_id
    }

    pub fn legal_next_stages(&self) -> Vec<String> {
        self.stage_order
            .iter()
            .filter_map(|stage_id| {
                let rt = self.runtime.get(stage_id)?;
                match rt.status {
                    StageStatus::Ready => Some(stage_id.clone()),
                    _ => None,
                }
            })
            .collect()
    }

    pub fn start_stage(
        &mut self,
        stage_id: &str,
        owner_session_id: &str,
        started_at: String,
    ) -> Result<(), StagedOperationError> {
        self.refresh_blocked_and_ready();

        let rt = self
            .runtime
            .get_mut(stage_id)
            .ok_or_else(|| StagedOperationError::StageNotFound(stage_id.to_string()))?;

        if rt.status == StageStatus::Running {
            return Err(StagedOperationError::StageAlreadyRunning(
                stage_id.to_string(),
            ));
        }
        if rt.status != StageStatus::Ready {
            return Err(StagedOperationError::StageNotExecutable(
                rt.blocked_reason
                    .clone()
                    .unwrap_or_else(|| format!("stage '{}' is not ready", stage_id)),
            ));
        }

        rt.status = StageStatus::Running;
        rt.owner_session_id = Some(owner_session_id.to_string());
        rt.blocked_reason = None;
        rt.failure_details = None;
        rt.started_at = Some(started_at);
        rt.completed_at = None;
        rt.attempt_count = rt.attempt_count.saturating_add(1);

        tracing::info!(operation_id = %self.operation_id, stage_id, "stage started");
        Ok(())
    }

    pub fn complete_stage(
        &mut self,
        stage_id: &str,
        completed_at: String,
    ) -> Result<(), StagedOperationError> {
        let rt = self
            .runtime
            .get_mut(stage_id)
            .ok_or_else(|| StagedOperationError::StageNotFound(stage_id.to_string()))?;

        if rt.status != StageStatus::Running {
            return Err(StagedOperationError::InvalidTransition {
                stage: stage_id.to_string(),
                from: rt.status,
                to: StageStatus::Succeeded,
            });
        }

        rt.status = StageStatus::Succeeded;
        rt.completed_at = Some(completed_at);
        rt.blocked_reason = None;

        tracing::info!(operation_id = %self.operation_id, stage_id, "stage completed");
        self.refresh_blocked_and_ready();
        Ok(())
    }

    pub fn fail_stage(
        &mut self,
        stage_id: &str,
        completed_at: String,
        failure_details: String,
    ) -> Result<(), StagedOperationError> {
        let rt = self
            .runtime
            .get_mut(stage_id)
            .ok_or_else(|| StagedOperationError::StageNotFound(stage_id.to_string()))?;

        if rt.status != StageStatus::Running {
            return Err(StagedOperationError::InvalidTransition {
                stage: stage_id.to_string(),
                from: rt.status,
                to: StageStatus::Failed,
            });
        }

        rt.status = StageStatus::Failed;
        rt.completed_at = Some(completed_at);
        rt.failure_details = Some(failure_details);

        tracing::warn!(operation_id = %self.operation_id, stage_id, "stage failed");
        self.refresh_blocked_and_ready();
        Ok(())
    }

    pub fn create_checkpoint(
        &self,
        checkpoint_id: String,
        recorded_at: String,
    ) -> CheckpointSnapshot {
        let stages = self.snapshot();
        let completed_stages = stages
            .iter()
            .filter(|s| matches!(s.status, StageStatus::Succeeded | StageStatus::Compensated))
            .map(|s| s.stage_id.clone())
            .collect::<Vec<_>>();
        let remaining_stages = stages
            .iter()
            .filter(|s| {
                !matches!(
                    s.status,
                    StageStatus::Succeeded
                        | StageStatus::Compensated
                        | StageStatus::Cancelled
                        | StageStatus::ReviewRequired
                )
            })
            .map(|s| s.stage_id.clone())
            .collect::<Vec<_>>();
        let running_stage = stages
            .iter()
            .find(|s| s.status == StageStatus::Running)
            .map(|s| s.stage_id.clone());
        let recovery_actions_available = stages
            .iter()
            .filter(|s| {
                s.status == StageStatus::Succeeded
                    && s.execution_class == StageExecutionClass::Reversible
                    && s.compensation_action.is_some()
            })
            .map(|s| s.stage_id.clone())
            .collect::<Vec<_>>();
        let resume_safe = stages.iter().all(|s| {
            !matches!(
                s.status,
                StageStatus::ReviewRequired | StageStatus::Compensating
            )
        });

        tracing::info!(operation_id = %self.operation_id, checkpoint_id, "checkpoint recorded");
        CheckpointSnapshot {
            operation_id: self.operation_id.clone(),
            owner_session_id: self.owner_session_id.clone(),
            checkpoint_id,
            recorded_at,
            completed_stages,
            remaining_stages,
            running_stage,
            recovery_actions_available,
            resume_safe,
            stages,
        }
    }

    pub fn restore_checkpoint(
        &mut self,
        checkpoint: &CheckpointSnapshot,
    ) -> Result<(), StagedOperationError> {
        if checkpoint.operation_id != self.operation_id {
            return Err(StagedOperationError::CheckpointMismatch);
        }
        if checkpoint.stages.len() != self.stage_order.len() {
            return Err(StagedOperationError::CheckpointMismatch);
        }

        for snap in &checkpoint.stages {
            let def = self
                .definitions
                .get(&snap.stage_id)
                .ok_or(StagedOperationError::CheckpointMismatch)?;
            if def.prerequisites != snap.prerequisites
                || def.execution_class != snap.execution_class
            {
                return Err(StagedOperationError::CheckpointMismatch);
            }
        }

        for snap in &checkpoint.stages {
            let rt = self
                .runtime
                .get_mut(&snap.stage_id)
                .ok_or(StagedOperationError::CheckpointMismatch)?;
            rt.status = snap.status;
            rt.blocked_reason = snap.blocked_reason.clone();
            rt.attempt_count = snap.attempt_count;
            rt.started_at = snap.started_at.clone();
            rt.completed_at = snap.completed_at.clone();
            rt.owner_session_id = snap.owner_session_id.clone();
            rt.failure_details = snap.failure_details.clone();
        }

        tracing::info!(operation_id = %self.operation_id, checkpoint_id = %checkpoint.checkpoint_id, "operation resumed from checkpoint");
        self.refresh_blocked_and_ready();
        Ok(())
    }

    pub fn mark_stale_running_without_owner(
        &mut self,
        live_owners: &HashSet<String>,
        observed_at: String,
    ) {
        for stage_id in self.stage_order.clone() {
            let Some(rt) = self.runtime.get_mut(&stage_id) else {
                continue;
            };
            if rt.status != StageStatus::Running {
                continue;
            }
            let owner_alive = rt
                .owner_session_id
                .as_ref()
                .is_some_and(|owner| live_owners.contains(owner));
            if owner_alive {
                continue;
            }

            let execution_class = self
                .definitions
                .get(&stage_id)
                .map(|d| d.execution_class)
                .unwrap_or(StageExecutionClass::Reversible);
            rt.completed_at = Some(observed_at.clone());

            match execution_class {
                StageExecutionClass::Reversible => {
                    rt.status = StageStatus::Failed;
                    rt.failure_details = Some(
                        "running stage lost owner; safe retry requires re-execution".to_string(),
                    );
                }
                StageExecutionClass::Irreversible => {
                    rt.status = StageStatus::ReviewRequired;
                    rt.failure_details =
                        Some("running irreversible stage lost owner; outcome unknown".to_string());
                }
            }
        }

        self.refresh_blocked_and_ready();
    }

    pub fn prepare_recovery(&mut self, reason: &str) {
        let ordered = self.recovery_order_candidates();
        self.recovery_queue.clear();

        tracing::warn!(operation_id = %self.operation_id, reason, "recovery started");
        for stage_id in ordered {
            let Some(def) = self.definitions.get(&stage_id) else {
                continue;
            };
            let Some(rt) = self.runtime.get_mut(&stage_id) else {
                continue;
            };

            match def.execution_class {
                StageExecutionClass::Irreversible => {
                    rt.status = StageStatus::ReviewRequired;
                    rt.failure_details = Some(format!(
                        "manual review required: irreversible stage cannot be compensated ({reason})"
                    ));
                }
                StageExecutionClass::Reversible => {
                    if def.compensation_action.is_some() {
                        self.recovery_queue.push_back(stage_id.clone());
                    } else {
                        rt.status = StageStatus::ReviewRequired;
                        rt.failure_details = Some(format!(
                            "manual review required: no compensation action registered ({reason})"
                        ));
                    }
                }
            }
        }
    }

    pub fn start_next_compensation(
        &mut self,
        owner_session_id: &str,
        started_at: String,
    ) -> Option<String> {
        let stage_id = self.recovery_queue.pop_front()?;
        let rt = self.runtime.get_mut(&stage_id)?;
        if rt.status != StageStatus::Succeeded {
            return self.start_next_compensation(owner_session_id, started_at);
        }

        rt.status = StageStatus::Compensating;
        rt.owner_session_id = Some(owner_session_id.to_string());
        rt.started_at = Some(started_at);
        rt.attempt_count = rt.attempt_count.saturating_add(1);
        rt.blocked_reason = None;
        tracing::info!(operation_id = %self.operation_id, stage_id, "compensation started");
        Some(stage_id)
    }

    pub fn finish_compensation(
        &mut self,
        stage_id: &str,
        completed_at: String,
        success: bool,
        details: Option<String>,
    ) -> Result<(), StagedOperationError> {
        let rt = self
            .runtime
            .get_mut(stage_id)
            .ok_or_else(|| StagedOperationError::StageNotFound(stage_id.to_string()))?;

        if rt.status != StageStatus::Compensating {
            return Err(StagedOperationError::InvalidTransition {
                stage: stage_id.to_string(),
                from: rt.status,
                to: StageStatus::Compensated,
            });
        }

        rt.completed_at = Some(completed_at);
        if success {
            rt.status = StageStatus::Compensated;
            rt.failure_details = details;
            tracing::info!(operation_id = %self.operation_id, stage_id, "compensation completed");
        } else {
            rt.status = StageStatus::ReviewRequired;
            rt.failure_details = Some(details.unwrap_or_else(|| "compensation failed".to_string()));
            tracing::warn!(operation_id = %self.operation_id, stage_id, "compensation failed");
        }

        self.refresh_blocked_and_ready();
        Ok(())
    }

    pub fn dry_run_plan(&self) -> DryRunPlan {
        let ready_stages = self.legal_next_stages();
        let blocked_stages = self
            .stage_order
            .iter()
            .filter_map(|stage_id| {
                let rt = self.runtime.get(stage_id)?;
                (rt.status == StageStatus::Blocked).then(|| {
                    (
                        stage_id.clone(),
                        rt.blocked_reason.clone().unwrap_or_default(),
                    )
                })
            })
            .collect::<Vec<_>>();

        let reversible_stages = self
            .stage_order
            .iter()
            .filter(|stage_id| {
                self.definitions
                    .get(*stage_id)
                    .is_some_and(|d| d.execution_class == StageExecutionClass::Reversible)
            })
            .cloned()
            .collect::<Vec<_>>();
        let irreversible_stages = self
            .stage_order
            .iter()
            .filter(|stage_id| {
                self.definitions
                    .get(*stage_id)
                    .is_some_and(|d| d.execution_class == StageExecutionClass::Irreversible)
            })
            .cloned()
            .collect::<Vec<_>>();

        let checkpoint_candidates = self
            .stage_order
            .iter()
            .filter(|stage_id| {
                self.definitions
                    .get(*stage_id)
                    .is_some_and(|d| d.checkpoint_ref.is_some())
            })
            .cloned()
            .collect::<Vec<_>>();

        DryRunPlan {
            operation_id: self.operation_id.clone(),
            ordered_stages: self.topological_order(),
            ready_stages,
            blocked_stages,
            reversible_stages,
            irreversible_stages,
            checkpoint_candidates,
            recovery_sequence: self.recovery_order_candidates(),
        }
    }

    pub fn snapshot(&self) -> Vec<StageSnapshot> {
        self.stage_order
            .iter()
            .filter_map(|stage_id| {
                let def = self.definitions.get(stage_id)?;
                let rt = self.runtime.get(stage_id)?;
                Some(StageSnapshot {
                    operation_id: self.operation_id.clone(),
                    stage_id: stage_id.clone(),
                    status: rt.status,
                    prerequisites: def.prerequisites.clone(),
                    blocked_reason: rt.blocked_reason.clone(),
                    attempt_count: rt.attempt_count,
                    started_at: rt.started_at.clone(),
                    completed_at: rt.completed_at.clone(),
                    execution_class: def.execution_class,
                    checkpoint_ref: def.checkpoint_ref.clone(),
                    compensation_action: def.compensation_action.clone(),
                    failure_details: rt.failure_details.clone(),
                    owner_session_id: rt.owner_session_id.clone(),
                })
            })
            .collect()
    }

    fn refresh_blocked_and_ready(&mut self) {
        let topo = self.topological_order();
        for stage_id in topo {
            let Some(def) = self.definitions.get(&stage_id) else {
                continue;
            };
            let Some(current_status) = self.runtime.get(&stage_id).map(|rt| rt.status) else {
                continue;
            };

            if matches!(
                current_status,
                StageStatus::Running
                    | StageStatus::Succeeded
                    | StageStatus::Failed
                    | StageStatus::Compensating
                    | StageStatus::Compensated
                    | StageStatus::Cancelled
                    | StageStatus::ReviewRequired
            ) {
                continue;
            }
            let mut failed_prereq = None;
            let mut pending_prereq = None;

            for prereq in &def.prerequisites {
                let Some(prereq_rt) = self.runtime.get(prereq) else {
                    continue;
                };
                match prereq_rt.status {
                    StageStatus::Succeeded => {}
                    StageStatus::Failed
                    | StageStatus::Cancelled
                    | StageStatus::ReviewRequired
                    | StageStatus::Compensating
                    | StageStatus::Compensated => {
                        failed_prereq = Some(prereq.clone());
                        break;
                    }
                    _ => {
                        pending_prereq = Some(prereq.clone());
                    }
                }
            }

            let Some(rt) = self.runtime.get_mut(&stage_id) else {
                continue;
            };

            if let Some(prereq) = failed_prereq {
                rt.status = StageStatus::Blocked;
                rt.blocked_reason = Some(format!(
                    "prerequisite '{}' did not succeed; stage cannot run",
                    prereq
                ));
            } else if let Some(prereq) = pending_prereq {
                rt.status = StageStatus::Blocked;
                rt.blocked_reason =
                    Some(format!("waiting for prerequisite '{}' to complete", prereq));
            } else {
                rt.status = StageStatus::Ready;
                rt.blocked_reason = None;
            }
        }
    }

    fn topological_order(&self) -> Vec<String> {
        let mut indegree = HashMap::<String, usize>::new();
        let mut outgoing = HashMap::<String, Vec<String>>::new();

        for stage_id in &self.stage_order {
            indegree.entry(stage_id.clone()).or_insert(0);
            outgoing.entry(stage_id.clone()).or_default();
        }
        for stage_id in &self.stage_order {
            if let Some(def) = self.definitions.get(stage_id) {
                for prereq in &def.prerequisites {
                    *indegree.entry(stage_id.clone()).or_insert(0) += 1;
                    outgoing
                        .entry(prereq.clone())
                        .or_default()
                        .push(stage_id.clone());
                }
            }
        }

        let mut queue = VecDeque::new();
        for stage_id in &self.stage_order {
            if indegree.get(stage_id).copied().unwrap_or_default() == 0 {
                queue.push_back(stage_id.clone());
            }
        }

        let mut ordered = Vec::new();
        while let Some(stage_id) = queue.pop_front() {
            ordered.push(stage_id.clone());
            if let Some(children) = outgoing.get(&stage_id) {
                for child in children {
                    if let Some(entry) = indegree.get_mut(child) {
                        *entry = entry.saturating_sub(1);
                        if *entry == 0 {
                            queue.push_back(child.clone());
                        }
                    }
                }
            }
        }

        if ordered.len() != self.stage_order.len() {
            return self.stage_order.clone();
        }
        ordered
    }

    fn recovery_order_candidates(&self) -> Vec<String> {
        let mut candidates = self
            .topological_order()
            .into_iter()
            .filter(|stage_id| {
                self.runtime
                    .get(stage_id)
                    .is_some_and(|rt| rt.status == StageStatus::Succeeded)
            })
            .collect::<Vec<_>>();
        candidates.reverse();
        candidates
    }
}

fn ensure_acyclic(
    definitions: &HashMap<String, StageDefinition>,
    stage_order: &[String],
) -> Result<(), StagedOperationError> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Mark {
        Temporary,
        Permanent,
    }

    fn visit(
        stage: &str,
        marks: &mut HashMap<String, Mark>,
        defs: &HashMap<String, StageDefinition>,
    ) -> Result<(), StagedOperationError> {
        match marks.get(stage).copied() {
            Some(Mark::Permanent) => return Ok(()),
            Some(Mark::Temporary) => return Err(StagedOperationError::DependencyCycle),
            None => {}
        }

        marks.insert(stage.to_string(), Mark::Temporary);
        if let Some(def) = defs.get(stage) {
            for prereq in &def.prerequisites {
                visit(prereq, marks, defs)?;
            }
        }
        marks.insert(stage.to_string(), Mark::Permanent);
        Ok(())
    }

    let mut marks = HashMap::new();
    for stage in stage_order {
        visit(stage, &mut marks, definitions)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stage(id: &str, prerequisites: &[&str], class: StageExecutionClass) -> StageDefinition {
        StageDefinition {
            stage_id: id.to_string(),
            prerequisites: prerequisites.iter().map(|v| v.to_string()).collect(),
            execution_class: class,
            checkpoint_ref: None,
            compensation_action: if class == StageExecutionClass::Reversible {
                Some(format!("undo_{id}"))
            } else {
                None
            },
        }
    }

    fn op(defs: Vec<StageDefinition>) -> StagedOperation {
        StagedOperation::new("op-1".to_string(), "sess-1".to_string(), defs).expect("valid op")
    }

    #[test]
    fn linear_three_stage_operation_completes_in_order() {
        let mut op = op(vec![
            stage("a", &[], StageExecutionClass::Reversible),
            stage("b", &["a"], StageExecutionClass::Reversible),
            stage("c", &["b"], StageExecutionClass::Reversible),
        ]);
        assert_eq!(op.legal_next_stages(), vec!["a".to_string()]);
        op.start_stage("a", "sess-1", "t1".to_string()).unwrap();
        op.complete_stage("a", "t2".to_string()).unwrap();
        assert_eq!(op.legal_next_stages(), vec!["b".to_string()]);
        op.start_stage("b", "sess-1", "t3".to_string()).unwrap();
        op.complete_stage("b", "t4".to_string()).unwrap();
        assert_eq!(op.legal_next_stages(), vec!["c".to_string()]);
        op.start_stage("c", "sess-1", "t5".to_string()).unwrap();
        op.complete_stage("c", "t6".to_string()).unwrap();
        assert!(op.legal_next_stages().is_empty());
    }

    #[test]
    fn downstream_stage_cannot_start_before_prerequisite() {
        let mut op = op(vec![
            stage("a", &[], StageExecutionClass::Reversible),
            stage("b", &["a"], StageExecutionClass::Reversible),
        ]);
        let err = op.start_stage("b", "sess-1", "t1".to_string()).unwrap_err();
        assert!(matches!(err, StagedOperationError::StageNotExecutable(_)));
    }

    #[test]
    fn independent_stages_can_proceed_without_blocking() {
        let op = op(vec![
            stage("a", &[], StageExecutionClass::Reversible),
            stage("b", &[], StageExecutionClass::Reversible),
        ]);
        assert_eq!(
            op.legal_next_stages(),
            vec!["a".to_string(), "b".to_string()]
        );
    }

    #[test]
    fn duplicate_execution_of_same_stage_is_prevented() {
        let mut op = op(vec![stage("a", &[], StageExecutionClass::Reversible)]);
        op.start_stage("a", "sess-1", "t1".to_string()).unwrap();
        let err = op.start_stage("a", "sess-1", "t2".to_string()).unwrap_err();
        assert!(matches!(err, StagedOperationError::StageAlreadyRunning(_)));
    }

    #[test]
    fn failed_prerequisite_blocks_dependent_stages() {
        let mut op = op(vec![
            stage("a", &[], StageExecutionClass::Reversible),
            stage("b", &["a"], StageExecutionClass::Reversible),
        ]);
        op.start_stage("a", "sess-1", "t1".to_string()).unwrap();
        op.fail_stage("a", "t2".to_string(), "boom".to_string())
            .unwrap();
        let b = op
            .snapshot()
            .into_iter()
            .find(|s| s.stage_id == "b")
            .unwrap();
        assert_eq!(b.status, StageStatus::Blocked);
    }

    #[test]
    fn cancellation_compensates_reversible_stages_in_reverse_order() {
        let mut op = op(vec![
            stage("a", &[], StageExecutionClass::Reversible),
            stage("b", &["a"], StageExecutionClass::Reversible),
        ]);
        op.start_stage("a", "sess-1", "t1".to_string()).unwrap();
        op.complete_stage("a", "t2".to_string()).unwrap();
        op.start_stage("b", "sess-1", "t3".to_string()).unwrap();
        op.complete_stage("b", "t4".to_string()).unwrap();

        op.prepare_recovery("cancelled");
        let first = op
            .start_next_compensation("sess-1", "t5".to_string())
            .unwrap();
        let second = op
            .start_next_compensation("sess-1", "t6".to_string())
            .unwrap();
        assert_eq!(first, "b");
        assert_eq!(second, "a");
    }

    #[test]
    fn irreversible_completed_stage_is_never_automatically_compensated() {
        let mut op = op(vec![stage("a", &[], StageExecutionClass::Irreversible)]);
        op.start_stage("a", "sess-1", "t1".to_string()).unwrap();
        op.complete_stage("a", "t2".to_string()).unwrap();
        op.prepare_recovery("cancelled");
        assert!(
            op.start_next_compensation("sess-1", "t3".to_string())
                .is_none()
        );
        let a = op
            .snapshot()
            .into_iter()
            .find(|s| s.stage_id == "a")
            .unwrap();
        assert_eq!(a.status, StageStatus::ReviewRequired);
    }

    #[test]
    fn compensation_failure_is_preserved_and_reported() {
        let mut op = op(vec![stage("a", &[], StageExecutionClass::Reversible)]);
        op.start_stage("a", "sess-1", "t1".to_string()).unwrap();
        op.complete_stage("a", "t2".to_string()).unwrap();
        op.prepare_recovery("cancelled");
        let stage_id = op
            .start_next_compensation("sess-1", "t3".to_string())
            .unwrap();
        op.finish_compensation(
            &stage_id,
            "t4".to_string(),
            false,
            Some("undo failed".to_string()),
        )
        .unwrap();
        let snap = op
            .snapshot()
            .into_iter()
            .find(|s| s.stage_id == "a")
            .unwrap();
        assert_eq!(snap.status, StageStatus::ReviewRequired);
        assert_eq!(snap.failure_details.as_deref(), Some("undo failed"));
    }

    #[test]
    fn checkpoint_restores_completed_stage_state() {
        let mut operation = op(vec![stage("a", &[], StageExecutionClass::Reversible)]);
        operation
            .start_stage("a", "sess-1", "t1".to_string())
            .unwrap();
        operation.complete_stage("a", "t2".to_string()).unwrap();

        let cp = operation.create_checkpoint("cp-1".to_string(), "t3".to_string());

        let mut restored = op(vec![stage("a", &[], StageExecutionClass::Reversible)]);
        restored.restore_checkpoint(&cp).unwrap();
        let snap = restored
            .snapshot()
            .into_iter()
            .find(|s| s.stage_id == "a")
            .unwrap();
        assert_eq!(snap.status, StageStatus::Succeeded);
    }

    #[test]
    fn successful_stages_are_not_repeated_after_resumption() {
        let mut operation = op(vec![
            stage("a", &[], StageExecutionClass::Reversible),
            stage("b", &["a"], StageExecutionClass::Reversible),
        ]);
        operation
            .start_stage("a", "sess-1", "t1".to_string())
            .unwrap();
        operation.complete_stage("a", "t2".to_string()).unwrap();
        let cp = operation.create_checkpoint("cp-1".to_string(), "t3".to_string());

        let mut restored = op(vec![
            stage("a", &[], StageExecutionClass::Reversible),
            stage("b", &["a"], StageExecutionClass::Reversible),
        ]);
        restored.restore_checkpoint(&cp).unwrap();
        assert_eq!(restored.legal_next_stages(), vec!["b".to_string()]);
    }

    #[test]
    fn stale_running_stage_is_detected_after_owner_loss() {
        let mut op = op(vec![stage("a", &[], StageExecutionClass::Reversible)]);
        op.start_stage("a", "owner-1", "t1".to_string()).unwrap();
        op.mark_stale_running_without_owner(&HashSet::new(), "t2".to_string());
        let snap = op
            .snapshot()
            .into_iter()
            .find(|s| s.stage_id == "a")
            .unwrap();
        assert_eq!(snap.status, StageStatus::Failed);
    }

    #[test]
    fn interrupted_irreversible_stage_requires_review() {
        let mut op = op(vec![stage("a", &[], StageExecutionClass::Irreversible)]);
        op.start_stage("a", "owner-1", "t1".to_string()).unwrap();
        op.mark_stale_running_without_owner(&HashSet::new(), "t2".to_string());
        let snap = op
            .snapshot()
            .into_iter()
            .find(|s| s.stage_id == "a")
            .unwrap();
        assert_eq!(snap.status, StageStatus::ReviewRequired);
    }

    #[test]
    fn dry_run_reports_plan_without_execution() {
        let op = op(vec![
            stage("a", &[], StageExecutionClass::Reversible),
            stage("b", &["a"], StageExecutionClass::Irreversible),
        ]);
        let plan = op.dry_run_plan();
        assert_eq!(plan.ordered_stages, vec!["a".to_string(), "b".to_string()]);
        let a = op
            .snapshot()
            .into_iter()
            .find(|s| s.stage_id == "a")
            .unwrap();
        assert_eq!(a.status, StageStatus::Ready);
    }

    #[test]
    fn disabled_non_staged_mode_preserves_current_behavior() {
        // This validates opt-in behavior by exercising workflow tracker untouched.
        let mut tracker = super::super::tracker::WorkflowTracker::default();
        let state = tracker.start_run(
            "run-1".to_string(),
            "name".to_string(),
            "objective".to_string(),
            vec![],
            Some(4),
            None,
        );
        assert_eq!(
            state.status,
            super::super::tracker::WorkflowRunStatus::Active
        );
    }

    #[test]
    fn dependency_cycle_is_detected_and_rejected() {
        let err = StagedOperation::new(
            "op-1".to_string(),
            "sess-1".to_string(),
            vec![
                stage("a", &["b"], StageExecutionClass::Reversible),
                stage("b", &["a"], StageExecutionClass::Reversible),
            ],
        )
        .unwrap_err();
        assert_eq!(err, StagedOperationError::DependencyCycle);
    }

    #[test]
    fn missing_prerequisite_reference_is_rejected() {
        let err = StagedOperation::new(
            "op-1".to_string(),
            "sess-1".to_string(),
            vec![stage("a", &["missing"], StageExecutionClass::Reversible)],
        )
        .unwrap_err();
        assert!(matches!(
            err,
            StagedOperationError::MissingPrerequisite { .. }
        ));
    }

    #[test]
    fn operation_state_remains_deterministic_across_repeated_reads() {
        let op = op(vec![
            stage("a", &[], StageExecutionClass::Reversible),
            stage("b", &["a"], StageExecutionClass::Reversible),
            stage("c", &["a"], StageExecutionClass::Reversible),
        ]);
        let first = op.snapshot();
        let second = op.snapshot();
        assert_eq!(first, second);
    }
}
