//! W13 卡券票款正式复核命令、岗位分离与复核链匹配。

use database::{DocumentRegistryExt, FileAssetExt, ReceivableExt, WorkItemExt};
use entities::document_registry::{WorkflowAction, WorkflowActionData, WorkflowActionType};
use entities::receivable::{
    AccountReviewStatus, CardFundsCommandFollowUp, CardFundsCommandReceipt, CardFundsCommandReceiptData,
    CustomerReceiptStatus, EntityCardFundsReviewConclusion, EntityCardFundsReviewResult,
    EntityCardFundsReviewType, InvoiceDirection, InvoiceStatus, ReceivableAccount, ReceivableFundsReview,
    ReceivableFundsReviewChain, ReceivableFundsReviewData, ReviewResult, CARD_FUNDS_REVIEW_ACTION,
};
use entities::work_item::{WorkItem, WorkItemStatus, WorkItemType};
use erp_audit::AuditExt;
use erp_core::common::time::Instant;
use erp_core::ids::{BusinessDocumentId, ReceivableAccountId, ReceivableFundsReviewId, WorkflowActionId};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};
use validator::Validate;

use std::collections::HashMap;

use super::card_funds_decision::{
    canonical_evidence as card_funds_canonical_evidence,
    validate_evidence_assets as validate_card_funds_evidence_assets,
    validated_from_dto as validated_card_funds_decision, workflow_comment as card_funds_workflow_comment,
};
use super::card_funds_identity::lock_review_work_item;
use super::card_funds_receipt::{complete_review_result, map_command_receipt_error};
use super::dto::{
    CardFundsReviewConclusion, CardFundsReviewDecision, CardFundsReviewResult, CardFundsReviewType,
    CompleteCardFundsReviewCommand, CompleteCardFundsReviewResult,
};
use super::mapping::{
    card_funds_review_chain, card_funds_snapshot_of, load_card_funds_snapshot, map_chain_error,
    parse_task_version, CardFundsSnapshot,
};
use super::{card_funds_task, ReceivableService};
use crate::errors::{Error, Result};
use crate::work_item::WorkItemService;
use application_core::AuditActor;
use erp_audit::AuditActorLogs;

impl ReceivableService {
    /// 以 W13 强类型领域命令完成卡券票款正式复核。
    ///
    /// 单事务锁定任务、应收账户、复核链和当前票款事实，重验全部任务/领域版本、
    /// 当前责任与岗位分离后，追加复核事实和 `workflow_action`，刷新账户查询缓存，
    /// 并由领域命令完成原任务。驳回在同一事务按当前责任规则创建同类型后继任务，
    /// 未决复核责任不得离开工作台。
    /// 审计记录同时充当不泄漏原始幂等键的稳定结果收据。
    ///
    /// # 错误
    /// 任务/领域版本漂移返回冲突；任务、证据、结论或岗位分离不满足时 fail closed。
    pub async fn complete_card_funds_review(
        &self,
        command: CompleteCardFundsReviewCommand,
        actor: &AuditActor,
    ) -> Result<CompleteCardFundsReviewResult> {
        command.validate()?;
        if command.work_item_id.as_ref().trim().is_empty()
            || command.work_item_id.as_ref().chars().count() > 128
        {
            return Err(Error::ValidationError("任务 ID 非法".to_string()));
        }
        let validated = validated_card_funds_decision(&command.decision)?;
        let expected_task_version = parse_task_version(&command.expected_task_version)?;
        let fingerprint =
            CardFundsCommandReceipt::payload_fingerprint(&command).map_err(map_command_receipt_error)?;
        let audit_id = CardFundsCommandReceipt::audit_id(actor.id(), &command.idempotency_key);
        if let Some(result) = self
            .replay_card_funds_review(&audit_id, &fingerprint, &command.work_item_id)
            .await?
        {
            return Ok(result);
        }

        let db = self.db.clone();
        let client = db.client().clone();
        let actor_owned = actor.clone();
        let actor_id = actor.id().to_string();
        let rbac_for_tx = self.rbac.clone();
        let command_for_tx = command.clone();
        let validated_for_tx = validated.clone();
        let audit_id_for_tx = audit_id.clone();
        let fingerprint_for_tx = fingerprint.clone();
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let decision = &command_for_tx.decision;
                    let mut work_item = db
                        .work_items()
                        .find_by_id(&command_for_tx.work_item_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("卡券票款复核任务不存在".to_string()))?;
                    lock_review_work_item(
                        &work_item,
                        &actor_id,
                        expected_task_version,
                        &command_for_tx.expected_subject_version,
                        &decision.receivable_account_id,
                        decision.review_type,
                    )?;

                    let mut account = db
                        .receivable_accounts()
                        .find_by_id(&decision.receivable_account_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("应收往来子账不存在".to_string()))?;
                    let snapshot = load_card_funds_snapshot(&db, &account, session).await?;
                    let chain = match_card_funds_command_snapshot(&account, &snapshot, decision, &work_item)?;
                    {
                        let assets = db
                            .file_assets()
                            .find_by_ids(validated_for_tx.evidence().document_ids(), session)
                            .await?;
                        validate_card_funds_evidence_assets(&validated_for_tx, &assets, Instant::now())?;
                    }
                    validate_card_funds_reviewer_separation(
                        &db, &account, &snapshot, &work_item, &actor_id, session,
                    )
                    .await?;
                    WorkItemService::new(db.clone(), rbac_for_tx)
                        .ensure_domain_decision_access(&actor_owned, &work_item, session)
                        .await?;

                    let completed_at = Instant::now();
                    let evidence = card_funds_canonical_evidence(&validated_for_tx);
                    let review_type = entity_review_type(match validated_for_tx.review_type() {
                        entities::receivable::EntityCardFundsReviewType::Opening => {
                            CardFundsReviewType::Opening
                        }
                        entities::receivable::EntityCardFundsReviewType::SyncDelta => {
                            CardFundsReviewType::SyncDelta
                        }
                    });
                    let review_result = entity_review_result(match validated_for_tx.review_result() {
                        entities::receivable::EntityCardFundsReviewResult::Approved => {
                            CardFundsReviewResult::Approved
                        }
                        entities::receivable::EntityCardFundsReviewResult::Rejected => {
                            CardFundsReviewResult::Rejected
                        }
                    });
                    let predecessor_id = chain.tail_id();
                    chain
                        .ensure_predecessor(predecessor_id)
                        .map_err(map_chain_error)?;
                    let review = ReceivableFundsReview::new(
                        ReceivableFundsReviewId::new(next_id()),
                        ReceivableFundsReviewData {
                            receivable_account_id: decision.receivable_account_id.clone(),
                            review_no: decision.expected_next_review_no,
                            review_type,
                            work_item_id: command_for_tx.work_item_id.clone(),
                            evidence_document_id: decision.evidence_document_ids.first().cloned(),
                            evidence_reference: evidence.clone(),
                            review_result,
                            reviewed_by: actor_id.clone(),
                            reviewed_at: completed_at,
                            supersedes_review_id: predecessor_id.map(ReceivableFundsReviewId::new),
                        },
                    )?;

                    let cache_status = match validated_for_tx.review_result() {
                        entities::receivable::EntityCardFundsReviewResult::Approved => {
                            AccountReviewStatus::Reviewed
                        }
                        entities::receivable::EntityCardFundsReviewResult::Rejected => {
                            let dto_type = match validated_for_tx.review_type() {
                                entities::receivable::EntityCardFundsReviewType::Opening => {
                                    CardFundsReviewType::Opening
                                }
                                entities::receivable::EntityCardFundsReviewType::SyncDelta => {
                                    CardFundsReviewType::SyncDelta
                                }
                            };
                            pending_review_status(dto_type)
                        }
                    };
                    let cache_update = match validated_for_tx.review_result() {
                        entities::receivable::EntityCardFundsReviewResult::Approved => {
                            entities::receivable::ReceivableAccountUpdate {
                                review_status: Some(cache_status),
                                reviewed_by: Some(actor_id.clone()),
                                reviewed_at: Some(completed_at),
                                review_evidence_reference: Some(evidence.clone().unwrap_or_else(|| {
                                    validated_for_tx
                                        .evidence()
                                        .document_ids()
                                        .first()
                                        .map(|id| id.to_string())
                                        .unwrap_or_default()
                                })),
                                gross_total: None,
                                invoiceable_total: None,
                            }
                        }
                        entities::receivable::EntityCardFundsReviewResult::Rejected => {
                            entities::receivable::ReceivableAccountUpdate {
                                review_status: Some(cache_status),
                                reviewed_by: Some(String::new()),
                                reviewed_at: None,
                                review_evidence_reference: Some(String::new()),
                                gross_total: None,
                                invoiceable_total: None,
                            }
                        }
                    };
                    account.update(cache_update, &actor_id)?;

                    let workflow = WorkflowAction::new(
                        WorkflowActionId::new(next_id()),
                        WorkflowActionData {
                            document_id: BusinessDocumentId::new(account.sales_order_id.to_string()),
                            action_type: match validated_for_tx.review_result() {
                                entities::receivable::EntityCardFundsReviewResult::Approved => {
                                    WorkflowActionType::Approve
                                }
                                entities::receivable::EntityCardFundsReviewResult::Rejected => {
                                    WorkflowActionType::Reject
                                }
                            },
                            from_status: account_review_status_code(pending_review_status(
                                match validated_for_tx.review_type() {
                                    entities::receivable::EntityCardFundsReviewType::Opening => {
                                        CardFundsReviewType::Opening
                                    }
                                    entities::receivable::EntityCardFundsReviewType::SyncDelta => {
                                        CardFundsReviewType::SyncDelta
                                    }
                                },
                            ))
                            .to_string(),
                            to_status: account_review_status_code(cache_status).to_string(),
                            actor_id: actor_id.clone(),
                            actor_role: work_item.owner_role.clone(),
                            comment: card_funds_workflow_comment(&validated_for_tx),
                        },
                    )?;
                    work_item.record_activity(&actor_id, completed_at)?;
                    work_item.complete_by_domain_command(&actor_id, completed_at)?;

                    db.receivable().append_funds_review(&review, session).await?;
                    db.receivable_accounts().update(&mut account, session).await?;
                    db.workflow_actions().create(&workflow, session).await?;
                    db.work_items().update(&mut work_item, session).await?;
                    let follow_up_work_item = if validated_for_tx.review_result()
                        == entities::receivable::EntityCardFundsReviewResult::Rejected
                    {
                        Some(
                            card_funds_task::ensure_card_funds_review_task(
                                &db,
                                &account,
                                &work_item.subject_version,
                                session,
                            )
                            .await?
                            .ok_or_else(|| Error::Internal("驳回后账户未形成待复核后继任务".to_string()))?,
                        )
                    } else {
                        None
                    };

                    let follow_up = match follow_up_work_item.as_ref() {
                        Some(item) => CardFundsCommandFollowUp::Rejected {
                            work_item_id: item.base.id.clone(),
                            work_item_type: item.work_item_type.as_str().to_string(),
                        },
                        None => CardFundsCommandFollowUp::None,
                    };
                    let receipt = CardFundsCommandReceipt::new(
                        fingerprint_for_tx.clone(),
                        CardFundsCommandReceiptData {
                            receivable_funds_review_id: review.base.id.clone(),
                            workflow_action_id: workflow.base.id.clone(),
                            review_no: review.review_no,
                            account_review_status: account.review_status.as_str().to_string(),
                            completed_at: completed_at.unix_secs(),
                            review_result: validated_for_tx.review_result(),
                            conclusion: validated_for_tx.conclusion(),
                            follow_up,
                        },
                    )
                    .map_err(map_command_receipt_error)?;
                    let audit = actor_owned.resource_log_with_id(
                        audit_id_for_tx,
                        CARD_FUNDS_REVIEW_ACTION,
                        "receivable_funds_review",
                        account.base.id.clone(),
                        Some(receipt.encode_message().map_err(map_command_receipt_error)?),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<CardFundsCommandReceipt, crate::errors::Error>(receipt)
                })
            })
            .await;

        let receipt = match transaction_result {
            Ok(receipt) => receipt,
            Err(error) => {
                if let Some(result) = self
                    .replay_card_funds_review(&audit_id, &fingerprint, &command.work_item_id)
                    .await?
                {
                    return Ok(result);
                }
                return Err(error);
            }
        };
        Ok(complete_review_result(
            &receipt,
            command.work_item_id.as_ref(),
            command.decision.receivable_account_id.as_ref(),
            &audit_id,
        ))
    }

    /// 按稳定审计收据严格重放已完成的 W13 正式结果。
    async fn replay_card_funds_review(
        &self,
        audit_id: &str,
        expected_fingerprint: &str,
        work_item_id: &erp_core::ids::WorkItemId,
    ) -> Result<Option<CompleteCardFundsReviewResult>> {
        let Some(audit) = self
            .db
            .audit_logs()
            .find_by_id(audit_id, &mut NoTransaction)
            .await?
        else {
            return Ok(None);
        };
        if audit.action != CARD_FUNDS_REVIEW_ACTION
            || audit.resource_type != "receivable_funds_review"
            || !audit.success
        {
            return Err(Error::Internal("卡券票款复核幂等收据身份非法".to_string()));
        }
        let account_id = audit
            .resource_id
            .as_deref()
            .ok_or_else(|| Error::Internal("卡券票款复核幂等收据缺少应收账户".to_string()))?;
        let mut receipt = CardFundsCommandReceipt::parse(
            audit
                .message
                .as_deref()
                .ok_or_else(|| Error::Internal("卡券票款复核幂等收据为空".to_string()))?,
            expected_fingerprint,
        )
        .map_err(map_command_receipt_error)?;
        if receipt.requires_legacy_rejected_follow_up() {
            let follow_up = self
                .ensure_legacy_rejected_card_funds_follow_up(
                    &receipt.data().receivable_funds_review_id,
                    work_item_id,
                    account_id,
                )
                .await?;
            receipt
                .attach_follow_up(&follow_up.base.id, follow_up.work_item_type.as_str())
                .map_err(map_command_receipt_error)?;
        }
        Ok(Some(complete_review_result(
            &receipt,
            work_item_id.as_ref(),
            account_id,
            audit_id,
        )))
    }

    /// 为七字段旧版驳回收据补建或复用正式 W13 后继任务。
    ///
    /// 旧版驳回事务已经完成原任务并保留账户待复核状态，但收据没有记录后继任务。
    /// 本迁移在事务内核对原任务、正式复核事实和账户状态，再按当前财务责任规则
    /// 建立后继任务；重复回放复用同一开放任务，后继已处理时由相邻复核事实定位，
    /// 不修改历史审计收据。
    async fn ensure_legacy_rejected_card_funds_follow_up(
        &self,
        review_id: &str,
        work_item_id: &erp_core::ids::WorkItemId,
        account_id: &str,
    ) -> Result<WorkItem> {
        let db = self.db.clone();
        let client = db.client().clone();
        let review_id = review_id.to_string();
        let work_item_id = work_item_id.clone();
        let account_id = ReceivableAccountId::new(account_id.to_string());
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let original = db
                        .work_items()
                        .find_by_id(&work_item_id, session)
                        .await?
                        .ok_or_else(|| {
                            Error::ConflictError("旧版驳回复核的原工作项不存在，无法补建后继任务".to_string())
                        })?;
                    if original.status != WorkItemStatus::Completed
                        || original.business_object_type != "receivable_account"
                        || original.business_object_id != account_id.to_string()
                        || !matches!(
                            original.work_item_type,
                            WorkItemType::CardFundsReview | WorkItemType::CardFundsDeltaReview
                        )
                    {
                        return Err(Error::ConflictError(
                            "旧版驳回复核的原工作项身份或状态不一致".to_string(),
                        ));
                    }

                    let review = db
                        .receivable_funds_reviews()
                        .find_by_id(&review_id, session)
                        .await?
                        .ok_or_else(|| {
                            Error::ConflictError("旧版驳回复核的正式复核事实不存在".to_string())
                        })?;
                    if review.work_item_id != work_item_id || review.review_result != ReviewResult::Rejected {
                        return Err(Error::ConflictError(
                            "旧版驳回复核的正式事实与原工作项不一致".to_string(),
                        ));
                    }
                    let next_review_no = review
                        .review_no
                        .checked_add(1)
                        .ok_or_else(|| Error::Internal("旧版驳回复核的复核号已达到上限".to_string()))?;
                    let completed_successor_id = db
                        .receivable_funds_reviews()
                        .find_review_by_supersedes(
                            &ReceivableFundsReviewId::new(review.base.id.clone()),
                            session,
                        )
                        .await?
                        .and_then(|candidate| {
                            (candidate.review_no == next_review_no).then_some(candidate.work_item_id.clone())
                        });
                    if let Some(successor_id) = completed_successor_id {
                        let successor = db
                            .work_items()
                            .find_by_id(&successor_id, session)
                            .await?
                            .ok_or_else(|| {
                                Error::ConflictError("旧版驳回复核的已处理后继工作项不存在".to_string())
                            })?;
                        if successor.status != WorkItemStatus::Completed
                            || successor.work_item_type != original.work_item_type
                            || successor.business_object_type != "receivable_account"
                            || successor.business_object_id != account_id.to_string()
                            || successor.subject_version != original.subject_version
                        {
                            return Err(Error::ConflictError(
                                "旧版驳回复核的已处理后继工作项身份不一致".to_string(),
                            ));
                        }
                        return Ok(successor);
                    }

                    let account = db
                        .receivable_accounts()
                        .find_by_id(&account_id, session)
                        .await?
                        .ok_or_else(|| Error::ConflictError("旧版驳回复核的应收账户不存在".to_string()))?;
                    let expected_type = match account.review_status {
                        AccountReviewStatus::OpeningPending => WorkItemType::CardFundsReview,
                        AccountReviewStatus::SyncDeltaPending => WorkItemType::CardFundsDeltaReview,
                        AccountReviewStatus::NotApplicable | AccountReviewStatus::Reviewed => {
                            return Err(Error::ConflictError(
                                "旧版驳回复核的应收账户已离开待复核状态，不能自动补建后继任务".to_string(),
                            ));
                        }
                    };
                    if original.work_item_type != expected_type {
                        return Err(Error::ConflictError(
                            "旧版驳回复核的任务类型与账户待复核状态不一致".to_string(),
                        ));
                    }

                    card_funds_task::ensure_card_funds_review_task(
                        &db,
                        &account,
                        &original.subject_version,
                        session,
                    )
                    .await?
                    .ok_or_else(|| Error::ConflictError("旧版驳回复核未处于可补建后继任务的状态".to_string()))
                })
            })
            .await
    }
}

/// 编排命令持有的账户／销售版本锁，并把链连续性、差额前置与票款对账交给 VO。
///
/// # 参数
/// * `account` - 事务内账户
/// * `snapshot` - 同一 executor 装载的票款快照
/// * `decision` - 命令期望版本
/// * `work_item` - 已锁定任务
///
/// # 返回
/// 返回已验证链，供写入 `supersedes_review_id`。
///
/// # 错误
/// 账户／销售版本漂移为 Conflict；差额空链与净额破坏为业务错误。
///
/// # 约束
/// 不复制链 hash／连续性／conclusion 规则。
fn match_card_funds_command_snapshot(
    account: &ReceivableAccount,
    snapshot: &CardFundsSnapshot,
    decision: &CardFundsReviewDecision,
    work_item: &WorkItem,
) -> Result<ReceivableFundsReviewChain> {
    let expected_status = pending_review_status(decision.review_type);
    if account.account_seq != decision.expected_account_seq
        || account.base.version.to_string() != decision.expected_account_domain_version
        || account.review_status != expected_status
    {
        return Err(Error::ConflictError(
            "应收账户领域版本或复核状态已变化".to_string(),
        ));
    }
    if snapshot.current_sales_order_revision_id != decision.expected_sales_order_revision_id
        || snapshot.current_sales_order_revision_id != work_item.subject_version
    {
        return Err(Error::ConflictError("销售单当前版本已变化".to_string()));
    }
    let chain = card_funds_review_chain(&snapshot.reviews)?;
    chain
        .ensure_predecessor(decision.expected_review_chain_tail_id.as_deref())
        .map_err(|_| Error::ConflictError("复核链已变化，请刷新后重试".to_string()))?;
    if chain.version() != decision.expected_review_chain_version {
        return Err(Error::ConflictError("复核链已变化，请刷新后重试".to_string()));
    }
    let next_review_no = chain.next_review_no().map_err(map_chain_error)?;
    if next_review_no != decision.expected_next_review_no {
        return Err(Error::ConflictError("下一复核号已变化，请刷新后重试".to_string()));
    }
    if decision.review_type == CardFundsReviewType::SyncDelta {
        chain
            .ensure_sync_delta_allowed()
            .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
    }
    let funds = card_funds_snapshot_of(snapshot)?;
    if funds.fact_version(account) != decision.expected_funds_fact_version {
        return Err(Error::ConflictError(
            "票款事实版本已变化，请刷新后重试".to_string(),
        ));
    }
    funds
        .reconcile(account)
        .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
    funds
        .validate_conclusion(
            map_entity_review_type(decision.review_type),
            map_entity_review_result(decision.review_result),
            map_entity_conclusion(decision.conclusion),
        )
        .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
    Ok(chain)
}

/// 重验责任资格，并对已登记票款事实执行可证明的经办/复核岗位分离。
///
/// 审计事实经 `list_separation_facts_by_resources` 单次批量装载（数量增长时
/// 查询保持常数），SoD 政策解释仍在本函数与 `check_fact_separation` 中。
pub(super) async fn validate_card_funds_reviewer_separation(
    db: &Database,
    account: &ReceivableAccount,
    snapshot: &CardFundsSnapshot,
    work_item: &WorkItem,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let _ = (work_item, actor_id);

    for receipt in &snapshot.receipts {
        if !matches!(
            receipt.status,
            CustomerReceiptStatus::Posted | CustomerReceiptStatus::Reversed
        ) || receipt.counterparty_party_id != account.counterparty_party_id
        {
            return Err(Error::BusinessLogicError(
                "复核引用的回款事实未正式过账或往来主体不一致".to_string(),
            ));
        }
    }

    for invoice in &snapshot.invoices {
        if invoice.invoice_direction != InvoiceDirection::Sales
            || !matches!(
                invoice.stable.status(),
                InvoiceStatus::Registered | InvoiceStatus::RedInvoiced
            )
            || invoice.party_id != account.counterparty_party_id
        {
            return Err(Error::BusinessLogicError(
                "复核引用的销项发票未正式登记或往来主体不一致".to_string(),
            ));
        }
    }

    let mut pairs = Vec::with_capacity(snapshot.receipts.len() + snapshot.invoices.len());
    for receipt in &snapshot.receipts {
        pairs.push(("customer_receipt".to_string(), receipt.base.id.clone()));
    }
    for invoice in &snapshot.invoices {
        pairs.push(("invoice".to_string(), invoice.base.id.clone()));
    }
    let facts = db
        .audit_logs()
        .list_separation_facts_by_resources(&pairs, executor)
        .await?;
    let mut by_resource: HashMap<(String, String), Vec<&erp_audit::SeparationAuditFact>> = HashMap::new();
    for fact in &facts {
        if let Some(resource_id) = fact.resource_id.as_deref() {
            by_resource
                .entry((fact.resource_type.clone(), resource_id.to_string()))
                .or_default()
                .push(fact);
        }
    }

    for receipt in &snapshot.receipts {
        let key = ("customer_receipt".to_string(), receipt.base.id.clone());
        let empty = Vec::new();
        let resource_facts = by_resource.get(&key).unwrap_or(&empty);
        check_fact_separation(
            resource_facts,
            actor_id,
            &["customer_receipt.create", "customer_receipt.post:"],
            &["customer_receipt.post:"],
        )?;
    }

    for invoice in &snapshot.invoices {
        let key = ("invoice".to_string(), invoice.base.id.clone());
        let empty = Vec::new();
        let resource_facts = by_resource.get(&key).unwrap_or(&empty);
        check_fact_separation(
            resource_facts,
            actor_id,
            &["invoice.create", "invoice.post", "invoice.red_issue"],
            &["invoice.post", "invoice.red_issue"],
        )?;
    }
    Ok(())
}

/// 从批量装载的最小审计事实证明票款已正式登记且当前复核人不是其经办人。
///
/// 纯策略解释：缺正式证据 fail closed；同 actor 经办冲突拒绝；仅非正式或
/// 失败事件（调用方批量查询已限定成功）不算证据。SoD 规则、当前 actor、
/// 拒绝文案与授权决定保留 Service，不得下沉。
fn check_fact_separation(
    facts: &[&erp_audit::SeparationAuditFact],
    actor_id: &str,
    operator_actions: &[&str],
    formal_actions: &[&str],
) -> Result<()> {
    let matches_action =
        |action: &str, prefixes: &[&str]| prefixes.iter().any(|prefix| action.starts_with(prefix));
    if !facts
        .iter()
        .any(|fact| matches_action(&fact.action, formal_actions))
    {
        return Err(Error::Forbidden(
            "无法从审计事实证明票款已经正式登记，岗位分离校验失败关闭".to_string(),
        ));
    }
    if facts
        .iter()
        .any(|fact| fact.actor_id == actor_id && matches_action(&fact.action, operator_actions))
    {
        return Err(Error::Forbidden(
            "票款事实经办人与最终复核人必须岗位分离".to_string(),
        ));
    }
    Ok(())
}

/// 将 HTTP 复核类型转换为领域事实枚举。
fn entity_review_type(review_type: CardFundsReviewType) -> entities::receivable::FundsReviewType {
    match review_type {
        CardFundsReviewType::Opening => entities::receivable::FundsReviewType::Opening,
        CardFundsReviewType::SyncDelta => entities::receivable::FundsReviewType::SyncDelta,
    }
}

/// 将 HTTP 复核结果转换为领域事实枚举。
fn entity_review_result(result: CardFundsReviewResult) -> ReviewResult {
    match result {
        CardFundsReviewResult::Approved => ReviewResult::Passed,
        CardFundsReviewResult::Rejected => ReviewResult::Rejected,
    }
}

/// 返回复核类型对应的待复核账户缓存状态。
pub(super) fn pending_review_status(review_type: CardFundsReviewType) -> AccountReviewStatus {
    match review_type {
        CardFundsReviewType::Opening => AccountReviewStatus::OpeningPending,
        CardFundsReviewType::SyncDelta => AccountReviewStatus::SyncDeltaPending,
    }
}

/// 返回账户复核状态的稳定工作流代码。
fn account_review_status_code(status: AccountReviewStatus) -> &'static str {
    match status {
        AccountReviewStatus::NotApplicable => "NOT_APPLICABLE",
        AccountReviewStatus::OpeningPending => "OPENING_PENDING",
        AccountReviewStatus::Reviewed => "REVIEWED",
        AccountReviewStatus::SyncDeltaPending => "SYNC_DELTA_PENDING",
    }
}

/// 映射服务复核类型到实体复核类型。
///
/// # 参数
/// * `review_type` - 服务 DTO
///
/// # 返回
/// 实体枚举。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 纯映射。
fn map_entity_review_type(review_type: CardFundsReviewType) -> EntityCardFundsReviewType {
    match review_type {
        CardFundsReviewType::Opening => EntityCardFundsReviewType::Opening,
        CardFundsReviewType::SyncDelta => EntityCardFundsReviewType::SyncDelta,
    }
}

/// 映射服务复核结果到实体复核结果。
///
/// # 参数
/// * `review_result` - 服务 DTO
///
/// # 返回
/// 实体枚举。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 纯映射。
fn map_entity_review_result(review_result: CardFundsReviewResult) -> EntityCardFundsReviewResult {
    match review_result {
        CardFundsReviewResult::Approved => EntityCardFundsReviewResult::Approved,
        CardFundsReviewResult::Rejected => EntityCardFundsReviewResult::Rejected,
    }
}

/// 映射服务复核结论到实体复核结论。
///
/// # 参数
/// * `conclusion` - 服务 DTO
///
/// # 返回
/// 实体枚举。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 纯映射。
fn map_entity_conclusion(conclusion: CardFundsReviewConclusion) -> EntityCardFundsReviewConclusion {
    match conclusion {
        CardFundsReviewConclusion::NoHistoryFromZero => EntityCardFundsReviewConclusion::NoHistoryFromZero,
        CardFundsReviewConclusion::RecordedFactsReconciled => {
            EntityCardFundsReviewConclusion::RecordedFactsReconciled
        }
        CardFundsReviewConclusion::Rejected => EntityCardFundsReviewConclusion::Rejected,
    }
}

#[cfg(test)]
mod card_funds_review_tests {
    use entities::file_asset::{
        ContentHmac, FileAsset, FileAssetData, RetentionClass, SecurityScanStatus, SensitivityClass,
    };
    use erp_core::common::time::Instant;
    use erp_core::ids::{FileAssetId, ReceivableAccountId};

    use super::super::card_funds_decision::{
        canonical_evidence, validate_evidence_assets, validated_from_dto,
    };
    use super::super::card_funds_receipt::{complete_review_result, map_command_receipt_error};
    use super::{
        parse_task_version, CardFundsReviewConclusion, CardFundsReviewDecision, CardFundsReviewResult,
        CardFundsReviewType, Error,
    };
    use entities::receivable::CardFundsCommandReceipt;

    fn opening_decision() -> CardFundsReviewDecision {
        CardFundsReviewDecision {
            receivable_account_id: ReceivableAccountId::new("ra-1"),
            expected_account_seq: 1,
            expected_account_domain_version: "3".to_string(),
            expected_review_chain_tail_id: None,
            expected_review_chain_version: "rcv:empty".to_string(),
            expected_next_review_no: 1,
            expected_sales_order_revision_id: "sor-1".to_string(),
            expected_funds_fact_version: "ffv:empty".to_string(),
            review_type: CardFundsReviewType::Opening,
            review_result: CardFundsReviewResult::Approved,
            conclusion: CardFundsReviewConclusion::NoHistoryFromZero,
            evidence_document_ids: vec![FileAssetId::new("file-1")],
            evidence_references: Vec::new(),
            comment: Some("已核对".to_string()),
            reason_code: None,
        }
    }

    fn evidence_asset(id: &str, passed: bool) -> FileAsset {
        let mut asset = FileAsset::new(
            FileAssetId::new(id),
            FileAssetData {
                storage_object_key: format!("receivable-review/{id}"),
                file_name: format!("{id}.pdf"),
                content_type: "application/pdf".to_string(),
                byte_size: 1,
                content_hmac: ContentHmac::parse("a".repeat(64)).unwrap(),
                sensitivity_class: SensitivityClass::Sensitive,
                retention_class: RetentionClass::LongTerm,
                expires_at: None,
                created_by: "reviewer-1".to_string(),
            },
        )
        .unwrap();
        if passed {
            asset.mark_scan_result(SecurityScanStatus::Passed).unwrap();
        }
        asset
    }

    #[test]
    fn semantic_validation_rejects_result_conclusion_drift_and_missing_evidence() {
        let mut invalid = opening_decision();
        invalid.conclusion = CardFundsReviewConclusion::Rejected;
        assert!(validated_from_dto(&invalid).is_err());

        let mut invalid = opening_decision();
        invalid.evidence_document_ids.clear();
        invalid.evidence_references.clear();
        assert!(validated_from_dto(&invalid).is_err());

        let mut invalid = opening_decision();
        invalid.reason_code = Some("OTHER".to_string());
        // Approved 携带驳回原因应拒绝
        assert!(validated_from_dto(&invalid).is_err());
    }

    #[test]
    fn evidence_keeps_additional_documents_as_controlled_references() {
        let mut decision = opening_decision();
        decision.evidence_document_ids.push(FileAssetId::new("file-2"));
        decision.evidence_references.push("BANK-REF-1".to_string());
        let validated = validated_from_dto(&decision).unwrap();
        // canonical 为排序后结果：BANK-REF-1 与 file_asset:file-2 的字典序
        let mut expected = ["BANK-REF-1".to_string(), "file_asset:file-2".to_string()];
        expected.sort();
        assert_eq!(
            canonical_evidence(&validated).as_deref(),
            Some(expected.join("; ").as_str())
        );
        assert_eq!(validated.evidence().document_ids().len(), 2);
    }

    #[test]
    fn evidence_batch_restores_input_order_and_accepts_unordered_results() {
        let mut decision = opening_decision();
        decision.evidence_document_ids.push(FileAssetId::new("file-2"));
        let validated = validated_from_dto(&decision).unwrap();
        let assets = vec![evidence_asset("file-2", true), evidence_asset("file-1", true)];
        assert!(
            validate_evidence_assets(&validated, &assets, Instant::from_unix_secs(1_700_000_000),).is_ok()
        );
    }

    #[test]
    fn evidence_batch_reports_first_requested_missing_file_before_later_scan_error() {
        let mut decision = opening_decision();
        decision.evidence_document_ids =
            vec![FileAssetId::new("file-missing"), FileAssetId::new("file-pending")];
        let validated = validated_from_dto(&decision).unwrap();
        let assets = vec![evidence_asset("file-pending", false)];

        let error = validate_evidence_assets(&validated, &assets, Instant::from_unix_secs(1_700_000_000))
            .unwrap_err();

        assert!(matches!(
            error,
            Error::NotFound(message) if message == "复核证据文件不存在: file-missing"
        ));
    }

    #[test]
    fn canonical_evidence_is_sorted_and_byte_stable() {
        let mut decision = opening_decision();
        decision.evidence_document_ids.push(FileAssetId::new("file-2"));
        decision.evidence_references.push("z-ref".to_string());
        decision.evidence_references.push("a-ref".to_string());
        let v1 = validated_from_dto(&decision).unwrap();
        let c1 = canonical_evidence(&v1).unwrap();
        let v2 = validated_from_dto(&decision).unwrap();
        assert_eq!(canonical_evidence(&v2).unwrap(), c1);
        // 手工排序验证：a-ref 位于 file_asset:file-2 之前（字典序）
        assert!(c1.contains("a-ref"));
        assert!(c1.contains("file_asset:file-2"));
    }

    #[test]
    fn evidence_usability_uses_file_asset_point_in_time_check() {
        let decision = opening_decision();
        let validated = validated_from_dto(&decision).unwrap();
        let now = Instant::from_unix_secs(1_700_000_000);
        let mut expired_asset = evidence_asset("file-1", true);
        expired_asset.expires_at = Some(Instant::from_unix_secs(1_699_999_999));
        assert!(validate_evidence_assets(&validated, &[expired_asset], now).is_err());
        let mut destroyed_asset = evidence_asset("file-1", true);
        destroyed_asset
            .destroy(Instant::from_unix_secs(1_700_000_000))
            .unwrap();
        assert!(validate_evidence_assets(&validated, &[destroyed_asset], now).is_err());
    }

    #[test]
    fn old_helpers_are_deleted_and_new_vo_is_unique_entry() {
        let source = concat!(
            include_str!("mod.rs"),
            include_str!("mapping.rs"),
            include_str!("account.rs"),
            include_str!("card_funds_register.rs"),
            include_str!("customer_receipt.rs"),
            include_str!("card_funds_review.rs"),
        );
        // 仅检查生产代码部分，避免本测试自身字面量触发误判
        let production = source
            .split("fn old_helpers_are_deleted")
            .next()
            .unwrap_or(source);
        assert!(source.contains("validated_from_dto"));
        assert!(source.contains("validate_evidence_assets"));
        assert!(source.contains("canonical_evidence"));
        assert!(source.contains("workflow_comment"));
        assert!(source.contains("ValidatedCardFundsReviewDecision"));
        assert!(source.contains("CardFundsReviewEvidence"));
        assert!(source.contains("is_usable_at") || source.contains("validate_usable_at"));
        // 旧 Service 私有 helper 已删除（FIN-E10 四处纯规则已收敛至实体/VO）
        assert!(!production.contains("fn validate_card_funds_decision"));
        assert!(!production.contains("fn validate_card_funds_evidence_facts"));
        assert!(!production.contains("fn canonical_review_evidence"));
        assert!(!production.contains("fn workflow_decision_comment"));
        // FIN-E11 / FIN-E12 / FIN-E13 旧规则源已删除
        assert!(!production.contains("fn validate_card_funds_work_item"));
        assert!(!production.contains("fn next_review_no"));
        assert!(!production.contains("fn review_chain_version"));
        assert!(!production.contains("fn funds_fact_version"));
        assert!(!production.contains("fn net_receipt_allocated"));
        assert!(!production.contains("fn net_sales_allocated"));
        assert!(!production.contains("fn pending_allocated_total"));
        assert!(!production.contains("fn plan_card_funds_receipt_allocations"));
        assert!(!production.contains("fn validate_card_funds_facts"));
        assert!(!production.contains("fn validate_card_funds_versions"));
        assert!(production.contains("ReceivableFundsReviewChain"));
        assert!(production.contains("ReceivableFundsSnapshot"));
        assert!(production.contains("ReceivableFundsLedger"));
        assert!(production.contains("lock_review_work_item"));
        assert!(production.contains("card_funds_snapshot_facts"));
        assert!(production.contains("apply_settlements_many"));
        assert!(production.contains("create_receipt_allocations_many"));
        assert!(production.contains("find_review_by_supersedes"));
        assert!(production.contains("ensure_predecessor"));
        assert!(production.contains("ensure_sync_delta_allowed"));
        assert!(production.contains("match_card_funds_command_snapshot"));
        // FIN-E14 旧规则源已删除
        assert!(!production.contains("fn normalized_registration_no"));
        assert!(!production.contains("fn stable_registration_no"));
        assert!(!production.contains("fn card_funds_registration_fingerprint"));
        assert!(!production.contains("fn card_funds_registration_audit_id"));
        assert!(!production.contains("fn card_funds_registration_receipt_message"));
        assert!(!production.contains("fn card_funds_command_fingerprint"));
        assert!(!production.contains("fn card_funds_audit_id"));
        assert!(!production.contains("fn card_funds_receipt_message"));
        assert!(!production.contains("fn parse_card_funds_receipt"));
        assert!(!production.contains("struct CardFundsReviewReceipt"));
        assert!(production.contains("CardFundsCommandReceipt"));
        assert!(production.contains("CardFundsRegistrationReceipt"));
    }

    #[test]
    fn task_version_requires_canonical_positive_integer_string() {
        assert_eq!(parse_task_version("12").unwrap(), 12);
        assert!(parse_task_version("0").is_err());
        assert!(parse_task_version("01").is_err());
        assert!(parse_task_version("1.0").is_err());
    }

    #[test]
    fn receipt_payload_drift_maps_to_conflict_and_legacy_view_has_no_follow_up() {
        let fingerprint = "c".repeat(64);
        let rejected_message = format!(
            "command_sha256={fingerprint};result=review-2|workflow-2|2|opening_pending|1700000001|REJECTED|REJECTED"
        );
        let rejected = CardFundsCommandReceipt::parse(&rejected_message, &fingerprint).unwrap();
        assert!(rejected.requires_legacy_rejected_follow_up());
        let result = complete_review_result(&rejected, "wi-1", "ra-1", "operation-1");
        assert!(result.business_result.follow_up_work_item.is_none());

        let nine_field = format!(
            "command_sha256={};result=review-1|workflow-1|1|opening_pending|1700000000|REJECTED|REJECTED|wi-2|CARD_FUNDS_REVIEW",
            "a".repeat(64)
        );
        assert!(matches!(
            map_command_receipt_error(
                CardFundsCommandReceipt::parse(&nine_field, &"b".repeat(64)).unwrap_err()
            ),
            Error::ConflictError(_)
        ));
    }
}

#[cfg(test)]
mod fact_separation_tests {
    use super::check_fact_separation;
    use erp_audit::SeparationAuditFact;

    fn fact(actor: &str, action: &str) -> SeparationAuditFact {
        SeparationAuditFact {
            resource_type: "customer_receipt".to_string(),
            resource_id: Some("cr-1".to_string()),
            actor_id: actor.to_string(),
            action: action.to_string(),
        }
    }

    const OPERATOR: &[&str] = &["customer_receipt.create", "customer_receipt.post:"];
    const FORMAL: &[&str] = &["customer_receipt.post:"];

    /// 正式证据存在且经办人不同时通过。
    #[test]
    fn formal_evidence_by_other_actor_passes() {
        let facts = [
            fact("creator-1", "customer_receipt.create"),
            fact("poster-1", "customer_receipt.post:registered"),
        ];
        let refs = facts.iter().collect::<Vec<_>>();
        assert!(check_fact_separation(&refs, "reviewer-1", OPERATOR, FORMAL).is_ok());
    }

    /// 缺正式证据 fail closed。
    #[test]
    fn missing_formal_evidence_fails_closed() {
        let facts = [fact("creator-1", "customer_receipt.create")];
        let refs = facts.iter().collect::<Vec<_>>();
        assert!(check_fact_separation(&refs, "reviewer-1", OPERATOR, FORMAL).is_err());
        let empty: Vec<&SeparationAuditFact> = Vec::new();
        assert!(check_fact_separation(&empty, "reviewer-1", OPERATOR, FORMAL).is_err());
    }

    /// 同 actor 经办冲突拒绝。
    #[test]
    fn same_actor_operator_conflict_is_rejected() {
        let facts = [
            fact("reviewer-1", "customer_receipt.create"),
            fact("poster-1", "customer_receipt.post:registered"),
        ];
        let refs = facts.iter().collect::<Vec<_>>();
        assert!(check_fact_separation(&refs, "reviewer-1", OPERATOR, FORMAL).is_err());
    }

    /// 仅非正式事件不算证据。
    #[test]
    fn informal_only_events_do_not_count_as_evidence() {
        let facts = [
            fact("creator-1", "customer_receipt.draft_saved"),
            fact("creator-1", "customer_receipt.preview"),
        ];
        let refs = facts.iter().collect::<Vec<_>>();
        assert!(check_fact_separation(&refs, "reviewer-1", OPERATOR, FORMAL).is_err());
    }
}
