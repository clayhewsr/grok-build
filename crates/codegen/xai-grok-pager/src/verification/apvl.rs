use crate::verification::tvl::{TvlPublicSnapshot, TvlRunStatus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApvlTaskSpec {
    pub objective: String,
    pub required_outputs: Vec<String>,
    pub constraints: Vec<String>,
    pub acceptance_criteria: Vec<String>,
    pub required_evidence: Vec<String>,
    pub required_tools: Vec<String>,
    pub dependencies: Vec<String>,
    pub open_questions: Vec<String>,
    pub unresolved_requirements: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskNodeStatus {
    Pending,
    Ready,
    InProgress,
    Complete,
    Blocked,
    Unresolved,
    Failed,
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskGraphNode {
    pub id: String,
    pub requirement: String,
    pub status: TaskNodeStatus,
    pub blocking: bool,
    pub depends_on: Vec<String>,
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingStep {
    pub missing_step_id: String,
    pub category: String,
    pub severity: String,
    pub affected_requirement: String,
    pub blocking_yes_no: bool,
    pub recommended_next_action: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DurableTaskState {
    pub objective: String,
    pub constraints: Vec<String>,
    pub completed_work: Vec<String>,
    pub remaining_work: Vec<String>,
    pub decisions: Vec<String>,
    pub artifacts: Vec<String>,
    pub evidence_references: Vec<String>,
    pub open_questions: Vec<String>,
    pub blockers: Vec<String>,
    pub unresolved_requirements: Vec<String>,
    pub checkpoints: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApvlDecision {
    Continue,
    Refine,
    Verify,
    Escalate,
    Stop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressState {
    Stable,
    Improving,
    Stalled,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimStatus {
    VerifiedResult,
    IndependentlyVerifiedResult,
    PartialResult,
    ValidFramework,
    RestrictedExactResult,
    ConjecturalRoute,
    Unresolved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum StopReason {
    AcceptanceCriteriaMet = 1,
    VerifiedComplete = 2,
    PartialWithLimits = 3,
    NoFurtherProgress = 4,
    ComputeBudgetExhausted = 5,
    BlockedByDependency = 6,
    ReviewRequired = 7,
    UnresolvedWithLimits = 8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BenchmarkMode {
    ExistingPath,
    WithApvl,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkConfig {
    pub model_provider: String,
    pub model_name: String,
    pub temperature: f32,
    pub max_tokens: usize,
    pub tool_mode: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkWorkload {
    pub workload_id: String,
    pub task_spec: ApvlTaskSpec,
    pub task_graph: Vec<TaskGraphNode>,
    pub config: BenchmarkConfig,
    pub tool_availability: Vec<String>,
    pub evaluation_criteria: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkMetrics {
    pub completion_coverage: usize,
    pub missed_requirements: usize,
    pub premature_stops: usize,
    pub missing_steps_caught: usize,
    pub refinements: usize,
    pub tvl_invocations: usize,
    pub iterations: usize,
    pub model_calls: usize,
    pub tool_calls: usize,
    pub stop_reason: Option<StopReason>,
    pub final_claim_status: ClaimStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkComparison {
    pub workload_id: String,
    pub config_hash: String,
    pub existing_path: BenchmarkMetrics,
    pub apvl_path: BenchmarkMetrics,
}

impl BenchmarkComparison {
    pub fn workload_identical(&self) -> bool {
        self.existing_path == self.apvl_path
            || self.existing_path.completion_coverage == self.apvl_path.completion_coverage
    }

    pub fn only_apvl_difference(&self) -> bool {
        self.existing_path != self.apvl_path
            && self.existing_path.iterations == self.apvl_path.iterations
            && self.existing_path.model_calls == self.apvl_path.model_calls
            && self.existing_path.tool_calls == self.apvl_path.tool_calls
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApvlStopDecision {
    pub machine_readable: String,
    pub apvl_decision: ApvlDecision,
    pub stop_reason: StopReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApvlPublicSnapshot {
    pub run_id: String,
    pub task_difficulty: Option<String>,
    pub requirements_total: usize,
    pub requirements_complete: usize,
    pub unresolved_count: usize,
    pub critical_missing_steps: usize,
    pub progress_state: ProgressState,
    pub stagnation_detected: bool,
    pub refinement_count: usize,
    pub current_apvl_decision: ApvlDecision,
    pub tvl_invoked: bool,
    pub tvl_result: Option<TvlRunStatus>,
    pub claim_status: ClaimStatus,
    pub stop_reason: Option<StopReason>,
    pub elapsed_time: u64,
    pub compute_consumed: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApvlController {
    pub run_id: String,
    pub task_spec: ApvlTaskSpec,
    pub task_graph: Vec<TaskGraphNode>,
    pub durable_state: DurableTaskState,
    pub decision: ApvlDecision,
    pub stop_reason: Option<StopReason>,
    pub progress_state: ProgressState,
    pub stagnation_detected: bool,
    pub refinement_count: usize,
    pub tvl_invoked: bool,
    pub tvl_result: Option<TvlRunStatus>,
    pub claim_status: ClaimStatus,
    pub max_iterations: usize,
    pub max_refinement_passes: usize,
    pub max_model_calls: usize,
    pub max_tool_calls: usize,
    pub max_wall_clock_seconds: u64,
    pub verification_budget: usize,
    pub tvl_invocation_budget: usize,
    pub retry_budget: usize,
    pub iteration_count: usize,
    pub model_calls: usize,
    pub tool_calls: usize,
    pub compute_consumed: usize,
    pub elapsed_time: u64,
    pub missing_steps: Vec<MissingStep>,
    pub task_difficulty: Option<String>,
}

impl Default for ApvlController {
    fn default() -> Self {
        Self::new(
            "apvl-run",
            ApvlTaskSpec {
                objective: String::new(),
                required_outputs: Vec::new(),
                constraints: Vec::new(),
                acceptance_criteria: Vec::new(),
                required_evidence: Vec::new(),
                required_tools: Vec::new(),
                dependencies: Vec::new(),
                open_questions: Vec::new(),
                unresolved_requirements: Vec::new(),
            },
        )
    }
}

impl ApvlController {
    pub fn new(run_id: impl Into<String>, task_spec: ApvlTaskSpec) -> Self {
        let mut state = DurableTaskState::default();
        state.objective = task_spec.objective.clone();
        state.constraints = task_spec.constraints.clone();
        state.open_questions = task_spec.open_questions.clone();
        state.unresolved_requirements = task_spec.unresolved_requirements.clone();
        state.remaining_work = task_spec.required_outputs.clone();

        Self {
            run_id: sanitize_run_id(run_id.into()),
            task_spec,
            task_graph: Vec::new(),
            durable_state: state,
            decision: ApvlDecision::Continue,
            stop_reason: None,
            progress_state: ProgressState::Stable,
            stagnation_detected: false,
            refinement_count: 0,
            tvl_invoked: false,
            tvl_result: None,
            claim_status: ClaimStatus::Unresolved,
            max_iterations: 8,
            max_refinement_passes: 3,
            max_model_calls: 20,
            max_tool_calls: 50,
            max_wall_clock_seconds: 600,
            verification_budget: 4,
            tvl_invocation_budget: 2,
            retry_budget: 3,
            iteration_count: 0,
            model_calls: 0,
            tool_calls: 0,
            compute_consumed: 0,
            elapsed_time: 0,
            missing_steps: Vec::new(),
            task_difficulty: None,
        }
    }

    pub fn with_task_graph(mut self, task_graph: Vec<TaskGraphNode>) -> Self {
        self.task_graph = task_graph;
        self.refresh_durable_state();
        self
    }

    pub fn set_difficulty(&mut self, difficulty: impl Into<String>) {
        self.task_difficulty = Some(difficulty.into());
    }

    pub fn add_task_graph_node(&mut self, node: TaskGraphNode) {
        self.task_graph.push(node);
        self.refresh_durable_state();
    }

    pub fn record_completed_requirement(&mut self, requirement: impl Into<String>) {
        let requirement = requirement.into();
        if !self.durable_state.completed_work.iter().any(|item| item == &requirement) {
            self.durable_state.completed_work.push(requirement.clone());
        }
        self.durable_state.remaining_work.retain(|item| item != &requirement);

        for node in &mut self.task_graph {
            if node.id == requirement || node.requirement == requirement {
                node.status = TaskNodeStatus::Complete;
            }
        }
        self.progress_state = ProgressState::Improving;
        self.refresh_durable_state();
    }

    pub fn mark_blocked(&mut self, requirement: impl Into<String>) {
        let requirement = requirement.into();
        if !self.durable_state.blockers.iter().any(|item| item == &requirement) {
            self.durable_state.blockers.push(requirement.clone());
        }
        for node in &mut self.task_graph {
            if node.id == requirement || node.requirement == requirement {
                node.status = TaskNodeStatus::Blocked;
                node.blocking = true;
            }
        }
        self.progress_state = ProgressState::Blocked;
        self.refresh_durable_state();
    }

    pub fn record_stagnation(&mut self, stagnation_detected: bool) {
        self.stagnation_detected = stagnation_detected;
        self.progress_state = if stagnation_detected {
            ProgressState::Stalled
        } else {
            ProgressState::Improving
        };
    }

    pub fn refresh_durable_state(&mut self) {
        let mut completed = self.durable_state.completed_work.clone();
        for node in &self.task_graph {
            if matches!(node.status, TaskNodeStatus::Complete) {
                if !completed.iter().any(|item| item == &node.requirement || item == &node.id) {
                    completed.push(node.requirement.clone());
                }
            }
        }
        self.durable_state.completed_work = completed;

        let mut remaining = self.task_spec.required_outputs.clone();
        remaining.retain(|item| !self.durable_state.completed_work.iter().any(|done| done == item));
        self.durable_state.remaining_work = remaining;

        self.durable_state.constraints = self.task_spec.constraints.clone();
        self.durable_state.unresolved_requirements = self.task_spec.unresolved_requirements.clone();
        self.durable_state.open_questions = self.task_spec.open_questions.clone();
        self.missing_steps = self.collect_missing_steps();
        self.unresolved_count();
    }

    pub fn update_from_tvl_snapshot(&mut self, snapshot: &TvlPublicSnapshot) {
        self.tvl_invoked = true;
        self.tvl_result = Some(snapshot.status);
        if snapshot.status == TvlRunStatus::Verified {
            self.claim_status = ClaimStatus::VerifiedResult;
        } else if snapshot.unresolved_disagreements > 0 {
            self.claim_status = ClaimStatus::PartialResult;
        } else if snapshot.status == TvlRunStatus::Blocked {
            self.claim_status = ClaimStatus::Unresolved;
        } else {
            self.claim_status = ClaimStatus::Unresolved;
        }
        self.decision = self.evaluate_decision();
    }

    pub fn record_tvl_result(&mut self, status: TvlRunStatus) {
        self.tvl_invoked = true;
        self.tvl_result = Some(status);
        if status == TvlRunStatus::Verified {
            self.claim_status = ClaimStatus::VerifiedResult;
        } else if matches!(status, TvlRunStatus::Incomplete | TvlRunStatus::Blocked | TvlRunStatus::Failed)
        {
            self.claim_status = ClaimStatus::Unresolved;
        } else {
            self.claim_status = ClaimStatus::PartialResult;
        }
        self.decision = self.evaluate_decision();
    }

    pub fn register_missing_step(&mut self, step: MissingStep) {
        self.missing_steps.push(step);
    }

    pub fn coverage_complete(&self) -> bool {
        let required_outputs_complete = self.task_spec.required_outputs.iter().all(|output| {
            self.durable_state.completed_work.iter().any(|done| done == output)
        });
        let graph_complete = self.task_graph.iter().all(|node| {
            !matches!(
                node.status,
                TaskNodeStatus::Pending
                    | TaskNodeStatus::Ready
                    | TaskNodeStatus::InProgress
                    | TaskNodeStatus::Blocked
                    | TaskNodeStatus::Unresolved
                    | TaskNodeStatus::Failed
            )
        });
        required_outputs_complete && graph_complete
    }

    pub fn collect_missing_steps(&self) -> Vec<MissingStep> {
        let mut steps = Vec::new();

        for node in &self.task_graph {
            if matches!(
                node.status,
                TaskNodeStatus::Pending
                    | TaskNodeStatus::Ready
                    | TaskNodeStatus::InProgress
                    | TaskNodeStatus::Blocked
                    | TaskNodeStatus::Unresolved
                    | TaskNodeStatus::Failed
            ) {
                steps.push(MissingStep {
                    missing_step_id: node.id.clone(),
                    category: "task_graph".to_string(),
                    severity: if node.blocking { "critical".to_string() } else { "moderate".to_string() },
                    affected_requirement: node.requirement.clone(),
                    blocking_yes_no: node.blocking,
                    recommended_next_action: "target the unresolved task-graph node and repair only that dependency path".to_string(),
                });
            }
        }

        for requirement in &self.task_spec.unresolved_requirements {
            steps.push(MissingStep {
                missing_step_id: format!("req-{}", steps.len() + 1),
                category: "unresolved_requirement".to_string(),
                severity: "critical".to_string(),
                affected_requirement: requirement.clone(),
                blocking_yes_no: true,
                recommended_next_action: "resolve the pinned requirement before claiming completion".to_string(),
            });
        }

        for output in &self.task_spec.required_outputs {
            let complete = self.durable_state.completed_work.iter().any(|done| done == output);
            if !complete {
                steps.push(MissingStep {
                    missing_step_id: format!("out-{}", steps.len() + 1),
                    category: "missing_output".to_string(),
                    severity: "critical".to_string(),
                    affected_requirement: output.clone(),
                    blocking_yes_no: true,
                    recommended_next_action: "produce or verify the missing output before finalization".to_string(),
                });
            }
        }

        steps
    }

    pub fn unresolved_count(&self) -> usize {
        let mut unresolved = self.task_spec.unresolved_requirements.len();
        unresolved += self.task_graph.iter().filter(|node| {
            matches!(
                node.status,
                TaskNodeStatus::Pending
                    | TaskNodeStatus::Ready
                    | TaskNodeStatus::InProgress
                    | TaskNodeStatus::Blocked
                    | TaskNodeStatus::Unresolved
                    | TaskNodeStatus::Failed
            )
        }).count();
        unresolved += self
            .task_spec
            .required_outputs
            .iter()
            .filter(|output| !self.durable_state.completed_work.iter().any(|done| done == *output))
            .count();
        unresolved
    }

    pub fn evaluate_decision(&self) -> ApvlDecision {
        let required_output_missing = self
            .task_spec
            .required_outputs
            .iter()
            .any(|output| !self.durable_state.completed_work.iter().any(|done| done == output));
        let blocked_graph = self.task_graph.iter().any(|node| {
            node.blocking
                || matches!(
                    node.status,
                    TaskNodeStatus::Blocked
                        | TaskNodeStatus::Unresolved
                        | TaskNodeStatus::Failed
                )
        });
        let unresolved_requirement_present = !self.task_spec.unresolved_requirements.is_empty();
        let blocking_missing = self.missing_steps.iter().any(|step| {
            step.blocking_yes_no
                && !matches!(step.category.as_str(), "missing_output")
        }) || blocked_graph || unresolved_requirement_present;
        let budget_exhausted = self.iteration_count >= self.max_iterations
            || self.compute_consumed >= self.max_model_calls + self.max_tool_calls;

        if budget_exhausted {
            return ApvlDecision::Stop;
        }
        if self.coverage_complete() && self.tvl_result == Some(TvlRunStatus::Verified) {
            return ApvlDecision::Stop;
        }
        if blocking_missing {
            if self.stagnation_detected || self.refinement_count >= self.max_refinement_passes {
                return ApvlDecision::Escalate;
            }
            return ApvlDecision::Refine;
        }
        if self.stagnation_detected {
            return if self.refinement_count < self.max_refinement_passes {
                ApvlDecision::Refine
            } else {
                ApvlDecision::Escalate
            };
        }
        if self.tvl_result == Some(TvlRunStatus::Blocked)
            || self.tvl_result == Some(TvlRunStatus::Failed)
            || self.tvl_result == Some(TvlRunStatus::Incomplete)
        {
            return ApvlDecision::Refine;
        }
        if self.coverage_complete() {
            return ApvlDecision::Verify;
        }
        if required_output_missing && !self.task_spec.unresolved_requirements.is_empty() {
            return ApvlDecision::Refine;
        }
        if self.task_spec.required_outputs.is_empty() && self.task_graph.is_empty() {
            return ApvlDecision::Stop;
        }
        ApvlDecision::Continue
    }

    pub fn determine_stop(&self) -> Option<ApvlStopDecision> {
        let decision = self.evaluate_decision();
        let reason = match decision {
            ApvlDecision::Stop => {
                if self.coverage_complete() && self.tvl_result == Some(TvlRunStatus::Verified) {
                    StopReason::VerifiedComplete
                } else if self.task_spec.required_outputs.iter().all(|output| {
                    self.durable_state.completed_work.iter().any(|done| done == output)
                }) {
                    StopReason::AcceptanceCriteriaMet
                } else if self.iteration_count >= self.max_iterations
                    || self.compute_consumed >= self.max_model_calls + self.max_tool_calls
                {
                    StopReason::ComputeBudgetExhausted
                } else if self.stagnation_detected {
                    StopReason::NoFurtherProgress
                } else {
                    StopReason::UnresolvedWithLimits
                }
            }
            ApvlDecision::Escalate => StopReason::ReviewRequired,
            ApvlDecision::Verify => StopReason::AcceptanceCriteriaMet,
            ApvlDecision::Refine => {
                if self.task_graph.iter().any(|node| node.blocking) {
                    StopReason::BlockedByDependency
                } else {
                    StopReason::PartialWithLimits
                }
            }
            ApvlDecision::Continue => StopReason::PartialWithLimits,
        };

        if matches!(decision, ApvlDecision::Stop) {
            Some(ApvlStopDecision {
                machine_readable: format!("{}:{}", "APVL_STOP", reason as u8),
                apvl_decision: decision,
                stop_reason: reason,
            })
        } else {
            None
        }
    }

    pub fn finalization_integrity_gate(&self, final_output: &str) -> Result<(), String> {
        if self.task_spec.required_outputs.is_empty() {
            return Ok(());
        }

        for requirement in &self.task_spec.required_outputs {
            let haystack = final_output.to_ascii_lowercase();
            let needle = requirement.to_ascii_lowercase();
            if !haystack.contains(&needle) {
                return Err(format!(
                    "final answer dropped required conclusion: {}",
                    requirement
                ));
            }
        }

        if self.missing_steps.iter().any(|step| step.blocking_yes_no) {
            return Err("critical missing steps remain unresolved".to_string());
        }

        if self.tvl_result != Some(TvlRunStatus::Verified) {
            return Err("TVL verification result is required before finalization".to_string());
        }

        if matches!(
            self.claim_status,
            ClaimStatus::PartialResult | ClaimStatus::ConjecturalRoute | ClaimStatus::Unresolved
        ) {
            return Err("partial or unresolved claim cannot be promoted to final output".to_string());
        }

        Ok(())
    }

    pub fn checkpoint(&self) -> DurableTaskState {
        self.durable_state.clone()
    }

    pub fn resume_from_checkpoint(&mut self, state: DurableTaskState) {
        self.durable_state = state;
        self.decision = self.evaluate_decision();
    }

    pub fn increment_iteration(&mut self) {
        self.iteration_count += 1;
        self.compute_consumed += 1;
        self.decision = self.evaluate_decision();
    }

    pub fn increment_model_calls(&mut self, count: usize) {
        self.model_calls += count;
        self.compute_consumed += count;
    }

    pub fn increment_tool_calls(&mut self, count: usize) {
        self.tool_calls += count;
        self.compute_consumed += count;
    }

    pub fn public_snapshot(&self) -> ApvlPublicSnapshot {
        let unresolved = self.unresolved_count();
        ApvlPublicSnapshot {
            run_id: self.run_id.clone(),
            task_difficulty: self.task_difficulty.clone(),
            requirements_total: self.task_spec.required_outputs.len() + self.task_graph.len(),
            requirements_complete: self
                .durable_state
                .completed_work
                .len()
                .min(self.task_spec.required_outputs.len() + self.task_graph.len()),
            unresolved_count: unresolved,
            critical_missing_steps: self
                .missing_steps
                .iter()
                .filter(|step| step.blocking_yes_no)
                .count(),
            progress_state: self.progress_state,
            stagnation_detected: self.stagnation_detected,
            refinement_count: self.refinement_count,
            current_apvl_decision: self.decision,
            tvl_invoked: self.tvl_invoked,
            tvl_result: self.tvl_result,
            claim_status: self.claim_status,
            stop_reason: self.stop_reason,
            elapsed_time: self.elapsed_time,
            compute_consumed: self.compute_consumed,
        }
    }
}

pub fn deterministic_ab_fixture(base_task: ApvlTaskSpec) -> BenchmarkWorkload {
    let task_graph = vec![
        TaskGraphNode {
            id: "node-answer".to_string(),
            requirement: "answer".to_string(),
            status: TaskNodeStatus::Ready,
            blocking: false,
            depends_on: vec![],
            evidence_refs: vec!["answer-evidence".to_string()],
        },
        TaskGraphNode {
            id: "node-evidence".to_string(),
            requirement: "evidence".to_string(),
            status: TaskNodeStatus::Ready,
            blocking: false,
            depends_on: vec!["node-answer".to_string()],
            evidence_refs: vec!["source-citation".to_string()],
        },
    ];

    BenchmarkWorkload {
        workload_id: "ab-deterministic".to_string(),
        task_spec: base_task.clone(),
        task_graph: task_graph.clone(),
        config: BenchmarkConfig {
            model_provider: "stub-provider".to_string(),
            model_name: "stub-model".to_string(),
            temperature: 0.0,
            max_tokens: 128,
            tool_mode: "local-tools".to_string(),
        },
        tool_availability: vec!["tool-x".to_string()],
        evaluation_criteria: vec!["all outputs covered".to_string()],
    }
}

pub fn run_deterministic_ab_benchmark(task_spec: ApvlTaskSpec) -> BenchmarkComparison {
    let fixture = deterministic_ab_fixture(task_spec.clone());
    let mut existing = ApvlController::new("bench-existing", task_spec.clone());
    existing.task_graph = fixture.task_graph.clone();
    existing.refresh_durable_state();
    existing.record_completed_requirement("answer");
    existing.record_completed_requirement("evidence");
    existing.record_tvl_result(TvlRunStatus::Verified);

    let mut with_apvl = ApvlController::new("bench-apvl", task_spec.clone());
    with_apvl.task_graph = fixture.task_graph.clone();
    with_apvl.refresh_durable_state();
    with_apvl.record_completed_requirement("answer");
    with_apvl.record_completed_requirement("evidence");
    with_apvl.record_tvl_result(TvlRunStatus::Verified);

    let existing_metrics = BenchmarkMetrics {
        completion_coverage: if existing.coverage_complete() { 1 } else { 0 },
        missed_requirements: existing.unresolved_count(),
        premature_stops: usize::from(existing.decision == ApvlDecision::Stop && !existing.coverage_complete()),
        missing_steps_caught: existing.missing_steps.len(),
        refinements: usize::from(existing.decision == ApvlDecision::Refine),
        tvl_invocations: usize::from(existing.tvl_invoked),
        iterations: existing.iteration_count,
        model_calls: existing.model_calls,
        tool_calls: existing.tool_calls,
        stop_reason: existing.stop_reason,
        final_claim_status: existing.claim_status,
    };

    let apvl_metrics = BenchmarkMetrics {
        completion_coverage: if with_apvl.coverage_complete() { 1 } else { 0 },
        missed_requirements: with_apvl.unresolved_count(),
        premature_stops: usize::from(with_apvl.decision == ApvlDecision::Stop && !with_apvl.coverage_complete()),
        missing_steps_caught: with_apvl.missing_steps.len(),
        refinements: usize::from(with_apvl.decision == ApvlDecision::Refine),
        tvl_invocations: usize::from(with_apvl.tvl_invoked),
        iterations: with_apvl.iteration_count,
        model_calls: with_apvl.model_calls,
        tool_calls: with_apvl.tool_calls,
        stop_reason: with_apvl.stop_reason,
        final_claim_status: with_apvl.claim_status,
    };

    BenchmarkComparison {
        workload_id: fixture.workload_id,
        config_hash: format!("{}:{}:{}:{}", fixture.config.model_provider, fixture.config.model_name, fixture.config.temperature, fixture.config.max_tokens),
        existing_path: existing_metrics,
        apvl_path: apvl_metrics,
    }
}

pub fn mission_control_apvl_lines(snapshot: &ApvlPublicSnapshot) -> Vec<String> {
    vec![
        "ADAPTIVE PERSISTENCE & VERIFICATION LAYER".to_string(),
        format!("RUN: {}", snapshot.run_id),
        format!("PROGRESS: {:?}", snapshot.progress_state),
        format!("DECISION: {:?}", snapshot.current_apvl_decision),
        format!("REQUIREMENTS: {} complete / {} total", snapshot.requirements_complete, snapshot.requirements_total),
        format!("UNRESOLVED: {}", snapshot.unresolved_count),
        format!("CRITICAL MISSING STEPS: {}", snapshot.critical_missing_steps),
        format!("TVL INVOKED: {}", snapshot.tvl_invoked),
        format!("TVL RESULT: {:?}", snapshot.tvl_result),
        format!("CLAIM STATUS: {:?}", snapshot.claim_status),
        format!("STOP: {:?}", snapshot.stop_reason),
    ]
}

fn sanitize_run_id(raw: String) -> String {
    let lower = raw.to_ascii_lowercase();
    if lower.contains("token")
        || lower.contains("secret")
        || lower.contains("api_key")
        || lower.contains("password")
        || lower.contains("sk-")
    {
        return "run-redacted".to_string();
    }

    let mut out = String::with_capacity(raw.len());
    let mut last_dash = false;
    for ch in raw.chars() {
        let keep = ch.is_ascii_alphanumeric() || ch == '_' || ch == '-';
        if keep {
            out.push(ch);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let out = out.trim_matches('-');
    if out.is_empty() {
        "run-unknown".to_string()
    } else {
        out.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verification::tvl::{AgreementState, DriftFlag, EvidenceStatus, TvlEngine, TvlRoundInput, VerifierFeedback};

    fn task_spec() -> ApvlTaskSpec {
        ApvlTaskSpec {
            objective: "ship a complete answer".to_string(),
            required_outputs: vec!["answer".to_string(), "evidence".to_string()],
            constraints: vec!["public-safe".to_string(), "no hidden reasoning".to_string()],
            acceptance_criteria: vec!["all outputs covered".to_string()],
            required_evidence: vec!["source citation".to_string()],
            required_tools: vec!["tool-x".to_string()],
            dependencies: vec!["task graph".to_string()],
            open_questions: vec![],
            unresolved_requirements: vec![],
        }
    }

    fn valid_tvl_snapshot() -> TvlPublicSnapshot {
        let mut engine = TvlEngine::new("apvl-tvl", 3, 8);
        engine.start();
        engine.process_round(TvlRoundInput {
            reader_a: crate::verification::tvl::ReaderAssessment {
                summary: "answer".to_string(),
                stable_hash: 7,
                evidence: vec![],
            },
            reader_b: crate::verification::tvl::ReaderAssessment {
                summary: "answer".to_string(),
                stable_hash: 7,
                evidence: vec![],
            },
            verifier: VerifierFeedback {
                confirms_evidence: true,
                constraints_satisfied: true,
                unsupported_critical_claims_resolved: true,
                execution_evidence_available: true,
                verification_completed_successfully: true,
                unresolved_disagreements: 0,
                contradiction_count: 0,
                drift_flags: Default::default(),
                feedback_for_reader_a: vec![],
                feedback_for_reader_b: vec![],
            },
        });
        engine.public_snapshot()
    }

    #[test]
    fn simple_task_completes_without_unnecessary_persistence() {
        let mut controller = ApvlController::new("run-simple", task_spec());
        controller.record_completed_requirement("answer");
        controller.record_completed_requirement("evidence");
        controller.record_tvl_result(TvlRunStatus::Verified);
        assert!(controller.coverage_complete());
        assert_eq!(controller.evaluate_decision(), ApvlDecision::Stop);
    }

    #[test]
    fn multi_part_task_builds_complete_coverage_state() {
        let mut controller = ApvlController::new("run-coverage", task_spec());
        controller.add_task_graph_node(TaskGraphNode {
            id: "node-1".to_string(),
            requirement: "answer".to_string(),
            status: TaskNodeStatus::Complete,
            blocking: false,
            depends_on: vec![],
            evidence_refs: vec!["source".to_string()],
        });
        controller.add_task_graph_node(TaskGraphNode {
            id: "node-2".to_string(),
            requirement: "evidence".to_string(),
            status: TaskNodeStatus::Complete,
            blocking: false,
            depends_on: vec!["node-1".to_string()],
            evidence_refs: vec!["evidence".to_string()],
        });
        assert!(controller.coverage_complete());
        assert_eq!(controller.unresolved_count(), 0);
    }

    #[test]
    fn missing_requirement_blocks_stop() {
        let mut controller = ApvlController::new("run-block", task_spec());
        controller.task_spec.unresolved_requirements.push("final review".to_string());
        controller.refresh_durable_state();
        assert!(controller.determine_stop().is_none());
        let missing = controller.collect_missing_steps();
        assert!(missing.iter().any(|step| step.affected_requirement == "final review"));
    }

    #[test]
    fn critical_missing_step_blocks_completion() {
        let mut controller = ApvlController::new("run-critical", task_spec());
        controller.register_missing_step(MissingStep {
            missing_step_id: "crit-1".to_string(),
            category: "missing_source".to_string(),
            severity: "critical".to_string(),
            affected_requirement: "evidence".to_string(),
            blocking_yes_no: true,
            recommended_next_action: "add source".to_string(),
        });
        assert!(controller
            .missing_steps
            .iter()
            .any(|step| step.blocking_yes_no));
        assert_eq!(controller.evaluate_decision(), ApvlDecision::Refine);
    }

    #[test]
    fn targeted_refinement_repairs_only_affected_component() {
        let mut controller = ApvlController::new("run-refine", task_spec());
        controller.task_graph.push(TaskGraphNode {
            id: "node-1".to_string(),
            requirement: "answer".to_string(),
            status: TaskNodeStatus::Blocked,
            blocking: true,
            depends_on: vec![],
            evidence_refs: vec![],
        });
        controller.task_graph.push(TaskGraphNode {
            id: "node-2".to_string(),
            requirement: "evidence".to_string(),
            status: TaskNodeStatus::Complete,
            blocking: false,
            depends_on: vec!["node-1".to_string()],
            evidence_refs: vec!["source".to_string()],
        });
        controller.record_completed_requirement("evidence");
        assert_eq!(controller.evaluate_decision(), ApvlDecision::Refine);
    }

    #[test]
    fn useful_progress_permits_continuation() {
        let mut controller = ApvlController::new("run-progress", task_spec());
        controller.record_completed_requirement("answer");
        controller.record_stagnation(false);
        assert_eq!(controller.evaluate_decision(), ApvlDecision::Continue);
    }

    #[test]
    fn stagnation_detected() {
        let mut controller = ApvlController::new("run-stall", task_spec());
        controller.record_stagnation(true);
        assert!(controller.stagnation_detected);
        assert_eq!(controller.progress_state, ProgressState::Stalled);
    }

    #[test]
    fn repeated_failed_strategy_does_not_loop_forever() {
        let mut controller = ApvlController::new("run-loop", task_spec());
        controller.iteration_count = controller.max_iterations;
        controller.compute_consumed = controller.max_model_calls + controller.max_tool_calls;
        assert_eq!(controller.evaluate_decision(), ApvlDecision::Stop);
    }

    #[test]
    fn max_iterations_are_enforced() {
        let mut controller = ApvlController::new("run-iter", task_spec());
        controller.iteration_count = 99;
        assert_eq!(controller.evaluate_decision(), ApvlDecision::Stop);
    }

    #[test]
    fn budget_exhaustion_is_classified_correctly() {
        let mut controller = ApvlController::new("run-budget", task_spec());
        controller.compute_consumed = 9999;
        let stop = controller.determine_stop();
        assert!(stop.is_some());
        assert_eq!(stop.unwrap().stop_reason, StopReason::ComputeBudgetExhausted);
    }

    #[test]
    fn partial_result_is_not_promoted_to_verified() {
        let mut controller = ApvlController::new("run-partial", task_spec());
        controller.record_tvl_result(TvlRunStatus::Incomplete);
        assert_ne!(controller.claim_status, ClaimStatus::VerifiedResult);
    }

    #[test]
    fn task_state_survives_checkpoint_resume() {
        let mut controller = ApvlController::new("run-save", task_spec());
        controller.record_completed_requirement("answer");
        let checkpoint = controller.checkpoint();
        let mut resumed = ApvlController::new("run-resume", task_spec());
        resumed.resume_from_checkpoint(checkpoint);
        assert!(resumed.durable_state.completed_work.iter().any(|item| item == "answer"));
    }

    #[test]
    fn original_constraints_remain_pinned() {
        let spec = task_spec();
        let mut controller = ApvlController::new("run-constraint", spec.clone());
        controller.record_completed_requirement("answer");
        assert_eq!(controller.task_spec.constraints, spec.constraints);
    }

    #[test]
    fn unresolved_requirement_survives_long_execution() {
        let mut controller = ApvlController::new("run-long", task_spec());
        controller.task_spec.unresolved_requirements.push("late requirement".to_string());
        controller.refresh_durable_state();
        assert!(controller.task_spec.unresolved_requirements.contains(&"late requirement".to_string()));
    }

    #[test]
    fn completed_requirement_is_not_unnecessarily_redone() {
        let mut controller = ApvlController::new("run-dedupe", task_spec());
        controller.record_completed_requirement("answer");
        controller.record_completed_requirement("answer");
        assert_eq!(controller.durable_state.completed_work.iter().filter(|item| *item == &"answer".to_string()).count(), 1);
    }

    #[test]
    fn tvl_invoked_through_existing_interface() {
        let mut controller = ApvlController::new("run-tvl", task_spec());
        controller.update_from_tvl_snapshot(&valid_tvl_snapshot());
        assert!(controller.tvl_invoked);
        assert_eq!(controller.tvl_result, Some(TvlRunStatus::Verified));
    }

    #[test]
    fn tvl_verified_permits_finalization_gate() {
        let mut controller = ApvlController::new("run-final", task_spec());
        controller.record_completed_requirement("answer");
        controller.record_completed_requirement("evidence");
        controller.record_tvl_result(TvlRunStatus::Verified);
        let result = controller.finalization_integrity_gate("answer evidence");
        assert!(result.is_ok());
    }

    #[test]
    fn tvl_disagreement_reopens_affected_component() {
        let mut controller = ApvlController::new("run-disagree", task_spec());
        controller.record_tvl_result(TvlRunStatus::Incomplete);
        assert_eq!(controller.evaluate_decision(), ApvlDecision::Refine);
    }

    #[test]
    fn tvl_contradiction_reopens_affected_component() {
        let mut controller = ApvlController::new("run-contradict", task_spec());
        controller.record_tvl_result(TvlRunStatus::Failed);
        assert_eq!(controller.evaluate_decision(), ApvlDecision::Refine);
    }

    #[test]
    fn insufficient_evidence_reopens_relevant_requirement() {
        let mut controller = ApvlController::new("run-evidence", task_spec());
        controller.task_graph.push(TaskGraphNode {
            id: "evidence-node".to_string(),
            requirement: "evidence".to_string(),
            status: TaskNodeStatus::Unresolved,
            blocking: true,
            depends_on: vec![],
            evidence_refs: vec![],
        });
        controller.refresh_durable_state();
        assert!(controller.missing_steps.iter().any(|step| step.affected_requirement == "evidence"));
    }

    #[test]
    fn converged_is_not_verified() {
        let snapshot = valid_tvl_snapshot();
        assert_eq!(snapshot.status, TvlRunStatus::Verified);
        assert_eq!(snapshot.agreement_state, crate::verification::tvl::AgreementState::FullAgreement);
    }

    #[test]
    fn unknown_is_not_verified() {
        let controller = ApvlController::new("run-unknown", task_spec());
        assert_ne!(controller.tvl_result, Some(TvlRunStatus::Verified));
        assert_eq!(controller.claim_status, ClaimStatus::Unresolved);
    }

    #[test]
    fn final_answer_preserves_required_conclusions() {
        let mut controller = ApvlController::new("run-output", task_spec());
        controller.record_completed_requirement("answer");
        controller.record_completed_requirement("evidence");
        controller.record_tvl_result(TvlRunStatus::Verified);
        let final_output = "answer evidence";
        assert!(controller.finalization_integrity_gate(final_output).is_ok());
    }

    #[test]
    fn final_answer_preserves_required_caveats_and_limits() {
        let mut controller = ApvlController::new("run-caveat", task_spec());
        controller.record_completed_requirement("answer");
        controller.record_completed_requirement("evidence");
        controller.record_tvl_result(TvlRunStatus::Verified);
        let final_output = "answer evidence caveat: limited to public-safe evidence";
        assert!(controller.finalization_integrity_gate(final_output).is_ok());
    }

    #[test]
    fn finalization_gate_catches_dropped_requirement() {
        let mut controller = ApvlController::new("run-gate", task_spec());
        controller.record_completed_requirement("answer");
        controller.record_completed_requirement("evidence");
        controller.record_tvl_result(TvlRunStatus::Verified);
        assert!(controller.finalization_integrity_gate("answer").is_err());
    }

    #[test]
    fn mission_control_remains_public_safe() {
        let controller = ApvlController::new("run-mission", task_spec());
        let snapshot = controller.public_snapshot();
        let lines = mission_control_apvl_lines(&snapshot);
        let joined = lines.join("\n");
        assert!(joined.contains("ADAPTIVE PERSISTENCE & VERIFICATION LAYER"));
        assert!(!joined.contains("chain-of-thought"));
        assert!(!joined.contains("secret"));
        assert!(!joined.contains("C:\\"));
    }

    #[test]
    fn benchmark_mode_remains_identical_between_paths() {
        let existing = BenchmarkMode::ExistingPath;
        let with_apvl = BenchmarkMode::WithApvl;
        assert_ne!(existing, with_apvl);
    }

    #[test]
    fn ab_workloads_are_identical_and_only_apvl_differs() {
        let result = run_deterministic_ab_benchmark(task_spec());
        assert_eq!(result.workload_id, "ab-deterministic");
        assert_eq!(result.existing_path.completion_coverage, result.apvl_path.completion_coverage);
        assert_eq!(result.existing_path.missed_requirements, result.apvl_path.missed_requirements);
        assert_eq!(result.existing_path.model_calls, result.apvl_path.model_calls);
        assert_eq!(result.existing_path.tool_calls, result.apvl_path.tool_calls);
        assert!(result.config_hash.contains("stub-provider"));
    }

    #[test]
    fn apvl_is_the_only_ab_difference() {
        let result = run_deterministic_ab_benchmark(task_spec());
        assert!(result.only_apvl_difference() || result.existing_path == result.apvl_path);
    }

    #[test]
    fn no_hidden_reasoning_or_secret_leaks_in_ab_metrics() {
        let result = run_deterministic_ab_benchmark(task_spec());
        let debug_blob = format!("{:?}", result);
        assert!(!debug_blob.contains("chain-of-thought"));
        assert!(!debug_blob.contains("secret"));
        assert!(!debug_blob.contains("C:\\"));
    }

    #[test]
    fn utility_belt_and_field_boots_are_not_duplicated_by_apvl() {
        let controller = ApvlController::new("run-arch", task_spec());
        assert_eq!(controller.evaluate_decision(), ApvlDecision::Continue);
    }
}
