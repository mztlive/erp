//! W29 固定证据策略、直接对账原因注册表与跨域权威事实只读端口。

use std::future::Future;
use std::pin::Pin;

use database::{
    Executor, IntegrationOpsExt, MallAfterSalesExt, MallOrderExt, ReturnsExt, SupplierFulfillmentExt,
};
use entities::integration_ops::{
    difference_terminal_policy, error_terminal_policy,
    reconciliation_reason_registry as domain_reason_registry, CanonicalEvidenceReference, DirectConclusion,
    EvidenceRecordRef, EvidenceReferenceSet, EvidenceSubjectBindings, InboxMessageStatus, InboxMessageUpdate,
    IntegrationErrorTask, MessageType, ReconciliationDifference, ReplayOriginalReference,
    RequiredEvidenceKind, ResolutionAction, TerminalEvidencePolicy,
};
use entities::mall_order::ProcessingStatus;
use entities::returns::{CustomerRefundStatus, SupplierRefundStatus};
use mongodb::Database;

use super::{
    ActionBlockerView, ControlledEvidenceKind, ControlledEvidenceRef, DifferenceReasonCode,
    DirectReconciliationConclusion, EvidencePolicyKey, ReconciliationReasonRegistryView,
    RegisteredReconciliationReasonView, ResolutionEvidencePolicyView, ReviewerSeparation,
};
use crate::errors::{Error, Result};

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
}

/// 已由权威仓储重验的证据。
#[derive(Debug, Clone)]
pub(super) struct VerifiedEvidence {
    /// 归一化后的客户端证据引用。
    pub reference: ControlledEvidenceRef,
    /// 可写入领域证据字段的稳定引用。
    pub canonical_reference: CanonicalEvidenceReference,
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
                    return Ok(OriginalResultFact::Terminal(
                        canonical_verified(
                            "inbox_message",
                            &message.base.id,
                            Some(message.base.version),
                            "processed",
                        )?
                        .into_wire(),
                    ));
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
            Ok(evidence_reference_grammar(ReplayOriginalReference::new(
                &message.base.id,
                message.base.version,
                &message.business_fact_key,
            ))?
            .into_wire())
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
            Ok(canonical_verified(
                "mall_order_fact",
                &fact.base.id,
                Some(fact.base.version),
                "attributed",
            )?
            .into_wire())
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
            let parsed = evidence_reference_grammar(EvidenceRecordRef::parse(&evidence.record_id))?;
            let canonical_reference = match parsed.kind() {
                "inbox_message" => {
                    if evidence.kind != ControlledEvidenceKind::ExternalCaseResult {
                        return kind_mismatch();
                    }
                    let message = self
                        .inbox_messages()
                        .find_by_id(parsed.id(), executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("证据引用的入站消息不存在".to_string()))?;
                    if message.status != InboxMessageStatus::Processed || message.processed_at.is_none() {
                        return Err(Error::BusinessLogicError(
                            "证据引用的入站消息尚未形成已处理终态".to_string(),
                        ));
                    }
                    ensure_association(subject, std::slice::from_ref(&message.base.id))?;
                    canonical_verified(
                        "inbox_message",
                        &message.base.id,
                        Some(message.base.version),
                        "processed",
                    )?
                }
                "mall_order_fact" => {
                    if evidence.kind != ControlledEvidenceKind::BusinessObjectVerification {
                        return kind_mismatch();
                    }
                    let fact = self
                        .mall_order_facts()
                        .find_by_id(parsed.id(), executor)
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
                    canonical_verified(
                        "mall_order_fact",
                        &fact.base.id,
                        Some(fact.base.version),
                        "attributed",
                    )?
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
                        .find_by_id(parsed.id(), executor)
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
                    canonical_verified(
                        "customer_refund",
                        &refund.base.id,
                        Some(refund.base.version),
                        "posted",
                    )?
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
                        .find_by_id(parsed.id(), executor)
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
                    canonical_verified(
                        "supplier_refund",
                        &refund.base.id,
                        Some(refund.base.version),
                        "posted",
                    )?
                }
                "mall_refund" => {
                    ensure_compensation_kind(evidence.kind)?;
                    let refund = self
                        .mall_refunds()
                        .find_by_id(parsed.id(), executor)
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
                    canonical_verified(
                        "mall_refund",
                        &refund.base.id,
                        Some(refund.base.version),
                        "succeeded",
                    )?
                }
                "supplier_refund_fact" => {
                    ensure_compensation_kind(evidence.kind)?;
                    let fact = self
                        .supplier_refund_facts()
                        .find_by_id(parsed.id(), executor)
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
                    canonical_verified(
                        "supplier_refund_fact",
                        &fact.base.id,
                        Some(fact.base.version),
                        "succeeded",
                    )?
                }
                "mall_balance_restoration" => {
                    ensure_compensation_kind(evidence.kind)?;
                    let restoration = self
                        .mall_balance_restorations()
                        .find_by_id(parsed.id(), executor)
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
                    canonical_verified(
                        "mall_balance_restoration",
                        &restoration.base.id,
                        Some(restoration.base.version),
                        "succeeded",
                    )?
                }
                "reconciliation_difference_resolution" => {
                    if evidence.kind != ControlledEvidenceKind::DistinctReview {
                        return kind_mismatch();
                    }
                    let record = self
                        .reconciliation_difference_resolutions()
                        .find_by_id(parsed.id(), executor)
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
                    canonical_verified(
                        "reconciliation_difference_resolution",
                        &record.base.id,
                        None,
                        "reviewed",
                    )?
                }
                _ => {
                    return Err(Error::BusinessLogicError(format!(
                        "证据对象类型 {} 尚未注册权威验证器",
                        parsed.kind()
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
                        )?);
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
                    )?);
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
                for record in records {
                    if record.resolution_action == ResolutionAction::AddEvidence
                        && record.evidence_reference.is_some()
                    {
                        evidence.push(controlled_ref(
                            ControlledEvidenceKind::DistinctReview,
                            "reconciliation_difference_resolution",
                            &record.id,
                            "差异独立复核记录",
                        )?);
                    }
                }
            }
            discover_compensation(self, subject, executor, &mut evidence).await?;
            Ok(evidence)
        })
    }
}

/// 返回错误任务当前固定证据策略视图（规则归领域，此处只做 view 映射）。
///
/// # 参数
/// * `task` - 集成错误任务
///
/// # 返回
/// 返回响应视图；策略身份、资金影响与类型要求来自领域策略。
pub(super) fn error_evidence_policy(task: &IntegrationErrorTask) -> ResolutionEvidencePolicyView {
    policy_view(&error_terminal_policy(task))
}

/// 返回对账差异当前固定证据策略视图（规则归领域，此处只做 view 映射）。
///
/// # 参数
/// * `difference` - 对账差异
///
/// # 返回
/// 返回响应视图；策略身份、资金影响与类型要求来自领域策略。
pub(super) fn difference_evidence_policy(
    difference: &ReconciliationDifference,
) -> ResolutionEvidencePolicyView {
    policy_view(&difference_terminal_policy(difference))
}

/// 返回无任务直接对账固定原因注册表视图（注册表归领域，此处只做 view 映射）。
///
/// # 返回
/// 返回响应视图；原因、结论与类型要求来自领域注册表。
pub(super) fn reconciliation_reason_registry() -> ReconciliationReasonRegistryView {
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
pub(super) fn domain_kinds(refs: &[ControlledEvidenceRef]) -> Vec<RequiredEvidenceKind> {
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
pub(super) fn blocker_view(blocker: &entities::integration_ops::ActionBlocker) -> ActionBlockerView {
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
pub(super) fn ensure_direct_reason(
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
    evidence_reference_grammar(EvidenceReferenceSet::try_from_canonical(
        verified
            .iter()
            .map(|evidence| evidence.canonical_reference.clone()),
    ))
    .map(EvidenceReferenceSet::into_wire)
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
    db.supplier_refund_facts()
        .exists_by_inbox_message(message_id, executor)
        .await
        .map_err(Into::into)
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
    let bindings = EvidenceSubjectBindings::new(
        subject.message_id.as_deref(),
        subject.business_object_id.as_deref(),
        &subject.fact_references,
    );
    let candidate = subject
        .business_object_id
        .as_deref()
        .filter(|_| {
            subject
                .business_object_type
                .as_deref()
                .is_none_or(|kind| kind.eq_ignore_ascii_case("mall_order_fact"))
        })
        .or_else(|| bindings.referenced_id("mall_order_fact"));
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
                push_compensation_refs(evidence, "customer_refund", &refund.base.id, "已过账客户退款")?;
                return Ok(());
            }
        }
    }
    if kind.is_empty() || kind == "supplier_refund" {
        if let Some(refund) = db.supplier_refunds().find_by_id(id, executor).await? {
            if refund.status == SupplierRefundStatus::Posted {
                push_compensation_refs(evidence, "supplier_refund", &refund.base.id, "已过账供应商退款")?;
                return Ok(());
            }
        }
    }
    if kind.is_empty() || kind == "mall_refund" {
        if let Some(refund) = db.mall_refunds().find_by_id(id, executor).await? {
            push_compensation_refs(evidence, "mall_refund", &refund.base.id, "商城退款成功事实")?;
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
            )?;
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
            )?;
        }
    }
    Ok(())
}

fn push_compensation_refs(
    refs: &mut Vec<ControlledEvidenceRef>,
    kind: &str,
    id: &str,
    label: &str,
) -> Result<()> {
    refs.push(controlled_ref(
        ControlledEvidenceKind::CompensationResult,
        kind,
        id,
        label,
    )?);
    refs.push(controlled_ref(
        ControlledEvidenceKind::FinancialReconciliation,
        kind,
        id,
        label,
    )?);
    Ok(())
}

/// 由权威对象类型与记录 ID 构造客户端证据引用。
///
/// # 参数
/// * `evidence_kind` - 受控证据类型
/// * `record_kind` - 权威对象类型
/// * `id` - 记录身份 ID
/// * `label` - 展示标签
///
/// # 返回
/// 返回 `type:id` 记录引用。
///
/// # 错误
/// 对象类型或 ID 不符合精确 grammar 时返回校验错误。
///
/// # 约束
/// 语法由 [`EvidenceRecordRef`] 独占；本函数只做 Service DTO 装配。
fn controlled_ref(
    evidence_kind: ControlledEvidenceKind,
    record_kind: &str,
    id: &str,
    label: &str,
) -> Result<ControlledEvidenceRef> {
    Ok(ControlledEvidenceRef {
        kind: evidence_kind,
        record_id: evidence_reference_grammar(EvidenceRecordRef::new(record_kind, id))?.to_string(),
        label: label.to_string(),
    })
}

fn ensure_association(subject: &EvidenceSubject, ids: &[String]) -> Result<()> {
    let bindings = EvidenceSubjectBindings::new(
        subject.message_id.as_deref(),
        subject.business_object_id.as_deref(),
        &subject.fact_references,
    );
    if bindings.associates_any(ids.iter().map(String::as_str)) {
        return Ok(());
    }
    Err(Error::ConflictError(
        "证据记录与当前业务项没有可验证的正式关联".to_string(),
    ))
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
fn evidence_reference_grammar<T>(result: entities::Result<T>) -> Result<T> {
    result.map_err(|error| Error::ValidationError(error.to_string()))
}

/// 由权威仓储事实构造 canonical 证据引用。
///
/// # 参数
/// * `kind` - 对象类型
/// * `id` - 记录身份 ID
/// * `version` - 正式版本；无版本形态传 `None`
/// * `status` - 终态或核验状态
///
/// # 返回
/// 返回可写入终态证据字段的 canonical 引用。
///
/// # 错误
/// 身份段不符合精确 grammar 时返回校验错误。
///
/// # 约束
/// 编码规则由 [`CanonicalEvidenceReference::verified`] 独占。
fn canonical_verified(
    kind: &str,
    id: &str,
    version: Option<u64>,
    status: &str,
) -> Result<CanonicalEvidenceReference> {
    evidence_reference_grammar(CanonicalEvidenceReference::verified(kind, id, version, status))
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

#[cfg(test)]
mod tests {
    use super::{
        ensure_completion_policy, ensure_direct_reason, error_evidence_policy, reconciliation_reason_registry,
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
