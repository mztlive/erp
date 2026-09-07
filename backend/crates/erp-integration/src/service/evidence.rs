//! 集成证据政策、原因注册表与受控引用的唯一校验和投影。
use crate::dto::{
    ActionBlockerView, ControlledEvidenceKind, ControlledEvidenceRef, DifferenceReasonCode,
    DirectReconciliationConclusion, EvidencePolicyKey, ReconciliationReasonRegistryView,
    RegisteredReconciliationReasonView, ResolutionEvidencePolicyView, ReviewerSeparation,
};
use crate::entity::integration_ops::{
    difference_terminal_policy, error_terminal_policy,
    reconciliation_reason_registry as domain_reason_registry, DirectConclusion, EvidenceReferenceSet,
    IntegrationErrorTask, ReconciliationDifference, RequiredEvidenceKind, TerminalEvidencePolicy,
};
use crate::ports::evidence::{EvidenceSubject, IntegrationEvidenceAuthority, VerifiedEvidence};
use crate::{Error, Result};
use persistence_core::Executor;

/// 返回错误任务当前固定证据策略视图（规则归领域，此处只做 view 映射）。
///
/// # 参数
/// * `task` - 集成错误任务
///
/// # 返回
/// 返回响应视图；策略身份、资金影响与类型要求来自领域策略。
pub fn error_evidence_policy(task: &IntegrationErrorTask) -> ResolutionEvidencePolicyView {
    policy_view(&error_terminal_policy(task))
}

/// 返回对账差异当前固定证据策略视图（规则归领域，此处只做 view 映射）。
///
/// # 参数
/// * `difference` - 对账差异
///
/// # 返回
/// 返回响应视图；策略身份、资金影响与类型要求来自领域策略。
pub fn difference_evidence_policy(difference: &ReconciliationDifference) -> ResolutionEvidencePolicyView {
    policy_view(&difference_terminal_policy(difference))
}

/// 返回无任务直接对账固定原因注册表视图（注册表归领域，此处只做 view 映射）。
///
/// # 返回
/// 返回响应视图；原因、结论与类型要求来自领域注册表。
pub fn reconciliation_reason_registry() -> ReconciliationReasonRegistryView {
    let registry = domain_reason_registry();
    ReconciliationReasonRegistryView {
        reason_registry_id: registry.id.to_string(),
        reason_registry_version: registry.version,
        registered_reasons: registry
            .reasons
            .iter()
            .map(|reason| RegisteredReconciliationReasonView {
                registered_reason_id: reason.id.to_string(),
                registered_reason_version: reason.version,
                conclusion: dto_conclusion(reason.conclusion),
                label: reason.label.to_string(),
                required_evidence_kinds: reason
                    .required
                    .iter()
                    .map(|kind| ControlledEvidenceKind::from(*kind))
                    .collect(),
            })
            .collect(),
    }
}

/// 将领域终态证据策略映射为响应视图。
///
/// # 参数
/// * `policy` - 领域终态证据策略
///
/// # 返回
/// 返回响应视图；岗位分离要求恒为无（领域当前无复核分离规则）。
///
/// # 约束
/// 只做词汇映射，不维护第二份类型要求。
fn policy_view(policy: &TerminalEvidencePolicy) -> ResolutionEvidencePolicyView {
    ResolutionEvidencePolicyView {
        evidence_policy_id: policy.policy_id.to_string(),
        evidence_policy_version: policy.version,
        key: EvidencePolicyKey {
            error_type: policy.error_type.clone(),
            funds_impact: policy.funds_impact.as_str().to_string(),
        },
        required_evidence_kinds: policy
            .required
            .iter()
            .map(|kind| ControlledEvidenceKind::from(*kind))
            .collect(),
        reviewer_separation: ReviewerSeparation::None,
    }
}

/// 将受控证据引用投影为领域证据类型集合。
///
/// # 参数
/// * `refs` - 客户端提交或服务端发现的受控证据引用
///
/// # 返回
/// 返回领域证据类型集合（顺序保留提交顺序，不去重）。
pub fn domain_kinds(refs: &[ControlledEvidenceRef]) -> Vec<RequiredEvidenceKind> {
    refs.iter()
        .map(|evidence| RequiredEvidenceKind::from(evidence.kind))
        .collect()
}

/// 将领域动作阻断映射为响应视图。
///
/// # 参数
/// * `blocker` - 领域动作阻断
///
/// # 返回
/// 返回阻断响应视图；稳定代码与说明来自领域。
pub fn blocker_view(blocker: &crate::entity::integration_ops::ActionBlocker) -> ActionBlockerView {
    ActionBlockerView {
        action: blocker.action.as_str().to_string(),
        code: blocker.code.to_string(),
        message: blocker.message.to_string(),
    }
}

/// 将领域终态结论映射为服务 DTO 结论。
///
/// # 参数
/// * `conclusion` - 领域终态结论
///
/// # 返回
/// 返回一一对应的服务 DTO 结论。
fn dto_conclusion(conclusion: DirectConclusion) -> DirectReconciliationConclusion {
    match conclusion {
        DirectConclusion::ConfirmNoError => DirectReconciliationConclusion::ConfirmNoError,
        DirectConclusion::ConfirmValidDifference => DirectReconciliationConclusion::ConfirmValidDifference,
    }
}

/// 受控证据类型与领域证据类型的双向映射（1:1，wire 代码不变）。
impl From<ControlledEvidenceKind> for RequiredEvidenceKind {
    /// 将服务受控证据类型转换为领域证据类型。
    ///
    /// # 参数
    /// * `kind` - 服务受控证据类型
    ///
    /// # 返回
    /// 返回一一对应的领域证据类型。
    fn from(kind: ControlledEvidenceKind) -> Self {
        match kind {
            ControlledEvidenceKind::ExternalCaseResult => Self::ExternalCaseResult,
            ControlledEvidenceKind::BusinessObjectVerification => Self::BusinessObjectVerification,
            ControlledEvidenceKind::FinancialReconciliation => Self::FinancialReconciliation,
            ControlledEvidenceKind::CompensationResult => Self::CompensationResult,
            ControlledEvidenceKind::DistinctReview => Self::DistinctReview,
        }
    }
}

/// 领域证据类型到服务受控证据类型的映射（1:1，wire 代码不变）。
impl From<RequiredEvidenceKind> for ControlledEvidenceKind {
    /// 将领域证据类型转换为服务受控证据类型。
    ///
    /// # 参数
    /// * `kind` - 领域证据类型
    ///
    /// # 返回
    /// 返回一一对应的服务受控证据类型。
    fn from(kind: RequiredEvidenceKind) -> Self {
        match kind {
            RequiredEvidenceKind::ExternalCaseResult => Self::ExternalCaseResult,
            RequiredEvidenceKind::BusinessObjectVerification => Self::BusinessObjectVerification,
            RequiredEvidenceKind::FinancialReconciliation => Self::FinancialReconciliation,
            RequiredEvidenceKind::CompensationResult => Self::CompensationResult,
            RequiredEvidenceKind::DistinctReview => Self::DistinctReview,
        }
    }
}

/// 校验任务完成命令引用的策略身份、键与证据类型集合。
pub fn ensure_completion_policy(
    submitted_id: &str,
    submitted_version: u64,
    submitted_key: &EvidencePolicyKey,
    submitted_refs: &[ControlledEvidenceRef],
    expected: &ResolutionEvidencePolicyView,
) -> Result<()> {
    if submitted_id != expected.evidence_policy_id
        || submitted_version != expected.evidence_policy_version
        || submitted_key != &expected.key
    {
        return Err(Error::ConflictError(
            "终态证据策略已变化，请刷新后重试".to_string(),
        ));
    }
    kinds_subset(
        &domain_kinds(submitted_refs),
        &expected
            .required_evidence_kinds
            .iter()
            .map(|kind| RequiredEvidenceKind::from(*kind))
            .collect::<Vec<_>>(),
    )
}

/// 校验直接对账原因注册表身份、原因、结论与所需证据类型。
///
/// 注册原因与结论映射归领域，此处只做请求校验与错误映射。
pub fn ensure_direct_reason(
    registry_id: &str,
    registry_version: u64,
    registered_reason_id: &str,
    reason_code: DifferenceReasonCode,
    conclusion: DirectReconciliationConclusion,
    evidence_refs: &[ControlledEvidenceRef],
) -> Result<()> {
    let registry = domain_reason_registry();
    if registry_id != registry.id || registry_version != registry.version {
        return Err(Error::ConflictError(
            "对账原因注册表已变化，请刷新后重试".to_string(),
        ));
    }
    if registered_reason_id != reason_code.as_str() {
        return Err(Error::ValidationError("注册原因 ID 与原因代码不一致".to_string()));
    }
    let reason = registry
        .find(registered_reason_id)
        .ok_or_else(|| Error::ValidationError("对账原因未注册".to_string()))?;
    if dto_conclusion(reason.conclusion) != conclusion {
        return Err(Error::ValidationError("对账原因与结论不一致".to_string()));
    }
    kinds_subset(&domain_kinds(evidence_refs), reason.required)
}

/// 校验已提交证据类型覆盖全部必需类型（纯集合逻辑，类型要求归领域）。
///
/// # 参数
/// * `submitted` - 已提交的证据类型
/// * `required` - 必需的证据类型
///
/// # 返回
/// 全覆盖返回 `Ok(())`，否则返回业务错误。
///
/// # 错误
/// 存在未覆盖的必需类型时返回 `BusinessLogicError`。
fn kinds_subset(submitted: &[RequiredEvidenceKind], required: &[RequiredEvidenceKind]) -> Result<()> {
    if required.iter().all(|kind| submitted.contains(kind)) {
        return Ok(());
    }
    Err(Error::BusinessLogicError(
        "终态证据尚未满足固定策略要求".to_string(),
    ))
}

/// 逐条调用权威端口重验证据，并返回可持久化的稳定引用。
pub async fn verify_evidence_refs(
    authority: &dyn IntegrationEvidenceAuthority,
    subject: &EvidenceSubject,
    refs: &[ControlledEvidenceRef],
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<VerifiedEvidence>> {
    if refs.is_empty() {
        return Err(Error::ValidationError("必须提供受控证据引用".to_string()));
    }
    let mut verified = Vec::with_capacity(refs.len());
    for evidence in refs {
        verified.push(
            authority
                .verify_evidence(subject, evidence, actor_id, executor)
                .await?,
        );
    }
    Ok(verified)
}

/// 从验证结果派生单一稳定证据引用；多条以分号连接。
pub fn verified_reference(verified: &[VerifiedEvidence]) -> Result<String> {
    evidence_reference_grammar(EvidenceReferenceSet::try_from_canonical(
        verified
            .iter()
            .map(|evidence| evidence.canonical_reference.clone()),
    ))
    .map(EvidenceReferenceSet::into_wire)
}

/// 把领域证据 grammar 错误映射为既有 HTTP 400 校验错误。
///
/// # 参数
/// * `result` - 领域引用解析或集合构造结果
///
/// # 返回
/// 成功时返回领域值。
///
/// # 错误
/// 非法 grammar、空集合或超过 512 字节时返回 [`Error::ValidationError`]。
///
/// # 约束
/// 不重复实现 grammar；仅保留 wire 错误类别。
pub fn evidence_reference_grammar<T>(result: erp_core::Result<T>) -> Result<T> {
    result.map_err(|error| Error::ValidationError(error.to_string()))
}
#[cfg(test)]
mod tests {
    use super::{
        ensure_completion_policy, ensure_direct_reason, error_evidence_policy, reconciliation_reason_registry,
    };
    use crate::dto::{
        ControlledEvidenceKind, ControlledEvidenceRef, DifferenceReasonCode, DirectReconciliationConclusion,
    };
    use crate::entity::integration_ops::{
        ErrorClass, IntegrationErrorTask, IntegrationErrorTaskData, IntegrationErrorTaskId,
    };

    fn task() -> IntegrationErrorTask {
        IntegrationErrorTask::new(
            IntegrationErrorTaskId::new("task-1"),
            IntegrationErrorTaskData {
                message_id: Some(erp_core::ids::InboxMessageId::new("message-1")),
                business_object_id: None,
                error_class: ErrorClass::ResultUnknown,
                owner_role: Some("role-operations".to_string()),
                owner_user_id: None,
            },
        )
        .unwrap()
    }

    fn evidence(kind: ControlledEvidenceKind) -> ControlledEvidenceRef {
        ControlledEvidenceRef {
            kind,
            record_id: "inbox_message:message-1".to_string(),
            label: "result".to_string(),
        }
    }

    #[test]
    fn completion_policy_requires_exact_identity_key_and_kinds() {
        let expected = error_evidence_policy(&task());
        ensure_completion_policy(
            &expected.evidence_policy_id,
            expected.evidence_policy_version,
            &expected.key,
            &[evidence(ControlledEvidenceKind::ExternalCaseResult)],
            &expected,
        )
        .unwrap();
        assert!(ensure_completion_policy(
            "stale",
            expected.evidence_policy_version,
            &expected.key,
            &[evidence(ControlledEvidenceKind::ExternalCaseResult)],
            &expected,
        )
        .is_err());
    }

    #[test]
    fn direct_reason_registry_binds_reason_to_conclusion_and_evidence() {
        let registry = reconciliation_reason_registry();
        let evidence = ControlledEvidenceRef {
            kind: ControlledEvidenceKind::BusinessObjectVerification,
            record_id: "mall_order_fact:fact-1".to_string(),
            label: "attributed".to_string(),
        };
        ensure_direct_reason(
            &registry.reason_registry_id,
            registry.reason_registry_version,
            "SOURCE_CORRECTED_AND_REATTRIBUTED",
            DifferenceReasonCode::SourceCorrectedAndReattributed,
            DirectReconciliationConclusion::ConfirmValidDifference,
            std::slice::from_ref(&evidence),
        )
        .unwrap();
        assert!(ensure_direct_reason(
            &registry.reason_registry_id,
            registry.reason_registry_version,
            "SOURCE_CORRECTED_AND_REATTRIBUTED",
            DifferenceReasonCode::SourceCorrectedAndReattributed,
            DirectReconciliationConclusion::ConfirmNoError,
            &[evidence],
        )
        .is_err());
    }

    /// 生产代码（测试模块之前部分），供分层守卫断言，避免字面量自匹配。
    ///
    /// # 返回
    /// 返回去掉测试模块后的生产代码全文。
    fn production_source() -> &'static str {
        include_str!("evidence.rs")
            .split("mod tests {")
            .next()
            .expect("必须存在生产代码")
    }

    /// 分层守卫（INT-E21）：证据策略与原因注册表归领域，服务只做 view 映射。
    ///
    /// 锁定旧规则源（策略常量、类型集合、逐条集合校验与原因装配）已删除；
    /// 策略与注册表来自领域，服务只保留词汇映射与请求校验。
    #[test]
    fn evidence_tables_are_owned_by_domain() {
        let source = production_source();
        assert!(!source.contains("ERROR_EXTERNAL_RESULT"));
        assert!(!source.contains("ERROR_BUSINESS_REPAIR"));
        assert!(!source.contains("DIFFERENCE_REPAIR"));
        assert!(!source.contains("DIFFERENCE_COMPENSATION"));
        assert!(!source.contains("NO_ERROR_REVIEW"));
        assert!(!source.contains("fn ensure_required_kinds"));
        assert!(!source.contains("fn reason_view"));
        assert!(!source.contains("fn evidence_satisfies_policy"));
        assert!(source.contains("error_terminal_policy(task)"));
        assert!(source.contains("difference_terminal_policy(difference)"));
        assert!(source.contains("domain_reason_registry()"));
    }
}

#[cfg(test)]
mod authority_sequence_tests {
    use super::verify_evidence_refs;
    use crate::dto::{ControlledEvidenceKind, ControlledEvidenceRef};
    use crate::entity::integration_ops::CanonicalEvidenceReference;
    use crate::ports::evidence::{
        EvidenceFuture, EvidenceSubject, IntegrationEvidenceAuthority, OriginalResultFact, VerifiedEvidence,
    };
    use crate::Error;
    use persistence_core::Executor;
    use std::sync::Mutex;

    struct TestExecutor {
        _identity: u8,
    }
    impl Executor for TestExecutor {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            None
        }
    }
    #[derive(Default)]
    struct RecordingAuthority {
        calls: Mutex<Vec<(String, usize)>>,
        fail: Option<&'static str>,
    }
    impl IntegrationEvidenceAuthority for RecordingAuthority {
        fn query_original<'a>(
            &'a self,
            _: &'a EvidenceSubject,
            _: &'a mut dyn Executor,
        ) -> EvidenceFuture<'a, OriginalResultFact> {
            Box::pin(async { Err(Error::Internal("unexpected query".into())) })
        }
        fn replay_original<'a>(
            &'a self,
            _: &'a EvidenceSubject,
            _: &'a mut dyn Executor,
        ) -> EvidenceFuture<'a, String> {
            Box::pin(async { Err(Error::Internal("unexpected replay".into())) })
        }
        fn verify_reattribution<'a>(
            &'a self,
            _: &'a EvidenceSubject,
            _: &'a mut dyn Executor,
        ) -> EvidenceFuture<'a, String> {
            Box::pin(async { Err(Error::Internal("unexpected reattribution".into())) })
        }
        fn discover_evidence<'a>(
            &'a self,
            _: &'a EvidenceSubject,
            _: &'a mut dyn Executor,
        ) -> EvidenceFuture<'a, Vec<ControlledEvidenceRef>> {
            Box::pin(async { Err(Error::Internal("unexpected discover".into())) })
        }
        fn verify_evidence<'a>(
            &'a self,
            _: &'a EvidenceSubject,
            evidence: &'a ControlledEvidenceRef,
            _: &'a str,
            executor: &'a mut dyn Executor,
        ) -> EvidenceFuture<'a, VerifiedEvidence> {
            Box::pin(async move {
                self.calls.lock().unwrap().push((
                    evidence.record_id.clone(),
                    executor as *mut dyn Executor as *mut () as usize,
                ));
                if self.fail == Some(evidence.record_id.as_str()) {
                    return Err(Error::BusinessLogicError(evidence.record_id.clone()));
                }
                Ok(VerifiedEvidence {
                    reference: evidence.clone(),
                    canonical_reference: CanonicalEvidenceReference::verified(
                        "inbox_message",
                        &evidence.record_id,
                        Some(1),
                        "processed",
                    )?,
                })
            })
        }
    }
    fn subject() -> EvidenceSubject {
        EvidenceSubject {
            item_id: "task".into(),
            message_id: None,
            business_object_type: None,
            business_object_id: None,
            fact_references: vec![],
        }
    }
    fn refs() -> Vec<ControlledEvidenceRef> {
        ["first", "second", "third"]
            .into_iter()
            .map(|id| ControlledEvidenceRef {
                kind: ControlledEvidenceKind::ExternalCaseResult,
                record_id: id.into(),
                label: id.into(),
            })
            .collect()
    }
    #[tokio::test]
    async fn evidence_verification_keeps_request_order_and_executor() {
        let authority = RecordingAuthority::default();
        let mut executor = TestExecutor { _identity: 1 };
        let id = &mut executor as *mut TestExecutor as usize;
        let verified = verify_evidence_refs(&authority, &subject(), &refs(), "actor", &mut executor)
            .await
            .unwrap();
        assert_eq!(
            verified
                .iter()
                .map(|e| e.reference.record_id.as_str())
                .collect::<Vec<_>>(),
            ["first", "second", "third"]
        );
        assert_eq!(
            *authority.calls.lock().unwrap(),
            [("first".into(), id), ("second".into(), id), ("third".into(), id)]
        );
    }
    #[tokio::test]
    async fn evidence_verification_preserves_first_error_and_stops() {
        let authority = RecordingAuthority {
            fail: Some("second"),
            ..Default::default()
        };
        let mut executor = TestExecutor { _identity: 1 };
        let result = verify_evidence_refs(&authority, &subject(), &refs(), "actor", &mut executor).await;
        assert!(matches!(result,Err(Error::BusinessLogicError(message)) if message=="second"));
        assert_eq!(
            authority
                .calls
                .lock()
                .unwrap()
                .iter()
                .map(|(id, _)| id.as_str())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );
    }
    #[tokio::test]
    async fn empty_evidence_is_rejected_before_authority_reads() {
        let authority = RecordingAuthority::default();
        let mut executor = TestExecutor { _identity: 1 };
        let result = verify_evidence_refs(&authority, &subject(), &[], "actor", &mut executor).await;
        assert!(matches!(result,Err(Error::ValidationError(message)) if message=="必须提供受控证据引用"));
        assert!(authority.calls.lock().unwrap().is_empty());
    }
}
