//! 人工保存采购草稿。

use database::{AccessControlExt, Executor, NoTransaction, PurchaseOrderExt, SalesOrderExt};
use entities::ids::PurchaseOrderSubmissionId;
use entities::money::Amount;
use entities::purchase_order::{
    validate_draft_line_edits, DraftLineEditViolation, LegacyReceiptIdScheme, PurchaseCommandReceipt,
    PurchaseCommandReceiptError, PurchaseOrder, PurchaseOrderStatus, PurchaseOrderSubmission,
    PurchaseOrderSubmissionData, PurchaseOrderSubmissionLine, SalesProcurementCoverage, SubmissionStatus,
};
use id_generator::next_id;
use mongodb::ClientSession;
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::authorization::{ensure_purchase_order_actor_account, PurchaseOrderAuthorization};
use super::coverage::load_sales_procurement_coverage;
use super::dto::{
    SavePurchaseOrderDraftRequest, SavePurchaseOrderDraftResult, SavePurchaseOrderLine, TotalsView,
    SAVE_ACTION,
};
use super::line_input::{build_submission_lines, compute_request_totals, to_line_inputs};
use super::procurement_task_sync::sync_procurement_tasks_for_sales_order;
use super::PurchaseOrderService;
use crate::audit::AuditActor;
use crate::errors::{Error, Result};

const SAVE_PERMISSION: &str = "purchase_order:update";
const SAVE_RECEIPT_PREFIX: &str = "purchase-order-save-draft-command-";

/// 保存草稿幂等命令上下文。
struct SaveDraftCommand<'a> {
    /// 当前路径采购单 ID。
    purchase_order_id: &'a str,
    /// 原始保存请求。
    request: &'a SavePurchaseOrderDraftRequest,
    /// 稳定命令收据 ID。
    receipt_id: &'a str,
    /// 已排除幂等键的请求指纹。
    request_fingerprint: &'a str,
    /// 已认证审计操作人。
    actor: &'a AuditActor,
}

/// 保存草稿命令收据载荷。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct SaveDraftReceipt {
    /// 采购单主键。
    purchase_order_id: String,
    /// 保存完成时的乐观锁版本。
    lock_version: u64,
    /// 保存完成时的含税金额。
    gross: String,
    /// 保存完成时的不含税金额。
    net: String,
    /// 保存完成时的税额。
    tax: String,
    /// 首次成功响应的业务引用。
    reference: String,
}

/// 待写入的新采购草稿提交及金额。
struct DraftReplacement {
    /// 新草稿提交头。
    submission: PurchaseOrderSubmission,
    /// 新草稿提交行。
    lines: Vec<PurchaseOrderSubmissionLine>,
    /// 含税金额。
    gross: Amount,
    /// 不含税金额。
    net: Amount,
    /// 税额。
    tax: Amount,
}

impl PurchaseOrderService {
    /// 保存采购草稿（表头 + 完整行替换，单事务）。
    ///
    /// # 参数
    /// * `id` - 采购单 ID
    /// * `req` - 期望版本、完整草稿行与幂等键
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 首次执行返回新版本与服务端金额；同键同载荷返回首次成功的原结果。
    ///
    /// # 错误
    /// 采购单或草稿不存在、操作人不是原采购制单人、版本或状态冲突、来源行变化、
    /// 超采、同键异载荷、事务写入失败或提交结果仍无法确认时返回错误。
    ///
    /// # 关键业务约束
    /// 先校验 `created_by` 再校验版本和状态；草稿替换、覆盖重算、任务同步和命令
    /// 收据必须同事务提交，事务失败后必须回读收据覆盖已提交但响应丢失的情况。
    pub async fn save_draft(
        &self,
        id: &str,
        req: SavePurchaseOrderDraftRequest,
        actor: &AuditActor,
    ) -> Result<SavePurchaseOrderDraftResult> {
        req.validate()?;
        req.ensure_shape()?;
        let authorization = self.authorize_actor_permission(actor, SAVE_PERMISSION).await?;
        let fingerprint = req.request_fingerprint(id)?;
        let receipt_identity = PurchaseCommandReceipt::<SaveDraftReceipt>::identity(
            SAVE_RECEIPT_PREFIX,
            actor.id(),
            SAVE_ACTION,
            Some(id),
            &req.idempotency_key,
            LegacyReceiptIdScheme::None,
        )?;
        let receipt_id = receipt_identity.receipt_id().to_string();
        if let Some(result) =
            replay_saved_draft(&self.db, &receipt_id, &fingerprint, id, actor, &mut NoTransaction).await?
        {
            return Ok(result);
        }
        execute_save_draft_transaction(self, id, req, receipt_id, fingerprint, actor, authorization).await
    }
}

/// 执行保存草稿事务并在失败后回读命令收据。
///
/// # 参数
/// * `service` - 采购单服务
/// * `purchase_order_id` - 当前路径采购单 ID
/// * `request` - 已通过 DTO 校验的保存请求
/// * `receipt_id` - 稳定命令收据 ID
/// * `fingerprint` - 请求载荷指纹
/// * `actor` - 已认证审计操作人
/// * `authorization` - 与事务提交绑定的授权源和策略版本
///
/// # 返回
/// 返回首次事务结果或提交成功后回读到的原结果。
///
/// # 错误
/// 事务失败且没有匹配收据，或回读发现同键异载荷时返回错误。
///
/// # 关键业务约束
/// 任意事务错误都必须执行一次无事务收据回读，以覆盖提交响应丢失。
async fn execute_save_draft_transaction(
    service: &PurchaseOrderService,
    purchase_order_id: &str,
    request: SavePurchaseOrderDraftRequest,
    receipt_id: String,
    fingerprint: String,
    actor: &AuditActor,
    authorization: PurchaseOrderAuthorization,
) -> Result<SavePurchaseOrderDraftResult> {
    let db = service.db.clone();
    let PurchaseOrderAuthorization {
        rbac,
        policy_revision,
    } = authorization;
    let transaction_order_id = purchase_order_id.to_string();
    let transaction_actor = actor.clone();
    let transaction_receipt_id = receipt_id.clone();
    let transaction_fingerprint = fingerprint.clone();
    let transaction_result = rbac
        .run_authorized_policy_transaction(policy_revision, move |session| {
            Box::pin(async move {
                ensure_purchase_order_actor_account(&db, &transaction_actor, session).await?;
                let command = SaveDraftCommand {
                    purchase_order_id: &transaction_order_id,
                    request: &request,
                    receipt_id: &transaction_receipt_id,
                    request_fingerprint: &transaction_fingerprint,
                    actor: &transaction_actor,
                };
                save_draft_in_transaction(&db, &command, session).await
            })
        })
        .await;
    recover_saved_draft(
        transaction_result,
        &service.db,
        &receipt_id,
        &fingerprint,
        purchase_order_id,
        actor,
    )
    .await
}

/// 在 MongoDB 事务内校验并替换采购草稿提交。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `command` - 保存请求、收据身份和操作人上下文
/// * `session` - MongoDB 事务会话
///
/// # 返回
/// 返回首次保存结果或事务内命中的原收据结果。
///
/// # 错误
/// 目标、版本、状态、来源数量、金额计算或持久化失败时返回错误。
///
/// # 关键业务约束
/// 事务内先查收据；未命中时先校验创建人，再校验版本和状态。
async fn save_draft_in_transaction(
    db: &mongodb::Database,
    command: &SaveDraftCommand<'_>,
    session: &mut ClientSession,
) -> Result<SavePurchaseOrderDraftResult> {
    if let Some(result) = replay_saved_draft(
        db,
        command.receipt_id,
        command.request_fingerprint,
        command.purchase_order_id,
        command.actor,
        session,
    )
    .await?
    {
        return Ok(result);
    }
    let mut order = load_purchase_order(db, command.purchase_order_id, session).await?;
    ensure_save_target(
        &order.stable.created_by,
        order.base.version,
        order.stable.status,
        command.request.expected_lock_version,
        command.actor.id(),
    )?;
    order
        .ensure_payment_term_unchanged(command.request.payment_term_code.as_deref())
        .map_err(map_draft_edit_violation)?;
    replace_current_draft(db, &mut order, command, session).await
}

/// 加载并替换当前采购草稿提交。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `order` - 已完成创建人、版本和状态校验的采购单
/// * `command` - 保存请求、收据身份和操作人上下文
/// * `session` - MongoDB 事务会话
///
/// # 返回
/// 返回事务内持久化后的保存结果。
///
/// # 错误
/// 草稿、销售来源、覆盖、金额计算、实体更新或仓储写入失败时返回错误。
///
/// # 关键业务约束
/// 销售 procurement guard 推进后必须重算覆盖，再校验本次完整行替换。
async fn replace_current_draft(
    db: &mongodb::Database,
    order: &mut PurchaseOrder,
    command: &SaveDraftCommand<'_>,
    session: &mut ClientSession,
) -> Result<SavePurchaseOrderDraftResult> {
    let (mut old_draft, old_lines) = load_current_draft(db, order, session).await?;
    let coverage = advance_guard_and_load_coverage(db, order, command.actor.id(), session).await?;
    let requested_lines = command.request.resolve_lines(&old_lines)?;
    let requested_edits = requested_lines
        .iter()
        .map(SavePurchaseOrderLine::to_draft_edit)
        .collect::<Vec<_>>();
    validate_draft_line_edits(&requested_edits, &old_lines, &coverage.lines)
        .map_err(map_draft_edit_violation)?;
    let replacement = build_draft_replacement(order, &old_draft, &requested_lines)?;
    order.update(Default::default(), command.actor.id())?;
    old_draft.mark_superseded()?;
    order.current_submission_id = Some(replacement.submission.base.id.clone());
    persist_draft_replacement(db, order, &mut old_draft, &replacement, command, session).await
}

/// 按 ID 加载采购单。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `purchase_order_id` - 采购单 ID
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回存在的采购单。
///
/// # 错误
/// 采购单不存在或仓储读取失败时返回错误。
///
/// # 关键业务约束
/// 本函数不执行版本或状态校验，避免打乱调用方的安全校验顺序。
async fn load_purchase_order(
    db: &mongodb::Database,
    purchase_order_id: &str,
    executor: &mut dyn Executor,
) -> Result<PurchaseOrder> {
    db.purchase_orders()
        .find_by_id(purchase_order_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("采购单不存在".to_string()))
}

/// 校验采购草稿保存目标的创建人、版本和状态。
///
/// # 参数
/// * `created_by` - 采购单创建人 ID
/// * `current_version` - 采购单当前乐观锁版本
/// * `status` - 采购单当前状态
/// * `expected_lock_version` - 客户端期望版本
/// * `actor_id` - 当前操作人 ID
///
/// # 返回
/// 创建人、版本和草稿状态全部匹配时返回 `Ok(())`。
///
/// # 错误
/// 非创建人统一返回不存在；其后才允许返回版本或状态错误。
///
/// # 关键业务约束
/// 创建人校验必须先于版本和状态，禁止向其他账号泄露资源版本或生命周期状态。
fn ensure_save_target(
    created_by: &str,
    current_version: u64,
    status: PurchaseOrderStatus,
    expected_lock_version: u64,
    actor_id: &str,
) -> Result<()> {
    if created_by != actor_id {
        return Err(Error::NotFound("采购单不存在或不可编辑".to_string()));
    }
    if current_version != expected_lock_version {
        return Err(Error::ConflictError(
            "数据已被其他请求修改，请刷新后重试".to_string(),
        ));
    }
    if status != PurchaseOrderStatus::Draft {
        return Err(Error::BusinessLogicError(
            "只有草稿状态的采购单可以编辑".to_string(),
        ));
    }
    Ok(())
}

/// 加载当前可编辑草稿提交及其完整行。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `order` - 已校验可编辑的采购单
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回草稿提交头和该提交的完整行。
///
/// # 错误
/// 当前草稿引用缺失、提交不存在、提交已冻结或仓储读取失败时返回错误。
///
/// # 关键业务约束
/// 只允许替换状态仍为 `Draft` 的当前提交。
async fn load_current_draft(
    db: &mongodb::Database,
    order: &PurchaseOrder,
    executor: &mut dyn Executor,
) -> Result<(PurchaseOrderSubmission, Vec<PurchaseOrderSubmissionLine>)> {
    let draft_id = order
        .current_submission_id
        .as_ref()
        .map(ToString::to_string)
        .ok_or_else(|| Error::BusinessLogicError("采购单缺少草稿提交".to_string()))?;
    let draft_id = PurchaseOrderSubmissionId::new(draft_id);
    let draft = db
        .purchase_order_submissions()
        .find_by_id(draft_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("草稿提交不存在".to_string()))?;
    if draft.status != SubmissionStatus::Draft {
        return Err(Error::BusinessLogicError("草稿提交已冻结，不能保存".to_string()));
    }
    let lines = db
        .purchase_order_submission_lines()
        .find_lines_by_submission_ids(std::slice::from_ref(&draft_id), executor)
        .await?;
    Ok((draft, lines))
}

/// 推进销售采购 guard 并加载最新采购覆盖。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `order` - 当前采购单
/// * `actor_id` - 当前操作人 ID
/// * `session` - MongoDB 事务会话
///
/// # 返回
/// 返回 guard CAS 成功后的最新采购覆盖。
///
/// # 错误
/// 来源销售单不存在、guard 冲突或覆盖加载失败时返回错误。
///
/// # 关键业务约束
/// guard 与后续草稿写入处于同一事务，保证与采购创建命令串行化。
pub(super) async fn advance_guard_and_load_coverage(
    db: &mongodb::Database,
    order: &PurchaseOrder,
    actor_id: &str,
    session: &mut ClientSession,
) -> Result<SalesProcurementCoverage> {
    let mut sales_order = db
        .sales_orders()
        .find_by_id(&order.sales_order_id, session)
        .await?
        .ok_or_else(|| Error::NotFound("来源销售单不存在".to_string()))?;
    sales_order.advance_procurement_guard(actor_id)?;
    db.sales_orders().update(&mut sales_order, session).await?;
    load_sales_procurement_coverage(db, &sales_order, session).await
}

/// 构造完整替换后的新草稿提交、行和金额。
///
/// # 参数
/// * `order` - 当前采购单
/// * `old_draft` - 当前草稿提交
/// * `lines` - 已通过草稿编辑校验的完整请求行
///
/// # 返回
/// 返回已通过实体校验的新提交、完整行和服务端金额。
///
/// # 错误
/// 金额计算、提交构造或草稿行构造失败时返回错误。
///
/// # 关键业务约束
/// 供应商、采购类型、履约责任和供应商快照均继承原草稿，不接受客户端改写。
fn build_draft_replacement(
    order: &PurchaseOrder,
    old_draft: &PurchaseOrderSubmission,
    lines: &[SavePurchaseOrderLine],
) -> Result<DraftReplacement> {
    let inputs = to_line_inputs(lines)?;
    let (gross, net, tax) = compute_request_totals(&inputs)?;
    let submission = PurchaseOrderSubmission::new(
        PurchaseOrderSubmissionId::new(next_id()),
        PurchaseOrderSubmissionData {
            purchase_order_id: order.base.id.clone().into(),
            submission_no: format!("DRAFT-{}", &next_id()[..8]),
            supplier_id: old_draft.supplier_id.clone(),
            purchase_type: old_draft.purchase_type,
            fulfillment_responsibility: old_draft.fulfillment_responsibility,
            supplier_revision_id: old_draft.supplier_revision_id.clone(),
            supplier_snapshot: old_draft.supplier_snapshot.clone(),
            payment_term_snapshot: old_draft.payment_term_snapshot.clone(),
            gross_amount: gross,
            net_amount: net,
            tax_amount: tax,
        },
    )?;
    let lines = build_submission_lines(&submission.base.id.clone().into(), &inputs)?;
    Ok(DraftReplacement {
        submission,
        lines,
        gross,
        net,
        tax,
    })
}

/// 持久化草稿替换、同步任务并写入稳定命令收据。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `order` - 已切换当前提交指针的采购单
/// * `old_draft` - 已标记为被替代的旧草稿
/// * `replacement` - 新草稿提交、完整行和金额
/// * `command` - 保存请求、收据身份和操作人上下文
/// * `session` - MongoDB 事务会话
///
/// # 返回
/// 返回首次成功响应中需要稳定回放的结果。
///
/// # 错误
/// 任一仓储写入、任务同步、收据编码或审计写入失败时返回错误。
///
/// # 关键业务约束
/// 采购单更新后取得的新版本必须写入同事务命令收据。
async fn persist_draft_replacement(
    db: &mongodb::Database,
    order: &mut PurchaseOrder,
    old_draft: &mut PurchaseOrderSubmission,
    replacement: &DraftReplacement,
    command: &SaveDraftCommand<'_>,
    session: &mut ClientSession,
) -> Result<SavePurchaseOrderDraftResult> {
    db.purchase_order_submissions().update(old_draft, session).await?;
    db.purchase_order_submissions()
        .create(&replacement.submission, session)
        .await?;
    for line in &replacement.lines {
        db.purchase_order_submission_lines().create(line, session).await?;
    }
    db.purchase_orders().update(order, session).await?;
    sync_procurement_tasks_for_sales_order(db, &order.sales_order_id, session).await?;
    let receipt = SaveDraftReceipt::from_saved(order, replacement);
    let audit = command.actor.clone().resource_log_with_id(
        command.receipt_id.to_string(),
        SAVE_ACTION,
        "purchase_order",
        order.base.id.clone(),
        Some(
            PurchaseCommandReceipt::new(command.request_fingerprint.to_string(), receipt.clone())
                .encode_message()?,
        ),
    )?;
    db.audit_logs().create(&audit, session).await?;
    Ok(receipt.into_result())
}

/// 查询并校验保存草稿命令收据。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `receipt_id` - 稳定命令收据 ID
/// * `expected_fingerprint` - 当前请求载荷指纹
/// * `purchase_order_id` - 当前路径采购单 ID
/// * `actor` - 当前操作人
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 收据不存在返回 `None`；身份与载荷一致时返回首次保存结果。
///
/// # 错误
/// 同键异载荷、收据身份不一致、收据损坏、采购单缺失或版本倒退时返回错误。
///
/// # 关键业务约束
/// 事务前、事务内和事务失败后必须复用同一回放校验。
async fn replay_saved_draft(
    db: &mongodb::Database,
    receipt_id: &str,
    expected_fingerprint: &str,
    purchase_order_id: &str,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<Option<SavePurchaseOrderDraftResult>> {
    let Some(audit) = db.audit_logs().find_by_id(receipt_id, executor).await? else {
        return Ok(None);
    };
    let receipt = match PurchaseCommandReceipt::<SaveDraftReceipt>::decode(
        &audit,
        actor.id(),
        SAVE_ACTION,
        Some(purchase_order_id),
        expected_fingerprint,
    ) {
        Ok(receipt) => receipt,
        Err(PurchaseCommandReceiptError::IdentityMismatch | PurchaseCommandReceiptError::PayloadConflict) => {
            return Err(Error::ConflictError("幂等键已用于不同采购命令".to_string()));
        }
        Err(PurchaseCommandReceiptError::Corrupted(message)) => {
            return Err(Error::Internal(message));
        }
    };
    if receipt.payload().purchase_order_id != purchase_order_id {
        return Err(Error::ConflictError(
            "采购草稿保存收据与业务资源不一致".to_string(),
        ));
    }
    let order = load_purchase_order(db, purchase_order_id, executor).await?;
    if order.base.version < receipt.payload().lock_version {
        return Err(Error::Internal("采购草稿保存收据版本超前".to_string()));
    }
    Ok(Some(receipt.into_payload().into_result()))
}

/// 在事务错误后回读保存草稿收据并决定最终响应。
///
/// # 参数
/// * `transaction_result` - MongoDB 事务返回结果
/// * `db` - MongoDB 数据库
/// * `receipt_id` - 稳定命令收据 ID
/// * `fingerprint` - 当前请求载荷指纹
/// * `purchase_order_id` - 当前路径采购单 ID
/// * `actor` - 当前操作人
///
/// # 返回
/// 事务成功返回原结果；事务失败但收据存在时返回已提交结果。
///
/// # 错误
/// 事务失败且没有匹配收据，或回读收据冲突、损坏时返回错误。
///
/// # 关键业务约束
/// 回读只执行一次，不把没有收据的失败请求误判为成功。
async fn recover_saved_draft(
    transaction_result: Result<SavePurchaseOrderDraftResult>,
    db: &mongodb::Database,
    receipt_id: &str,
    fingerprint: &str,
    purchase_order_id: &str,
    actor: &AuditActor,
) -> Result<SavePurchaseOrderDraftResult> {
    match transaction_result {
        Ok(result) => Ok(result),
        Err(error) => replay_saved_draft(
            db,
            receipt_id,
            fingerprint,
            purchase_order_id,
            actor,
            &mut NoTransaction,
        )
        .await?
        .ok_or(error),
    }
}

impl SaveDraftReceipt {
    /// 从已持久化采购单和新草稿金额构造稳定收据。
    ///
    /// # 参数
    /// * `order` - Repository 更新后带新版本的采购单
    /// * `replacement` - 新草稿提交和金额
    ///
    /// # 返回
    /// 返回可持久化并稳定回放的保存结果载荷。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 版本和业务引用必须与首次成功响应完全一致。
    fn from_saved(order: &PurchaseOrder, replacement: &DraftReplacement) -> Self {
        Self {
            purchase_order_id: order.base.id.clone(),
            lock_version: order.base.version,
            gross: replacement.gross.to_string(),
            net: replacement.net.to_string(),
            tax: replacement.tax.to_string(),
            reference: format!("SAVED-V{}", order.base.version),
        }
    }

    /// 转换为保存草稿 API 结果。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回首次执行与后续回放共享的原始结果。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 回放不得使用采购单当前版本或重新计算金额覆盖收据结果。
    fn into_result(self) -> SavePurchaseOrderDraftResult {
        SavePurchaseOrderDraftResult {
            lock_version: self.lock_version,
            totals: TotalsView {
                gross: self.gross,
                net: self.net,
                tax: self.tax,
            },
            reference: self.reference,
        }
    }
}

/// 把领域草稿编辑校验失败映射为稳定 HTTP 语义。
///
/// # 参数
/// * `violation` - 领域校验失败原因
///
/// # 返回
/// 返回校验、业务或冲突错误，文案与既有实现一致。
pub(super) fn map_draft_edit_violation(violation: DraftLineEditViolation) -> Error {
    match violation {
        DraftLineEditViolation::SourceLineCountChanged
        | DraftLineEditViolation::DuplicateSalesLine
        | DraftLineEditViolation::RewrittenSalesLine
        | DraftLineEditViolation::RewrittenSourceReference
        | DraftLineEditViolation::MissingSalesLineId
        | DraftLineEditViolation::MissingQuantity
        | DraftLineEditViolation::MissingAllocatedQuantity
        | DraftLineEditViolation::InvalidQuantity(_)
        | DraftLineEditViolation::QuantityAllocationMismatch
        | DraftLineEditViolation::PaymentTermChanged => Error::ValidationError(violation.to_string()),
        DraftLineEditViolation::SourceLineRemoved | DraftLineEditViolation::ExceedsAvailableQuantity => {
            Error::ConflictError(violation.to_string())
        }
        DraftLineEditViolation::MissingSalesStableLine
        | DraftLineEditViolation::MissingOriginalAllocatedQuantity => {
            Error::BusinessLogicError(violation.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use entities::purchase_order::{PurchaseLineType, PurchaseOrderStatus};
    use validator::Validate;

    use super::ensure_save_target;
    use crate::errors::Error;
    use crate::purchase_order::dto::{SavePurchaseOrderDraftRequest, SavePurchaseOrderLine};

    /// 构造最小保存草稿请求。
    ///
    /// # 参数
    /// * `quantity` - 商品行采购与分配数量
    ///
    /// # 返回
    /// 返回用于请求指纹测试的完整 DTO。
    ///
    /// # 错误
    /// 无。
    fn save_request(quantity: &str) -> SavePurchaseOrderDraftRequest {
        SavePurchaseOrderDraftRequest {
            expected_lock_version: 3,
            payment_term_code: Some(" NET-30 ".to_string()),
            lines: vec![SavePurchaseOrderLine {
                line_type: PurchaseLineType::ItemService,
                procurement_confirmation_line_id: None,
                sku_id: Some("sku-1".to_string()),
                sku_revision_id: Some("sku-rev-1".to_string()),
                product_name: Some("产品".to_string()),
                specification: None,
                quantity: Some(quantity.to_string()),
                base_unit_code: Some("EA".to_string()),
                unit_cost_gross: Some("10".to_string()),
                input_tax_rate: Some("0.13".to_string()),
                expected_delivery_date: Some("2026-08-25".to_string()),
                sales_order_line_id: Some("sales-line-1".to_string()),
                sales_order_revision_line_id: Some("sales-revision-line-1".to_string()),
                sales_order_submission_line_id: Some("sales-submission-line-1".to_string()),
                allocated_quantity: Some(quantity.to_string()),
                gross_amount: None,
            }],
            line_patches: vec![],
            idempotency_key: "save-key-1".to_string(),
        }
    }

    /// 验证非创建人无法观察目标采购单的版本或状态错误。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 创建人校验未优先返回统一不存在错误时测试失败。
    #[test]
    fn save_target_checks_creator_before_version_and_status() {
        let mut request = save_request("1");
        request.expected_lock_version = 1;
        let status = PurchaseOrderStatus::Voided;
        let result = ensure_save_target("creator-1", 99, status, request.expected_lock_version, "actor-2");

        assert!(matches!(result, Err(Error::NotFound(_))));
    }

    /// 验证保存请求拒绝超过审计收据边界的幂等键。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 超过 128 字符的幂等键通过 DTO 校验时测试失败。
    #[test]
    fn save_request_rejects_unbounded_idempotency_key() {
        let mut request = save_request("1");
        request.idempotency_key = "k".repeat(129);

        assert!(request.validate().is_err());
    }

    /// 验证保存请求指纹排除幂等键并覆盖实际命令载荷。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 相同业务载荷因幂等键变化而改变或数量变化未改变指纹时测试失败。
    #[test]
    fn save_fingerprint_is_payload_stable_and_key_independent() {
        let first = save_request("1");
        let mut same_payload = first.clone();
        same_payload.idempotency_key = "another-key".to_string();
        let different_payload = save_request("2");

        assert_eq!(
            first.request_fingerprint("po-1").unwrap(),
            same_payload.request_fingerprint("po-1").unwrap()
        );
        assert_ne!(
            first.request_fingerprint("po-1").unwrap(),
            different_payload.request_fingerprint("po-1").unwrap()
        );
    }

    /// 验证采购草稿保存把操作人授权绑定到事务提交。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 保存命令缺少稳定授权快照、事务内账号重验或 policy revision CAS 时测试失败。
    #[test]
    fn save_draft_binds_actor_authorization_to_commit() {
        let production = include_str!("draft_edit.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码必须存在");

        assert!(production.contains("authorize_actor_permission(actor, SAVE_PERMISSION)"));
        assert!(production.contains("ensure_purchase_order_actor_account"));
        assert!(production.contains("run_authorized_policy_transaction(policy_revision"));
    }

    /// 验证草稿编辑规则已下沉 DTO 与实体，服务只编排事实并映射领域错误。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 保存路径重新实现补丁合并或来源校验时测试失败。
    #[test]
    fn draft_edit_rules_are_domain_owned() {
        let production = include_str!("draft_edit.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码必须存在");

        assert!(production.contains("req.ensure_shape()"));
        assert!(production.contains("resolve_lines(&old_lines)"));
        assert!(production.contains("to_draft_edit"));
        assert!(production.contains("validate_draft_line_edits"));
        assert!(production.contains("ensure_payment_term_unchanged"));
        assert!(production.contains("map_draft_edit_violation"));
        assert!(!production.contains("fn ensure_save_request_shape"));
        assert!(!production.contains("fn resolve_line_patches"));
        assert!(!production.contains("fn validate_procurement_line_edit"));
        assert!(!production.contains("fn ensure_source_references_unchanged"));
        assert!(!production.contains("fn parse_required_quantity"));
    }
}
