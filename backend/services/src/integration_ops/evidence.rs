//! W29 固定证据策略、直接对账原因注册表与跨域权威事实只读端口。

use std::future::Future;
use std::pin::Pin;

use database::{
    Executor, IntegrationOpsExt, MallAfterSalesExt, MallOrderExt, ReturnsExt, SupplierFulfillmentExt,
};
use entities::integration_ops::{
    InboxMessageStatus, InboxMessageUpdate, IntegrationErrorTask, MessageType, ReconciliationDifference,
    ResolutionAction,
};
use entities::mall_order::ProcessingStatus;
use entities::returns::{CustomerRefundStatus, SupplierRefundStatus};
use mongodb::{bson::doc, Database};

use super::{
    ControlledEvidenceKind, ControlledEvidenceRef, DifferenceReasonCode, DirectReconciliationConclusion,
    EvidencePolicyKey, ReconciliationReasonRegistryView, RegisteredReconciliationReasonView,
    ResolutionEvidencePolicyView, ReviewerSeparation,
};
use crate::errors::{Error, Result};

const ERROR_POLICY_ID: &str = "w29-error-terminal-evidence";
const DIFFERENCE_POLICY_ID: &str = "w29-difference-terminal-evidence";
const EVIDENCE_POLICY_VERSION: u64 = 1;
const REASON_REGISTRY_ID: &str = "w29-reconciliation-reasons";
const REASON_REGISTRY_VERSION: u64 = 1;

const ERROR_EXTERNAL_RESULT: &[ControlledEvidenceKind] = &[ControlledEvidenceKind::ExternalCaseResult];
const ERROR_BUSINESS_REPAIR: &[ControlledEvidenceKind] =
    &[ControlledEvidenceKind::BusinessObjectVerification];
const DIFFERENCE_REPAIR: &[ControlledEvidenceKind] = &[ControlledEvidenceKind::BusinessObjectVerification];
const DIFFERENCE_COMPENSATION: &[ControlledEvidenceKind] = &[
    ControlledEvidenceKind::CompensationResult,
    ControlledEvidenceKind::FinancialReconciliation,
];
const NO_ERROR_REVIEW: &[ControlledEvidenceKind] = &[
    ControlledEvidenceKind::BusinessObjectVerification,
    ControlledEvidenceKind::DistinctReview,
];

/// W29 当前业务项的证据上下文；只包含关联校验所需的稳定身份。
#[derive(Debug, Clone)]
pub(super) struct EvidenceSubject {
    /// W29 业务项 ID。
    pub item_id: String,
    /// 错误任务关联的入站消息。
    pub message_id: Option<String>,
    /// 差异对象类型。
    pub business_object_type: Option<String>,
    /// 错误任务或差异关联的业务对象。
    pub business_object_id: Option<String>,
    /// 差异两侧不可变事实引用。
    pub fact_references: Vec<String>,
}

impl EvidenceSubject {
    /// 从错误任务构造证据上下文。
    pub(super) fn error(task: &IntegrationErrorTask) -> Self {
        Self {
            item_id: task.base.id.clone(),
            message_id: task.message_id.as_ref().map(ToString::to_string),
            business_object_type: None,
            business_object_id: task.business_object_id.clone(),
            fact_references: Vec::new(),
        }
    }

    /// 从对账差异构造证据上下文。
    pub(super) fn difference(difference: &ReconciliationDifference) -> Self {
        Self {
            item_id: difference.base.id.clone(),
            message_id: None,
            business_object_type: Some(difference.business_object_type.clone()),
            business_object_id: Some(difference.business_object_id.clone()),
            fact_references: [
                difference.left_fact_reference.clone(),
                difference.right_fact_reference.clone(),
            ]
            .into_iter()
            .flatten()
            .collect(),
        }
    }

    fn is_associated_with(&self, ids: &[String]) -> bool {
        ids.iter().any(|id| {
            self.message_id.as_deref() == Some(id.as_str())
                || self.business_object_id.as_deref() == Some(id.as_str())
                || self
                    .fact_references
                    .iter()
                    .any(|reference| reference_mentions(reference, id))
        })
    }
}

/// 已由权威仓储重验的证据。
#[derive(Debug, Clone)]
pub(super) struct VerifiedEvidence {
    /// 归一化后的客户端证据引用。
    pub reference: ControlledEvidenceRef,
    /// 可写入领域证据字段的稳定引用。
    pub canonical_reference: String,
}

/// 查询原结果的服务端事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum OriginalResultFact {
    /// 已找到终态或正式修复事实。
    Terminal(String),
    /// 已在注册适配器内确认没有结果，可以安全重放。
    NoResult,
    /// 当前模型无法权威判断。
    Unknown,
}

type EvidenceFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

/// W29 跨域权威证据端口。
///
/// 新对象类型必须在实现中显式注册并校验状态与业务关联；默认分支失败关闭。
pub(super) trait IntegrationEvidenceAuthority: Send + Sync {
    /// 查询原动作的当前结果。
    fn query_original<'a>(
        &'a self,
        subject: &'a EvidenceSubject,
        executor: &'a mut dyn Executor,
    ) -> EvidenceFuture<'a, OriginalResultFact>;

    /// 沿服务器锁定的入站消息身份重新排队。
    fn replay_original<'a>(
        &'a self,
        subject: &'a EvidenceSubject,
        executor: &'a mut dyn Executor,
    ) -> EvidenceFuture<'a, String>;

    /// 验证既有归集事实已经进入终态。
    fn verify_reattribution<'a>(
        &'a self,
        subject: &'a EvidenceSubject,
        executor: &'a mut dyn Executor,
    ) -> EvidenceFuture<'a, String>;

    /// 重验单条受控证据的类型、存在性、终态与业务关联。
    fn verify_evidence<'a>(
        &'a self,
        subject: &'a EvidenceSubject,
        evidence: &'a ControlledEvidenceRef,
        actor_id: &'a str,
        executor: &'a mut dyn Executor,
    ) -> EvidenceFuture<'a, VerifiedEvidence>;

    /// 发现当前对象已经存在且可安全投影的权威证据。
    fn discover_evidence<'a>(
        &'a self,
        subject: &'a EvidenceSubject,
        executor: &'a mut dyn Executor,
    ) -> EvidenceFuture<'a, Vec<ControlledEvidenceRef>>;
}

impl IntegrationEvidenceAuthority for Database {
    fn query_original<'a>(
        &'a self,
        subject: &'a EvidenceSubject,
        executor: &'a mut dyn Executor,
    ) -> EvidenceFuture<'a, OriginalResultFact> {
        Box::pin(async move {
            if let Some(message_id) = subject.message_id.as_deref() {
                let message = self
                    .inbox_messages()
                    .find_by_id(message_id, executor)
                    .await?
                    .ok_or_else(|| Error::NotFound("关联入站消息不存在".to_string()))?;
                if message.status == InboxMessageStatus::Processed && message.processed_at.is_some() {
                    return Ok(OriginalResultFact::Terminal(format!(
                        "inbox_message:{}:v{}:processed",
                        message.base.id, message.base.version
                    )));
                }
                if replay_adapter_registered(message.message_type, message.payload_reference.as_deref())
                    && matches!(
                        message.status,
                        InboxMessageStatus::Failed | InboxMessageStatus::ToManual
                    )
                    && message.processed_at.is_none()
                    && !known_result_exists(self, message_id, executor).await?
                {
                    return Ok(OriginalResultFact::NoResult);
                }
            }
            let evidence = self.discover_evidence(subject, executor).await?;
            Ok(evidence.first().map_or(OriginalResultFact::Unknown, |evidence| {
                OriginalResultFact::Terminal(evidence.record_id.clone())
            }))
        })
    }

    fn replay_original<'a>(
        &'a self,
        subject: &'a EvidenceSubject,
        executor: &'a mut dyn Executor,
    ) -> EvidenceFuture<'a, String> {
        Box::pin(async move {
            let message_id = subject
                .message_id
                .as_deref()
                .ok_or_else(|| Error::BusinessLogicError("当前业务项没有可重放的原入站消息".to_string()))?;
            let mut message = self
                .inbox_messages()
                .find_by_id(message_id, executor)
                .await?
                .ok_or_else(|| Error::NotFound("关联入站消息不存在".to_string()))?;
            if !replay_adapter_registered(message.message_type, message.payload_reference.as_deref())
                || !matches!(
                    message.status,
                    InboxMessageStatus::Failed | InboxMessageStatus::ToManual
                )
                || message.processed_at.is_some()
                || known_result_exists(self, message_id, executor).await?
            {
                return Err(Error::BusinessLogicError(
                    "原动作当前不满足无结果且可安全重放条件".to_string(),
                ));
            }
            message.update(InboxMessageUpdate {
                status: Some(InboxMessageStatus::Received),
                processed_at: None,
            })?;
            self.inbox_messages().update(&mut message, executor).await?;
            Ok(format!(
                "inbox_message:{}:v{}:requeued;business_fact_key:{}",
                message.base.id, message.base.version, message.business_fact_key
            ))
        })
    }

    fn verify_reattribution<'a>(
        &'a self,
        subject: &'a EvidenceSubject,
        executor: &'a mut dyn Executor,
    ) -> EvidenceFuture<'a, String> {
        Box::pin(async move {
            let fact = find_subject_mall_fact(self, subject, executor)
                .await?
                .ok_or_else(|| {
                    Error::BusinessLogicError("当前对象类型没有已注册的重新归集事实".to_string())
                })?;
            if fact.processing_status != ProcessingStatus::Attributed {
                return Err(Error::BusinessLogicError(
                    "关联商城事实尚未完成重新归集".to_string(),
                ));
            }
            Ok(format!(
                "mall_order_fact:{}:v{}:attributed",
                fact.base.id, fact.base.version
            ))
        })
    }

    fn verify_evidence<'a>(
        &'a self,
        subject: &'a EvidenceSubject,
        evidence: &'a ControlledEvidenceRef,
        actor_id: &'a str,
        executor: &'a mut dyn Executor,
    ) -> EvidenceFuture<'a, VerifiedEvidence> {
        Box::pin(async move {
            let parsed = EvidenceRecordRef::parse(&evidence.record_id)?;
            let canonical_reference = match parsed.kind {
                "inbox_message" => {
                    if evidence.kind != ControlledEvidenceKind::ExternalCaseResult {
                        return kind_mismatch();
                    }
                    let message = self
                        .inbox_messages()
                        .find_by_id(parsed.id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("证据引用的入站消息不存在".to_string()))?;
                    if message.status != InboxMessageStatus::Processed || message.processed_at.is_none() {
                        return Err(Error::BusinessLogicError(
                            "证据引用的入站消息尚未形成已处理终态".to_string(),
                        ));
                    }
                    ensure_association(subject, std::slice::from_ref(&message.base.id))?;
                    format!(
                        "inbox_message:{}:v{}:processed",
                        message.base.id, message.base.version
                    )
                }
                "mall_order_fact" => {
                    if evidence.kind != ControlledEvidenceKind::BusinessObjectVerification {
                        return kind_mismatch();
                    }
                    let fact = self
                        .mall_order_facts()
                        .find_by_id(parsed.id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("证据引用的商城事实不存在".to_string()))?;
                    if fact.processing_status != ProcessingStatus::Attributed {
                        return Err(Error::BusinessLogicError(
                            "证据引用的商城事实尚未完成归集".to_string(),
                        ));
                    }
                    ensure_association(
                        subject,
                        &[
                            fact.base.id.clone(),
                            fact.inbox_message_id.to_string(),
                            fact.business_fact_key.clone(),
                        ],
                    )?;
                    format!(
                        "mall_order_fact:{}:v{}:attributed",
                        fact.base.id, fact.base.version
                    )
                }
                "customer_refund" => {
                    if !matches!(
                        evidence.kind,
                        ControlledEvidenceKind::CompensationResult
                            | ControlledEvidenceKind::FinancialReconciliation
                            | ControlledEvidenceKind::DistinctReview
                    ) {
                        return kind_mismatch();
                    }
                    let refund = self
                        .customer_refunds()
                        .find_by_id(parsed.id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("证据引用的客户退款不存在".to_string()))?;
                    if refund.status != CustomerRefundStatus::Posted {
                        return Err(Error::BusinessLogicError("客户退款尚未过账".to_string()));
                    }
                    if evidence.kind == ControlledEvidenceKind::DistinctReview
                        && refund.reviewed_by == actor_id
                    {
                        return Err(Error::BusinessLogicError(
                            "独立复核人不得是当前处理人".to_string(),
                        ));
                    }
                    ensure_association(
                        subject,
                        &refund_association_ids(
                            &refund.base.id,
                            refund.sales_return_case_id.as_ref().map(ToString::to_string),
                            refund.original_receipt_id.as_ref().map(ToString::to_string),
                            refund
                                .original_receivable_entry_id
                                .as_ref()
                                .map(ToString::to_string),
                        ),
                    )?;
                    format!(
                        "customer_refund:{}:v{}:posted",
                        refund.base.id, refund.base.version
                    )
                }
                "supplier_refund" => {
                    if !matches!(
                        evidence.kind,
                        ControlledEvidenceKind::CompensationResult
                            | ControlledEvidenceKind::FinancialReconciliation
                            | ControlledEvidenceKind::DistinctReview
                    ) {
                        return kind_mismatch();
                    }
                    let refund = self
                        .supplier_refunds()
                        .find_by_id(parsed.id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("证据引用的供应商退款不存在".to_string()))?;
                    if refund.status != SupplierRefundStatus::Posted {
                        return Err(Error::BusinessLogicError("供应商退款尚未过账".to_string()));
                    }
                    if evidence.kind == ControlledEvidenceKind::DistinctReview
                        && refund.reviewed_by == actor_id
                    {
                        return Err(Error::BusinessLogicError(
                            "独立复核人不得是当前处理人".to_string(),
                        ));
                    }
                    ensure_association(
                        subject,
                        &refund_association_ids(
                            &refund.base.id,
                            refund.purchase_return_order_id.as_ref().map(ToString::to_string),
                            refund.original_payment_id.as_ref().map(ToString::to_string),
                            refund.original_payable_entry_id.as_ref().map(ToString::to_string),
                        ),
                    )?;
                    format!(
                        "supplier_refund:{}:v{}:posted",
                        refund.base.id, refund.base.version
                    )
                }
                "mall_refund" => {
                    ensure_compensation_kind(evidence.kind)?;
                    let refund = self
                        .mall_refunds()
                        .find_by_id(parsed.id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("证据引用的商城退款不存在".to_string()))?;
                    ensure_association(
                        subject,
                        &[
                            refund.base.id.clone(),
                            refund.mall_order_fact_id.to_string(),
                            refund.after_sales_request_id.to_string(),
                            refund.mall_order_id.to_string(),
                        ],
                    )?;
                    format!(
                        "mall_refund:{}:v{}:succeeded",
                        refund.base.id, refund.base.version
                    )
                }
                "supplier_refund_fact" => {
                    ensure_compensation_kind(evidence.kind)?;
                    let fact = self
                        .supplier_refund_facts()
                        .find_by_id(parsed.id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("证据引用的供应商退款事实不存在".to_string()))?;
                    ensure_association(
                        subject,
                        &[
                            fact.base.id.clone(),
                            fact.inbox_message_id.to_string(),
                            fact.supplier_fulfillment_order_id.to_string(),
                            fact.source_event_id.clone(),
                        ],
                    )?;
                    format!(
                        "supplier_refund_fact:{}:v{}:succeeded",
                        fact.base.id, fact.base.version
                    )
                }
                "mall_balance_restoration" => {
                    ensure_compensation_kind(evidence.kind)?;
                    let restoration = self
                        .mall_balance_restorations()
                        .find_by_id(parsed.id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("证据引用的余额恢复事实不存在".to_string()))?;
                    ensure_association(
                        subject,
                        &[
                            restoration.base.id.clone(),
                            restoration.mall_order_fact_id.to_string(),
                            restoration.after_sales_request_id.to_string(),
                            restoration.mall_refund_id.to_string(),
                        ],
                    )?;
                    format!(
                        "mall_balance_restoration:{}:v{}:succeeded",
                        restoration.base.id, restoration.base.version
                    )
                }
                "reconciliation_difference_resolution" => {
                    if evidence.kind != ControlledEvidenceKind::DistinctReview {
                        return kind_mismatch();
                    }
                    let record = self
                        .reconciliation_difference_resolutions()
                        .find_by_id(parsed.id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("证据引用的差异复核记录不存在".to_string()))?;
                    if record.reconciliation_difference_id.to_string() != subject.item_id
                        || record.resolution_action != ResolutionAction::AddEvidence
                        || record.evidence_reference.is_none()
                        || record.handled_by == actor_id
                    {
                        return Err(Error::BusinessLogicError(
                            "差异复核记录不满足同一差异、已补证且岗位分离要求".to_string(),
                        ));
                    }
                    format!("reconciliation_difference_resolution:{}:reviewed", record.base.id)
                }
                _ => {
                    return Err(Error::BusinessLogicError(format!(
                        "证据对象类型 {} 尚未注册权威验证器",
                        parsed.kind
                    )))
                }
            };
            Ok(VerifiedEvidence {
                reference: ControlledEvidenceRef {
                    kind: evidence.kind,
                    record_id: evidence.record_id.trim().to_string(),
                    label: evidence.label.trim().to_string(),
                },
                canonical_reference,
            })
        })
    }

    fn discover_evidence<'a>(
        &'a self,
        subject: &'a EvidenceSubject,
        executor: &'a mut dyn Executor,
    ) -> EvidenceFuture<'a, Vec<ControlledEvidenceRef>> {
        Box::pin(async move {
            let mut evidence = Vec::new();
            if let Some(message_id) = subject.message_id.as_deref() {
                if let Some(message) = self.inbox_messages().find_by_id(message_id, executor).await? {
                    if message.status == InboxMessageStatus::Processed && message.processed_at.is_some() {
                        evidence.push(controlled_ref(
                            ControlledEvidenceKind::ExternalCaseResult,
                            "inbox_message",
                            &message.base.id,
                            "已处理入站结果",
                        ));
                    }
                }
            }
            if let Some(fact) = find_subject_mall_fact(self, subject, executor).await? {
                if fact.processing_status == ProcessingStatus::Attributed {
                    evidence.push(controlled_ref(
                        ControlledEvidenceKind::BusinessObjectVerification,
                        "mall_order_fact",
                        &fact.base.id,
                        "已归集商城事实",
                    ));
                }
            }
            if subject.business_object_type.as_deref() == Some("reconciliation_difference")
                || !subject.fact_references.is_empty()
            {
                let records = self
                    .reconciliation_difference_resolutions()
                    .search_resolutions(
                        &entities::ids::ReconciliationDifferenceId::new(subject.item_id.clone()),
                        executor,
                    )
                    .await?;
                evidence.extend(records.into_iter().filter_map(|record| {
                    (record.resolution_action == ResolutionAction::AddEvidence
                        && record.evidence_reference.is_some())
                    .then(|| {
                        controlled_ref(
                            ControlledEvidenceKind::DistinctReview,
                            "reconciliation_difference_resolution",
                            &record.id,
                            "差异独立复核记录",
                        )
                    })
                }));
            }
            discover_compensation(self, subject, executor, &mut evidence).await?;
            Ok(evidence)
        })
    }
}

/// 返回错误任务当前固定证据策略。
pub(super) fn error_evidence_policy(task: &IntegrationErrorTask) -> ResolutionEvidencePolicyView {
    let required = if task.message_id.is_some() {
        ERROR_EXTERNAL_RESULT
    } else {
        ERROR_BUSINESS_REPAIR
    };
    ResolutionEvidencePolicyView {
        evidence_policy_id: ERROR_POLICY_ID.to_string(),
        evidence_policy_version: EVIDENCE_POLICY_VERSION,
        key: EvidencePolicyKey {
            error_type: task.error_class.as_str().to_string(),
            funds_impact: "NONE".to_string(),
        },
        required_evidence_kinds: required.to_vec(),
        reviewer_separation: ReviewerSeparation::None,
    }
}

/// 返回对账差异当前固定证据策略。
pub(super) fn difference_evidence_policy(
    difference: &ReconciliationDifference,
) -> ResolutionEvidencePolicyView {
    let financial = has_financial_impact(&difference.difference_type);
    ResolutionEvidencePolicyView {
        evidence_policy_id: DIFFERENCE_POLICY_ID.to_string(),
        evidence_policy_version: EVIDENCE_POLICY_VERSION,
        key: EvidencePolicyKey {
            error_type: difference.difference_type.clone(),
            funds_impact: if financial { "POTENTIAL" } else { "NONE" }.to_string(),
        },
        required_evidence_kinds: if financial {
            DIFFERENCE_COMPENSATION.to_vec()
        } else {
            DIFFERENCE_REPAIR.to_vec()
        },
        reviewer_separation: ReviewerSeparation::None,
    }
}

/// 返回无任务直接对账固定原因注册表。
pub(super) fn reconciliation_reason_registry() -> ReconciliationReasonRegistryView {
    ReconciliationReasonRegistryView {
        reason_registry_id: REASON_REGISTRY_ID.to_string(),
        reason_registry_version: REASON_REGISTRY_VERSION,
        registered_reasons: vec![
            reason_view(
                DifferenceReasonCode::SourceCorrectedAndReattributed,
                DirectReconciliationConclusion::ConfirmValidDifference,
                "来源已更正并重新归集",
                DIFFERENCE_REPAIR,
            ),
            reason_view(
                DifferenceReasonCode::BusinessConfirmedNoError,
                DirectReconciliationConclusion::ConfirmNoError,
                "业务确认无误",
                NO_ERROR_REVIEW,
            ),
            reason_view(
                DifferenceReasonCode::CompensationClosed,
                DirectReconciliationConclusion::ConfirmValidDifference,
                "补偿已闭环",
                DIFFERENCE_COMPENSATION,
            ),
        ],
    }
}

/// 校验任务完成命令引用的策略身份、键与证据类型集合。
pub(super) fn ensure_completion_policy(
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
    ensure_required_kinds(submitted_refs, &expected.required_evidence_kinds)
}

/// 判断当前已发现证据是否满足固定策略的类型集合。
pub(super) fn evidence_satisfies_policy(
    refs: &[ControlledEvidenceRef],
    policy: &ResolutionEvidencePolicyView,
) -> bool {
    ensure_required_kinds(refs, &policy.required_evidence_kinds).is_ok()
}

/// 判断错误任务最近一次动作是否已由服务端确认原动作无结果。
pub(super) fn prior_query_confirmed_no_result(task: &IntegrationErrorTask) -> bool {
    task.last_attempt_summary.as_deref().is_some_and(|summary| {
        summary.contains("w29_action=QUERY_ORIGINAL_RESULT") && summary.contains("outcome=NoResultConfirmed")
    })
}

/// 校验直接对账原因注册表身份、原因、结论与所需证据类型。
#[allow(clippy::too_many_arguments)]
pub(super) fn ensure_direct_reason(
    registry_id: &str,
    registry_version: u64,
    registered_reason_id: &str,
    reason_code: DifferenceReasonCode,
    conclusion: DirectReconciliationConclusion,
    evidence_refs: &[ControlledEvidenceRef],
) -> Result<()> {
    let registry = reconciliation_reason_registry();
    if registry_id != registry.reason_registry_id || registry_version != registry.reason_registry_version {
        return Err(Error::ConflictError(
            "对账原因注册表已变化，请刷新后重试".to_string(),
        ));
    }
    if registered_reason_id != reason_code.as_str() {
        return Err(Error::ValidationError("注册原因 ID 与原因代码不一致".to_string()));
    }
    let reason = registry
        .registered_reasons
        .iter()
        .find(|candidate| candidate.registered_reason_id == registered_reason_id)
        .ok_or_else(|| Error::ValidationError("对账原因未注册".to_string()))?;
    if reason.conclusion != conclusion {
        return Err(Error::ValidationError("对账原因与结论不一致".to_string()));
    }
    ensure_required_kinds(evidence_refs, &reason.required_evidence_kinds)
}

/// 逐条调用权威端口重验证据，并返回可持久化的稳定引用。
pub(super) async fn verify_evidence_refs(
    authority: &impl IntegrationEvidenceAuthority,
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
pub(super) fn verified_reference(verified: &[VerifiedEvidence]) -> Result<String> {
    let reference = verified
        .iter()
        .map(|evidence| evidence.canonical_reference.as_str())
        .collect::<Vec<_>>()
        .join(";");
    if reference.is_empty() || reference.len() > 512 {
        return Err(Error::ValidationError("终态证据引用为空或过长".to_string()));
    }
    Ok(reference)
}

/// 根据已验证证据选择错误任务解决方式。
pub(super) fn resolution_type(verified: &[VerifiedEvidence]) -> entities::integration_ops::ResolutionType {
    if verified
        .iter()
        .any(|evidence| evidence.reference.kind == ControlledEvidenceKind::CompensationResult)
    {
        entities::integration_ops::ResolutionType::Compensate
    } else if verified
        .iter()
        .any(|evidence| evidence.reference.kind == ControlledEvidenceKind::BusinessObjectVerification)
    {
        entities::integration_ops::ResolutionType::FixMapping
    } else {
        entities::integration_ops::ResolutionType::QueryConfirm
    }
}

fn ensure_required_kinds(
    submitted_refs: &[ControlledEvidenceRef],
    required: &[ControlledEvidenceKind],
) -> Result<()> {
    if required
        .iter()
        .any(|kind| !submitted_refs.iter().any(|evidence| evidence.kind == *kind))
    {
        return Err(Error::BusinessLogicError(
            "终态证据尚未满足固定策略要求".to_string(),
        ));
    }
    Ok(())
}

fn reason_view(
    code: DifferenceReasonCode,
    conclusion: DirectReconciliationConclusion,
    label: &str,
    required: &[ControlledEvidenceKind],
) -> RegisteredReconciliationReasonView {
    RegisteredReconciliationReasonView {
        registered_reason_id: code.as_str().to_string(),
        registered_reason_version: REASON_REGISTRY_VERSION,
        conclusion,
        label: label.to_string(),
        required_evidence_kinds: required.to_vec(),
    }
}

fn has_financial_impact(difference_type: &str) -> bool {
    let value = difference_type.to_ascii_lowercase();
    [
        "amount",
        "fund",
        "payment",
        "refund",
        "balance",
        "receivable",
        "payable",
    ]
    .iter()
    .any(|keyword| value.contains(keyword))
}

async fn known_result_exists(db: &Database, message_id: &str, executor: &mut dyn Executor) -> Result<bool> {
    if db
        .mall_order_facts()
        .find_by_inbox_message(
            &entities::ids::InboxMessageId::new(message_id.to_string()),
            executor,
        )
        .await?
        .is_some()
    {
        return Ok(true);
    }
    Ok(!db
        .supplier_refund_facts()
        .find_many(doc! { "inbox_message_id": message_id }, executor)
        .await?
        .is_empty())
}

fn replay_adapter_registered(message_type: MessageType, payload_reference: Option<&str>) -> bool {
    matches!(
        message_type,
        MessageType::PaymentSucceeded
            | MessageType::OrderCanceled
            | MessageType::RefundSucceeded
            | MessageType::OrderCompleted
            | MessageType::CardBalanceRestored
    ) || (message_type == MessageType::SupplierCallback
        && payload_reference.is_some_and(|value| value.starts_with("supplier-refund-order:")))
}

async fn find_subject_mall_fact(
    db: &Database,
    subject: &EvidenceSubject,
    executor: &mut dyn Executor,
) -> Result<Option<entities::mall_order::MallOrderFact>> {
    if let Some(message_id) = subject.message_id.as_deref() {
        if let Some(fact) = db
            .mall_order_facts()
            .find_by_inbox_message(
                &entities::ids::InboxMessageId::new(message_id.to_string()),
                executor,
            )
            .await?
        {
            return Ok(Some(fact));
        }
    }
    let candidate = subject
        .business_object_id
        .as_deref()
        .filter(|_| {
            subject
                .business_object_type
                .as_deref()
                .is_none_or(|kind| kind.eq_ignore_ascii_case("mall_order_fact"))
        })
        .or_else(|| {
            subject
                .fact_references
                .iter()
                .find_map(|reference| referenced_id(reference, "mall_order_fact"))
        });
    match candidate {
        Some(id) => db
            .mall_order_facts()
            .find_by_id(id, executor)
            .await
            .map_err(Into::into),
        None => Ok(None),
    }
}

async fn discover_compensation(
    db: &Database,
    subject: &EvidenceSubject,
    executor: &mut dyn Executor,
    evidence: &mut Vec<ControlledEvidenceRef>,
) -> Result<()> {
    let Some(id) = subject.business_object_id.as_deref() else {
        return Ok(());
    };
    let kind = subject
        .business_object_type
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if kind.is_empty() || kind == "customer_refund" {
        if let Some(refund) = db.customer_refunds().find_by_id(id, executor).await? {
            if refund.status == CustomerRefundStatus::Posted {
                push_compensation_refs(evidence, "customer_refund", &refund.base.id, "已过账客户退款");
                return Ok(());
            }
        }
    }
    if kind.is_empty() || kind == "supplier_refund" {
        if let Some(refund) = db.supplier_refunds().find_by_id(id, executor).await? {
            if refund.status == SupplierRefundStatus::Posted {
                push_compensation_refs(evidence, "supplier_refund", &refund.base.id, "已过账供应商退款");
                return Ok(());
            }
        }
    }
    if kind.is_empty() || kind == "mall_refund" {
        if let Some(refund) = db.mall_refunds().find_by_id(id, executor).await? {
            push_compensation_refs(evidence, "mall_refund", &refund.base.id, "商城退款成功事实");
            return Ok(());
        }
    }
    if kind.is_empty() || kind == "supplier_refund_fact" {
        if let Some(refund) = db.supplier_refund_facts().find_by_id(id, executor).await? {
            push_compensation_refs(
                evidence,
                "supplier_refund_fact",
                &refund.base.id,
                "供应商退款成功事实",
            );
            return Ok(());
        }
    }
    if kind.is_empty() || kind == "mall_balance_restoration" {
        if let Some(restoration) = db.mall_balance_restorations().find_by_id(id, executor).await? {
            push_compensation_refs(
                evidence,
                "mall_balance_restoration",
                &restoration.base.id,
                "余额恢复成功事实",
            );
        }
    }
    Ok(())
}

fn push_compensation_refs(refs: &mut Vec<ControlledEvidenceRef>, kind: &str, id: &str, label: &str) {
    refs.push(controlled_ref(
        ControlledEvidenceKind::CompensationResult,
        kind,
        id,
        label,
    ));
    refs.push(controlled_ref(
        ControlledEvidenceKind::FinancialReconciliation,
        kind,
        id,
        label,
    ));
}

fn controlled_ref(
    evidence_kind: ControlledEvidenceKind,
    record_kind: &str,
    id: &str,
    label: &str,
) -> ControlledEvidenceRef {
    ControlledEvidenceRef {
        kind: evidence_kind,
        record_id: format!("{record_kind}:{id}"),
        label: label.to_string(),
    }
}

fn ensure_association(subject: &EvidenceSubject, ids: &[String]) -> Result<()> {
    if subject.is_associated_with(ids) {
        return Ok(());
    }
    Err(Error::ConflictError(
        "证据记录与当前业务项没有可验证的正式关联".to_string(),
    ))
}

fn ensure_compensation_kind(kind: ControlledEvidenceKind) -> Result<()> {
    if matches!(
        kind,
        ControlledEvidenceKind::CompensationResult | ControlledEvidenceKind::FinancialReconciliation
    ) {
        return Ok(());
    }
    kind_mismatch()
}

fn kind_mismatch<T>() -> Result<T> {
    Err(Error::ValidationError("证据类型与引用对象类型不匹配".to_string()))
}

fn refund_association_ids(
    id: &str,
    case_id: Option<String>,
    original_document_id: Option<String>,
    original_entry_id: Option<String>,
) -> Vec<String> {
    std::iter::once(id.to_string())
        .chain(case_id)
        .chain(original_document_id)
        .chain(original_entry_id)
        .collect()
}

struct EvidenceRecordRef<'a> {
    kind: &'a str,
    id: &'a str,
}

impl<'a> EvidenceRecordRef<'a> {
    fn parse(value: &'a str) -> Result<Self> {
        let value = value.trim();
        let (kind, id) = value
            .split_once(':')
            .ok_or_else(|| Error::ValidationError("证据记录 ID 必须使用 type:id 格式".to_string()))?;
        if kind.is_empty() || id.is_empty() || id.contains(':') {
            return Err(Error::ValidationError(
                "证据记录 ID 必须使用唯一的 type:id 格式".to_string(),
            ));
        }
        Ok(Self { kind, id })
    }
}

fn referenced_id<'a>(reference: &'a str, kind: &str) -> Option<&'a str> {
    let reference = reference.trim();
    reference
        .strip_prefix(&format!("{kind}://"))
        .or_else(|| reference.strip_prefix(&format!("{kind}:")))
        .filter(|id| !id.is_empty() && !id.contains([';', ',', '|']))
}

fn reference_mentions(reference: &str, id: &str) -> bool {
    reference == id
        || reference
            .split(|character: char| {
                character.is_whitespace() || matches!(character, ':' | '/' | ';' | '|' | ',' | '=')
            })
            .any(|token| token == id)
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_completion_policy, ensure_direct_reason, error_evidence_policy,
        reconciliation_reason_registry, EvidenceRecordRef,
    };
    use crate::integration_ops::{
        ControlledEvidenceKind, ControlledEvidenceRef, DifferenceReasonCode, DirectReconciliationConclusion,
    };
    use entities::integration_ops::{
        ErrorClass, IntegrationErrorTask, IntegrationErrorTaskData, IntegrationErrorTaskId,
    };

    fn task() -> IntegrationErrorTask {
        IntegrationErrorTask::new(
            IntegrationErrorTaskId::new("task-1"),
            IntegrationErrorTaskData {
                message_id: Some(entities::ids::InboxMessageId::new("message-1")),
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
    fn canonical_reference_parser_rejects_unregistered_nested_identity() {
        assert!(EvidenceRecordRef::parse("inbox_message:message-1").is_ok());
        assert!(EvidenceRecordRef::parse("inbox_message:message-1:forged").is_err());
        assert!(EvidenceRecordRef::parse("message-1").is_err());
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
}
