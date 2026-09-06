use std::collections::{BTreeSet, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TvlLifecycleState {
    Planned,
    ReadinessChecked,
    Started,
    Observed,
    Verified,
    Failed,
    Incomplete,
    Blocked,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TvlRunStatus {
    Idle,
    Verifying,
    Verified,
    Failed,
    Incomplete,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorStatus {
    Idle,
    Active,
    Complete,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgreementState {
    Unknown,
    FullAgreement,
    PartialAgreement,
    FactualDisagreement,
    EvidenceDisagreement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DriftFlag {
    Objective,
    Claim,
    Evidence,
    Constraint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceStatus {
    Unknown,
    Sufficient,
    Insufficient,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationState {
    Unknown,
    Pending,
    Passed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UtilityBeltStatus {
    Available,
    Unavailable,
    Degraded,
    PermissionRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldBootsStatus {
    Ready,
    Limited,
    PermissionRequired,
    EnvironmentBlocked,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TvlEventKind {
    TvlStarted,
    RoundStarted,
    ReaderAComplete,
    ReaderBComplete,
    VerifierStarted,
    VerifierComplete,
    DisagreementFound,
    DriftDetected,
    RevisionRequested,
    ConvergenceReached,
    VerificationPassed,
    VerificationFailed,
    TvlCompleted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TvlEvidenceKind {
    Objective,
    Claim,
    Constraint,
    Contradiction,
    Citation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TvlEvidenceRecord {
    pub kind: TvlEvidenceKind,
    pub label: String,
    pub supported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReaderAssessment {
    pub summary: String,
    pub stable_hash: u64,
    pub evidence: Vec<TvlEvidenceRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifierFeedback {
    pub confirms_evidence: bool,
    pub constraints_satisfied: bool,
    pub unsupported_critical_claims_resolved: bool,
    pub execution_evidence_available: bool,
    pub verification_completed_successfully: bool,
    pub unresolved_disagreements: usize,
    pub contradiction_count: usize,
    pub drift_flags: BTreeSet<DriftFlag>,
    pub feedback_for_reader_a: Vec<String>,
    pub feedback_for_reader_b: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TvlRoundInput {
    pub reader_a: ReaderAssessment,
    pub reader_b: ReaderAssessment,
    pub verifier: VerifierFeedback,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TvlOperationRecord {
    pub operation_id: u64,
    pub round: usize,
    pub lifecycle_state: TvlLifecycleState,
    pub verification_state: VerificationState,
    pub agreement_state: AgreementState,
    pub evidence_status: EvidenceStatus,
    pub unresolved_disagreements: usize,
    pub contradiction_count: usize,
    pub drift_flags: BTreeSet<DriftFlag>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TvlEvent {
    pub seq: u64,
    pub round: Option<usize>,
    pub kind: TvlEventKind,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TvlPublicSnapshot {
    pub run_id: String,
    pub round_count: usize,
    pub max_rounds: usize,
    pub status: TvlRunStatus,
    pub reader_a_status: ActorStatus,
    pub reader_b_status: ActorStatus,
    pub verifier_status: ActorStatus,
    pub agreement_state: AgreementState,
    pub unresolved_disagreements: usize,
    pub contradiction_count: usize,
    pub drift_flags: BTreeSet<DriftFlag>,
    pub evidence_status: EvidenceStatus,
    pub verification_state: VerificationState,
    pub final_result_available: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TvlTermination {
    Cancelled,
    Timeout,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TvlEngine {
    run_id: String,
    max_rounds: usize,
    history_cap: usize,
    lifecycle_state: TvlLifecycleState,
    status: TvlRunStatus,
    reader_a_status: ActorStatus,
    reader_b_status: ActorStatus,
    verifier_status: ActorStatus,
    agreement_state: AgreementState,
    unresolved_disagreements: usize,
    contradiction_count: usize,
    drift_flags: BTreeSet<DriftFlag>,
    evidence_status: EvidenceStatus,
    constraints_satisfied: bool,
    unsupported_critical_claims_resolved: bool,
    execution_evidence_available: bool,
    verification_completed_successfully: bool,
    verification_state: VerificationState,
    round_count: usize,
    op_seq: u64,
    event_seq: u64,
    utility_belt_status: UtilityBeltStatus,
    field_boots_status: FieldBootsStatus,
    operation_history: VecDeque<TvlOperationRecord>,
    timeline: VecDeque<TvlEvent>,
    reader_a_last_initial: Option<u64>,
    reader_b_last_initial: Option<u64>,
    reader_a_feedback_buffer: Vec<String>,
    reader_b_feedback_buffer: Vec<String>,
}

impl TvlEngine {
    pub fn new(run_id: impl Into<String>, max_rounds: usize, history_cap: usize) -> Self {
        Self {
            run_id: sanitize_run_id(run_id.into()),
            max_rounds: max_rounds.max(1),
            history_cap: history_cap.max(1),
            lifecycle_state: TvlLifecycleState::Planned,
            status: TvlRunStatus::Idle,
            reader_a_status: ActorStatus::Idle,
            reader_b_status: ActorStatus::Idle,
            verifier_status: ActorStatus::Idle,
            agreement_state: AgreementState::Unknown,
            unresolved_disagreements: 0,
            contradiction_count: 0,
            drift_flags: BTreeSet::new(),
            evidence_status: EvidenceStatus::Unknown,
            constraints_satisfied: false,
            unsupported_critical_claims_resolved: false,
            execution_evidence_available: false,
            verification_completed_successfully: false,
            verification_state: VerificationState::Unknown,
            round_count: 0,
            op_seq: 0,
            event_seq: 0,
            utility_belt_status: UtilityBeltStatus::Available,
            field_boots_status: FieldBootsStatus::Ready,
            operation_history: VecDeque::new(),
            timeline: VecDeque::new(),
            reader_a_last_initial: None,
            reader_b_last_initial: None,
            reader_a_feedback_buffer: Vec::new(),
            reader_b_feedback_buffer: Vec::new(),
        }
    }

    pub fn set_readiness(
        &mut self,
        utility_belt_status: UtilityBeltStatus,
        field_boots_status: FieldBootsStatus,
    ) {
        self.utility_belt_status = utility_belt_status;
        self.field_boots_status = field_boots_status;
        self.lifecycle_state = TvlLifecycleState::ReadinessChecked;

        if matches!(
            self.utility_belt_status,
            UtilityBeltStatus::Unavailable | UtilityBeltStatus::PermissionRequired
        ) || matches!(
            self.field_boots_status,
            FieldBootsStatus::PermissionRequired
                | FieldBootsStatus::EnvironmentBlocked
                | FieldBootsStatus::Unavailable
        ) {
            self.status = TvlRunStatus::Blocked;
            self.reader_a_status = ActorStatus::Blocked;
            self.reader_b_status = ActorStatus::Blocked;
            self.verifier_status = ActorStatus::Blocked;
            self.verification_state = VerificationState::Failed;
            self.lifecycle_state = TvlLifecycleState::Blocked;
            self.emit(TvlEventKind::VerificationFailed, None, "readiness blocked");
            self.emit(TvlEventKind::TvlCompleted, None, "tvl blocked");
            self.push_operation();
        }
    }

    pub fn start(&mut self) {
        if self.status == TvlRunStatus::Blocked {
            return;
        }

        self.lifecycle_state = TvlLifecycleState::Started;
        self.status = TvlRunStatus::Verifying;
        self.verification_state = VerificationState::Pending;
        self.emit(TvlEventKind::TvlStarted, None, "tvl started");
        self.push_operation();
    }

    pub fn process_round(&mut self, input: TvlRoundInput) {
        if !matches!(self.status, TvlRunStatus::Verifying | TvlRunStatus::Idle) {
            return;
        }
        if self.status == TvlRunStatus::Idle {
            self.start();
        }
        if self.round_count >= self.max_rounds {
            self.lifecycle_state = TvlLifecycleState::Incomplete;
            self.status = TvlRunStatus::Incomplete;
            self.verification_state = VerificationState::Failed;
            self.emit(TvlEventKind::VerificationFailed, None, "max rounds reached");
            self.emit(TvlEventKind::TvlCompleted, None, "tvl incomplete");
            self.push_operation();
            return;
        }

        self.round_count += 1;
        let round = self.round_count;
        self.lifecycle_state = TvlLifecycleState::Observed;
        self.reader_a_status = ActorStatus::Active;
        self.reader_b_status = ActorStatus::Active;
        self.verifier_status = ActorStatus::Idle;
        self.emit(TvlEventKind::RoundStarted, Some(round), "round started");

        self.reader_a_status = ActorStatus::Complete;
        self.emit(
            TvlEventKind::ReaderAComplete,
            Some(round),
            "reader a complete",
        );
        self.reader_b_status = ActorStatus::Complete;
        self.emit(
            TvlEventKind::ReaderBComplete,
            Some(round),
            "reader b complete",
        );

        self.verifier_status = ActorStatus::Active;
        self.emit(
            TvlEventKind::VerifierStarted,
            Some(round),
            "verifier started",
        );
        self.verifier_status = ActorStatus::Complete;
        self.emit(
            TvlEventKind::VerifierComplete,
            Some(round),
            "verifier complete",
        );

        self.unresolved_disagreements = input.verifier.unresolved_disagreements;
        self.contradiction_count = input.verifier.contradiction_count;
        self.drift_flags = input.verifier.drift_flags.clone();
        self.evidence_status = if input.verifier.confirms_evidence {
            EvidenceStatus::Sufficient
        } else {
            EvidenceStatus::Insufficient
        };
        self.constraints_satisfied = input.verifier.constraints_satisfied;
        self.unsupported_critical_claims_resolved =
            input.verifier.unsupported_critical_claims_resolved;
        self.execution_evidence_available = input.verifier.execution_evidence_available;
        self.verification_completed_successfully =
            input.verifier.verification_completed_successfully;

        let initial_a = input.reader_a.stable_hash;
        let initial_b = input.reader_b.stable_hash;
        if self.reader_a_last_initial.is_none() {
            self.reader_a_last_initial = Some(initial_a);
        }
        if self.reader_b_last_initial.is_none() {
            self.reader_b_last_initial = Some(initial_b);
        }

        self.reader_a_feedback_buffer = input.verifier.feedback_for_reader_a;
        self.reader_b_feedback_buffer = input.verifier.feedback_for_reader_b;

        self.agreement_state = classify_agreement(
            initial_a,
            initial_b,
            self.unresolved_disagreements,
            self.contradiction_count,
            self.evidence_status,
        );

        let converged = matches!(
            self.agreement_state,
            AgreementState::FullAgreement | AgreementState::PartialAgreement
        );
        if converged {
            self.emit(
                TvlEventKind::ConvergenceReached,
                Some(round),
                "convergence reached",
            );
        }
        if self.unresolved_disagreements > 0
            || matches!(
                self.agreement_state,
                AgreementState::FactualDisagreement | AgreementState::EvidenceDisagreement
            )
        {
            self.emit(
                TvlEventKind::DisagreementFound,
                Some(round),
                "disagreement found",
            );
        }
        if !self.drift_flags.is_empty() {
            self.emit(TvlEventKind::DriftDetected, Some(round), "drift detected");
        }

        let verified = self.can_verify();
        if verified {
            self.status = TvlRunStatus::Verified;
            self.lifecycle_state = TvlLifecycleState::Verified;
            self.verification_state = VerificationState::Passed;
            self.emit(
                TvlEventKind::VerificationPassed,
                Some(round),
                "verification passed",
            );
            self.emit(TvlEventKind::TvlCompleted, Some(round), "tvl completed");
        } else if self.round_count >= self.max_rounds {
            self.status = if self.unresolved_disagreements > 0 {
                TvlRunStatus::Incomplete
            } else {
                TvlRunStatus::Failed
            };
            self.lifecycle_state = match self.status {
                TvlRunStatus::Incomplete => TvlLifecycleState::Incomplete,
                _ => TvlLifecycleState::Failed,
            };
            self.verification_state = VerificationState::Failed;
            self.emit(
                TvlEventKind::VerificationFailed,
                Some(round),
                "verification failed",
            );
            self.emit(TvlEventKind::TvlCompleted, Some(round), "tvl completed");
        } else {
            self.status = TvlRunStatus::Verifying;
            self.verification_state = VerificationState::Pending;
            self.emit(
                TvlEventKind::RevisionRequested,
                Some(round),
                "revision requested",
            );
        }

        self.push_operation();
    }

    pub fn mark_reader_failed(&mut self, reader_a: bool) {
        if reader_a {
            self.reader_a_status = ActorStatus::Failed;
        } else {
            self.reader_b_status = ActorStatus::Failed;
        }
        self.status = TvlRunStatus::Failed;
        self.lifecycle_state = TvlLifecycleState::Failed;
        self.verification_state = VerificationState::Failed;
        self.emit(TvlEventKind::VerificationFailed, None, "reader failed");
        self.emit(TvlEventKind::TvlCompleted, None, "tvl failed");
        self.push_operation();
    }

    pub fn mark_reader_blocked(&mut self, reader_a: bool) {
        if reader_a {
            self.reader_a_status = ActorStatus::Blocked;
        } else {
            self.reader_b_status = ActorStatus::Blocked;
        }
        self.status = TvlRunStatus::Blocked;
        self.lifecycle_state = TvlLifecycleState::Blocked;
        self.verification_state = VerificationState::Failed;
        self.emit(TvlEventKind::VerificationFailed, None, "reader blocked");
        self.emit(TvlEventKind::TvlCompleted, None, "tvl blocked");
        self.push_operation();
    }

    pub fn terminate(&mut self, reason: TvlTermination) {
        match reason {
            TvlTermination::Cancelled => {
                self.status = TvlRunStatus::Blocked;
                self.lifecycle_state = TvlLifecycleState::Blocked;
                self.verification_state = VerificationState::Failed;
                self.emit(TvlEventKind::VerificationFailed, None, "cancelled");
                self.emit(TvlEventKind::TvlCompleted, None, "tvl cancelled");
            }
            TvlTermination::Timeout => {
                self.status = TvlRunStatus::Failed;
                self.lifecycle_state = TvlLifecycleState::Failed;
                self.verification_state = VerificationState::Failed;
                self.emit(TvlEventKind::VerificationFailed, None, "timeout");
                self.emit(TvlEventKind::TvlCompleted, None, "tvl timeout");
            }
        }
        self.push_operation();
    }

    pub fn public_snapshot(&self) -> TvlPublicSnapshot {
        TvlPublicSnapshot {
            run_id: self.run_id.clone(),
            round_count: self.round_count,
            max_rounds: self.max_rounds,
            status: self.status,
            reader_a_status: self.reader_a_status,
            reader_b_status: self.reader_b_status,
            verifier_status: self.verifier_status,
            agreement_state: self.agreement_state,
            unresolved_disagreements: self.unresolved_disagreements,
            contradiction_count: self.contradiction_count,
            drift_flags: self.drift_flags.clone(),
            evidence_status: self.evidence_status,
            verification_state: self.verification_state,
            final_result_available: matches!(
                self.status,
                TvlRunStatus::Verified
                    | TvlRunStatus::Failed
                    | TvlRunStatus::Incomplete
                    | TvlRunStatus::Blocked
            ),
        }
    }

    pub fn recent_operations(&self, limit: usize) -> Vec<TvlOperationRecord> {
        self.operation_history
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }

    pub fn timeline_events(&self, limit: usize) -> Vec<TvlEvent> {
        self.timeline.iter().rev().take(limit).cloned().collect()
    }

    pub fn consume_feedback_for_reader_a(&mut self) -> Vec<String> {
        std::mem::take(&mut self.reader_a_feedback_buffer)
    }

    pub fn consume_feedback_for_reader_b(&mut self) -> Vec<String> {
        std::mem::take(&mut self.reader_b_feedback_buffer)
    }

    fn can_verify(&self) -> bool {
        if self.verification_state == VerificationState::Unknown {
            return false;
        }
        if self.reader_a_status != ActorStatus::Complete {
            return false;
        }
        if self.reader_b_status != ActorStatus::Complete {
            return false;
        }
        if self.verifier_status != ActorStatus::Complete {
            return false;
        }
        if self.unresolved_disagreements > 0 {
            return false;
        }
        if self.contradiction_count > 0 {
            return false;
        }
        if !self.drift_flags.is_empty() {
            return false;
        }
        if self.evidence_status != EvidenceStatus::Sufficient {
            return false;
        }
        if !self.constraints_satisfied {
            return false;
        }
        if !self.unsupported_critical_claims_resolved {
            return false;
        }
        if !self.execution_evidence_available {
            return false;
        }
        if !self.verification_completed_successfully {
            return false;
        }
        matches!(
            self.agreement_state,
            AgreementState::FullAgreement | AgreementState::PartialAgreement
        )
    }

    fn emit(&mut self, kind: TvlEventKind, round: Option<usize>, summary: &str) {
        self.event_seq += 1;
        self.timeline.push_back(TvlEvent {
            seq: self.event_seq,
            round,
            kind,
            summary: summary.to_string(),
        });
        while self.timeline.len() > self.history_cap {
            self.timeline.pop_front();
        }
    }

    fn push_operation(&mut self) {
        self.op_seq += 1;
        self.operation_history.push_back(TvlOperationRecord {
            operation_id: self.op_seq,
            round: self.round_count,
            lifecycle_state: self.lifecycle_state,
            verification_state: self.verification_state,
            agreement_state: self.agreement_state,
            evidence_status: self.evidence_status,
            unresolved_disagreements: self.unresolved_disagreements,
            contradiction_count: self.contradiction_count,
            drift_flags: self.drift_flags.clone(),
        });
        while self.operation_history.len() > self.history_cap {
            self.operation_history.pop_front();
        }
    }
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

fn classify_agreement(
    a: u64,
    b: u64,
    unresolved: usize,
    contradictions: usize,
    evidence_status: EvidenceStatus,
) -> AgreementState {
    if contradictions > 0 {
        return AgreementState::FactualDisagreement;
    }
    if evidence_status == EvidenceStatus::Insufficient {
        return AgreementState::EvidenceDisagreement;
    }
    if unresolved > 0 {
        return AgreementState::PartialAgreement;
    }
    if a == b {
        AgreementState::FullAgreement
    } else {
        AgreementState::PartialAgreement
    }
}

pub fn mission_control_tvl_lines(snapshot: &TvlPublicSnapshot) -> Vec<String> {
    vec![
        "TRIANGULAR VERIFICATION LOOP".to_string(),
        format!("STATUS: {:?}", snapshot.status),
        format!("ROUND: {} / {}", snapshot.round_count, snapshot.max_rounds),
        format!("READER A: {:?}", snapshot.reader_a_status),
        format!("READER B: {:?}", snapshot.reader_b_status),
        format!("VERIFIER C: {:?}", snapshot.verifier_status),
        format!("AGREEMENT: {:?}", snapshot.agreement_state),
        format!(
            "UNRESOLVED: {} · CONTRADICTIONS: {}",
            snapshot.unresolved_disagreements, snapshot.contradiction_count
        ),
        format!("DRIFT FLAGS: {}", snapshot.drift_flags.len()),
        format!("EVIDENCE: {:?}", snapshot.evidence_status),
        format!("FINAL: {:?}", snapshot.verification_state),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assessment(summary: &str, stable_hash: u64, supported: bool) -> ReaderAssessment {
        ReaderAssessment {
            summary: summary.to_string(),
            stable_hash,
            evidence: vec![TvlEvidenceRecord {
                kind: TvlEvidenceKind::Claim,
                label: "c1".to_string(),
                supported,
            }],
        }
    }

    fn feedback(
        confirms_evidence: bool,
        unresolved_disagreements: usize,
        contradiction_count: usize,
        drift_flags: &[DriftFlag],
    ) -> VerifierFeedback {
        VerifierFeedback {
            confirms_evidence,
            constraints_satisfied: true,
            unsupported_critical_claims_resolved: true,
            execution_evidence_available: true,
            verification_completed_successfully: true,
            unresolved_disagreements,
            contradiction_count,
            drift_flags: drift_flags.iter().copied().collect(),
            feedback_for_reader_a: vec!["narrow claim".to_string()],
            feedback_for_reader_b: vec!["supply evidence".to_string()],
        }
    }

    #[test]
    fn agree_and_evidence_confirmed_yields_verified() {
        let mut engine = TvlEngine::new("run-1", 3, 16);
        engine.start();
        engine.process_round(TvlRoundInput {
            reader_a: assessment("ok", 7, true),
            reader_b: assessment("ok", 7, true),
            verifier: feedback(true, 0, 0, &[]),
        });
        assert_eq!(engine.public_snapshot().status, TvlRunStatus::Verified);
    }

    #[test]
    fn disagreement_requests_another_round() {
        let mut engine = TvlEngine::new("run-2", 3, 16);
        engine.start();
        engine.process_round(TvlRoundInput {
            reader_a: assessment("a", 1, true),
            reader_b: assessment("b", 2, true),
            verifier: feedback(true, 1, 0, &[]),
        });
        assert_eq!(engine.public_snapshot().status, TvlRunStatus::Verifying);
        assert!(
            engine
                .timeline_events(8)
                .iter()
                .any(|e| e.kind == TvlEventKind::RevisionRequested)
        );
    }

    #[test]
    fn partial_agreement_classified() {
        let mut engine = TvlEngine::new("run-3", 3, 16);
        engine.start();
        engine.process_round(TvlRoundInput {
            reader_a: assessment("a", 1, true),
            reader_b: assessment("b", 2, true),
            verifier: feedback(true, 1, 0, &[]),
        });
        assert_eq!(
            engine.public_snapshot().agreement_state,
            AgreementState::PartialAgreement
        );
    }

    #[test]
    fn factual_disagreement_classified() {
        let mut engine = TvlEngine::new("run-4", 3, 16);
        engine.start();
        engine.process_round(TvlRoundInput {
            reader_a: assessment("a", 1, true),
            reader_b: assessment("a", 1, true),
            verifier: feedback(true, 0, 1, &[]),
        });
        assert_eq!(
            engine.public_snapshot().agreement_state,
            AgreementState::FactualDisagreement
        );
    }

    #[test]
    fn evidence_disagreement_classified() {
        let mut engine = TvlEngine::new("run-5", 3, 16);
        engine.start();
        engine.process_round(TvlRoundInput {
            reader_a: assessment("a", 1, true),
            reader_b: assessment("a", 1, true),
            verifier: feedback(false, 0, 0, &[]),
        });
        assert_eq!(
            engine.public_snapshot().agreement_state,
            AgreementState::EvidenceDisagreement
        );
    }

    #[test]
    fn insufficient_evidence_prevents_verified() {
        let mut engine = TvlEngine::new("run-6", 3, 16);
        engine.start();
        engine.process_round(TvlRoundInput {
            reader_a: assessment("a", 1, true),
            reader_b: assessment("a", 1, true),
            verifier: feedback(false, 0, 0, &[]),
        });
        assert_ne!(engine.public_snapshot().status, TvlRunStatus::Verified);
    }

    #[test]
    fn contradiction_prevents_verified() {
        let mut engine = TvlEngine::new("run-7", 3, 16);
        engine.start();
        engine.process_round(TvlRoundInput {
            reader_a: assessment("a", 1, true),
            reader_b: assessment("a", 1, true),
            verifier: feedback(true, 0, 2, &[]),
        });
        assert_ne!(engine.public_snapshot().status, TvlRunStatus::Verified);
    }

    #[test]
    fn feedback_reaches_next_round_buffers() {
        let mut engine = TvlEngine::new("run-8", 3, 16);
        engine.start();
        engine.process_round(TvlRoundInput {
            reader_a: assessment("a", 1, true),
            reader_b: assessment("b", 2, true),
            verifier: feedback(true, 1, 0, &[]),
        });
        assert_eq!(engine.consume_feedback_for_reader_a(), vec!["narrow claim"]);
        assert_eq!(
            engine.consume_feedback_for_reader_b(),
            vec!["supply evidence"]
        );
    }

    #[test]
    fn reader_initial_assessments_stay_isolated() {
        let mut engine = TvlEngine::new("run-9", 3, 16);
        engine.start();
        engine.process_round(TvlRoundInput {
            reader_a: assessment("a", 10, true),
            reader_b: assessment("b", 20, true),
            verifier: feedback(true, 1, 0, &[]),
        });
        assert_eq!(engine.reader_a_last_initial, Some(10));
        assert_eq!(engine.reader_b_last_initial, Some(20));
    }

    #[test]
    fn max_rounds_enforced() {
        let mut engine = TvlEngine::new("run-10", 1, 16);
        engine.start();
        engine.process_round(TvlRoundInput {
            reader_a: assessment("a", 1, true),
            reader_b: assessment("b", 2, true),
            verifier: feedback(true, 1, 0, &[]),
        });
        assert_eq!(engine.public_snapshot().status, TvlRunStatus::Incomplete);
    }

    #[test]
    fn unresolved_at_max_rounds_incomplete() {
        let mut engine = TvlEngine::new("run-11", 1, 16);
        engine.start();
        engine.process_round(TvlRoundInput {
            reader_a: assessment("a", 1, true),
            reader_b: assessment("b", 2, true),
            verifier: feedback(true, 2, 0, &[]),
        });
        assert_eq!(engine.public_snapshot().status, TvlRunStatus::Incomplete);
    }

    #[test]
    fn verified_result_exits_early() {
        let mut engine = TvlEngine::new("run-12", 3, 16);
        engine.start();
        engine.process_round(TvlRoundInput {
            reader_a: assessment("a", 1, true),
            reader_b: assessment("a", 1, true),
            verifier: feedback(true, 0, 0, &[]),
        });
        let rounds = engine.public_snapshot().round_count;
        engine.process_round(TvlRoundInput {
            reader_a: assessment("a", 1, true),
            reader_b: assessment("a", 1, true),
            verifier: feedback(true, 0, 0, &[]),
        });
        assert_eq!(engine.public_snapshot().round_count, rounds);
    }

    #[test]
    fn failed_reader_handled_safely() {
        let mut engine = TvlEngine::new("run-13", 3, 16);
        engine.start();
        engine.mark_reader_failed(true);
        assert_eq!(engine.public_snapshot().status, TvlRunStatus::Failed);
    }

    #[test]
    fn blocked_reader_handled_safely() {
        let mut engine = TvlEngine::new("run-14", 3, 16);
        engine.start();
        engine.mark_reader_blocked(false);
        assert_eq!(engine.public_snapshot().status, TvlRunStatus::Blocked);
    }

    #[test]
    fn cancelled_run_terminates_safely() {
        let mut engine = TvlEngine::new("run-15", 3, 16);
        engine.start();
        engine.terminate(TvlTermination::Cancelled);
        assert_eq!(engine.public_snapshot().status, TvlRunStatus::Blocked);
    }

    #[test]
    fn timeout_terminates_safely() {
        let mut engine = TvlEngine::new("run-16", 3, 16);
        engine.start();
        engine.terminate(TvlTermination::Timeout);
        assert_eq!(engine.public_snapshot().status, TvlRunStatus::Failed);
    }

    #[test]
    fn utility_belt_unavailable_respected() {
        let mut engine = TvlEngine::new("run-17", 3, 16);
        engine.set_readiness(UtilityBeltStatus::Unavailable, FieldBootsStatus::Ready);
        engine.start();
        assert_eq!(engine.public_snapshot().status, TvlRunStatus::Blocked);
    }

    #[test]
    fn utility_belt_permission_required_respected() {
        let mut engine = TvlEngine::new("run-18", 3, 16);
        engine.set_readiness(
            UtilityBeltStatus::PermissionRequired,
            FieldBootsStatus::Ready,
        );
        engine.start();
        assert_eq!(engine.public_snapshot().status, TvlRunStatus::Blocked);
    }

    #[test]
    fn field_boots_environment_blocked_respected() {
        let mut engine = TvlEngine::new("run-19", 3, 16);
        engine.set_readiness(
            UtilityBeltStatus::Available,
            FieldBootsStatus::EnvironmentBlocked,
        );
        engine.start();
        assert_eq!(engine.public_snapshot().status, TvlRunStatus::Blocked);
    }

    #[test]
    fn objective_drift_detected() {
        let mut engine = TvlEngine::new("run-20", 3, 16);
        engine.start();
        engine.process_round(TvlRoundInput {
            reader_a: assessment("a", 1, true),
            reader_b: assessment("a", 1, true),
            verifier: feedback(true, 0, 0, &[DriftFlag::Objective]),
        });
        assert!(
            engine
                .public_snapshot()
                .drift_flags
                .contains(&DriftFlag::Objective)
        );
    }

    #[test]
    fn claim_drift_detected() {
        let mut engine = TvlEngine::new("run-21", 3, 16);
        engine.start();
        engine.process_round(TvlRoundInput {
            reader_a: assessment("a", 1, true),
            reader_b: assessment("a", 1, true),
            verifier: feedback(true, 0, 0, &[DriftFlag::Claim]),
        });
        assert!(
            engine
                .public_snapshot()
                .drift_flags
                .contains(&DriftFlag::Claim)
        );
    }

    #[test]
    fn evidence_drift_detected() {
        let mut engine = TvlEngine::new("run-22", 3, 16);
        engine.start();
        engine.process_round(TvlRoundInput {
            reader_a: assessment("a", 1, true),
            reader_b: assessment("a", 1, true),
            verifier: feedback(true, 0, 0, &[DriftFlag::Evidence]),
        });
        assert!(
            engine
                .public_snapshot()
                .drift_flags
                .contains(&DriftFlag::Evidence)
        );
    }

    #[test]
    fn constraint_drift_detected() {
        let mut engine = TvlEngine::new("run-23", 3, 16);
        engine.start();
        engine.process_round(TvlRoundInput {
            reader_a: assessment("a", 1, true),
            reader_b: assessment("a", 1, true),
            verifier: feedback(true, 0, 0, &[DriftFlag::Constraint]),
        });
        assert!(
            engine
                .public_snapshot()
                .drift_flags
                .contains(&DriftFlag::Constraint)
        );
    }

    #[test]
    fn stable_run_has_no_false_drift() {
        let mut engine = TvlEngine::new("run-24", 3, 16);
        engine.start();
        engine.process_round(TvlRoundInput {
            reader_a: assessment("a", 9, true),
            reader_b: assessment("a", 9, true),
            verifier: feedback(true, 0, 0, &[]),
        });
        assert!(engine.public_snapshot().drift_flags.is_empty());
    }

    #[test]
    fn convergence_not_equal_verification_when_evidence_insufficient() {
        let mut engine = TvlEngine::new("run-25", 3, 16);
        engine.start();
        engine.process_round(TvlRoundInput {
            reader_a: assessment("a", 9, true),
            reader_b: assessment("a", 9, true),
            verifier: feedback(false, 0, 0, &[]),
        });
        assert_eq!(
            engine.public_snapshot().agreement_state,
            AgreementState::EvidenceDisagreement
        );
        assert_ne!(engine.public_snapshot().status, TvlRunStatus::Verified);
    }

    #[test]
    fn readers_agree_without_execution_evidence_not_verified() {
        let mut engine = TvlEngine::new("run-exec-evidence", 3, 16);
        engine.start();
        let mut v = feedback(true, 0, 0, &[]);
        v.execution_evidence_available = false;
        engine.process_round(TvlRoundInput {
            reader_a: assessment("a", 3, true),
            reader_b: assessment("a", 3, true),
            verifier: v,
        });
        assert_ne!(engine.public_snapshot().status, TvlRunStatus::Verified);
    }

    #[test]
    fn readers_agree_without_constraints_not_verified() {
        let mut engine = TvlEngine::new("run-constraints", 3, 16);
        engine.start();
        let mut v = feedback(true, 0, 0, &[]);
        v.constraints_satisfied = false;
        engine.process_round(TvlRoundInput {
            reader_a: assessment("a", 3, true),
            reader_b: assessment("a", 3, true),
            verifier: v,
        });
        assert_ne!(engine.public_snapshot().status, TvlRunStatus::Verified);
    }

    #[test]
    fn readers_agree_with_unsupported_critical_claims_not_verified() {
        let mut engine = TvlEngine::new("run-unsupported", 3, 16);
        engine.start();
        let mut v = feedback(true, 0, 0, &[]);
        v.unsupported_critical_claims_resolved = false;
        engine.process_round(TvlRoundInput {
            reader_a: assessment("a", 3, true),
            reader_b: assessment("a", 3, true),
            verifier: v,
        });
        assert_ne!(engine.public_snapshot().status, TvlRunStatus::Verified);
    }

    #[test]
    fn started_not_verified() {
        let mut engine = TvlEngine::new("run-started", 3, 16);
        engine.start();
        assert_ne!(engine.public_snapshot().status, TvlRunStatus::Verified);
    }

    #[test]
    fn unknown_cannot_be_verified() {
        let engine = TvlEngine::new("run-26", 3, 16);
        let snap = engine.public_snapshot();
        assert_eq!(snap.verification_state, VerificationState::Unknown);
        assert_ne!(snap.status, TvlRunStatus::Verified);
    }

    #[test]
    fn deterministic_ordering() {
        let mut engine = TvlEngine::new("run-27", 3, 16);
        engine.start();
        engine.process_round(TvlRoundInput {
            reader_a: assessment("a", 1, true),
            reader_b: assessment("a", 1, true),
            verifier: feedback(true, 0, 0, &[]),
        });
        let mut events = engine.timeline_events(16);
        events.reverse();
        let kinds: Vec<TvlEventKind> = events.into_iter().map(|e| e.kind).collect();
        assert_eq!(
            kinds,
            vec![
                TvlEventKind::TvlStarted,
                TvlEventKind::RoundStarted,
                TvlEventKind::ReaderAComplete,
                TvlEventKind::ReaderBComplete,
                TvlEventKind::VerifierStarted,
                TvlEventKind::VerifierComplete,
                TvlEventKind::ConvergenceReached,
                TvlEventKind::VerificationPassed,
                TvlEventKind::TvlCompleted,
            ]
        );
    }

    #[test]
    fn bounded_state_history_behavior() {
        let mut engine = TvlEngine::new("run-28", 6, 2);
        engine.start();
        for i in 0..4 {
            engine.process_round(TvlRoundInput {
                reader_a: assessment("a", i, true),
                reader_b: assessment("b", i + 1, true),
                verifier: feedback(true, 1, 0, &[]),
            });
        }
        assert!(engine.recent_operations(10).len() <= 2);
        assert!(engine.timeline_events(10).len() <= 2);
    }

    #[test]
    fn mission_control_lines_are_public_safe() {
        let mut engine = TvlEngine::new("run-29", 3, 16);
        engine.start();
        engine.process_round(TvlRoundInput {
            reader_a: assessment("a", 7, true),
            reader_b: assessment("a", 7, true),
            verifier: feedback(true, 0, 0, &[]),
        });
        let lines = mission_control_tvl_lines(&engine.public_snapshot());
        let joined = lines.join("\n");
        assert!(joined.contains("TRIANGULAR VERIFICATION LOOP"));
        assert!(!joined.contains("summary"));
    }

    #[test]
    fn no_secret_or_private_path_leak_in_public_snapshot() {
        let mut engine = TvlEngine::new("C:\\private\\secrets\\sk-prod-token", 3, 16);
        engine.start();
        engine.process_round(TvlRoundInput {
            reader_a: assessment("a", 7, true),
            reader_b: assessment("a", 7, true),
            verifier: feedback(true, 0, 0, &[]),
        });
        let debug = format!("{:?}", engine.public_snapshot());
        assert!(!debug.contains("C:\\"));
        assert!(!debug.contains("sk-prod-token"));
        assert!(!debug.to_ascii_lowercase().contains("token"));
    }

    #[test]
    fn mission_control_lines_do_not_leak_hidden_reasoning_or_prompts() {
        let mut engine = TvlEngine::new("run-31", 3, 16);
        engine.start();
        engine.process_round(TvlRoundInput {
            reader_a: assessment("chain-of-thought: hidden", 4, true),
            reader_b: assessment("prompt: raw verifier prompt", 4, true),
            verifier: feedback(true, 0, 0, &[]),
        });
        let joined = mission_control_tvl_lines(&engine.public_snapshot()).join("\n");
        assert!(!joined.contains("chain-of-thought"));
        assert!(!joined.contains("prompt:"));
    }

    #[test]
    fn event_ordering_exists_for_ledger_integration() {
        let mut engine = TvlEngine::new("run-31", 3, 16);
        engine.start();
        engine.process_round(TvlRoundInput {
            reader_a: assessment("a", 1, true),
            reader_b: assessment("b", 2, true),
            verifier: feedback(true, 1, 0, &[]),
        });
        let mut events = engine.timeline_events(16);
        events.reverse();
        let seqs: Vec<u64> = events.iter().map(|e| e.seq).collect();
        assert!(seqs.windows(2).all(|w| w[0] < w[1]));
    }
}
