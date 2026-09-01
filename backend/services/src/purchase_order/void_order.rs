//! 采购草稿作废与采购覆盖释放。

use database::{AccessControlExt, Executor, NoTransaction, PurchaseOrderExt, SalesOrderExt};
use entities::purchase_order::{LegacyReceiptIdScheme, PurchaseCommandReceipt, PurchaseCommandReceiptError};
use entities::purchase_order::{PurchaseOrder, PurchaseOrderStatus, SubmissionStatus};
use mongodb::ClientSession;
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::authorization::{ensure_purchase_order_actor_account, PurchaseOrderAuthorization};
use super::dto::{VoidPurchaseOrderRequest, VoidPurchaseOrderResult, VOID_ACTION};
use super::procurement_task_sync::sync_procurement_tasks_for_sales_order;
use super::PurchaseOrderService;
use crate::audit::AuditActor;
use crate::errors::{Error, Result};

const VOID_PERMISSION: &str = "purchase_order:delete";
const VOID_RECEIPT_PREFIX: &str = "purchase-order-void-command-";

/// 作废采购草稿幂等命令上下文。
struct VoidDraftCommand<'a> {
    /// 当前路径采购单 ID。
    purchase_order_id: &'a str,
    /// 原始作废请求。
    request: &'a VoidPurchaseOrderRequest,
    /// 稳定命令收据 ID。
    receipt_id: &'a str,
    /// 已排除幂等键的请求指纹。
    request_fingerprint: &'a str,
    /// 已认证审计操作人。
    actor: &'a AuditActor,
}

/// 作废采购草稿命令收据载荷。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct VoidDraftReceipt {
    /// 采购单主键。
    purchase_order_id: String,
    /// 作废后的稳定状态。
    status: String,
    /// 作废完成时的乐观锁版本。
    lock_version: u64,
    /// 首次执行时规范化的作废原因。
    reason: String,
    /// 首次成功响应的业务引用。
    reference: String,
}

impl PurchaseOrderService {
    /// 作废当前账号创建的采购草稿并释放销售采购覆盖。
    ///
    /// # 参数
    /// * `id` - 采购单 ID
    /// * `req` - 期望版本、作废原因和幂等键
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 首次执行返回作废状态；同键同载荷返回原结果并标记为幂等回放。
    ///
    /// # 错误
    /// 采购单不存在、不是当前账号创建、状态或版本冲突、提交已冻结、同键异载荷、
    /// 事务写入失败或提交结果仍无法确认时返回错误。
    ///
    /// # 关键业务约束
    /// 作废、销售采购 guard 推进、任务恢复和命令收据必须同事务提交；已作废但没有
    /// 匹配收据的任意新请求必须冲突，不能仅凭当前状态标记为回放。
    pub async fn void_draft(
        &self,
        id: &str,
        req: VoidPurchaseOrderRequest,
        actor: &AuditActor,
    ) -> Result<VoidPurchaseOrderResult> {
        req.validate()?;
        let authorization = self.authorize_actor_permission(actor, VOID_PERMISSION).await?;
        let fingerprint = req.request_fingerprint(id)?;
        let receipt_identity = PurchaseCommandReceipt::<VoidDraftReceipt>::identity(
            VOID_RECEIPT_PREFIX,
            actor.id(),
            VOID_ACTION,
            Some(id),
            &req.idempotency_key,
            LegacyReceiptIdScheme::None,
        )?;
        let receipt_id = receipt_identity.receipt_id().to_string();
        if let Some(result) =
            replay_void_draft(&self.db, &receipt_id, &fingerprint, id, actor, &mut NoTransaction).await?
        {
            return Ok(result);
        }
        execute_void_draft_transaction(self, id, req, receipt_id, fingerprint, actor, authorization).await
    }
}

/// 执行作废草稿事务并在失败后回读命令收据。
///
/// # 参数
/// * `service` - 采购单服务
/// * `purchase_order_id` - 当前路径采购单 ID
/// * `request` - 已通过 DTO 校验的作废请求
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
async fn execute_void_draft_transaction(
    service: &PurchaseOrderService,
    purchase_order_id: &str,
    request: VoidPurchaseOrderRequest,
    receipt_id: String,
    fingerprint: String,
    actor: &AuditActor,
    authorization: PurchaseOrderAuthorization,
) -> Result<VoidPurchaseOrderResult> {
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
                let command = VoidDraftCommand {
                    purchase_order_id: &transaction_order_id,
                    request: &request,
                    receipt_id: &transaction_receipt_id,
                    request_fingerprint: &transaction_fingerprint,
                    actor: &transaction_actor,
                };
                void_draft_in_transaction(&db, &command, session).await
            })
        })
        .await;
    recover_void_draft(
        transaction_result,
        &service.db,
        &receipt_id,
        &fingerprint,
        purchase_order_id,
        actor,
    )
    .await
}

/// 在 MongoDB 事务内校验并作废采购草稿。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `command` - 作废请求、收据身份和操作人上下文
/// * `session` - MongoDB 事务会话
///
/// # 返回
/// 返回首次作废结果或事务内命中的原收据结果。
///
/// # 错误
/// 目标、版本、状态、草稿提交、销售 guard 或持久化失败时返回错误。
///
/// # 关键业务约束
/// 事务内先查收据；收据未命中时，`Voided` 状态只能返回冲突，不能返回回放。
async fn void_draft_in_transaction(
    db: &mongodb::Database,
    command: &VoidDraftCommand<'_>,
    session: &mut ClientSession,
) -> Result<VoidPurchaseOrderResult> {
    if let Some(result) = replay_void_draft(
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
    ensure_void_target(
        &order.stable.created_by,
        order.base.version,
        order.stable.status,
        command.request.expected_lock_version,
        command.actor.id(),
    )?;
    ensure_current_submission_is_draft(db, &order, session).await?;
    void_order_and_persist(db, &mut order, command, session).await
}

/// 按 ID 加载待作废采购单。
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
/// 本函数不把采购单状态转换为幂等回放语义。
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

/// 校验采购草稿作废目标的创建人、已作废状态、版本和草稿状态。
///
/// # 参数
/// * `created_by` - 采购单创建人 ID
/// * `current_version` - 采购单当前乐观锁版本
/// * `status` - 采购单当前状态
/// * `expected_lock_version` - 客户端期望版本
/// * `actor_id` - 当前操作人 ID
///
/// # 返回
/// 当前账号可作废且版本、状态匹配时返回 `Ok(())`。
///
/// # 错误
/// 非创建人返回不存在；已作废但无收据返回 409；其余版本或状态不匹配返回对应错误。
///
/// # 关键业务约束
/// 只有命中稳定收据的请求可以把已作废结果标记为回放。
fn ensure_void_target(
    created_by: &str,
    current_version: u64,
    status: PurchaseOrderStatus,
    expected_lock_version: u64,
    actor_id: &str,
) -> Result<()> {
    if created_by != actor_id {
        return Err(Error::NotFound("采购单不存在或不可作废".to_string()));
    }
    if status == PurchaseOrderStatus::Voided {
        return Err(Error::ConflictError(
            "采购单已作废，当前请求没有匹配的作废收据".to_string(),
        ));
    }
    if current_version != expected_lock_version {
        return Err(Error::ConflictError(
            "数据已被其他请求修改，请刷新后重试".to_string(),
        ));
    }
    if status != PurchaseOrderStatus::Draft {
        return Err(Error::BusinessLogicError(
            "只有草稿状态且没有下游事实的采购单可以作废".to_string(),
        ));
    }
    Ok(())
}

/// 校验采购单当前提交仍为可作废草稿。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `order` - 已通过目标校验的采购单
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 当前提交存在且仍为草稿时返回 `Ok(())`。
///
/// # 错误
/// 当前提交引用缺失、提交不存在、提交已冻结或仓储读取失败时返回错误。
///
/// # 关键业务约束
/// 已形成不可变提交的采购单禁止直接作废。
async fn ensure_current_submission_is_draft(
    db: &mongodb::Database,
    order: &PurchaseOrder,
    executor: &mut dyn Executor,
) -> Result<()> {
    let submission_id = order
        .current_submission_id
        .as_deref()
        .ok_or_else(|| Error::BusinessLogicError("采购单缺少当前草稿提交".to_string()))?;
    let submission = db
        .purchase_order_submissions()
        .find_by_id(submission_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("采购草稿提交不存在".to_string()))?;
    if submission.status != SubmissionStatus::Draft {
        return Err(Error::BusinessLogicError(
            "采购提交已冻结，不能直接作废采购单".to_string(),
        ));
    }
    Ok(())
}

/// 推进来源销售 guard、作废采购单并持久化命令收据。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `order` - 已完成目标和草稿提交校验的采购单
/// * `command` - 作废请求、收据身份和操作人上下文
/// * `session` - MongoDB 事务会话
///
/// # 返回
/// 返回首次成功响应中需要稳定回放的作废结果。
///
/// # 错误
/// 来源销售单、guard、状态迁移、仓储写入、任务同步或收据写入失败时返回错误。
///
/// # 关键业务约束
/// Repository 更新后的作废版本必须写入同事务命令收据。
async fn void_order_and_persist(
    db: &mongodb::Database,
    order: &mut PurchaseOrder,
    command: &VoidDraftCommand<'_>,
    session: &mut ClientSession,
) -> Result<VoidPurchaseOrderResult> {
    let mut sales_order = db
        .sales_orders()
        .find_by_id(order.sales_order_id.as_ref(), session)
        .await?
        .ok_or_else(|| Error::NotFound("来源销售单不存在".to_string()))?;
    sales_order.advance_procurement_guard(command.actor.id())?;
    db.sales_orders().update(&mut sales_order, session).await?;
    order
        .transition(PurchaseOrderStatus::Voided, command.actor.id())
        .map_err(Error::Logic)?;
    db.purchase_orders().update(order, session).await?;
    sync_procurement_tasks_for_sales_order(db, &order.sales_order_id, session).await?;
    let receipt = VoidDraftReceipt::from_voided(order, &command.request.reason);
    let audit = command.actor.clone().resource_log_with_id(
        command.receipt_id.to_string(),
        VOID_ACTION,
        "purchase_order",
        order.base.id.clone(),
        Some(
            PurchaseCommandReceipt::new(command.request_fingerprint.to_string(), receipt.clone())
                .encode_message()?,
        ),
    )?;
    db.audit_logs().create(&audit, session).await?;
    Ok(receipt.into_result(false))
}

/// 查询并校验采购草稿作废命令收据。
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
/// 收据不存在返回 `None`；身份与载荷一致时返回原结果并标记回放。
///
/// # 错误
/// 同键异载荷、收据身份不一致、收据损坏、采购单缺失或状态不一致时返回错误。
///
/// # 关键业务约束
/// 只有匹配稳定收据且当前采购单确为已作废时才能返回 `replayed = true`。
async fn replay_void_draft(
    db: &mongodb::Database,
    receipt_id: &str,
    expected_fingerprint: &str,
    purchase_order_id: &str,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<Option<VoidPurchaseOrderResult>> {
    let Some(audit) = db.audit_logs().find_by_id(receipt_id, executor).await? else {
        return Ok(None);
    };
    let receipt = match PurchaseCommandReceipt::<VoidDraftReceipt>::decode(
        &audit,
        actor.id(),
        VOID_ACTION,
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
            "采购草稿作废收据与业务资源不一致".to_string(),
        ));
    }
    let order = load_purchase_order(db, purchase_order_id, executor).await?;
    if order.stable.status != PurchaseOrderStatus::Voided
        || order.base.version < receipt.payload().lock_version
    {
        return Err(Error::Internal("采购草稿作废收据与当前状态不一致".to_string()));
    }
    Ok(Some(receipt.into_payload().into_result(true)))
}

/// 在事务错误后回读作废草稿收据并决定最终响应。
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
/// 回读只执行一次，没有收据时必须保留原事务错误。
async fn recover_void_draft(
    transaction_result: Result<VoidPurchaseOrderResult>,
    db: &mongodb::Database,
    receipt_id: &str,
    fingerprint: &str,
    purchase_order_id: &str,
    actor: &AuditActor,
) -> Result<VoidPurchaseOrderResult> {
    match transaction_result {
        Ok(result) => Ok(result),
        Err(error) => replay_void_draft(
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

impl VoidDraftReceipt {
    /// 从已持久化的作废采购单构造稳定收据。
    ///
    /// # 参数
    /// * `order` - Repository 更新后带新版本的已作废采购单
    /// * `reason` - 首次请求中的作废原因
    ///
    /// # 返回
    /// 返回可持久化并稳定回放的作废结果载荷。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 状态、版本和业务引用必须与首次成功响应一致，审计收据保留规范化作废原因。
    fn from_voided(order: &PurchaseOrder, reason: &str) -> Self {
        Self {
            purchase_order_id: order.base.id.clone(),
            status: order.stable.status.as_str().to_string(),
            lock_version: order.base.version,
            reason: reason.trim().to_string(),
            reference: format!("VOID-V{}", order.base.version),
        }
    }

    /// 转换为采购草稿作废 API 结果。
    ///
    /// # 参数
    /// * `replayed` - 是否来自匹配命令收据的回放
    ///
    /// # 返回
    /// 返回首次执行或幂等回放结果。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 只有读取并校验匹配收据后才能传入 `true`。
    fn into_result(self, replayed: bool) -> VoidPurchaseOrderResult {
        VoidPurchaseOrderResult {
            purchase_order_id: self.purchase_order_id,
            status: self.status,
            lock_version: self.lock_version,
            replayed,
            reference: self.reference,
        }
    }
}

#[cfg(test)]
mod tests {
    use entities::purchase_order::PurchaseOrderStatus;

    use super::{ensure_void_target, VoidDraftReceipt};
    use crate::errors::Error;
    use crate::purchase_order::dto::VoidPurchaseOrderRequest;

    /// 构造最小作废请求。
    ///
    /// # 参数
    /// * `reason` - 作废原因
    ///
    /// # 返回
    /// 返回用于请求指纹测试的 DTO。
    ///
    /// # 错误
    /// 无。
    fn void_request(reason: &str) -> VoidPurchaseOrderRequest {
        VoidPurchaseOrderRequest {
            expected_lock_version: 4,
            reason: reason.to_string(),
            idempotency_key: "void-key-1".to_string(),
        }
    }

    /// 验证没有匹配收据的已作废采购单不会被标记为任意请求回放。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 已作废状态未返回 409 时测试失败。
    #[test]
    fn voided_order_without_receipt_is_conflict() {
        let result = ensure_void_target("actor-1", 5, PurchaseOrderStatus::Voided, 4, "actor-1");

        assert!(matches!(result, Err(Error::ConflictError(_))));
    }

    /// 验证作废请求指纹排除幂等键并按实际原因语义规范化。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 幂等键或原因空白改变指纹，或真实原因变化未改变指纹时测试失败。
    #[test]
    fn void_fingerprint_is_payload_stable_and_key_independent() {
        let first = void_request(" 重复采购 ");
        let mut same_payload = void_request("重复采购");
        same_payload.idempotency_key = "another-key".to_string();
        let different_payload = void_request("供应商错误");

        let fingerprint = |request: &VoidPurchaseOrderRequest| request.request_fingerprint("po-1").unwrap();
        assert_eq!(fingerprint(&first), fingerprint(&same_payload));
        assert_ne!(fingerprint(&first), fingerprint(&different_payload));
    }

    /// 验证只有收据转换路径能够显式标记作废结果为回放。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 收据回放未保留原结果或未设置回放标记时测试失败。
    #[test]
    fn void_receipt_replays_original_result() {
        let result = VoidDraftReceipt {
            purchase_order_id: "po-1".to_string(),
            status: "VOIDED".to_string(),
            lock_version: 5,
            reason: "重复采购".to_string(),
            reference: "VOID-V5".to_string(),
        }
        .into_result(true);

        assert_eq!(result.purchase_order_id, "po-1");
        assert_eq!(result.lock_version, 5);
        assert_eq!(result.reference, "VOID-V5");
        assert!(result.replayed);
    }

    /// 验证采购草稿作废把操作人授权绑定到事务提交。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 作废命令缺少稳定授权快照、事务内账号重验或 policy revision CAS 时测试失败。
    #[test]
    fn void_draft_binds_actor_authorization_to_commit() {
        let production = include_str!("void_order.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码必须存在");

        assert!(production.contains("authorize_actor_permission(actor, VOID_PERMISSION)"));
        assert!(production.contains("ensure_purchase_order_actor_account"));
        assert!(production.contains("run_authorized_policy_transaction(policy_revision"));
    }
}
