//! 销售单最终通过：包装既有 `formalize_submission` 仓储端口。

use std::collections::BTreeMap;
use std::str::FromStr;

use database::{
    AccessControlExt, DocumentRegistryExt, Executor, NoTransaction, ProjectionExt, ReceivableExt,
    SalesOrderExt, Transactional, WorkItemExt,
};
use entities::common::time::{BusinessDate, Instant};
use entities::ids::{
    ReceivableAccountId, ReceivableEntryId, SalesOrderId, SalesOrderProjectionDeliveryId,
    SalesOrderProjectionId, SalesOrderProjectionRevisionId, SalesOrderRevisionId, SalesOrderSubmissionId,
    WorkItemId,
};
use entities::money::Amount;
use entities::projection::{
    CardForm as ProjectionCardForm, ProjectionDeliveryStatus, ProjectionSource, SalesOrderProjection,
    SalesOrderProjectionData, SalesOrderProjectionDelivery, SalesOrderProjectionDeliveryData,
    SalesOrderProjectionRevision, SalesOrderProjectionRevisionData,
};
use entities::receivable::{
    AccountReviewStatus, EntryDirection, ReceivableAccount, ReceivableAccountData, ReceivableEntry,
    ReceivableEntryData, ReceivableEntryType,
};
use entities::sales_order::{
    procurement_responsibility_key, CardForm, FormalRevisionContext, FormalRevisionIdentities,
    FormalRevisionLineIdentity, FormalRevisionSubtypeIdentity, RevisionSource, SalesOrder,
    SalesOrderRevisionAggregate, SalesOrderSubmission, SalesOrderSubmissionLine,
};
use entities::work_item::{AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemType};
use id_generator::next_id;
use mongodb::Database;

use super::adapter::{
    ensure_final_approve_formalize, is_voucher_sales_order, sales_order_responsible_org_id,
};
use super::dto::SalesOrderDetailView;
use super::procurement::submission_procurement_inputs;
use super::SalesOrderService;
use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::procurement_responsibility::{
    AuthorizedResolutionPlan, ProcurementResponsibilityService, ResolutionInput,
};
use crate::projection::projection_content_hash;
use crate::purchase_order::sync_procurement_tasks_for_sales_order;

/// 事务外授权并在销售形式化事务内重验的采购责任计划。
struct ProcurementFormalizationPlan {
    inputs: Vec<ResolutionInput>,
    resolution: AuthorizedResolutionPlan,
}

/// 销售形式化事务需要一次性消费的完整写入上下文。
struct FormalizedSubmissionWrite {
    db: Database,
    rbac: crate::iam::SharedRbacService,
    order_id: String,
    order: SalesOrder,
    submission: SalesOrderSubmission,
    aggregate: SalesOrderRevisionAggregate,
    procurement: Option<ProcurementFormalizationPlan>,
    procurement_items: Vec<WorkItem>,
    projection: Option<(
        SalesOrderProjection,
        SalesOrderProjectionRevision,
        SalesOrderProjectionDelivery,
    )>,
    audit: entities::AuditLog,
    now: Instant,
}

impl SalesOrderService {
    /// 最终通过并形式化已批准提交。
    ///
    /// 只包装既有 `repository formalize_submission`：先把销售单推进到
    /// `EFFECTIVE` / `APPROVED`，再写入正式修订。卡券在同一事务向商城发出执行投影。
    /// 不得 `$set` 绕过领域不变式，也不得按卡券运营节点写专用副作用分支。
    ///
    /// # 参数
    /// * `id` - 销售单主键
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回形式化后的销售单详情。
    ///
    /// # 错误
    /// 非审批中、缺少提交、卡券投影字段缺失或仓储失败时返回错误。
    #[tracing::instrument(
        name = "sales_order.formalize_approved_submission",
        skip_all,
        fields(
            layer = "service",
            domain = "sales_order",
            operation = "formalize_approved_submission"
        )
    )]
    pub async fn formalize_approved_submission(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<SalesOrderDetailView> {
        let mut order = self
            .db
            .sales_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
        if order.is_fully_formalized() {
            return self.sales_order_detail(id, None).await;
        }
        ensure_final_approve_formalize(&order)?;
        let (submission, lines) = load_latest_submission(&self.db, id, &mut NoTransaction).await?;
        let procurement = self.build_procurement_formalization_plan(&order, &lines).await?;
        persist_formalized_submission(
            &self.db,
            self.require_rbac()?.clone(),
            &mut order,
            submission,
            lines,
            procurement,
            actor,
        )
        .await?;
        self.sales_order_detail(id, None).await
    }

    /// 在审批运行时持有的事务内形式化最终通过的销售单。
    ///
    /// # 参数
    /// * `id` - 销售单主键
    /// * `actor` - 已认证操作人
    /// * `session` - 审批运行时持有的唯一事务会话
    ///
    /// # 返回
    /// 正式版本、应收、供给任务、投影和成功审计全部写入时返回 `Ok(())`。
    ///
    /// # 错误
    /// 单据状态、提交、采购责任或持久化不变量失败时返回错误。
    pub(crate) async fn formalize_approved_submission_in_transaction(
        &self,
        id: &str,
        actor: &AuditActor,
        session: &mut mongodb::ClientSession,
    ) -> Result<()> {
        let mut order = self
            .db
            .sales_orders()
            .find_by_id(id, session)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
        if order.is_fully_formalized() {
            return Ok(());
        }
        ensure_final_approve_formalize(&order)?;
        let (submission, lines) = load_latest_submission(&self.db, id, session).await?;
        let procurement = self.build_procurement_formalization_plan(&order, &lines).await?;
        let write = prepare_formalized_submission_write(
            &self.db,
            self.require_rbac()?.clone(),
            &mut order,
            submission,
            lines,
            procurement,
            actor,
        )?;
        persist_formalized_submission_write(write, session).await
    }

    /// 为实物及服务销售单构造事务外授权的采购责任计划。
    ///
    /// # 参数
    /// * `order` - 待最终生效销售单
    /// * `lines` - 最新冻结提交行
    ///
    /// # 返回
    /// 卡券销售单返回 `None`；实物服务单返回逐行具体负责人计划。
    ///
    /// # 错误
    /// 任一行责任无法确定或负责人不合格时失败关闭。
    async fn build_procurement_formalization_plan(
        &self,
        order: &SalesOrder,
        lines: &[SalesOrderSubmissionLine],
    ) -> Result<Option<ProcurementFormalizationPlan>> {
        if !order.business_type.is_goods_service() {
            return Ok(None);
        }
        let inputs = submission_procurement_inputs(lines)?;
        let resolution = ProcurementResponsibilityService::new(self.db.clone(), self.require_rbac()?.clone())
            .resolve_strict(&inputs)
            .await?;
        Ok(Some(ProcurementFormalizationPlan { inputs, resolution }))
    }
}

/// 读取该销售单最新提交及其明细。
///
/// # 错误
/// 无提交或仓储失败时返回错误。
async fn load_latest_submission(
    db: &Database,
    sales_order_id: &str,
    executor: &mut dyn Executor,
) -> Result<(SalesOrderSubmission, Vec<SalesOrderSubmissionLine>)> {
    let order_id = SalesOrderId::new(sales_order_id);
    let submission = db
        .sales_order_submissions()
        .find_latest_by_order(&order_id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("销售单没有可形式化的提交".to_string()))?;
    let submission_id = SalesOrderSubmissionId::new(submission.base.id.clone());
    let lines = db
        .sales_order_submission_lines()
        .list_lines_by_submissions(&[submission_id], executor)
        .await?;
    Ok((submission, lines))
}

/// 在同一提交边界内推进状态并调用既有 `formalize_submission`。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `rbac` - 共享授权服务
/// * `order` - 待生效销售单
/// * `submission` - 最新冻结提交
/// * `lines` - 最新冻结提交行
/// * `procurement` - 可选采购责任授权计划
/// * `actor` - 已认证审计操作人
///
/// # 返回
/// 销售形式化与全部副作用原子提交后返回 `Ok(())`。
///
/// # 错误
/// 状态不允许、采购授权版本变化、行字段缺失或写入失败时返回错误。
async fn persist_formalized_submission(
    db: &Database,
    rbac: crate::iam::SharedRbacService,
    order: &mut SalesOrder,
    submission: SalesOrderSubmission,
    lines: Vec<SalesOrderSubmissionLine>,
    procurement: Option<ProcurementFormalizationPlan>,
    actor: &AuditActor,
) -> Result<()> {
    let policy_revision = procurement.as_ref().map(|plan| plan.resolution.policy_revision);
    let write =
        prepare_formalized_submission_write(db, rbac.clone(), order, submission, lines, procurement, actor)?;
    let client = db.client().clone();
    if let Some(policy_revision) = policy_revision {
        rbac.run_authorized_policy_transaction(policy_revision, move |session| {
            Box::pin(persist_formalized_submission_write(write, session))
        })
        .await?;
    } else {
        client
            .with_transaction(move |session| Box::pin(persist_formalized_submission_write(write, session)))
            .await?;
    }
    let _ = SalesOrderId::new(order.base.id.clone());
    Ok(())
}

/// 完成销售形式化的纯领域计算并生成待写上下文。
///
/// # 错误
/// 状态、版本、投影或任务字段不合法时返回错误。
fn prepare_formalized_submission_write(
    db: &Database,
    rbac: crate::iam::SharedRbacService,
    order: &mut SalesOrder,
    mut submission: SalesOrderSubmission,
    lines: Vec<SalesOrderSubmissionLine>,
    procurement: Option<ProcurementFormalizationPlan>,
    actor: &AuditActor,
) -> Result<FormalizedSubmissionWrite> {
    let now = Instant::now();
    let aggregate = build_revision_for_order(order, &submission, &lines, now)?;
    let projection = if is_voucher_sales_order(order.business_type) {
        Some(build_voucher_execution_projection(
            order,
            &submission,
            &aggregate,
            now,
        )?)
    } else {
        None
    };
    let procurement_items = procurement
        .as_ref()
        .map(|plan| build_procurement_work_items(order, &submission, plan))
        .transpose()?
        .unwrap_or_default();
    order.approve(now, actor.id())?;
    order.attach_revision(aggregate.revision.base.id.clone(), actor.id());
    submission.approve(actor.id())?;
    let audit = actor
        .clone()
        .resource_log("sales_order.formalize", "sales_order", order.base.id.clone())?;
    Ok(FormalizedSubmissionWrite {
        db: db.clone(),
        rbac: rbac.clone(),
        order_id: order.base.id.clone(),
        order: order.clone(),
        submission,
        aggregate,
        procurement,
        procurement_items,
        projection,
        audit,
        now,
    })
}

/// 在调用方选定的事务边界内写入销售形式化的全部业务事实。
///
/// # 参数
/// * `write` - 已完成领域计算的销售形式化写入上下文
/// * `session` - MongoDB 事务会话
///
/// # 返回
/// 全部业务事实与审计写入成功时返回 `Ok(())`。
///
/// # 错误
/// 采购责任重验、正式版本、投影、应收或审计任一写入失败时返回错误。
async fn persist_formalized_submission_write(
    mut write: FormalizedSubmissionWrite,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    if let Some(plan) = write.procurement.as_ref() {
        ProcurementResponsibilityService::new(write.db.clone(), write.rbac.clone())
            .revalidate_plan(&plan.inputs, &plan.resolution, session)
            .await?;
        persist_procurement_work_items(&write.db, &write.procurement_items, session).await?;
    }
    if let Some(mut document) = write
        .db
        .business_documents()
        .find_by_id(&write.order_id, session)
        .await?
    {
        document.formalize(write.now);
        write
            .db
            .business_documents()
            .update(&mut document, session)
            .await?;
    }
    write
        .db
        .sales_order()
        .formalize_submission(
            &mut write.order,
            &write.aggregate.revision,
            &write.aggregate.lines,
            &write.aggregate.goods_lines,
            &write.aggregate.voucher_lines,
            session,
        )
        .await?;
    if write.procurement.is_some() {
        sync_procurement_tasks_for_sales_order(
            &write.db,
            &SalesOrderId::new(write.order_id.clone()),
            session,
        )
        .await?;
    }
    write
        .db
        .sales_order_submissions()
        .update(&mut write.submission, session)
        .await?;
    if let Some((projection, revision, delivery)) = write.projection {
        persist_voucher_projection(&write.db, &projection, &revision, &delivery, session).await?;
    }
    // 销售单生效即形成原始应收（§6.8/§8.1.1）：子账 + 原始应收分录原子写入。
    // 后续销售变更差额由 sales_review 生效路径另行入账，本路径只写首次生效。
    create_original_receivable(&write.db, &write.order, &write.aggregate, write.now, session).await?;
    write.db.audit_logs().create(&write.audit, session).await?;
    Ok(())
}

/// 按负责人分组构造供给分配任务。
///
/// # 参数
/// * `order` - 待生效销售单
/// * `submission` - 冻结提交
/// * `plan` - 已授权逐行责任计划
///
/// # 返回
/// 返回每位具体负责人一条任务，责任键冻结稳定销售行集合。
///
/// # 错误
/// 责任组织缺失、行集合为空或任务字段非法时返回错误。
fn build_procurement_work_items(
    order: &SalesOrder,
    submission: &SalesOrderSubmission,
    plan: &ProcurementFormalizationPlan,
) -> Result<Vec<WorkItem>> {
    let mut groups = BTreeMap::<String, Vec<String>>::new();
    for line in &plan.resolution.lines {
        groups
            .entry(line.owner_user_id.clone())
            .or_default()
            .push(line.line_key.clone());
    }
    let organization_id = sales_order_responsible_org_id(order)?;
    groups
        .into_iter()
        .map(|(owner_user_id, mut line_ids)| {
            line_ids.sort();
            line_ids.dedup();
            let responsibility_key = procurement_responsibility_key(&line_ids)?;
            let line_count = line_ids.len();
            WorkItem::new_with_responsibility_scope(
                WorkItemId::new(next_id()),
                WorkItemData {
                    work_item_type: WorkItemType::ProcurementOrderCreation,
                    business_object_type: "sales_order".to_string(),
                    business_object_id: order.base.id.clone(),
                    subject_version: submission.base.id.clone(),
                    owner_role: "role-procurement".to_string(),
                    owner_organization_id: organization_id.clone(),
                    owner_user_id,
                    assignment_source: AssignmentSource::SystemRule,
                    priority: WorkItemPriority::Normal,
                    due_at: None,
                    reason_code: Some("SALES_ORDER_EFFECTIVE".to_string()),
                    impact_summary: Some(format!("销售单 {} 的 {line_count} 行待分配供给", order.order_no)),
                },
                responsibility_key,
                line_ids,
            )
            .map_err(Error::Logic)
        })
        .collect()
}

/// 幂等写入同一销售生效事务中的供给分配任务。
///
/// # 参数
/// * `db` - 数据库
/// * `items` - 按负责人分组后的完整任务集合
/// * `session` - 销售形式化事务会话
///
/// # 返回
/// 全部任务已存在或全部创建成功时返回 `Ok(())`。
///
/// # 错误
/// 同一责任键存在多条开放任务或任一写入失败时返回错误并回滚事务。
async fn persist_procurement_work_items(
    db: &Database,
    items: &[WorkItem],
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    for item in items {
        let responsibility_key = item
            .responsibility_key()
            .ok_or_else(|| Error::Internal("供给分配任务缺少责任键".to_string()))?;
        let existing = db
            .work_items()
            .list_open_procurement_by_responsibility(
                &SalesOrderId::new(item.business_object_id.clone()),
                responsibility_key,
                session,
            )
            .await?;
        if existing.len() > 1 {
            return Err(Error::ConflictError(
                "同一销售责任行集合存在多条开放供给分配任务".to_string(),
            ));
        }
        if let Some(existing) = existing.first() {
            if existing.responsibility_scope_ids() != item.responsibility_scope_ids() {
                return Err(Error::ConflictError(
                    "开放供给分配任务的冻结责任范围与当前解析不一致".to_string(),
                ));
            }
        }
        if existing.is_empty() {
            db.work_items().create(item, session).await?;
        }
    }
    Ok(())
}

/// 构建并写入销售单生效的应收往来子账与原始应收分录（§6.8/§8.1.1）。
///
/// 只应由首次生效（最终审批通过）路径调用：子账 `account_seq = 1`，分录类型
/// 为原始应收（增加方向，金额 = 版本含税合计）；后续销售变更差额由
/// `sales_review` 生效路径写入。写在同一事务内，重复生效由提交状态守卫拦截。
///
/// # 参数
/// * `db` - 数据库实例
/// * `order` - 已生效的销售单（事务内最新版本）
/// * `aggregate` - 生效版本聚合
/// * `posted_at` - 入账时间
/// * `session` - 事务会话执行器
///
/// # 返回
/// 无返回值；写入失败时返回错误。
async fn create_original_receivable(
    db: &Database,
    order: &SalesOrder,
    aggregate: &SalesOrderRevisionAggregate,
    posted_at: Instant,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let account_id = ReceivableAccountId::new(next_id());
    let entry_id = ReceivableEntryId::new(next_id());
    let revision_id = aggregate.revision.base.id.clone();
    let gross = aggregate.revision.gross_amount;
    let account = ReceivableAccount::new(
        account_id.clone(),
        ReceivableAccountData {
            sales_order_id: order.base.id.clone().into(),
            account_seq: 1,
            customer_id: order.customer_id.clone(),
            counterparty_party_id: order.settlement_party_id.clone(),
            source_sales_order_revision_id: revision_id.clone().into(),
            review_status: AccountReviewStatus::initial_for_sales_business_type(order.business_type),
            reviewed_by: None,
            reviewed_at: None,
            review_evidence_reference: None,
            gross_total: gross,
            settled_total: Amount::from_str("0.00").expect("静态零值必须合法"),
            invoiceable_total: gross,
            invoiced_total: Amount::from_str("0.00").expect("静态零值必须合法"),
        },
        "system",
    )
    .map_err(Error::Logic)?;
    let entry = ReceivableEntry::new(
        entry_id,
        ReceivableEntryData {
            receivable_account_id: account_id,
            entry_type: ReceivableEntryType::Original,
            direction: EntryDirection::Increase,
            amount: gross,
            due_date: BusinessDate::today(),
            source_fact_type: "sales_order".to_string(),
            source_document_id: order.base.id.clone(),
            source_revision_id: revision_id,
            source_sequence: 1,
            posted_at,
        },
    )
    .map_err(Error::Logic)?;
    db.receivable()
        .create_receivable_with_entry(&account, &entry, session)
        .await?;
    crate::receivable::card_funds_task::ensure_initial_card_funds_review_task(db, &account, session).await?;
    crate::receivable::invoice_task::ensure_sales_invoice_task(db, &account, session).await?;
    Ok(())
}

/// 为提交行分配正式版本头、公共行和子类型身份。
///
/// # 参数
/// * `lines` - 已冻结提交行
///
/// # 返回
/// 返回与行顺序一致的身份清单。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// ID 由服务层生成，工厂不得调用 ID 生成器。
fn allocate_formal_revision_identities(lines: &[SalesOrderSubmissionLine]) -> FormalRevisionIdentities {
    FormalRevisionIdentities::new(
        SalesOrderRevisionId::new(next_id()),
        lines
            .iter()
            .map(|line| {
                FormalRevisionLineIdentity::new(
                    entities::ids::SalesOrderRevisionLineId::new(next_id()),
                    FormalRevisionSubtypeIdentity::from_line_type(line.line_type, next_id()),
                )
            })
            .collect(),
    )
}

/// 按业务性质构造正式版本。
///
/// # 错误
/// 行类型与业务性质不一致或字段缺失时返回错误。
fn build_revision_for_order(
    order: &SalesOrder,
    submission: &SalesOrderSubmission,
    submission_lines: &[SalesOrderSubmissionLine],
    effective_at: Instant,
) -> Result<SalesOrderRevisionAggregate> {
    SalesOrderRevisionAggregate::from_sales_order_submission(
        allocate_formal_revision_identities(submission_lines),
        FormalRevisionContext::new(
            1,
            RevisionSource::ErpApproval,
            order.stable.current_revision_id.clone().map(Into::into),
            order.business_type,
            effective_at,
        ),
        submission,
        submission_lines,
    )
    .map_err(Error::Logic)
}

/// 由已形式化卡券版本构造商城执行投影与待下发记录。
///
/// # 错误
/// 冻结投影字段缺失或卡券行不唯一时返回错误。
fn build_voucher_execution_projection(
    order: &SalesOrder,
    submission: &SalesOrderSubmission,
    aggregate: &SalesOrderRevisionAggregate,
    at: Instant,
) -> Result<(
    SalesOrderProjection,
    SalesOrderProjectionRevision,
    SalesOrderProjectionDelivery,
)> {
    let voucher = aggregate
        .voucher_lines
        .first()
        .ok_or_else(|| Error::BusinessLogicError("卡券销售正式版本必须且只能包含一条卡券明细".to_string()))?;
    let target_mall_id = submission
        .target_mall_id
        .clone()
        .ok_or_else(|| Error::BusinessLogicError("卡券销售提交缺少冻结目标商城".to_string()))?;
    let projection_id = SalesOrderProjectionId::new(next_id());
    let projection = SalesOrderProjection::new(
        projection_id.clone(),
        SalesOrderProjectionData {
            sales_order_id: SalesOrderId::new(order.base.id.clone()),
            target_mall_id: target_mall_id.clone(),
        },
    )?;
    let projection_revision_id = SalesOrderProjectionRevisionId::new(next_id());
    let mut projection_revision = SalesOrderProjectionRevision::new(
        projection_revision_id.clone(),
        1,
        SalesOrderProjectionRevisionData {
            projection_id,
            projection_source: ProjectionSource::ErpRevision,
            sales_order_revision_id: aggregate.revision.base.id.clone().into(),
            customer_external_identity: submission
                .customer_external_identity
                .clone()
                .ok_or_else(|| Error::BusinessLogicError("卡券销售提交缺少冻结商城客户身份".to_string()))?,
            voucher_category_external_identity: submission
                .voucher_category_external_identity
                .clone()
                .ok_or_else(|| {
                    Error::BusinessLogicError("卡券销售提交缺少冻结商城卡券类目身份".to_string())
                })?,
            voucher_expiry_at: submission
                .voucher_expiry_at
                .ok_or_else(|| Error::BusinessLogicError("卡券销售提交缺少冻结履约期限".to_string()))?,
            face_value: voucher.face_value,
            card_count: voucher.card_count,
            card_form: match voucher.card_form {
                CardForm::Electronic => ProjectionCardForm::Electronic,
                CardForm::Physical => ProjectionCardForm::Physical,
            },
            effective_at: at,
            content_hash: "pending".to_string(),
        },
    )?;
    projection_revision.content_hash = projection_content_hash(&projection_revision);
    let delivery = SalesOrderProjectionDelivery::new(
        SalesOrderProjectionDeliveryId::new(next_id()),
        SalesOrderProjectionDeliveryData {
            projection_revision_id,
            target_mall_id,
            status: ProjectionDeliveryStatus::PendingSend,
            attempt_count: 0,
            next_attempt_at: None,
            mall_ack_at: None,
            mall_execution_baseline: None,
            error_code: None,
            error_summary: None,
        },
    )?;
    Ok((projection, projection_revision, delivery))
}

/// 在同一事务写入执行投影版本与待下发记录。
///
/// # 错误
/// 仓储写入失败时返回错误。
async fn persist_voucher_projection(
    db: &Database,
    projection: &SalesOrderProjection,
    revision: &SalesOrderProjectionRevision,
    delivery: &SalesOrderProjectionDelivery,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    db.projection()
        .create_projection_revision(projection, revision, session)
        .await?;
    db.sales_order_projection_deliveries()
        .create(delivery, session)
        .await?;
    Ok(())
}

/// 验证销售形式化事务的状态、仓储与供给任务合同。
#[cfg(test)]
mod tests {
    use super::{ensure_final_approve_formalize, procurement_responsibility_key};
    use entities::ids::{CustomerAccountId, PartyId, SalesOrderId};
    use entities::receivable::AccountReviewStatus;
    use entities::sales_order::{BusinessType, CommercialStatus, ReviewStatus, SalesOrder, SalesOrderData};

    fn draft_order() -> SalesOrder {
        SalesOrder::new(
            SalesOrderId::new("so-1"),
            SalesOrderData {
                order_no: "SO-1".into(),
                business_type: BusinessType::GoodsService,
                origin_system: entities::sales_order::OriginSystem::Erp,
                source_identity_id: None,
                customer_id: CustomerAccountId::new("cust-1"),
                contract_id: None,
                settlement_party_id: PartyId::new("party-1"),
                source_status_code: None,
            },
            "user-1",
        )
        .expect("草稿必须可构造")
    }

    /// 验证销售形式化的状态闸门、仓储入口与采购授权提交栅栏。
    ///
    /// 生产代码必须只接受审批中状态，并通过 policy CAS 提交采购责任授权快照。
    #[test]
    fn formalize_wraps_repository_and_only_accepts_in_approval() {
        let source = include_str!("formalize.rs");
        let production = source.split("/// 验证销售形式化事务").next().expect("生产代码");
        let formalize_at = production
            .find(".formalize_submission(")
            .expect("写入销售当前版本");
        let synchronize_at = production[formalize_at..]
            .find("sync_procurement_tasks_for_sales_order(")
            .map(|offset| formalize_at + offset)
            .expect("校准供给分配任务");
        assert!(
            synchronize_at > formalize_at,
            "供给任务必须在销售当前版本落库后按权威覆盖量校准"
        );
        assert!(production.contains("ensure_final_approve_formalize"));
        assert!(production.contains("build_voucher_execution_projection"));
        assert!(production.contains("create_projection_revision"));
        assert!(production.contains("run_authorized_policy_transaction(policy_revision"));
        assert!(!production.contains("CARD_SALES_APPROVAL"));
        let mut order = draft_order();
        assert!(ensure_final_approve_formalize(&order).is_err());
        order.start_approval_submission("user-1").expect("提交进入审批中");
        assert_eq!(order.review_status, ReviewStatus::InApproval);
        assert!(ensure_final_approve_formalize(&order).is_ok());
        order.commercial_status = CommercialStatus::Effective;
        order.review_status = ReviewStatus::Approved;
        assert!(order.is_fully_formalized());
        assert!(ensure_final_approve_formalize(&order).is_err());
    }

    #[test]
    fn procurement_responsibility_key_is_stable_and_boundary_safe() {
        let first = procurement_responsibility_key(&["line-1".to_string(), "line-23".to_string()])
            .expect("稳定行集合合法");
        let repeated = procurement_responsibility_key(&["line-1".to_string(), "line-23".to_string()])
            .expect("重复计算合法");
        let different = procurement_responsibility_key(&["line-12".to_string(), "line-3".to_string()])
            .expect("不同边界集合合法");

        assert_eq!(first, repeated);
        assert_ne!(first, different);
        assert!(first.starts_with("sales-lines:"));
        assert!(procurement_responsibility_key(&[]).is_err());
    }

    /// 卡券最终通过同样只接受审批中，并与投影写入同事务。
    #[test]
    fn voucher_formalize_accepts_in_approval_and_projects() {
        let source = include_str!("formalize.rs");
        assert!(source.contains("is_voucher_sales_order"));
        assert!(source.contains("persist_voucher_projection"));
        let mut order = draft_order();
        order.business_type = BusinessType::Voucher;
        assert!(ensure_final_approve_formalize(&order).is_err());
        order.start_approval_submission("user-1").expect("卡券进入审批中");
        assert_eq!(order.review_status, ReviewStatus::InApproval);
        assert!(ensure_final_approve_formalize(&order).is_ok());
        assert_eq!(
            AccountReviewStatus::initial_for_sales_business_type(order.business_type),
            AccountReviewStatus::OpeningPending
        );
        assert_eq!(
            AccountReviewStatus::initial_for_sales_business_type(BusinessType::GoodsService),
            AccountReviewStatus::NotApplicable
        );
    }
}
