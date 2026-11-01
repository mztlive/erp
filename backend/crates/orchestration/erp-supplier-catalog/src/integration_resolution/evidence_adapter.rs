//! 集成证据消费方 Port 的唯一跨域 Mongo 实现；所有操作复用调用方 Executor。
use erp_integration::dto::{ControlledEvidenceKind, ControlledEvidenceRef};
use erp_integration::entity::integration_ops::{
    CanonicalEvidenceReference, EvidenceRecordRef, EvidenceSubjectBindings, InboxMessageStatus,
    InboxMessageUpdate, MessageType, ReplayOriginalReference, ResolutionAction,
};
use erp_integration::ports::evidence::{
    EvidenceFuture, EvidenceSubject, IntegrationEvidenceAuthority, OriginalResultFact, VerifiedEvidence,
};
use erp_integration::repository::IntegrationOpsExt;
use erp_integration::repository::prelude::*;
use erp_integration::service::evidence::evidence_reference_grammar;
use erp_integration::{Error, Result};
use erp_returns::entity::returns::{CustomerRefundStatus, SupplierRefundStatus};
use erp_returns::repository::ReturnsExt;
use erp_supply::repository::SupplierFulfillmentExt;
use erp_supply::repository::prelude::*;
use mongodb::Database;
use persistence_core::Executor;
/// 跨域权威证据适配器；由组合根创建一份并注入命令及详情。
pub struct MongoIntegrationEvidenceAuthority {
    db: Database,
}
impl MongoIntegrationEvidenceAuthority {
    /// 保存组合根数据库；方法不自开事务或外发请求。
    ///
    /// # 参数
    /// * `db` - 组合根数据库。
    ///
    /// # 返回
    /// 证据适配器。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

impl IntegrationEvidenceAuthority for MongoIntegrationEvidenceAuthority {
    fn query_original<'a>(
        &'a self,
        subject: &'a EvidenceSubject,
        executor: &'a mut dyn Executor,
    ) -> EvidenceFuture<'a, OriginalResultFact> {
        Box::pin(async move {
            if let Some(message_id) = subject.message_id.as_deref() {
                let message = self
                    .db
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
                    && matches!(message.status, InboxMessageStatus::Failed | InboxMessageStatus::ToManual)
                    && message.processed_at.is_none()
                    && !known_result_exists(&self.db, message_id, executor).await?
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
                .db
                .inbox_messages()
                .find_by_id(message_id, executor)
                .await?
                .ok_or_else(|| Error::NotFound("关联入站消息不存在".to_string()))?;
            if !replay_adapter_registered(message.message_type, message.payload_reference.as_deref())
                || !matches!(message.status, InboxMessageStatus::Failed | InboxMessageStatus::ToManual)
                || message.processed_at.is_some()
                || known_result_exists(&self.db, message_id, executor).await?
            {
                return Err(Error::BusinessLogicError("原动作当前不满足无结果且可安全重放条件".to_string()));
            }
            message.update(InboxMessageUpdate {
                status: Some(InboxMessageStatus::Received),
                processed_at: None,
            })?;
            self.db.inbox_messages().update(&mut message, executor).await?;
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
        _subject: &'a EvidenceSubject,
        _executor: &'a mut dyn Executor,
    ) -> EvidenceFuture<'a, String> {
        Box::pin(async move {
            // 商城事实已移除，当前无已注册的重新归集事实。
            Err(Error::BusinessLogicError("当前对象类型没有已注册的重新归集事实".to_string()))
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
            let canonical_reference =
                self.verify_record(subject, &parsed, evidence.kind, actor_id, executor).await?;
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
            if let Some(message_id) = subject.message_id.as_deref()
                && let Some(message) = self.db.inbox_messages().find_by_id(message_id, executor).await?
                && message.status == InboxMessageStatus::Processed
                && message.processed_at.is_some()
            {
                evidence.push(controlled_ref(
                    ControlledEvidenceKind::ExternalCaseResult,
                    "inbox_message",
                    &message.base.id,
                    "已处理入站结果",
                )?);
            }
            if subject.business_object_type.as_deref() == Some("reconciliation_difference")
                || !subject.fact_references.is_empty()
            {
                let records = self
                    .db
                    .reconciliation_difference_resolutions()
                    .search_resolutions(
                        &erp_core::ids::ReconciliationDifferenceId::new(subject.item_id.clone()),
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
            discover_compensation(&self.db, subject, executor, &mut evidence).await?;
            Ok(evidence)
        })
    }
}

impl MongoIntegrationEvidenceAuthority {
    async fn verify_record(
        &self,
        subject: &EvidenceSubject,
        record: &EvidenceRecordRef,
        kind: ControlledEvidenceKind,
        actor_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<CanonicalEvidenceReference> {
        match record.kind() {
            "inbox_message" => self.verify_inbox_message(subject, record.id(), kind, executor).await,
            "customer_refund" => {
                self.verify_customer_refund(subject, record.id(), kind, actor_id, executor).await
            },
            "supplier_refund" => {
                self.verify_supplier_refund(subject, record.id(), kind, actor_id, executor).await
            },
            "supplier_refund_fact" => {
                self.verify_supplier_refund_fact(subject, record.id(), kind, executor).await
            },
            "reconciliation_difference_resolution" => {
                self.verify_difference_resolution(subject, record.id(), kind, actor_id, executor).await
            },
            _ => Err(Error::BusinessLogicError(format!("证据对象类型 {} 尚未注册权威验证器", record.kind()))),
        }
    }

    async fn verify_inbox_message(
        &self,
        subject: &EvidenceSubject,
        record_id: &str,
        kind: ControlledEvidenceKind,
        executor: &mut dyn Executor,
    ) -> Result<CanonicalEvidenceReference> {
        if kind != ControlledEvidenceKind::ExternalCaseResult {
            return kind_mismatch();
        }
        let message = self
            .db
            .inbox_messages()
            .find_by_id(record_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("证据引用的入站消息不存在".to_string()))?;
        if message.status != InboxMessageStatus::Processed || message.processed_at.is_none() {
            return Err(Error::BusinessLogicError("证据引用的入站消息尚未形成已处理终态".to_string()));
        }
        ensure_association(subject, std::slice::from_ref(&message.base.id))?;
        canonical_verified("inbox_message", &message.base.id, Some(message.base.version), "processed")
    }

    async fn verify_customer_refund(
        &self,
        subject: &EvidenceSubject,
        record_id: &str,
        kind: ControlledEvidenceKind,
        actor_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<CanonicalEvidenceReference> {
        if !matches!(
            kind,
            ControlledEvidenceKind::CompensationResult
                | ControlledEvidenceKind::FinancialReconciliation
                | ControlledEvidenceKind::DistinctReview
        ) {
            return kind_mismatch();
        }
        let refund = self
            .db
            .customer_refunds()
            .find_by_id(record_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("证据引用的客户退款不存在".to_string()))?;
        if refund.status != CustomerRefundStatus::Posted {
            return Err(Error::BusinessLogicError("客户退款尚未过账".to_string()));
        }
        if kind == ControlledEvidenceKind::DistinctReview && refund.reviewed_by == actor_id {
            return Err(Error::BusinessLogicError("独立复核人不得是当前处理人".to_string()));
        }
        ensure_association(
            subject,
            &refund_association_ids(
                &refund.base.id,
                refund.sales_return_case_id.as_ref().map(ToString::to_string),
                refund.original_receipt_id.as_ref().map(ToString::to_string),
                refund.original_receivable_entry_id.as_ref().map(ToString::to_string),
            ),
        )?;
        canonical_verified("customer_refund", &refund.base.id, Some(refund.base.version), "posted")
    }

    async fn verify_supplier_refund(
        &self,
        subject: &EvidenceSubject,
        record_id: &str,
        kind: ControlledEvidenceKind,
        actor_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<CanonicalEvidenceReference> {
        if !matches!(
            kind,
            ControlledEvidenceKind::CompensationResult
                | ControlledEvidenceKind::FinancialReconciliation
                | ControlledEvidenceKind::DistinctReview
        ) {
            return kind_mismatch();
        }
        let refund = self
            .db
            .supplier_refunds()
            .find_by_id(record_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("证据引用的供应商退款不存在".to_string()))?;
        if refund.status != SupplierRefundStatus::Posted {
            return Err(Error::BusinessLogicError("供应商退款尚未过账".to_string()));
        }
        if kind == ControlledEvidenceKind::DistinctReview && refund.reviewed_by == actor_id {
            return Err(Error::BusinessLogicError("独立复核人不得是当前处理人".to_string()));
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
        canonical_verified("supplier_refund", &refund.base.id, Some(refund.base.version), "posted")
    }

    async fn verify_supplier_refund_fact(
        &self,
        subject: &EvidenceSubject,
        record_id: &str,
        kind: ControlledEvidenceKind,
        executor: &mut dyn Executor,
    ) -> Result<CanonicalEvidenceReference> {
        ensure_compensation_kind(kind)?;
        let fact = self
            .db
            .supplier_refund_facts()
            .find_by_id(record_id, executor)
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
        canonical_verified("supplier_refund_fact", &fact.base.id, Some(fact.base.version), "succeeded")
    }

    async fn verify_difference_resolution(
        &self,
        subject: &EvidenceSubject,
        record_id: &str,
        kind: ControlledEvidenceKind,
        actor_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<CanonicalEvidenceReference> {
        if kind != ControlledEvidenceKind::DistinctReview {
            return kind_mismatch();
        }
        let record = self
            .db
            .reconciliation_difference_resolutions()
            .find_by_id(record_id, executor)
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
        canonical_verified("reconciliation_difference_resolution", &record.base.id, None, "reviewed")
    }
}

async fn known_result_exists(db: &Database, message_id: &str, executor: &mut dyn Executor) -> Result<bool> {
    // 商城事实已移除，仅以供应商退款事实判断已知结果。
    db.supplier_refund_facts().exists_by_inbox_message(message_id, executor).await.map_err(Into::into)
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

async fn discover_compensation(
    db: &Database,
    subject: &EvidenceSubject,
    executor: &mut dyn Executor,
    evidence: &mut Vec<ControlledEvidenceRef>,
) -> Result<()> {
    let Some(id) = subject.business_object_id.as_deref() else {
        return Ok(());
    };
    let kind = subject.business_object_type.as_deref().unwrap_or_default().to_ascii_lowercase();
    if (kind.is_empty() || kind == "customer_refund")
        && let Some(refund) = db.customer_refunds().find_by_id(id, executor).await?
        && refund.status == CustomerRefundStatus::Posted
    {
        push_compensation_refs(evidence, "customer_refund", &refund.base.id, "已过账客户退款")?;
        return Ok(());
    }
    if (kind.is_empty() || kind == "supplier_refund")
        && let Some(refund) = db.supplier_refunds().find_by_id(id, executor).await?
        && refund.status == SupplierRefundStatus::Posted
    {
        push_compensation_refs(evidence, "supplier_refund", &refund.base.id, "已过账供应商退款")?;
        return Ok(());
    }
    if (kind.is_empty() || kind == "supplier_refund_fact")
        && let Some(refund) = db.supplier_refund_facts().find_by_id(id, executor).await?
    {
        push_compensation_refs(evidence, "supplier_refund_fact", &refund.base.id, "供应商退款成功事实")?;
        return Ok(());
    }
    Ok(())
}

fn push_compensation_refs(
    refs: &mut Vec<ControlledEvidenceRef>,
    kind: &str,
    id: &str,
    label: &str,
) -> Result<()> {
    refs.push(controlled_ref(ControlledEvidenceKind::CompensationResult, kind, id, label)?);
    refs.push(controlled_ref(ControlledEvidenceKind::FinancialReconciliation, kind, id, label)?);
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
    Err(Error::ConflictError("证据记录与当前业务项没有可验证的正式关联".to_string()))
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
    use erp_integration::Error;
    use erp_integration::dto::ControlledEvidenceKind;
    use erp_integration::ports::evidence::EvidenceSubject;

    use super::{canonical_verified, ensure_association, ensure_compensation_kind, refund_association_ids};

    #[test]
    fn refund_evidence_accepts_formal_link_and_rejects_unrelated_subject() {
        let ids = refund_association_ids("refund-1", Some("return-1".into()), None, Some("entry-1".into()));
        assert_eq!(ids, ["refund-1", "return-1", "entry-1"]);
        let mut subject = EvidenceSubject::new("task-1".into());
        subject.business_object_id = Some("return-1".into());
        ensure_association(&subject, &ids).unwrap();
        subject.business_object_id = Some("unrelated".into());
        assert!(matches!(
            ensure_association(&subject, &ids),
            Err(Error::ConflictError(message)) if message == "证据记录与当前业务项没有可验证的正式关联"
        ));
    }

    #[test]
    fn refund_fact_requires_compensation_kind_and_canonical_terminal_identity() {
        ensure_compensation_kind(ControlledEvidenceKind::CompensationResult).unwrap();
        ensure_compensation_kind(ControlledEvidenceKind::FinancialReconciliation).unwrap();
        assert!(matches!(
            ensure_compensation_kind(ControlledEvidenceKind::DistinctReview),
            Err(Error::ValidationError(message)) if message == "证据类型与引用对象类型不匹配"
        ));
        let reference = canonical_verified("supplier_refund_fact", "refund-1", Some(3), "succeeded").unwrap();
        assert_eq!(reference.kind(), "supplier_refund_fact");
        assert_eq!(reference.id(), "refund-1");
        assert!(canonical_verified("supplier_refund_fact", "nested:id", Some(3), "succeeded").is_err());
    }
}
