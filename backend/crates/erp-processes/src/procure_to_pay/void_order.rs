//! 采购草稿作废与采购覆盖释放。

use application_core::AuditActor;
use async_trait::async_trait;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_procurement::dto::purchase_order::{VOID_ACTION, VoidPurchaseOrderRequest, VoidPurchaseOrderResult};
use erp_procurement::entity::purchase_order::{
    LegacyReceiptIdScheme, PurchaseCommandReceipt, PurchaseCommandReceiptError, PurchaseOrder,
    PurchaseOrderStatus,
};
use erp_procurement::service::purchase_order::void_order::{
    ensure_current_submission_is_draft, ensure_void_target, load_purchase_order, persist_voided_order,
};
use erp_sales::repository::SalesOrderExt;
use persistence_core::{Executor, NoTransaction};
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::PurchaseOrderProcess;
use super::authorization::{PurchaseOrderAuthorization, ensure_purchase_order_actor_account};
use super::procurement_task_sync::sync_procurement_tasks_for_sales_order;
use crate::procure_to_pay::adapters::audit::audit_receipt_fact;
use crate::{Error, Result};

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

impl PurchaseOrderProcess {
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
/// 写事务必须重验采购对象范围，历史参与不授予作废。
async fn execute_void_draft_transaction(
    service: &PurchaseOrderProcess,
    purchase_order_id: &str,
    request: VoidPurchaseOrderRequest,
    receipt_id: String,
    fingerprint: String,
    actor: &AuditActor,
    authorization: PurchaseOrderAuthorization,
) -> Result<VoidPurchaseOrderResult> {
    let db = service.db.clone();
    let PurchaseOrderAuthorization { rbac, policy_revision } = authorization;
    let object_scope = service.command_access(actor, "delete")?;
    let transaction_order_id = purchase_order_id.to_string();
    let transaction_actor = actor.clone();
    let transaction_receipt_id = receipt_id.clone();
    let transaction_fingerprint = fingerprint.clone();
    let transaction_result = rbac
        .run_authorized_policy_transaction(policy_revision, move |executor| {
            Box::pin(async move {
                ensure_purchase_order_actor_account(&db, &transaction_actor, executor).await?;
                object_scope
                    .revalidate(&transaction_order_id, request.expected_lock_version, executor)
                    .await?;
                let command = VoidDraftCommand {
                    purchase_order_id: &transaction_order_id,
                    request: &request,
                    receipt_id: &transaction_receipt_id,
                    request_fingerprint: &transaction_fingerprint,
                    actor: &transaction_actor,
                };
                void_draft_apply(&db, &command, executor).await
            })
        })
        .await;
    recover_void_draft(transaction_result, &service.db, &receipt_id, &fingerprint, purchase_order_id, actor)
        .await
}

/// 在 MongoDB 事务内校验并作废采购草稿。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `command` - 作废请求、收据身份和操作人上下文
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回首次作废结果或事务内命中的原收据结果。
///
/// # 错误
/// 目标、版本、状态、草稿提交、销售 guard 或持久化失败时返回错误。
///
/// # 关键业务约束
/// 事务内先查收据；收据未命中时，`Voided` 状态只能返回冲突，不能返回回放。
async fn void_draft_apply(
    db: &mongodb::Database,
    command: &VoidDraftCommand<'_>,
    executor: &mut dyn Executor,
) -> Result<VoidPurchaseOrderResult> {
    if let Some(result) = replay_void_draft(
        db,
        command.receipt_id,
        command.request_fingerprint,
        command.purchase_order_id,
        command.actor,
        executor,
    )
    .await?
    {
        return Ok(result);
    }
    let mut order = load_purchase_order(db, command.purchase_order_id, executor).await?;
    ensure_void_target(
        &order.stable.created_by,
        order.base.version,
        order.stable.status,
        command.request.expected_lock_version,
        command.actor.id(),
    )?;
    ensure_current_submission_is_draft(db, &order, executor).await?;
    void_order_and_persist(db, &mut order, command, executor).await
}

/// 推进来源销售 guard、作废采购单并持久化命令收据。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `order` - 已完成目标和草稿提交校验的采购单
/// * `command` - 作废请求、收据身份和操作人上下文
/// * `executor` - 数据访问执行器
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
    executor: &mut dyn Executor,
) -> Result<VoidPurchaseOrderResult> {
    execute_void_steps(&mut VoidPosting { db, order, command }, executor).await
}

/// 原作废写段在命令收据写入前的三个副作用边界。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VoidStep {
    SalesGuard,
    Order,
    Tasks,
}

#[async_trait]
trait VoidSteps: Send {
    /// 用原执行器执行一个写入步骤，保持首个错误。
    async fn apply(&mut self, step: VoidStep, executor: &mut dyn Executor) -> Result<()>;
    /// 在先前写入均成功后构造并持久化原响应收据。
    async fn receipt(&mut self, executor: &mut dyn Executor) -> Result<VoidPurchaseOrderResult>;
}

/// 生产作废写序；收据必须在销售 guard、采购版本和任务同步全部成功后写入。
async fn execute_void_steps(
    steps: &mut impl VoidSteps,
    executor: &mut dyn Executor,
) -> Result<VoidPurchaseOrderResult> {
    for step in [VoidStep::SalesGuard, VoidStep::Order, VoidStep::Tasks] {
        steps.apply(step, executor).await?;
    }
    steps.receipt(executor).await
}

struct VoidPosting<'a, 'command> {
    db: &'a mongodb::Database,
    order: &'a mut PurchaseOrder,
    command: &'a VoidDraftCommand<'command>,
}

#[async_trait]
impl VoidSteps for VoidPosting<'_, '_> {
    async fn apply(&mut self, step: VoidStep, executor: &mut dyn Executor) -> Result<()> {
        match step {
            VoidStep::SalesGuard => {
                let mut sales_order = self
                    .db
                    .sales_orders()
                    .find_by_id(self.order.sales_order_id.as_ref(), executor)
                    .await?
                    .ok_or_else(|| Error::NotFound("来源销售单不存在".to_string()))?;
                sales_order.advance_procurement_guard(self.command.actor.id())?;
                self.db.sales_orders().update(&mut sales_order, executor).await?;
            },
            VoidStep::Order => {
                persist_voided_order(self.db, self.order, self.command.actor.id(), executor).await?;
            },
            VoidStep::Tasks => {
                sync_procurement_tasks_for_sales_order(self.db, &self.order.sales_order_id, executor).await?;
            },
        }
        Ok(())
    }

    async fn receipt(&mut self, executor: &mut dyn Executor) -> Result<VoidPurchaseOrderResult> {
        let receipt = VoidDraftReceipt::from_voided(self.order, &self.command.request.reason);
        let audit = self.command.actor.clone().resource_log_with_id(
            self.command.receipt_id.to_string(),
            VOID_ACTION,
            "purchase_order",
            self.order.base.id.clone(),
            Some(
                PurchaseCommandReceipt::new(self.command.request_fingerprint.to_string(), receipt.clone())
                    .encode_message()?,
            ),
        )?;
        self.db.audit_logs().create(&audit, executor).await?;
        Ok(receipt.into_result(false))
    }
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
        &audit_receipt_fact(&audit),
        actor.id(),
        VOID_ACTION,
        Some(purchase_order_id),
        expected_fingerprint,
    ) {
        Ok(receipt) => receipt,
        Err(PurchaseCommandReceiptError::IdentityMismatch | PurchaseCommandReceiptError::PayloadConflict) => {
            return Err(Error::ConflictError("幂等键已用于不同采购命令".to_string()));
        },
        Err(PurchaseCommandReceiptError::Corrupted(message)) => {
            return Err(Error::Internal(message));
        },
    };
    if receipt.payload().purchase_order_id != purchase_order_id {
        return Err(Error::ConflictError("采购草稿作废收据与业务资源不一致".to_string()));
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
        Err(error) => {
            replay_void_draft(db, receipt_id, fingerprint, purchase_order_id, actor, &mut NoTransaction)
                .await?
                .ok_or(error)
        },
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
    use erp_procurement::dto::purchase_order::VoidPurchaseOrderRequest;

    use super::VoidDraftReceipt;

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
        let production =
            include_str!("void_order.rs").split("#[cfg(test)]").next().expect("生产代码必须存在");

        assert!(production.contains("authorize_actor_permission(actor, VOID_PERMISSION)"));
        assert!(production.contains("ensure_purchase_order_actor_account"));
        assert!(production.contains("run_authorized_policy_transaction(policy_revision"));
    }
    /// 非零大小执行器用于验证每个生产步骤实际传递的调用方实例。
    struct TestExecutor {
        _identity: u8,
    }

    impl persistence_core::Executor for TestExecutor {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            None
        }
    }

    struct RecordingSteps {
        expected_executor: usize,
        seen: Vec<usize>,
        fail_at: Option<usize>,
    }

    impl RecordingSteps {
        fn visit(&mut self, step: usize, executor: &mut dyn persistence_core::Executor) -> crate::Result<()> {
            assert_eq!(
                executor as *mut dyn persistence_core::Executor as *mut () as usize,
                self.expected_executor
            );
            self.seen.push(step);
            if self.fail_at == Some(step) {
                return Err(crate::Error::ConflictError("original-step-error".into()));
            }
            Ok(())
        }
    }

    #[async_trait::async_trait]
    impl super::VoidSteps for RecordingSteps {
        async fn apply(
            &mut self,
            step: super::VoidStep,
            executor: &mut dyn persistence_core::Executor,
        ) -> crate::Result<()> {
            self.visit(
                match step {
                    super::VoidStep::SalesGuard => 0,
                    super::VoidStep::Order => 1,
                    super::VoidStep::Tasks => 2,
                },
                executor,
            )
        }

        async fn receipt(
            &mut self,
            executor: &mut dyn persistence_core::Executor,
        ) -> crate::Result<super::VoidPurchaseOrderResult> {
            self.visit(3, executor)?;
            Ok(super::VoidDraftReceipt {
                purchase_order_id: "po-1".into(),
                status: "VOIDED".into(),
                lock_version: 5,
                reason: "重复采购".into(),
                reference: "VOID-V5".into(),
            }
            .into_result(false))
        }
    }

    /// 销售 guard、采购 CAS、任务同步和命令审计按原顺序共用执行器。
    #[tokio::test]
    async fn void_posting_keeps_original_order_and_executor() {
        let mut executor = TestExecutor { _identity: 1 };
        let mut steps = RecordingSteps {
            expected_executor: &mut executor as *mut TestExecutor as usize,
            seen: vec![],
            fail_at: None,
        };
        let result = super::execute_void_steps(&mut steps, &mut executor).await.unwrap();
        assert_eq!(steps.seen, [0, 1, 2, 3]);
        assert_eq!(result.lock_version, 5);
        assert_eq!(result.reference, "VOID-V5");
        assert!(!result.replayed);
    }

    /// 销售 guard、采购 CAS、任务同步或回执任一步失败均保留原错并停止。
    #[tokio::test]
    async fn void_posting_stops_at_each_failure() {
        for index in 0..4 {
            let mut executor = TestExecutor { _identity: 1 };
            let mut steps = RecordingSteps {
                expected_executor: &mut executor as *mut TestExecutor as usize,
                seen: vec![],
                fail_at: Some(index),
            };
            let error = super::execute_void_steps(&mut steps, &mut executor).await.unwrap_err();
            assert!(
                matches!(error, crate::Error::ConflictError(message) if message == "original-step-error")
            );
            assert_eq!(steps.seen, (0..=index).collect::<Vec<_>>());
        }
    }
}
