//! 域 D33 `supplier_settlement` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 创建结算草稿：结算单 + 全部明细同事务（`create_statement_with_items` 要求事务
//!   执行器，§6.20）；
//! - 结算确认（§8.4 第 6 条）：锁定结算单并重验差异处理结果、形成结算单应付
//!   （D19 `PayableRepository::create_payable_with_entry`，来源类型
//!   `SupplierSettlement`）并更新结算状态，同一事务完成；最终成本差额（D20
//!   `cost_entry`）不在本域声明依赖内，见 PR「未实现且已知的缺口」。
//!
//! 跨域协作只经 DatabaseExt 调对方域 Repository（P3 §2）：D32 `supplier_fulfillment`
//! （履约订单与明细存在性）、D19 `payable`（应付账户与原始分录）。
//!
//! 资金/状态机入口一律幂等：创建键为 `statement_no`，确认/提交复核/作废重复提交
//! 返回原结算单当前视图（不重复形成应付、不重复推进状态）；差异处理以版本 CAS
//! 防并发覆盖。

use database::{
    AccessControlExt, CostExt, Executor, NoTransaction, PayableExt, SupplierSettlementExt, Transactional,
    WorkItemExt,
};
use entities::common::time::Instant;
use entities::cost::CostEntry;
use entities::ids::{PayableAccountId, PayableEntryId, WorkItemId};
use entities::money::Amount;
use entities::payable::{
    EntryDirection, PayableAccount, PayableAccountData, PayableEntry, PayableEntryData, PayableEntryType,
    PayableSourceType,
};
use entities::supplier_settlement::{
    SettlementDifferenceStatus, SettlementReviewDecision, SettlementReviewResult, SettlementStatus,
    SupplierSettlementDifference, SupplierSettlementDifferenceUpdate, SupplierSettlementItem,
    SupplierSettlementStatement, SupplierSettlementStatementUpdate,
};
use entities::work_item::{
    AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemStatus, WorkItemType,
};
use id_generator::next_id;
use mongodb::Database;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::str::FromStr;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::supplier_fulfillment::dto::SortDir;
use crate::work_item::WorkItemService;

mod draft;
mod dto;
mod evidence;
mod source;

use self::dto::StatementListQuery;
pub use self::dto::{
    CreateSettlementStatementRequest, RecordSettlementSourceEvidenceLineRequest,
    RecordSettlementSourceEvidenceRequest, RefreshSettlementStatementRequest,
    SettlementDifferenceDecisionRequest, SettlementDifferenceDecisionResult,
    SettlementDifferenceEvidenceRequest, SettlementDifferenceEvidenceResult,
    SettlementDifferenceEvidenceView, SettlementDraftAction, SettlementDraftCommandResult,
    SettlementPageView, SettlementReviewCommand, SettlementReviewDecisionResult,
    SubmitSettlementReviewRequest, SubmitSettlementReviewResult, SupplierSettlementDifferenceListParams,
    SupplierSettlementDifferenceView, SupplierSettlementItemListParams, SupplierSettlementItemView,
    SupplierSettlementSourceEvidenceQuery, SupplierSettlementSourceEvidenceView,
    SupplierSettlementStatementDetailView, SupplierSettlementStatementListParams,
    SupplierSettlementStatementListView, SupplierSettlementStatementView, VoidSettlementRequest,
};

const REVIEW_CUTOFF_POLICY_ID: &str = "supplier-settlement-review-cutoff";
const REVIEW_CUTOFF_POLICY_VERSION: &str = "1";
const SETTLEMENT_REVIEW_OWNER_ROLE: &str = "role-finance";
/// 当前结算模型尚无更细组织上下文，使用明确的最小公司根并在资格校验中重验。
const SETTLEMENT_REVIEW_OWNER_ORGANIZATION_ID: &str = "company";
const COMMAND_RECEIPT_PREFIX: &str = "supplier-settlement-command-";
const COMMAND_FINGERPRINT_PREFIX: &str = "command_sha256=";

/// 结算单列表筛选条件类型（经 `SupplierSettlementExt` 关联类型跨 crate 可达）。
type StatementFilter = <mongodb::Database as SupplierSettlementExt>::SupplierSettlementStatementFilter;
/// 结算明细列表筛选条件类型。
type ItemFilter = <mongodb::Database as SupplierSettlementExt>::SupplierSettlementItemFilter;
/// 结算差异列表筛选条件类型。
type DifferenceFilter = <mongodb::Database as SupplierSettlementExt>::SupplierSettlementDifferenceFilter;

/// 供应商结算服务。
///
/// 提供供应商周期结算单的创建、查询、复核/确认/作废与差异处理编排。
pub struct SupplierSettlementService {
    db: Database,
}

impl SupplierSettlementService {
    /// 创建供应商结算服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 分页查询供应商结算单列表。
    ///
    /// 排序字段白名单在 Service 层校验（api-contract §4），禁止任意字段透传。
    ///
    /// # 参数
    /// * `params` - 查询参数（`statement_no`/`supplier_id`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn supplier_settlement_statement_list(
        &self,
        params: &SupplierSettlementStatementListParams,
    ) -> Result<SupplierSettlementStatementListView> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = statement_filter(&query);
        let page = self
            .db
            .supplier_settlement_statements()
            .search_supplier_settlement_statements(&filter, &mut NoTransaction)
            .await?;
        let stats = self
            .db
            .supplier_settlement_statements()
            .aggregate_supplier_settlement_statement_stats(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| SupplierSettlementStatementView {
                id: row.id,
                statement_no: row.statement_no,
                supplier_id: row.supplier_id.to_string(),
                period_start: row.period_start.to_string(),
                period_end: row.period_end.to_string(),
                period_policy_id: row.period_policy_id,
                period_policy_version: row.period_policy_version,
                period_timezone: row.period_timezone,
                external_bill_no: row.external_bill_no,
                external_bill_version: row.external_bill_version,
                erp_amount: row.erp_amount,
                supplier_amount: row.supplier_amount,
                difference_amount: row.difference_amount,
                status: row.status,
                subject_hash: row.subject_hash,
                source_as_of: row.source_as_of.unix_secs(),
                source_snapshot_at: row.source_snapshot_at.unix_secs(),
                source_snapshot_hash: row.source_snapshot_hash,
                refresh_cutoff_policy_id: row.refresh_cutoff_policy_id,
                refresh_cutoff_policy_version: row.refresh_cutoff_policy_version,
                prepared_by: row.prepared_by,
                reviewed_by: row.reviewed_by,
                review_result: row.review_result,
                review_reason_code: row.review_reason_code,
                review_comment: row.review_comment,
                reviewed_at: row.reviewed_at.map(|time| time.unix_secs()),
                confirmed_at: row.confirmed_at.map(|t| t.unix_secs()),
                payable_account_id: row.payable_account_id.map(|id| id.to_string()),
                version: row.version,
                created_at: row.created_at,
            })
            .collect();

        let stats = stats.map_or_else(
            || dto::SettlementStatementListStatsView {
                pending_reconciliation_count: 0,
                has_difference_count: 0,
                pending_review_count: 0,
                confirmed_amount: zero_amount(),
            },
            |stats| dto::SettlementStatementListStatsView {
                pending_reconciliation_count: stats.pending_reconciliation_count,
                has_difference_count: stats.has_difference_count,
                pending_review_count: stats.pending_review_count,
                confirmed_amount: stats.confirmed_amount,
            },
        );
        Ok(SupplierSettlementStatementListView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
            stats,
            processing_state: if page.total == 0 { "EMPTY" } else { "READY" }.to_string(),
        })
    }

    /// 查询供应商结算单详情（结算单 + 全部明细 + 全部差异）。
    ///
    /// # 参数
    /// * `id` - 结算单 ID
    ///
    /// # 返回
    /// 返回详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 结算单不存在
    /// * `RepositoryError` - 数据库查询失败
    pub async fn supplier_settlement_statement_detail(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<SupplierSettlementStatementDetailView> {
        let statement = self.load_statement(id).await?;
        let items = self
            .db
            .supplier_settlement_items()
            .find_many_sorted(
                mongodb::bson::doc! { "statement_id": id },
                mongodb::bson::doc! { "created_at": 1 },
                &mut NoTransaction,
            )
            .await?;
        let item_id_list: Vec<String> = items.iter().map(|item| item.base.id.clone()).collect();
        let differences = self
            .db
            .supplier_settlement_differences()
            .find_many_sorted(
                mongodb::bson::doc! { "statement_item_id": { "$in": item_id_list } },
                mongodb::bson::doc! { "created_at": 1 },
                &mut NoTransaction,
            )
            .await?;
        let difference_ids = differences
            .iter()
            .map(|difference| difference.base.id.clone())
            .collect::<Vec<_>>();
        let evidence = self
            .db
            .supplier_settlement_difference_evidence()
            .find_by_difference_ids(&difference_ids, &mut NoTransaction)
            .await?;
        let mut evidence_by_difference = HashMap::<String, Vec<dto::SettlementDifferenceEvidenceView>>::new();
        for value in evidence {
            evidence_by_difference
                .entry(value.difference_id.to_string())
                .or_default()
                .push(evidence::evidence_view(value));
        }
        let evidenced_difference_count = evidence_by_difference.len();
        let pending_difference_count = differences
            .iter()
            .filter(|difference| difference.status == SettlementDifferenceStatus::Pending)
            .count();
        let (mut allowed_actions, mut action_blockers, processing_state) =
            settlement_object_actions(&statement, &differences, actor);
        let difference_count = differences.len();
        let item_count = items.len();
        let order_amount = items
            .iter()
            .fold(zero_amount(), |total, item| total.checked_add(item.order_amount));
        let freight_amount = items.iter().fold(zero_amount(), |total, item| {
            total.checked_add(item.freight_amount)
        });
        let service_fee_amount = items.iter().fold(zero_amount(), |total, item| {
            total.checked_add(item.service_fee_amount)
        });
        let refund_amount = items
            .iter()
            .fold(zero_amount(), |total, item| total.checked_add(item.refund_amount));
        let erp_amount = statement.erp_amount;
        let supplier_amount = statement.supplier_amount;
        let difference_amount = statement.difference_amount;
        let cost_delta = accepted_cost_delta(&items, &differences)?;
        let cost_adjustment_ready = cost_delta.gross == zero_amount()
            && cost_delta.net == zero_amount()
            && cost_delta.tax == zero_amount();
        let difference_views = differences
            .into_iter()
            .map(|difference| {
                let difference_id = difference.base.id.clone();
                let mut view = settlement_difference_view(difference);
                view.evidence = evidence_by_difference.remove(&difference_id).unwrap_or_default();
                view
            })
            .collect();
        let (review_work_item, review_action_blockers, review_domain_actions) =
            self.settlement_review_work_item_view(&statement, actor).await?;
        allowed_actions.extend(
            review_domain_actions
                .into_iter()
                .filter(|action| action != "CONFIRM" || cost_adjustment_ready),
        );
        if !cost_adjustment_ready {
            action_blockers.push(review_blocker(
                "CONFIRM",
                "AUTHORITATIVE_COST_ALLOCATION_MISSING",
                "当前非零成本差额尚未锁定原成本与分配链，禁止确认并伪造成本事实",
            ));
        }
        if let Some(work_item) = &review_work_item {
            action_blockers.extend(work_item.action_blockers.clone());
        }
        let review_processing_state = if review_action_blockers.is_empty() {
            dto::SettlementReviewProcessingState::Ready
        } else {
            dto::SettlementReviewProcessingState::ApprovalBlocked
        };

        Ok(SupplierSettlementStatementDetailView {
            statement: statement.into(),
            items: items.into_iter().map(settlement_item_view).collect(),
            differences: difference_views,
            stats: dto::SettlementStatementStatsView {
                item_count,
                difference_count,
                pending_difference_count,
                evidenced_difference_count,
                order_amount,
                freight_amount,
                service_fee_amount,
                refund_amount,
                erp_amount,
                supplier_amount,
                difference_amount,
            },
            processing_state,
            review_work_item,
            review_processing_state,
            review_action_blockers,
            allowed_actions,
            action_blockers,
        })
    }

    /// 提交冻结结算主题并原子创建唯一财务复核任务。
    ///
    /// 命令重验结算单版本、服务端主题摘要、刷新截止策略与完整差异结论；结算单
    /// 进入待复核、`SUPPLIER_SETTLEMENT_REVIEW` 任务和幂等审计在同一事务写入。
    ///
    /// # 错误
    /// 路径身份、版本、主题、策略或差异状态不一致时 fail-closed。
    pub async fn submit_review(
        &self,
        id: &str,
        req: SubmitSettlementReviewRequest,
        actor: &AuditActor,
    ) -> Result<SubmitSettlementReviewResult> {
        req.validate()?;
        ensure_same_id(id, &req.statement_id, "结算单")?;
        let fingerprint = submit_review_fingerprint(&req);
        let audit_id = command_audit_id(
            actor.id(),
            "supplier_settlement.submit_review",
            id,
            &req.idempotency_key,
        );
        if let Some(result) = self.replay_review_submission(&audit_id, &fingerprint, id).await? {
            return Ok(result);
        }
        let statement = self.load_statement(id).await?;
        if statement.prepared_by != actor.id() {
            return Err(Error::Forbidden("只有当前结算经办人可以提交财务复核".to_string()));
        }
        self.ensure_version(&statement, req.expected_lock_version)?;
        validate_review_submission_snapshot(&statement, &req)?;
        let work_item = WorkItem::new(
            WorkItemId::new(next_id()),
            WorkItemData {
                work_item_type: WorkItemType::SupplierSettlementReview,
                business_object_type: "supplier_settlement_statement".to_string(),
                business_object_id: statement.base.id.clone(),
                subject_version: statement.subject_hash.clone(),
                owner_role: SETTLEMENT_REVIEW_OWNER_ROLE.to_string(),
                owner_organization_id: SETTLEMENT_REVIEW_OWNER_ORGANIZATION_ID.to_string(),
                owner_user_id: req.reviewer_user_id.clone(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::High,
                due_at: None,
                reason_code: Some("supplier_settlement_review_dispatched".to_string()),
                impact_summary: Some(format!("复核供应商结算单 {}", statement.statement_no)),
            },
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let actor_id = actor.id().to_string();
        let expected_subject_hash = statement.subject_hash.clone();
        let audit_actor = actor.clone();
        let operation_id = req.operation_id.clone();
        let operation_id_for_tx = operation_id.clone();
        let fingerprint_for_tx = fingerprint.clone();
        let audit_id_for_tx = audit_id.clone();
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut current = db
                        .supplier_settlement_statements()
                        .find_by_id(&statement.base.id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("供应商结算单不存在".to_string()))?;
                    if current.base.version != statement.base.version
                        || current.subject_hash != expected_subject_hash
                    {
                        return Err(Error::ConflictError(
                            "结算单版本或主题已变化，请刷新后重试".to_string(),
                        ));
                    }
                    ensure_review_submission_ready(&db, &current, &actor_id, session).await?;
                    current.submit_review()?;
                    db.supplier_settlement_statements()
                        .update(&mut current, session)
                        .await?;
                    db.work_items().create(&work_item, session).await?;
                    let receipt = ReviewSubmissionReceipt {
                        operation_id: operation_id_for_tx,
                        statement_version: current.base.version,
                        work_item_id: work_item.base.id.clone(),
                        task_version: work_item.base.version,
                    };
                    let audit = audit_actor.resource_log_with_id(
                        audit_id_for_tx,
                        "supplier_settlement.submit_review",
                        "supplier_settlement_statement",
                        current.base.id.clone(),
                        Some(review_submission_receipt_message(&fingerprint_for_tx, &receipt)),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(SupplierSettlementStatement, WorkItem), crate::errors::Error>((current, work_item))
                })
            })
            .await;
        let (statement, work_item) = match transaction_result {
            Ok(result) => result,
            Err(error) => {
                if let Some(result) = self.replay_review_submission(&audit_id, &fingerprint, id).await? {
                    return Ok(result);
                }
                return Err(error);
            }
        };
        Ok(SubmitSettlementReviewResult {
            result_status: dto::SettlementReviewSubmissionStatus::Submitted,
            message: "结算主题已冻结并提交财务复核".to_string(),
            operation_id,
            statement: statement.into(),
            work_item_id: work_item.base.id,
        })
    }

    /// 使用当前正式任务完成供应商结算复核决定。
    ///
    /// `REJECT` 与 `CONFIRM` 均校验任务 CAS、主题摘要、结算单 CAS、当前责任、
    /// 财务角色/组织范围与岗位分离；业务事实、任务完成、审计以及确认形成的应付
    /// 和成本差额在同一事务写入。
    ///
    /// # 错误
    /// 任务、责任、主题、结算版本或正式业务前置条件不一致时 fail-closed。
    pub async fn decide_review(
        &self,
        id: &str,
        req: SettlementReviewCommand,
        actor: &AuditActor,
    ) -> Result<SettlementReviewDecisionResult> {
        req.validate()?;
        ensure_same_id(id, &req.decision.statement_id, "结算单")?;
        validate_review_decision_reason(&req)?;
        let expected_task_version = parse_expected_version(&req.expected_task_version, "待办版本")?;
        let action = match req.decision.action {
            dto::SettlementReviewAction::Reject => "supplier_settlement.review_reject",
            dto::SettlementReviewAction::Confirm => "supplier_settlement.review_confirm",
        };
        let fingerprint = review_decision_fingerprint(&req);
        let audit_id = command_audit_id(actor.id(), action, id, &req.idempotency_key);
        if let Some(result) = self
            .replay_review_decision(&audit_id, &fingerprint, id, &req.work_item_id)
            .await?
        {
            return Ok(result);
        }
        let mut statement = self.load_statement(id).await?;
        self.ensure_version(&statement, req.decision.expected_lock_version)?;
        let mut work_item = self
            .db
            .work_items()
            .find_by_id(&req.work_item_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商结算复核待办不存在".to_string()))?;
        validate_settlement_review_work_item(
            &work_item,
            &statement,
            expected_task_version,
            &req.expected_subject_version,
            actor,
        )?;
        let items = self.load_statement_items(id, &mut NoTransaction).await?;
        let differences = self
            .load_statement_differences(&items, &mut NoTransaction)
            .await?;
        ensure_current_subject_and_resolved_differences(&statement, &differences)?;
        let now = Instant::now();
        let (payable, payable_entry, cost_entries, cost_delta, result_status) = match req.decision.action {
            dto::SettlementReviewAction::Confirm => {
                ensure_confirmable_external_bill(&statement)?;
                let cost_delta = accepted_cost_delta(&items, &differences)?;
                let payable_amount = statement.erp_amount.checked_add(cost_delta.gross);
                let (account, entry) = build_settlement_payable(&statement, payable_amount, actor, now)?;
                let cost_entries = build_settlement_cost_delta(&statement, &cost_delta, now)?;
                statement.record_review(
                    SettlementReviewDecision::Confirm {
                        payable_account_id: account.base.id.clone().into(),
                        comment: req.decision.comment.clone(),
                    },
                    actor.id(),
                    now,
                )?;
                (
                    Some(account),
                    Some(entry),
                    cost_entries,
                    Some(cost_delta.gross),
                    dto::SettlementReviewDecisionStatus::Confirmed,
                )
            }
            dto::SettlementReviewAction::Reject => {
                let return_status = if differences.is_empty() {
                    SettlementStatus::Draft
                } else {
                    SettlementStatus::HasDifference
                };
                statement.record_review(
                    SettlementReviewDecision::Reject {
                        return_status,
                        reason_code: req.decision.reason_code.clone().unwrap_or_default(),
                        comment: req.decision.comment.clone(),
                    },
                    actor.id(),
                    now,
                )?;
                (
                    None,
                    None,
                    Vec::new(),
                    None,
                    dto::SettlementReviewDecisionStatus::Rejected,
                )
            }
        };
        work_item.record_activity(actor.id(), now)?;
        work_item.complete_by_domain_command(actor.id(), now)?;

        let db = self.db.clone();
        let client = db.client().clone();
        let actor_id = actor.id().to_string();
        let audit_actor = actor.clone();
        let rbac_for_tx = crate::iam::shared_rbac_service(self.db.clone());
        let operation_id = req.decision.operation_id.clone();
        let operation_id_for_tx = operation_id.clone();
        let fingerprint_for_tx = fingerprint.clone();
        let audit_id_for_tx = audit_id.clone();
        let action_for_tx = action.to_string();
        let result_status_for_tx = result_status;
        let transaction_result =
            client
                .with_transaction(move |session| {
                    Box::pin(async move {
                        WorkItemService::new(db.clone(), rbac_for_tx.clone())
                            .ensure_domain_decision_access(&audit_actor, &work_item, session)
                            .await?;
                        ensure_settlement_reviewer_eligible(&db, &work_item, &statement, &actor_id, session)
                            .await?;
                        let tx_items = load_statement_items(&db, &statement.base.id, session).await?;
                        let tx_differences = load_statement_differences(&db, &tx_items, session).await?;
                        ensure_current_subject_and_resolved_differences(&statement, &tx_differences)?;
                        db.supplier_settlement_statements()
                            .update(&mut statement, session)
                            .await?;
                        db.work_items().update(&mut work_item, session).await?;
                        if let (Some(account), Some(entry)) = (&payable, &payable_entry) {
                            db.payable()
                                .create_payable_with_entry(account, entry, session)
                                .await?;
                        }
                        for cost_entry in &cost_entries {
                            db.cost()
                                .create_cost_entry_with_allocations(cost_entry, Vec::new(), session)
                                .await?;
                        }
                        let receipt = ReviewDecisionReceipt {
                            operation_id: operation_id_for_tx,
                            result_status: result_status_for_tx,
                            statement_version: statement.base.version,
                            task_version: work_item.base.version,
                            payable_account_id: payable.as_ref().map(|account| account.base.id.clone()),
                            cost_delta,
                        };
                        let audit = audit_actor.resource_log_with_id(
                            audit_id_for_tx,
                            &action_for_tx,
                            "supplier_settlement_statement",
                            statement.base.id.clone(),
                            Some(review_decision_receipt_message(&fingerprint_for_tx, &receipt)),
                        )?;
                        db.audit_logs().create(&audit, session).await?;
                        Ok::<
                            (SupplierSettlementStatement, WorkItem, ReviewDecisionReceipt),
                            crate::errors::Error,
                        >((statement, work_item, receipt))
                    })
                })
                .await;
        let (statement, work_item, receipt) = match transaction_result {
            Ok(result) => result,
            Err(error) => {
                if let Some(result) = self
                    .replay_review_decision(&audit_id, &fingerprint, id, &req.work_item_id)
                    .await?
                {
                    return Ok(result);
                }
                return Err(error);
            }
        };
        Ok(review_decision_result(
            statement,
            work_item,
            operation_id,
            receipt,
        ))
    }

    /// 作废尚未提交复核的结算草稿。
    ///
    /// # 参数
    /// * `id` - 结算单 ID
    /// * `req` - 作废请求（含期望版本与原因）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回作废后结算单的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 结算单不存在
    /// * `ConflictError` - 版本冲突
    pub async fn void_statement(
        &self,
        id: &str,
        req: VoidSettlementRequest,
        actor: &AuditActor,
    ) -> Result<SupplierSettlementStatementView> {
        req.validate()?;
        let mut statement = self.load_statement(id).await?;
        if statement.status == SettlementStatus::Voided {
            return Ok(statement.into());
        }
        if statement.prepared_by != actor.id()
            || !matches!(
                statement.status,
                SettlementStatus::Draft
                    | SettlementStatus::PendingReconciliation
                    | SettlementStatus::HasDifference
            )
        {
            return Err(Error::BusinessLogicError(
                "只有经办人可以作废尚未提交复核的结算草稿".to_string(),
            ));
        }
        self.ensure_version(&statement, req.version)?;
        statement.update(SupplierSettlementStatementUpdate {
            status: Some(SettlementStatus::Voided),
            ..Default::default()
        })?;
        let audit = actor.clone().resource_log(
            "supplier_settlement.void",
            "supplier_settlement_statement",
            id.to_string(),
        )?;
        self.update_statement_with_audit(&mut statement, &audit).await?;
        Ok(statement.into())
    }

    /// 分页查询供应商结算明细列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`statement_id` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn supplier_settlement_item_list(
        &self,
        params: &SupplierSettlementItemListParams,
    ) -> Result<SettlementPageView<SupplierSettlementItemView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = ItemFilter {
            statement_id: query.statement_id,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .supplier_settlement_items()
            .search_supplier_settlement_items(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| SupplierSettlementItemView {
                id: row.id,
                statement_id: row.statement_id.to_string(),
                supplier_fulfillment_order_id: row.supplier_fulfillment_order_id.to_string(),
                supplier_fulfillment_item_id: row.supplier_fulfillment_item_id.to_string(),
                quantity: row.quantity,
                order_amount: row.order_amount,
                freight_amount: row.freight_amount,
                service_fee_amount: row.service_fee_amount,
                refund_amount: row.refund_amount,
                erp_calculated_amount: row.erp_calculated_amount,
                erp_calculated_net_amount: row.erp_calculated_net_amount,
                erp_calculated_tax_amount: row.erp_calculated_tax_amount,
                supplier_billed_amount: row.supplier_billed_amount,
                supplier_billed_net_amount: row.supplier_billed_net_amount,
                supplier_billed_tax_amount: row.supplier_billed_tax_amount,
                created_at: row.created_at,
            })
            .collect();

        Ok(SettlementPageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 分页查询供应商结算差异列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`statement_item_id`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn supplier_settlement_difference_list(
        &self,
        params: &SupplierSettlementDifferenceListParams,
    ) -> Result<SettlementPageView<SupplierSettlementDifferenceView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = DifferenceFilter {
            statement_item_id: query.statement_item_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .supplier_settlement_differences()
            .search_supplier_settlement_differences(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| SupplierSettlementDifferenceView {
                id: row.id,
                statement_item_id: row.statement_item_id.to_string(),
                difference_type: row.difference_type,
                difference_amount: row.difference_amount,
                status: row.status,
                resolution: row.resolution,
                resolved_by: row.resolved_by,
                resolved_at: row.resolved_at.map(|t| t.unix_secs()),
                version: row.version,
                created_at: row.created_at,
                evidence: Vec::new(),
            })
            .collect();

        Ok(SettlementPageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 登记财务经办的强类型差异结论。
    ///
    /// 命令同时 CAS 结算单与差异版本，规范化受控原因/证据，推进主题摘要并写
    /// 幂等审计。客户端不能提交处理人、处理时间或任意持久化状态。
    ///
    /// # 错误
    /// 路径身份、归属、版本、经办责任或证据规则不一致时 fail-closed。
    pub async fn decide_difference(
        &self,
        id: &str,
        req: SettlementDifferenceDecisionRequest,
        actor: &AuditActor,
    ) -> Result<SettlementDifferenceDecisionResult> {
        req.validate()?;
        ensure_same_id(id, &req.difference_id, "结算差异")?;
        let evidence_reference_ids =
            normalized_evidence_references(&req.evidence_reference_ids, req.resolution)?;
        let reason_code = normalized_difference_reason_code(&req.reason_code, req.resolution)?;
        let fingerprint = difference_decision_fingerprint(&req, &reason_code, &evidence_reference_ids);
        let audit_id = command_audit_id(
            actor.id(),
            "supplier_settlement.difference_decision",
            id,
            &req.idempotency_key,
        );
        if let Some(result) = self
            .replay_difference_decision(&audit_id, &fingerprint, id)
            .await?
        {
            return Ok(result);
        }
        let mut difference = self
            .db
            .supplier_settlement_differences()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("结算差异不存在".to_string()))?;
        if difference.base.version != req.expected_difference_version {
            return Err(Error::ConflictError(
                "结算差异版本已变化，请刷新后重试".to_string(),
            ));
        }
        if difference.status != SettlementDifferenceStatus::Pending {
            return Err(Error::ConflictError("结算差异已有正式结论".to_string()));
        }
        let item = self
            .db
            .supplier_settlement_items()
            .find_by_id(&difference.statement_item_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("结算差异所属明细不存在".to_string()))?;
        ensure_same_id(&req.statement_id, item.statement_id.as_ref(), "结算单")?;
        let mut statement = self.load_statement(&req.statement_id).await?;
        self.ensure_version(&statement, req.expected_lock_version)?;
        if statement.prepared_by != actor.id() {
            return Err(Error::Forbidden(
                "只有当前结算经办人可以登记正式差异结论".to_string(),
            ));
        }
        if !matches!(
            statement.status,
            SettlementStatus::Draft
                | SettlementStatus::PendingReconciliation
                | SettlementStatus::HasDifference
        ) {
            return Err(Error::BusinessLogicError("当前结算状态禁止处理差异".to_string()));
        }
        if !evidence_reference_ids.is_empty() {
            let stored_evidence = self
                .db
                .supplier_settlement_difference_evidence()
                .find_by_difference_ids(&[difference.base.id.clone()], &mut NoTransaction)
                .await?;
            let stored_references = stored_evidence
                .iter()
                .flat_map(|evidence| evidence.evidence_reference_ids.iter())
                .map(String::as_str)
                .collect::<std::collections::HashSet<_>>();
            if evidence_reference_ids
                .iter()
                .any(|reference| !stored_references.contains(reference.as_str()))
            {
                return Err(Error::BusinessLogicError(
                    "差异决定引用了尚未通过补证命令登记的证据".to_string(),
                ));
            }
        }
        let resolution = formal_difference_resolution(&reason_code, &evidence_reference_ids)?;
        let now = Instant::now();
        difference.update(SupplierSettlementDifferenceUpdate {
            status: Some(req.resolution.status()),
            resolution: Some(resolution),
            resolved_by: Some(actor.id().to_string()),
            resolved_at: Some(now),
        })?;
        let items = self
            .load_statement_items(&statement.base.id, &mut NoTransaction)
            .await?;
        let mut differences = self
            .load_statement_differences(&items, &mut NoTransaction)
            .await?;
        let stored = differences
            .iter_mut()
            .find(|stored| stored.base.id == difference.base.id)
            .ok_or_else(|| Error::ConflictError("结算差异已不属于当前结算单".to_string()))?;
        *stored = difference.clone();
        statement.update(SupplierSettlementStatementUpdate {
            status: Some(SettlementStatus::HasDifference),
            ..Default::default()
        })?;
        statement.update_subject_hash(settlement_subject_hash(&statement, &differences))?;
        let db = self.db.clone();
        let client = db.client().clone();
        let audit_actor = actor.clone();
        let operation_id = req.operation_id.clone();
        let operation_id_for_tx = operation_id.clone();
        let fingerprint_for_tx = fingerprint.clone();
        let audit_id_for_tx = audit_id.clone();
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.supplier_settlement_statements()
                        .update(&mut statement, session)
                        .await?;
                    db.supplier_settlement_differences()
                        .update(&mut difference, session)
                        .await?;
                    let receipt = DifferenceDecisionReceipt {
                        operation_id: operation_id_for_tx,
                        statement_id: statement.base.id.clone(),
                        statement_version: statement.base.version,
                        difference_version: difference.base.version,
                    };
                    let audit = audit_actor.resource_log_with_id(
                        audit_id_for_tx,
                        "supplier_settlement.difference_decision",
                        "supplier_settlement_difference",
                        difference.base.id.clone(),
                        Some(difference_decision_receipt_message(&fingerprint_for_tx, &receipt)),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(SupplierSettlementStatement, SupplierSettlementDifference), crate::errors::Error>((
                        statement, difference,
                    ))
                })
            })
            .await;
        let (statement, difference) = match transaction_result {
            Ok(result) => result,
            Err(error) => {
                if let Some(result) = self
                    .replay_difference_decision(&audit_id, &fingerprint, id)
                    .await?
                {
                    return Ok(result);
                }
                return Err(error);
            }
        };
        Ok(SettlementDifferenceDecisionResult {
            result_status: dto::SettlementDifferenceDecisionStatus::Resolved,
            message: "结算差异正式结论已登记".to_string(),
            operation_id,
            statement_id: statement.base.id,
            statement_lock_version: statement.base.version,
            difference: settlement_difference_view(difference),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReviewSubmissionReceipt {
    operation_id: String,
    statement_version: u64,
    work_item_id: String,
    task_version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReviewDecisionReceipt {
    operation_id: String,
    result_status: dto::SettlementReviewDecisionStatus,
    statement_version: u64,
    task_version: u64,
    payable_account_id: Option<String>,
    cost_delta: Option<Amount>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DifferenceDecisionReceipt {
    operation_id: String,
    statement_id: String,
    statement_version: u64,
    difference_version: u64,
}

async fn load_statement_items(
    db: &Database,
    statement_id: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<SupplierSettlementItem>> {
    db.supplier_settlement_items()
        .find_many_sorted(
            mongodb::bson::doc! { "statement_id": statement_id },
            mongodb::bson::doc! { "created_at": 1, "id": 1 },
            executor,
        )
        .await
        .map_err(Into::into)
}

async fn load_statement_differences(
    db: &Database,
    items: &[SupplierSettlementItem],
    executor: &mut dyn Executor,
) -> Result<Vec<SupplierSettlementDifference>> {
    let item_ids = items.iter().map(|item| item.base.id.clone()).collect::<Vec<_>>();
    if item_ids.is_empty() {
        return Ok(Vec::new());
    }
    db.supplier_settlement_differences()
        .find_many_sorted(
            mongodb::bson::doc! { "statement_item_id": { "$in": item_ids } },
            mongodb::bson::doc! { "created_at": 1, "id": 1 },
            executor,
        )
        .await
        .map_err(Into::into)
}

async fn ensure_review_submission_ready(
    db: &Database,
    statement: &SupplierSettlementStatement,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    if statement.prepared_by != actor_id
        || !matches!(
            statement.status,
            SettlementStatus::Draft
                | SettlementStatus::PendingReconciliation
                | SettlementStatus::HasDifference
        )
    {
        return Err(Error::ConflictError(
            "结算单责任或状态已变化，请刷新后重试".to_string(),
        ));
    }
    let items = load_statement_items(db, &statement.base.id, executor).await?;
    if items.is_empty() {
        return Err(Error::BusinessLogicError("结算单没有冻结明细".to_string()));
    }
    let differences = load_statement_differences(db, &items, executor).await?;
    ensure_current_subject_and_resolved_differences(statement, &differences)
}

fn validate_review_submission_snapshot(
    statement: &SupplierSettlementStatement,
    req: &SubmitSettlementReviewRequest,
) -> Result<()> {
    if !matches!(req.action, dto::SettlementObjectAction::SubmitReview)
        || req.subject_hash != statement.subject_hash
        || req.refresh_cutoff_policy_id != statement.refresh_cutoff_policy_id
        || req.expected_refresh_cutoff_policy_version != statement.refresh_cutoff_policy_version
    {
        return Err(Error::ConflictError(
            "结算主题或刷新截止策略已变化，请刷新后重试".to_string(),
        ));
    }
    Ok(())
}

fn validate_settlement_review_work_item(
    item: &WorkItem,
    statement: &SupplierSettlementStatement,
    expected_task_version: u64,
    expected_subject_version: &str,
    actor: &AuditActor,
) -> Result<()> {
    if item.base.version != expected_task_version {
        return Err(Error::ConflictError(
            "复核任务责任或版本已变化，请刷新后重试".to_string(),
        ));
    }
    if expected_subject_version != statement.subject_hash || item.subject_version != statement.subject_hash {
        return Err(Error::ConflictError(
            "结算复核主题已变化，请刷新后重试".to_string(),
        ));
    }
    if statement.status != SettlementStatus::PendingReview
        || item.work_item_type != WorkItemType::SupplierSettlementReview
        || item.business_object_type != "supplier_settlement_statement"
        || item.business_object_id != statement.base.id
        || item.owner_role != SETTLEMENT_REVIEW_OWNER_ROLE
        || item.owner_organization_id != SETTLEMENT_REVIEW_OWNER_ORGANIZATION_ID
    {
        return Err(Error::BusinessLogicError(
            "待办与当前供应商结算复核不匹配".to_string(),
        ));
    }
    if !item.is_owned_by(actor.id()) {
        return Err(Error::Forbidden(
            "当前账号不是该复核任务责任人，或处理权已变化".to_string(),
        ));
    }
    Ok(())
}

async fn ensure_settlement_reviewer_eligible(
    db: &Database,
    item: &WorkItem,
    statement: &SupplierSettlementStatement,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let _ = (db, item, executor);
    if statement.prepared_by == actor_id {
        return Err(Error::Forbidden("结算经办人不得复核自己的结算单".to_string()));
    }
    Ok(())
}

fn ensure_current_subject_and_resolved_differences(
    statement: &SupplierSettlementStatement,
    differences: &[SupplierSettlementDifference],
) -> Result<()> {
    if differences
        .iter()
        .any(|difference| difference.status == SettlementDifferenceStatus::Pending)
    {
        return Err(Error::BusinessLogicError(
            "存在未解决差异，禁止提交或确认结算".to_string(),
        ));
    }
    if statement.subject_hash != settlement_subject_hash(statement, differences) {
        return Err(Error::ConflictError(
            "结算主题摘要与当前差异结论不一致，请刷新后重试".to_string(),
        ));
    }
    Ok(())
}

fn settlement_review_access(
    item: &WorkItem,
    actor_id: &str,
    eligible: bool,
    separation_satisfied: bool,
) -> (Vec<String>, Vec<dto::SettlementReviewActionBlockerView>) {
    if !eligible {
        return (
            Vec::new(),
            vec![review_blocker(
                "REVIEW_DECISION",
                "ASSIGNMENT_NOT_ELIGIBLE",
                "当前账号不具备该任务的财务角色或组织数据范围",
            )],
        );
    }
    if !separation_satisfied {
        return (
            Vec::new(),
            vec![review_blocker(
                "REVIEW_DECISION",
                "SEGREGATION_OF_DUTIES",
                "结算经办人不得复核自己的结算单",
            )],
        );
    }
    if item.is_owned_by(actor_id) {
        return (vec!["REJECT".to_string(), "CONFIRM".to_string()], Vec::new());
    }
    (
        Vec::new(),
        vec![review_blocker(
            "REVIEW_DECISION",
            "CURRENT_OWNER_MISMATCH",
            "该复核任务当前由其他账号负责",
        )],
    )
}

fn review_blocker(action: &str, code: &str, message: &str) -> dto::SettlementReviewActionBlockerView {
    dto::SettlementReviewActionBlockerView {
        action: action.to_string(),
        code: code.to_string(),
        message: message.to_string(),
    }
}

/// 由服务端状态、责任人与差异事实投影当前对象动作和处理态。
fn settlement_object_actions(
    statement: &SupplierSettlementStatement,
    differences: &[SupplierSettlementDifference],
    actor: &AuditActor,
) -> (Vec<String>, Vec<dto::SettlementReviewActionBlockerView>, String) {
    let editable = matches!(
        statement.status,
        SettlementStatus::Draft | SettlementStatus::PendingReconciliation | SettlementStatus::HasDifference
    );
    let pending = differences
        .iter()
        .filter(|difference| difference.status == SettlementDifferenceStatus::Pending)
        .count();
    let processing_state = if editable && pending == 0 {
        "READY_FOR_REVIEW"
    } else {
        match statement.status {
            SettlementStatus::Draft
            | SettlementStatus::PendingReconciliation
            | SettlementStatus::HasDifference => "EVIDENCE_OR_DECISION_REQUIRED",
            SettlementStatus::PendingReview => "REVIEW_PENDING",
            SettlementStatus::Confirmed => "COMPLETED",
            SettlementStatus::Voided => "VOIDED",
        }
    }
    .to_string();
    let mut actions = Vec::new();
    let mut blockers = Vec::new();
    if editable {
        actions.push("APPEND_EVIDENCE".to_string());
    }
    if editable && statement.prepared_by == actor.id() {
        actions.extend([
            "REFRESH_TRIAL".to_string(),
            "RESOLVE_DIFFERENCE".to_string(),
            "VOID_DRAFT".to_string(),
        ]);
        if pending == 0 {
            actions.push("SUBMIT_REVIEW".to_string());
        } else {
            blockers.push(review_blocker(
                "SUBMIT_REVIEW",
                "PENDING_DIFFERENCES",
                "存在未处理差异，禁止提交财务复核",
            ));
        }
    } else if editable {
        for action in [
            "REFRESH_TRIAL",
            "RESOLVE_DIFFERENCE",
            "VOID_DRAFT",
            "SUBMIT_REVIEW",
        ] {
            blockers.push(review_blocker(
                action,
                "PREPARER_REQUIRED",
                "该动作仅允许当前结算经办人执行",
            ));
        }
    } else {
        blockers.push(review_blocker(
            "EDIT_DRAFT",
            "STATEMENT_NOT_EDITABLE",
            "当前结算状态不允许刷新、补证或处理差异",
        ));
    }
    (actions, blockers, processing_state)
}

fn validate_review_decision_reason(req: &SettlementReviewCommand) -> Result<()> {
    match req.decision.action {
        dto::SettlementReviewAction::Reject => {
            let reason = normalized_reason_code(req.decision.reason_code.as_deref().unwrap_or_default())?;
            if !["NEEDS_MORE_EVIDENCE", "AMOUNT_MISMATCH", "OTHER"].contains(&reason.as_str()) {
                return Err(Error::ValidationError("结算驳回原因代码不受支持".to_string()));
            }
        }
        dto::SettlementReviewAction::Confirm if req.decision.reason_code.is_some() => {
            return Err(Error::ValidationError("确认结算不得携带驳回原因代码".to_string()));
        }
        dto::SettlementReviewAction::Confirm => {}
    }
    Ok(())
}

fn ensure_confirmable_external_bill(statement: &SupplierSettlementStatement) -> Result<()> {
    if statement.external_bill_no.is_none() || statement.external_bill_version.is_none() {
        return Err(Error::BusinessLogicError(
            "供应商账单身份未完整冻结，禁止确认结算".to_string(),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SettlementCostDelta {
    gross: Amount,
    net: Amount,
    tax: Amount,
}

/// 从 ERP 接受的正式差异结论计算逐明细成本差额。
///
/// 差异记录当前只冻结有符号含税金额，因此同一明细的全部 `ERP_ACCEPTED`
/// 差异含税合计必须精确等于该明细供应商侧与 ERP 侧的含税差。各分量差只从
/// 明细已冻结的两侧 `gross/net/tax` 三元组计算，禁止按含税金额猜税额。
fn accepted_cost_delta(
    items: &[SupplierSettlementItem],
    differences: &[SupplierSettlementDifference],
) -> Result<SettlementCostDelta> {
    let mut item_by_id = HashMap::with_capacity(items.len());
    for item in items {
        if item_by_id.insert(item.base.id.as_str(), item).is_some() {
            return Err(Error::BusinessLogicError(
                "结算快照包含重复明细，禁止确认".to_string(),
            ));
        }
    }

    let mut accepted_gross_by_item: HashMap<&str, Amount> = HashMap::new();
    for difference in differences
        .iter()
        .filter(|difference| difference.status == SettlementDifferenceStatus::ErpAcknowledged)
    {
        let item_id = difference.statement_item_id.as_ref();
        if !item_by_id.contains_key(item_id) {
            return Err(Error::BusinessLogicError(
                "ERP接受差异未指向当前结算快照明细，禁止确认".to_string(),
            ));
        }
        accepted_gross_by_item
            .entry(item_id)
            .and_modify(|amount| *amount = amount.checked_add(difference.difference_amount))
            .or_insert(difference.difference_amount);
    }

    let mut gross_total = zero_amount();
    let mut net_total = zero_amount();
    let mut tax_total = zero_amount();
    for (item_id, accepted_gross) in accepted_gross_by_item {
        let item = item_by_id[item_id];
        let gross = item
            .supplier_billed_amount
            .checked_sub(item.erp_calculated_amount);
        if accepted_gross != gross {
            return Err(Error::BusinessLogicError(format!(
                "结算明细 {item_id} 的 ERP_ACCEPTED 含税差异合计与冻结双方金额不一致"
            )));
        }
        let net = item
            .supplier_billed_net_amount
            .checked_sub(item.erp_calculated_net_amount);
        let tax = item
            .supplier_billed_tax_amount
            .checked_sub(item.erp_calculated_tax_amount);
        if net.checked_add(tax) != gross {
            return Err(Error::BusinessLogicError(format!(
                "结算明细 {item_id} 的冻结差额不满足 gross = net + tax"
            )));
        }
        gross_total = gross_total.checked_add(gross);
        net_total = net_total.checked_add(net);
        tax_total = tax_total.checked_add(tax);
    }
    if net_total.checked_add(tax_total) != gross_total {
        return Err(Error::BusinessLogicError(
            "结算成本差额汇总不满足 gross = net + tax".to_string(),
        ));
    }
    Ok(SettlementCostDelta {
        gross: gross_total,
        net: net_total,
        tax: tax_total,
    })
}

fn build_settlement_payable(
    statement: &SupplierSettlementStatement,
    amount: Amount,
    actor: &AuditActor,
    at: Instant,
) -> Result<(PayableAccount, PayableEntry)> {
    let account = PayableAccount::new(
        PayableAccountId::new(next_id()),
        PayableAccountData {
            source_document_id: statement.statement_no.clone(),
            supplier_id: statement.supplier_id.clone(),
            source_type: PayableSourceType::SupplierSettlement,
            gross_total: amount,
            settled_total: zero_amount(),
            invoiceable_total: amount,
            invoiced_total: zero_amount(),
        },
        actor.id(),
    )?;
    let entry = PayableEntry::new(
        PayableEntryId::new(next_id()),
        PayableEntryData {
            payable_account_id: account.base.id.clone().into(),
            entry_type: PayableEntryType::Original,
            direction: EntryDirection::Increase,
            amount,
            due_date: statement.period_end,
            source_fact_type: "supplier_settlement".to_string(),
            source_document_id: statement.statement_no.clone(),
            source_revision_id: statement.subject_hash.clone(),
            source_sequence: 1,
            posted_at: at,
        },
    )?;
    Ok((account, entry))
}

fn build_settlement_cost_delta(
    _statement: &SupplierSettlementStatement,
    delta: &SettlementCostDelta,
    _at: Instant,
) -> Result<Vec<CostEntry>> {
    if delta.gross == zero_amount() && delta.net == zero_amount() && delta.tax == zero_amount() {
        return Ok(Vec::new());
    }
    Err(Error::BusinessLogicError(
        "ERP_ACCEPTED 成本差额暂缺权威原成本、税率与消费分配链，禁止伪造 CostEntry".to_string(),
    ))
}

/// 校验路径身份与命令载荷身份一致。
fn ensure_same_id(path_id: &str, command_id: &str, object_name: &str) -> Result<()> {
    if path_id != command_id {
        return Err(Error::ValidationError(format!(
            "{object_name}路径ID与命令载荷不一致"
        )));
    }
    Ok(())
}

/// 解析跨端字符串版本并拒绝零版本。
fn parse_expected_version(value: &str, field: &str) -> Result<u64> {
    let version = value
        .trim()
        .parse::<u64>()
        .map_err(|_| Error::ValidationError(format!("{field}必须是正整数")))?;
    if version == 0 {
        return Err(Error::ValidationError(format!("{field}必须大于0")));
    }
    Ok(version)
}

/// 规范化受控原因代码。
fn normalized_reason_code(value: &str) -> Result<String> {
    let value = value.trim().to_ascii_uppercase();
    if value.is_empty() {
        return Err(Error::ValidationError("原因代码不能为空".to_string()));
    }
    if value.len() > 64 {
        return Err(Error::ValidationError("原因代码长度不能超过64".to_string()));
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(Error::ValidationError(
            "原因代码只能包含大写字母、数字、下划线、连字符或点".to_string(),
        ));
    }
    Ok(value)
}

/// 校验差异结论与受控原因代码的固定组合。
fn normalized_difference_reason_code(
    value: &str,
    resolution: dto::SettlementDifferenceResolution,
) -> Result<String> {
    let value = normalized_reason_code(value)?;
    let allowed: &[&str] = match resolution {
        dto::SettlementDifferenceResolution::SupplierAccepted => {
            &["BILL_ALIGNED", "NO_BUSINESS_IMPACT", "OTHER"]
        }
        dto::SettlementDifferenceResolution::ErpAccepted => &["ACCEPT_BILL", "AMOUNT_MISMATCH", "OTHER"],
        dto::SettlementDifferenceResolution::Compensated => &["COMPENSATED_ELSEWHERE", "OTHER"],
        dto::SettlementDifferenceResolution::ClosedNoAdjustment => &["NO_BUSINESS_IMPACT", "OTHER"],
    };
    if !allowed.contains(&value.as_str()) {
        return Err(Error::ValidationError(
            "差异结论与原因代码组合不受支持".to_string(),
        ));
    }
    Ok(value)
}

/// 规范化正式证据引用并强制需要证据的结论完整。
fn normalized_evidence_references(
    values: &[String],
    resolution: dto::SettlementDifferenceResolution,
) -> Result<Vec<String>> {
    if values.len() > 20 {
        return Err(Error::ValidationError("证据引用最多20项".to_string()));
    }
    let mut values = values
        .iter()
        .map(|value| value.trim().to_string())
        .collect::<Vec<_>>();
    if values.iter().any(|value| {
        value.is_empty()
            || value.len() > 128
            || value
                .chars()
                .any(|character| matches!(character, '|' | ';' | ','))
    }) {
        return Err(Error::ValidationError(
            "证据引用必须非空、长度不超过128且不得包含分隔符".to_string(),
        ));
    }
    values.sort();
    values.dedup();
    if matches!(
        resolution,
        dto::SettlementDifferenceResolution::Compensated
            | dto::SettlementDifferenceResolution::ClosedNoAdjustment
    ) && values.is_empty()
    {
        return Err(Error::ValidationError(
            "已补偿或无需调整关闭必须提供正式证据引用".to_string(),
        ));
    }
    Ok(values)
}

/// 编码差异正式结论的受控原因与证据引用。
fn formal_difference_resolution(reason_code: &str, evidence_reference_ids: &[String]) -> Result<String> {
    let value = format!(
        "reason={reason_code};evidence={}",
        evidence_reference_ids.join(",")
    );
    if value.len() > 512 {
        return Err(Error::ValidationError(
            "差异结论的原因与证据引用合计不能超过512字节".to_string(),
        ));
    }
    Ok(value)
}

/// 对字段逐项加入长度前缀后计算稳定摘要，消除拼接歧义。
fn digest_parts(parts: &[String]) -> String {
    let mut digest = Sha256::new();
    for part in parts {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part.as_bytes());
    }
    hex::encode(digest.finalize())
}

/// 计算不可变结算来源快照摘要。
/// 计算提交复核的正式主题摘要。
///
/// 摘要只覆盖冻结业务事实，不覆盖可变状态、实体元数据版本或复核结果，因此同一
/// 主题在 `PENDING_REVIEW` 与正式决定事务内保持不变。
fn settlement_subject_hash(
    statement: &SupplierSettlementStatement,
    differences: &[SupplierSettlementDifference],
) -> String {
    let mut parts = vec![
        "supplier-settlement-review-subject-v1".to_string(),
        statement.base.id.clone(),
        statement.statement_no.clone(),
        statement.supplier_id.to_string(),
        statement.period_start.to_string(),
        statement.period_end.to_string(),
        statement.period_policy_id.clone(),
        statement.period_policy_version.clone(),
        statement.period_timezone.clone(),
        statement.external_bill_no.clone().unwrap_or_default(),
        statement.external_bill_version.clone().unwrap_or_default(),
        statement.erp_amount.to_string(),
        statement.supplier_amount.to_string(),
        statement.difference_amount.to_string(),
        statement.source_as_of.unix_secs().to_string(),
        statement.source_snapshot_at.unix_secs().to_string(),
        statement.source_snapshot_hash.clone(),
        statement.refresh_cutoff_policy_id.clone(),
        statement.refresh_cutoff_policy_version.clone(),
    ];
    let mut differences = differences.iter().collect::<Vec<_>>();
    differences.sort_by(|left, right| left.base.id.cmp(&right.base.id));
    for difference in differences {
        parts.extend([
            difference.base.id.clone(),
            difference.statement_item_id.to_string(),
            difference.difference_type.as_str().to_string(),
            difference.difference_amount.to_string(),
            difference.status.as_str().to_string(),
            difference.resolution.clone().unwrap_or_default(),
            difference.resolved_by.clone().unwrap_or_default(),
            difference
                .resolved_at
                .map(|value| value.unix_secs().to_string())
                .unwrap_or_default(),
        ]);
    }
    digest_parts(&parts)
}

/// 计算提交复核命令指纹。
fn submit_review_fingerprint(req: &SubmitSettlementReviewRequest) -> String {
    digest_parts(&[
        "SUBMIT_REVIEW".to_string(),
        req.statement_id.clone(),
        req.expected_lock_version.to_string(),
        req.subject_hash.clone(),
        req.refresh_cutoff_policy_id.clone(),
        req.expected_refresh_cutoff_policy_version.clone(),
        req.reviewer_user_id.clone(),
        req.operation_id.clone(),
        req.comment.clone().unwrap_or_default(),
    ])
}

/// 计算正式复核决定命令指纹。
fn review_decision_fingerprint(req: &SettlementReviewCommand) -> String {
    let action = match req.decision.action {
        dto::SettlementReviewAction::Reject => "REJECT",
        dto::SettlementReviewAction::Confirm => "CONFIRM",
    };
    digest_parts(&[
        req.work_item_id.clone(),
        req.expected_task_version.trim().to_string(),
        req.expected_subject_version.clone(),
        req.decision.statement_id.clone(),
        req.decision.expected_lock_version.to_string(),
        action.to_string(),
        req.decision.operation_id.clone(),
        req.decision
            .reason_code
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .to_ascii_uppercase(),
        req.decision.comment.clone().unwrap_or_default(),
    ])
}

/// 计算差异决定命令指纹。
fn difference_decision_fingerprint(
    req: &SettlementDifferenceDecisionRequest,
    reason_code: &str,
    evidence_reference_ids: &[String],
) -> String {
    digest_parts(&[
        req.statement_id.clone(),
        req.difference_id.clone(),
        req.expected_lock_version.to_string(),
        req.expected_difference_version.to_string(),
        req.resolution.as_str().to_string(),
        reason_code.to_string(),
        evidence_reference_ids.join(","),
        req.operation_id.clone(),
    ])
}

/// 生成不暴露原始幂等键的稳定审计收据 ID。
fn command_audit_id(actor_id: &str, action: &str, resource_id: &str, idempotency_key: &str) -> String {
    let digest = digest_parts(&[
        actor_id.to_string(),
        action.to_string(),
        resource_id.to_string(),
        idempotency_key.to_string(),
    ]);
    format!("{COMMAND_RECEIPT_PREFIX}{digest}")
}

/// 编码提交复核幂等收据。
fn review_submission_receipt_message(fingerprint: &str, receipt: &ReviewSubmissionReceipt) -> String {
    format!(
        "{COMMAND_FINGERPRINT_PREFIX}{fingerprint};result={}|{}|{}|{}",
        receipt.operation_id, receipt.statement_version, receipt.work_item_id, receipt.task_version,
    )
}

/// 解析并校验提交复核幂等收据。
fn parse_review_submission_receipt(
    message: &str,
    expected_fingerprint: &str,
) -> Result<ReviewSubmissionReceipt> {
    let result = receipt_result(message, expected_fingerprint, "提交复核")?;
    let fields = result.split('|').collect::<Vec<_>>();
    let [operation_id, statement_version, work_item_id, task_version] = fields.as_slice() else {
        return Err(Error::Internal("提交复核幂等收据结果非法".to_string()));
    };
    Ok(ReviewSubmissionReceipt {
        operation_id: (*operation_id).to_string(),
        statement_version: parse_receipt_number(statement_version, "结算单版本")?,
        work_item_id: (*work_item_id).to_string(),
        task_version: parse_receipt_number(task_version, "待办版本")?,
    })
}

/// 编码正式复核决定幂等收据。
fn review_decision_receipt_message(fingerprint: &str, receipt: &ReviewDecisionReceipt) -> String {
    let status = match receipt.result_status {
        dto::SettlementReviewDecisionStatus::Confirmed => "C",
        dto::SettlementReviewDecisionStatus::Rejected => "R",
    };
    format!(
        "{COMMAND_FINGERPRINT_PREFIX}{fingerprint};result={}|{status}|{}|{}|{}|{}",
        receipt.operation_id,
        receipt.statement_version,
        receipt.task_version,
        receipt.payable_account_id.as_deref().unwrap_or("-"),
        receipt
            .cost_delta
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".to_string()),
    )
}

/// 解析并校验正式复核决定幂等收据。
fn parse_review_decision_receipt(message: &str, expected_fingerprint: &str) -> Result<ReviewDecisionReceipt> {
    let result = receipt_result(message, expected_fingerprint, "结算复核决定")?;
    let fields = result.split('|').collect::<Vec<_>>();
    let [operation_id, status, statement_version, task_version, payable_account_id, cost_delta] =
        fields.as_slice()
    else {
        return Err(Error::Internal("结算复核决定幂等收据结果非法".to_string()));
    };
    let result_status = match *status {
        "C" => dto::SettlementReviewDecisionStatus::Confirmed,
        "R" => dto::SettlementReviewDecisionStatus::Rejected,
        _ => return Err(Error::Internal("结算复核决定收据状态非法".to_string())),
    };
    Ok(ReviewDecisionReceipt {
        operation_id: (*operation_id).to_string(),
        result_status,
        statement_version: parse_receipt_number(statement_version, "结算单版本")?,
        task_version: parse_receipt_number(task_version, "待办版本")?,
        payable_account_id: (*payable_account_id != "-").then(|| (*payable_account_id).to_string()),
        cost_delta: if *cost_delta == "-" {
            None
        } else {
            Some(
                Amount::from_str(cost_delta)
                    .map_err(|_| Error::Internal("结算复核决定收据成本差额非法".to_string()))?,
            )
        },
    })
}

/// 编码差异决定幂等收据。
fn difference_decision_receipt_message(fingerprint: &str, receipt: &DifferenceDecisionReceipt) -> String {
    format!(
        "{COMMAND_FINGERPRINT_PREFIX}{fingerprint};result={}|{}|{}|{}",
        receipt.operation_id, receipt.statement_id, receipt.statement_version, receipt.difference_version,
    )
}

/// 解析并校验差异决定幂等收据。
fn parse_difference_decision_receipt(
    message: &str,
    expected_fingerprint: &str,
) -> Result<DifferenceDecisionReceipt> {
    let result = receipt_result(message, expected_fingerprint, "结算差异决定")?;
    let fields = result.split('|').collect::<Vec<_>>();
    let [operation_id, statement_id, statement_version, difference_version] = fields.as_slice() else {
        return Err(Error::Internal("结算差异决定幂等收据结果非法".to_string()));
    };
    Ok(DifferenceDecisionReceipt {
        operation_id: (*operation_id).to_string(),
        statement_id: (*statement_id).to_string(),
        statement_version: parse_receipt_number(statement_version, "结算单版本")?,
        difference_version: parse_receipt_number(difference_version, "差异版本")?,
    })
}

/// 提取审计消息中的命令指纹与结果载荷。
fn receipt_result<'a>(message: &'a str, expected_fingerprint: &str, command_name: &str) -> Result<&'a str> {
    let (fingerprint, result) = message
        .strip_prefix(COMMAND_FINGERPRINT_PREFIX)
        .and_then(|value| value.split_once(";result="))
        .ok_or_else(|| Error::Internal(format!("{command_name}幂等收据格式非法")))?;
    if fingerprint != expected_fingerprint {
        return Err(Error::ConflictError(format!(
            "幂等键已用于不同的{command_name}命令"
        )));
    }
    Ok(result)
}

/// 解析幂等收据中的正整数版本。
fn parse_receipt_number(value: &str, field: &str) -> Result<u64> {
    let value = value
        .parse::<u64>()
        .map_err(|_| Error::Internal(format!("结算命令收据{field}非法")))?;
    if value == 0 {
        return Err(Error::Internal(format!("结算命令收据{field}非法")));
    }
    Ok(value)
}

/// 校验幂等收据仍指向同一成功业务资源。
fn ensure_audit_resource(audit: &entities::AuditLog, resource_id: &str) -> Result<()> {
    if !audit.success || audit.resource_id.as_deref() != Some(resource_id) {
        return Err(Error::ConflictError("幂等收据与当前业务资源不一致".to_string()));
    }
    Ok(())
}

/// 由正式事实与收据构造结算复核响应。
fn review_decision_result(
    statement: SupplierSettlementStatement,
    work_item: WorkItem,
    operation_id: String,
    receipt: ReviewDecisionReceipt,
) -> SettlementReviewDecisionResult {
    let payable_account_id = receipt.payable_account_id;
    let payable_no = payable_account_id
        .as_ref()
        .map(|_| statement.statement_no.clone());
    SettlementReviewDecisionResult {
        result_status: receipt.result_status,
        message: match receipt.result_status {
            dto::SettlementReviewDecisionStatus::Confirmed => {
                "结算已确认，应付与成本差额已原子登记".to_string()
            }
            dto::SettlementReviewDecisionStatus::Rejected => "结算已驳回给经办人继续处理".to_string(),
        },
        operation_id,
        statement: statement.into(),
        work_item_id: work_item.base.id,
        work_item_status: WorkItemStatus::Completed,
        task_version: receipt.task_version,
        payable_no,
        payable_account_id,
        cost_delta_gross: receipt.cost_delta,
    }
}

impl SupplierSettlementService {
    /// 加载结算单全部冻结明细。
    async fn load_statement_items(
        &self,
        statement_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierSettlementItem>> {
        load_statement_items(&self.db, statement_id, executor).await
    }

    /// 加载结算明细关联的全部正式差异。
    async fn load_statement_differences(
        &self,
        items: &[SupplierSettlementItem],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierSettlementDifference>> {
        load_statement_differences(&self.db, items, executor).await
    }

    /// 为详情返回当前 actor 的 W27 领域动作，不把领域动作塞进通用责任注册表。
    async fn settlement_review_work_item_view(
        &self,
        statement: &SupplierSettlementStatement,
        actor: &AuditActor,
    ) -> Result<(
        Option<dto::SettlementReviewWorkItemView>,
        Vec<dto::SettlementReviewActionBlockerView>,
        Vec<String>,
    )> {
        if statement.status != SettlementStatus::PendingReview {
            return Ok((None, Vec::new(), Vec::new()));
        }
        let mut items = self
            .db
            .work_items()
            .list_active_by_object(
                "supplier_settlement_statement",
                &statement.base.id,
                &mut NoTransaction,
            )
            .await?
            .into_iter()
            .filter(|item| item.work_item_type == WorkItemType::SupplierSettlementReview)
            .collect::<Vec<_>>();
        if items.len() != 1 {
            return Ok((
                None,
                vec![review_blocker(
                    "REVIEW_DECISION",
                    "FORMAL_REVIEW_WORK_ITEM_MISSING_OR_AMBIGUOUS",
                    "未找到与当前结算主题唯一匹配的正式复核任务，已禁止决定",
                )],
                Vec::new(),
            ));
        }
        let item = items
            .pop()
            .ok_or_else(|| Error::Internal("正式结算复核任务读取失败".to_string()))?;
        if item.business_object_type != "supplier_settlement_statement"
            || item.business_object_id != statement.base.id
            || item.subject_version != statement.subject_hash
            || item.owner_role != SETTLEMENT_REVIEW_OWNER_ROLE
            || item.owner_organization_id != SETTLEMENT_REVIEW_OWNER_ORGANIZATION_ID
        {
            return Ok((
                None,
                vec![review_blocker(
                    "REVIEW_DECISION",
                    "FORMAL_REVIEW_WORK_ITEM_MISMATCH",
                    "复核任务与当前结算主题不一致，已禁止决定",
                )],
                Vec::new(),
            ));
        }
        let eligible = true;
        let separation_satisfied = statement.prepared_by != actor.id();
        let (domain_actions, action_blockers) =
            settlement_review_access(&item, actor.id(), eligible, separation_satisfied);
        Ok((
            Some(dto::SettlementReviewWorkItemView {
                work_item_id: item.base.id,
                work_item_type: item.work_item_type,
                task_version: item.base.version,
                subject_version: item.subject_version,
                status: item.status,
                processing_state: dto::SettlementReviewProcessingState::Ready,
                owner_role: item.owner_role,
                owner_organization_id: item.owner_organization_id,
                owner_user_id: item.owner_user_id,
                action_blockers,
            }),
            Vec::new(),
            domain_actions,
        ))
    }

    /// 重放提交复核命令并校验收据载荷。
    async fn replay_review_submission(
        &self,
        audit_id: &str,
        expected_fingerprint: &str,
        statement_id: &str,
    ) -> Result<Option<SubmitSettlementReviewResult>> {
        let Some(audit) = self
            .db
            .audit_logs()
            .find_by_id(audit_id, &mut NoTransaction)
            .await?
        else {
            return Ok(None);
        };
        ensure_audit_resource(&audit, statement_id)?;
        let receipt = parse_review_submission_receipt(
            audit.message.as_deref().unwrap_or_default(),
            expected_fingerprint,
        )?;
        let statement = self.load_statement(statement_id).await?;
        if statement.base.version < receipt.statement_version {
            return Err(Error::ConflictError(
                "提交复核幂等收据与当前结算事实不一致".to_string(),
            ));
        }
        let work_item = self
            .db
            .work_items()
            .find_by_id(&receipt.work_item_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("提交复核收据引用的任务不存在".to_string()))?;
        if work_item.base.version < receipt.task_version
            || work_item.work_item_type != WorkItemType::SupplierSettlementReview
            || work_item.business_object_type != "supplier_settlement_statement"
            || work_item.business_object_id != statement.base.id
            || work_item.subject_version != statement.subject_hash
            || work_item.owner_role != SETTLEMENT_REVIEW_OWNER_ROLE
            || work_item.owner_organization_id != SETTLEMENT_REVIEW_OWNER_ORGANIZATION_ID
        {
            return Err(Error::ConflictError(
                "提交复核幂等收据与当前正式任务不一致".to_string(),
            ));
        }
        Ok(Some(SubmitSettlementReviewResult {
            result_status: dto::SettlementReviewSubmissionStatus::Submitted,
            message: "结算主题已冻结并提交财务复核".to_string(),
            operation_id: receipt.operation_id,
            statement: statement.into(),
            work_item_id: receipt.work_item_id,
        }))
    }

    /// 重放正式复核决定并恢复同一业务结果。
    async fn replay_review_decision(
        &self,
        audit_id: &str,
        expected_fingerprint: &str,
        statement_id: &str,
        work_item_id: &str,
    ) -> Result<Option<SettlementReviewDecisionResult>> {
        let Some(audit) = self
            .db
            .audit_logs()
            .find_by_id(audit_id, &mut NoTransaction)
            .await?
        else {
            return Ok(None);
        };
        ensure_audit_resource(&audit, statement_id)?;
        let receipt = parse_review_decision_receipt(
            audit.message.as_deref().unwrap_or_default(),
            expected_fingerprint,
        )?;
        let statement = self.load_statement(statement_id).await?;
        let work_item = self
            .db
            .work_items()
            .find_by_id(work_item_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("复核决定收据引用的任务不存在".to_string()))?;
        let business_result_matches = match receipt.result_status {
            dto::SettlementReviewDecisionStatus::Confirmed => {
                statement.status == SettlementStatus::Confirmed
                    && statement.review_result == Some(SettlementReviewResult::Confirmed)
                    && statement.payable_account_id.as_ref().map(ToString::to_string)
                        == receipt.payable_account_id
            }
            dto::SettlementReviewDecisionStatus::Rejected => {
                matches!(
                    statement.status,
                    SettlementStatus::Draft | SettlementStatus::HasDifference
                ) && statement.review_result == Some(SettlementReviewResult::Rejected)
                    && statement.payable_account_id.is_none()
                    && receipt.payable_account_id.is_none()
            }
        };
        if statement.base.version != receipt.statement_version
            || work_item.base.version != receipt.task_version
            || work_item.status != WorkItemStatus::Completed
            || work_item.work_item_type != WorkItemType::SupplierSettlementReview
            || work_item.business_object_type != "supplier_settlement_statement"
            || work_item.business_object_id != statement.base.id
            || work_item.subject_version != statement.subject_hash
            || !business_result_matches
        {
            return Err(Error::ConflictError(
                "复核决定幂等收据与当前正式事实不一致".to_string(),
            ));
        }
        Ok(Some(review_decision_result(
            statement,
            work_item,
            receipt.operation_id.clone(),
            receipt,
        )))
    }

    /// 重放差异决定并恢复同一业务结果。
    async fn replay_difference_decision(
        &self,
        audit_id: &str,
        expected_fingerprint: &str,
        difference_id: &str,
    ) -> Result<Option<SettlementDifferenceDecisionResult>> {
        let Some(audit) = self
            .db
            .audit_logs()
            .find_by_id(audit_id, &mut NoTransaction)
            .await?
        else {
            return Ok(None);
        };
        ensure_audit_resource(&audit, difference_id)?;
        let receipt = parse_difference_decision_receipt(
            audit.message.as_deref().unwrap_or_default(),
            expected_fingerprint,
        )?;
        let difference = self
            .db
            .supplier_settlement_differences()
            .find_by_id(difference_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("差异决定收据引用的差异不存在".to_string()))?;
        let item = self
            .db
            .supplier_settlement_items()
            .find_by_id(&difference.statement_item_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("差异决定收据引用的结算明细不存在".to_string()))?;
        if receipt.statement_id != item.statement_id.as_ref()
            || difference.base.version != receipt.difference_version
            || difference.status == SettlementDifferenceStatus::Pending
        {
            return Err(Error::ConflictError(
                "差异决定幂等收据与当前正式事实不一致".to_string(),
            ));
        }
        let statement = self.load_statement(&receipt.statement_id).await?;
        if statement.base.version < receipt.statement_version {
            return Err(Error::ConflictError(
                "差异决定幂等收据的结算单版本非法".to_string(),
            ));
        }
        Ok(Some(SettlementDifferenceDecisionResult {
            result_status: dto::SettlementDifferenceDecisionStatus::Resolved,
            message: "结算差异正式结论已登记".to_string(),
            operation_id: receipt.operation_id,
            statement_id: receipt.statement_id,
            statement_lock_version: receipt.statement_version,
            difference: settlement_difference_view(difference),
        }))
    }

    /// 校验期望版本与当前版本一致（乐观锁前置校验）。
    ///
    /// # 参数
    /// * `statement` - 结算单实体
    /// * `expected` - 期望版本
    ///
    /// # 错误
    /// 版本不一致时返回 `ConflictError`。
    fn ensure_version(&self, statement: &SupplierSettlementStatement, expected: u64) -> Result<()> {
        if statement.base.version != expected {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        Ok(())
    }

    /// 在同一事务更新结算单并写审计。
    ///
    /// # 参数
    /// * `statement` - 结算单实体（就地更新）
    /// * `audit` - 审计日志
    ///
    /// # 错误
    /// 乐观锁冲突透出 `ConflictError`，提交结果未知透出 `OutcomeUnknown`。
    async fn update_statement_with_audit(
        &self,
        statement: &mut SupplierSettlementStatement,
        audit: &entities::AuditLog,
    ) -> Result<()> {
        let db = self.db.clone();
        let client = db.client().clone();
        let mut statement_for_tx = statement.clone();
        let audit_for_tx = audit.clone();
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.supplier_settlement_statements()
                        .update(&mut statement_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit_for_tx, session).await?;
                    Ok::<SupplierSettlementStatement, crate::errors::Error>(statement_for_tx)
                })
            })
            .await?;
        *statement = updated;
        Ok(())
    }

    /// 按 ID 加载未删除结算单。
    ///
    /// # 参数
    /// * `id` - 结算单 ID
    ///
    /// # 返回
    /// 返回结算单实体。
    ///
    /// # 错误
    /// * `NotFound` - 结算单不存在
    async fn load_statement(&self, id: &str) -> Result<SupplierSettlementStatement> {
        self.db
            .supplier_settlement_statements()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商结算单不存在".to_string()))
    }
}

/// 返回零金额（表头金额累加起点）。
fn zero_amount() -> Amount {
    Amount::from_str("0.00").expect("零是合法金额")
}

/// 构建结算单列表筛选条件。
///
/// # 参数
/// * `query` - 归一化查询参数
///
/// # 返回
/// 返回仓储筛选条件。
fn statement_filter(query: &StatementListQuery) -> StatementFilter {
    StatementFilter {
        statement_no: query.statement_no.clone(),
        supplier_id: query.supplier_id.clone(),
        status: query.status,
        period_from: query.period_from,
        period_to: query.period_to,
        page: query.paging.page,
        page_size: query.paging.page_size,
        sort_by: Some(query.paging.sort_by.to_string()),
        sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
    }
}

/// 从结算明细实体构造响应视图。
///
/// # 参数
/// * `item` - 结算明细实体
///
/// # 返回
/// 返回响应视图。
fn settlement_item_view(item: SupplierSettlementItem) -> SupplierSettlementItemView {
    SupplierSettlementItemView {
        id: item.base.id,
        statement_id: item.statement_id.to_string(),
        supplier_fulfillment_order_id: item.supplier_fulfillment_order_id.to_string(),
        supplier_fulfillment_item_id: item.supplier_fulfillment_item_id.to_string(),
        quantity: item.quantity,
        order_amount: item.order_amount,
        freight_amount: item.freight_amount,
        service_fee_amount: item.service_fee_amount,
        refund_amount: item.refund_amount,
        erp_calculated_amount: item.erp_calculated_amount,
        erp_calculated_net_amount: item.erp_calculated_net_amount,
        erp_calculated_tax_amount: item.erp_calculated_tax_amount,
        supplier_billed_amount: item.supplier_billed_amount,
        supplier_billed_net_amount: item.supplier_billed_net_amount,
        supplier_billed_tax_amount: item.supplier_billed_tax_amount,
        created_at: item.base.created_at,
    }
}

/// 从结算差异实体构造响应视图。
///
/// # 参数
/// * `difference` - 结算差异实体
///
/// # 返回
/// 返回响应视图。
fn settlement_difference_view(difference: SupplierSettlementDifference) -> SupplierSettlementDifferenceView {
    SupplierSettlementDifferenceView {
        id: difference.base.id,
        statement_item_id: difference.statement_item_id.to_string(),
        difference_type: difference.difference_type,
        difference_amount: difference.difference_amount,
        status: difference.status,
        resolution: difference.resolution,
        resolved_by: difference.resolved_by,
        resolved_at: difference.resolved_at.map(|t| t.unix_secs()),
        version: difference.base.version,
        created_at: difference.base.created_at,
        evidence: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use entities::common::time::BusinessDate;
    use entities::ids::{
        PayableAccountId, SupplierAccountId, SupplierFulfillmentItemId, SupplierFulfillmentOrderId,
        SupplierSettlementDifferenceId, SupplierSettlementItemId, SupplierSettlementStatementId,
    };
    use entities::supplier_settlement::{
        SettlementDifferenceType, SupplierSettlementDifferenceData, SupplierSettlementItemData,
        SupplierSettlementStatementData,
    };
    use entities::AccountKind;

    fn sample_statement() -> SupplierSettlementStatement {
        let mut statement = SupplierSettlementStatement::new(
            SupplierSettlementStatementId::new("statement-1"),
            SupplierSettlementStatementData {
                statement_no: "ST-2026-001".to_string(),
                supplier_id: SupplierAccountId::new("supplier-1"),
                period_start: BusinessDate::from_ymd(2026, 7, 1).unwrap(),
                period_end: BusinessDate::from_ymd(2026, 7, 31).unwrap(),
                period_policy_id: "calendar-month".to_string(),
                period_policy_version: "1".to_string(),
                period_timezone: "Asia/Shanghai".to_string(),
                external_bill_no: Some("BILL-1".to_string()),
                external_bill_version: Some("1".to_string()),
                erp_amount: Amount::from_str("100.00").unwrap(),
                supplier_amount: Amount::from_str("101.00").unwrap(),
                subject_hash: "a".repeat(64),
                source_as_of: Instant::from_unix_secs(1_700_000_000),
                source_snapshot_at: Instant::from_unix_secs(1_700_000_000),
                source_snapshot_hash: "b".repeat(64),
                refresh_cutoff_policy_id: REVIEW_CUTOFF_POLICY_ID.to_string(),
                refresh_cutoff_policy_version: REVIEW_CUTOFF_POLICY_VERSION.to_string(),
                prepared_by: "preparer-1".to_string(),
            },
        )
        .unwrap();
        statement
            .update_subject_hash(settlement_subject_hash(&statement, &[]))
            .unwrap();
        statement
    }

    fn sample_difference() -> SupplierSettlementDifference {
        SupplierSettlementDifference::new(
            SupplierSettlementDifferenceId::new("difference-1"),
            SupplierSettlementDifferenceData {
                statement_item_id: SupplierSettlementItemId::new("item-1"),
                difference_type: SettlementDifferenceType::Amount,
                difference_amount: Amount::from_str("1.00").unwrap(),
                status: SettlementDifferenceStatus::Pending,
                resolution: None,
                resolved_by: None,
                resolved_at: None,
            },
        )
        .unwrap()
    }

    fn sample_settlement_item() -> SupplierSettlementItem {
        SupplierSettlementItem::new(
            SupplierSettlementItemId::new("item-1"),
            SupplierSettlementItemData {
                statement_id: SupplierSettlementStatementId::new("statement-1"),
                supplier_fulfillment_order_id: SupplierFulfillmentOrderId::new("order-1"),
                supplier_fulfillment_item_id: SupplierFulfillmentItemId::new("fulfillment-item-1"),
                quantity: entities::money::Quantity::from_str("1").unwrap(),
                order_amount: Amount::from_str("100.00").unwrap(),
                freight_amount: zero_amount(),
                service_fee_amount: zero_amount(),
                refund_amount: zero_amount(),
                erp_calculated_amount: Amount::from_str("100.00").unwrap(),
                erp_calculated_net_amount: Amount::from_str("87.00").unwrap(),
                erp_calculated_tax_amount: Amount::from_str("13.00").unwrap(),
                supplier_billed_amount: Amount::from_str("101.00").unwrap(),
                supplier_billed_net_amount: Amount::from_str("87.87").unwrap(),
                supplier_billed_tax_amount: Amount::from_str("13.13").unwrap(),
            },
        )
        .unwrap()
    }

    fn sample_work_item(statement: &SupplierSettlementStatement) -> WorkItem {
        WorkItem::new_at(
            WorkItemId::new("work-item-1"),
            WorkItemData {
                work_item_type: WorkItemType::SupplierSettlementReview,
                business_object_type: "supplier_settlement_statement".to_string(),
                business_object_id: statement.base.id.clone(),
                subject_version: statement.subject_hash.clone(),
                owner_role: SETTLEMENT_REVIEW_OWNER_ROLE.to_string(),
                owner_organization_id: SETTLEMENT_REVIEW_OWNER_ORGANIZATION_ID.to_string(),
                owner_user_id: "reviewer-1".to_string(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::High,
                due_at: None,
                reason_code: None,
                impact_summary: None,
            },
            Instant::from_unix_secs(1_700_000_000),
        )
        .unwrap()
    }

    #[test]
    fn subject_hash_is_stable_across_review_state_and_covers_difference_conclusion() {
        let mut statement = sample_statement();
        let initial = settlement_subject_hash(&statement, &[]);
        statement.submit_review().unwrap();
        statement
            .record_review(
                SettlementReviewDecision::Confirm {
                    payable_account_id: PayableAccountId::new("payable-1"),
                    comment: Some("确认".to_string()),
                },
                "reviewer-1",
                Instant::from_unix_secs(1_700_000_100),
            )
            .unwrap();
        assert_eq!(settlement_subject_hash(&statement, &[]), initial);

        let mut difference = sample_difference();
        let pending = settlement_subject_hash(&statement, &[difference.clone()]);
        difference
            .update(SupplierSettlementDifferenceUpdate {
                status: Some(SettlementDifferenceStatus::ErpAcknowledged),
                resolution: Some("reason=ACCEPT_BILL;evidence=proof-1".to_string()),
                resolved_by: Some("preparer-1".to_string()),
                resolved_at: Some(Instant::from_unix_secs(1_700_000_200)),
            })
            .unwrap();
        assert_ne!(settlement_subject_hash(&statement, &[difference]), pending);
    }

    #[test]
    fn command_receipts_roundtrip_and_reject_fingerprint_reuse() {
        let fingerprint = "f".repeat(64);
        let submission = ReviewSubmissionReceipt {
            operation_id: "op-submit".to_string(),
            statement_version: 2,
            work_item_id: "work-item-1".to_string(),
            task_version: 1,
        };
        let message = review_submission_receipt_message(&fingerprint, &submission);
        assert_eq!(
            parse_review_submission_receipt(&message, &fingerprint).unwrap(),
            submission
        );
        assert!(parse_review_submission_receipt(&message, &"0".repeat(64)).is_err());

        let decision = ReviewDecisionReceipt {
            operation_id: "op-review".to_string(),
            result_status: dto::SettlementReviewDecisionStatus::Confirmed,
            statement_version: 3,
            task_version: 2,
            payable_account_id: Some("payable-1".to_string()),
            cost_delta: Some(Amount::from_str("1.00").unwrap()),
        };
        let message = review_decision_receipt_message(&fingerprint, &decision);
        assert_eq!(
            parse_review_decision_receipt(&message, &fingerprint).unwrap(),
            decision
        );

        let difference = DifferenceDecisionReceipt {
            operation_id: "op-difference".to_string(),
            statement_id: "statement-1".to_string(),
            statement_version: 2,
            difference_version: 2,
        };
        let message = difference_decision_receipt_message(&fingerprint, &difference);
        assert_eq!(
            parse_difference_decision_receipt(&message, &fingerprint).unwrap(),
            difference
        );
    }

    #[test]
    fn review_access_is_actor_specific_and_fail_closed() {
        let statement = sample_statement();
        let item = sample_work_item(&statement);
        let (domain_actions, blockers) = settlement_review_access(&item, "other-reviewer", true, true);
        assert!(domain_actions.is_empty());
        assert_eq!(blockers[0].code, "CURRENT_OWNER_MISMATCH");

        let (domain_actions, blockers) = settlement_review_access(&item, "reviewer-1", true, true);
        assert_eq!(domain_actions, vec!["REJECT", "CONFIRM"]);
        assert!(blockers.is_empty());

        let (domain_actions, blockers) = settlement_review_access(&item, "reviewer-1", false, true);
        assert!(domain_actions.is_empty());
        assert_eq!(blockers[0].code, "ASSIGNMENT_NOT_ELIGIBLE");
        let (domain_actions, blockers) = settlement_review_access(&item, "reviewer-1", true, false);
        assert!(domain_actions.is_empty());
        assert_eq!(blockers[0].code, "SEGREGATION_OF_DUTIES");
    }

    #[test]
    fn work_item_validation_requires_exact_three_versions_and_current_owner() {
        let mut statement = sample_statement();
        statement.submit_review().unwrap();
        let mut item = sample_work_item(&statement);
        let actor = AuditActor::new(
            "reviewer-1".to_string(),
            "reviewer".to_string(),
            AccountKind::Admin,
        );

        assert!(validate_settlement_review_work_item(
            &item,
            &statement,
            item.base.version,
            &statement.subject_hash,
            &actor,
        )
        .is_ok());
        assert!(validate_settlement_review_work_item(
            &item,
            &statement,
            item.base.version + 1,
            &statement.subject_hash,
            &actor,
        )
        .is_err());
        assert!(validate_settlement_review_work_item(
            &item,
            &statement,
            item.base.version,
            &"0".repeat(64),
            &actor,
        )
        .is_err());
        item.owner_user_id = Some("other-reviewer".to_string());
        assert!(validate_settlement_review_work_item(
            &item,
            &statement,
            item.base.version,
            &statement.subject_hash,
            &actor,
        )
        .is_err());
    }

    #[test]
    fn difference_evidence_and_command_ids_are_bounded() {
        assert!(
            normalized_evidence_references(&[], dto::SettlementDifferenceResolution::ClosedNoAdjustment,)
                .is_err()
        );
        assert_eq!(
            normalized_evidence_references(
                &[
                    " proof-2 ".to_string(),
                    "proof-1".to_string(),
                    "proof-1".to_string()
                ],
                dto::SettlementDifferenceResolution::ErpAccepted,
            )
            .unwrap(),
            vec!["proof-1".to_string(), "proof-2".to_string()]
        );
        let audit_id = command_audit_id(
            "actor-1",
            "supplier_settlement.review_confirm",
            "statement-1",
            "raw-idempotency-secret",
        );
        assert!(!audit_id.contains("raw-idempotency-secret"));
        assert!(audit_id.len() <= 128);
    }

    #[test]
    fn erp_accepted_difference_calculates_components_but_blocks_unproven_cost_lineage() {
        let statement = sample_statement();
        let item = sample_settlement_item();
        let mut difference = sample_difference();
        difference
            .update(SupplierSettlementDifferenceUpdate {
                status: Some(SettlementDifferenceStatus::ErpAcknowledged),
                resolution: Some("reason=ACCEPT_BILL;evidence=proof-1".to_string()),
                resolved_by: Some("preparer-1".to_string()),
                resolved_at: Some(Instant::from_unix_secs(1_700_000_200)),
            })
            .unwrap();

        let delta = accepted_cost_delta(&[item], &[difference]).unwrap();
        assert_eq!(delta.gross, Amount::from_str("1.00").unwrap());
        assert_eq!(delta.net, Amount::from_str("0.87").unwrap());
        assert_eq!(delta.tax, Amount::from_str("0.13").unwrap());
        assert!(
            build_settlement_cost_delta(&statement, &delta, Instant::from_unix_secs(1_700_000_300)).is_err()
        );
    }

    #[test]
    fn supplier_accepted_difference_does_not_change_erp_cost() {
        let item = sample_settlement_item();
        let mut difference = sample_difference();
        difference
            .update(SupplierSettlementDifferenceUpdate {
                status: Some(SettlementDifferenceStatus::SupplierAcknowledged),
                resolution: Some("reason=BILL_ALIGNED;evidence=proof-1".to_string()),
                resolved_by: Some("preparer-1".to_string()),
                resolved_at: Some(Instant::from_unix_secs(1_700_000_200)),
            })
            .unwrap();

        let delta = accepted_cost_delta(&[item], &[difference]).unwrap();
        assert_eq!(delta.gross, zero_amount());
        assert!(build_settlement_cost_delta(
            &sample_statement(),
            &delta,
            Instant::from_unix_secs(1_700_000_300),
        )
        .unwrap()
        .is_empty());
    }

    #[test]
    fn erp_accepted_difference_requires_exact_frozen_gross_sum() {
        let item = sample_settlement_item();
        let mut difference = sample_difference();
        difference.difference_amount = Amount::from_str("0.99").unwrap();
        difference
            .update(SupplierSettlementDifferenceUpdate {
                status: Some(SettlementDifferenceStatus::ErpAcknowledged),
                resolution: Some("reason=ACCEPT_BILL;evidence=proof-1".to_string()),
                resolved_by: Some("preparer-1".to_string()),
                resolved_at: Some(Instant::from_unix_secs(1_700_000_200)),
            })
            .unwrap();

        assert!(accepted_cost_delta(&[item], &[difference]).is_err());
    }
}
