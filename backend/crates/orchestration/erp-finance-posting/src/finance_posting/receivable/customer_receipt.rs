//! 客户回款单查询、创建、提交审批、撤回与过账编排。

use application_core::{AuditActor, CommandReceipt};
use async_trait::async_trait;
use erp_audit::AuditActorLogs;
use erp_core::common::time::Instant;
use erp_core::ids::CustomerReceiptId;
use erp_finance::entity::receivable::{CustomerReceipt, CustomerReceiptData};
use erp_finance::repository::ReceivableExt;
use erp_finance::service::command_receipt::FinanceCommandReceiptService;
use erp_finance::service::receivable::customer_receipt_commit::convert_allocations;
use erp_finance::service::receivable::mapping::ensure_expected_version;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::{Permission, receivable};
use erp_read_models::finance::receivable::ReceivableReadService;
use erp_workflow::service::approval::execution::idempotency::normalize_idempotency_key;
use erp_workflow::service::approval::execution::prepare_cancel;
use erp_workflow::service::document_registry::find_approval_binding;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::ReceivableProcess;
use super::adapter::{
    self, customer_receipt_adapter, customer_receipt_subject_ref, execute_customer_receipt_domain_action,
    require_frozen_binding, start_customer_receipt_approval,
};
use super::cancel_approval::{
    CustomerReceiptCancelPersistInput, build_customer_receipt_cancel_input, load_cancel_runtime,
    persist_customer_receipt_cancel,
};
use super::customer_receipt_command::{ReceiptReplayPort, recover_receipt_command, replay_receipt_command};
use super::customer_receipt_posting::{
    CommitTransactionRequest, persist_created_customer_receipt, prepare_customer_receipt_commit_candidate,
    run_commit_transaction,
};
pub use super::customer_receipt_posting::{
    cancel_customer_receipt_approval_apply, post_customer_receipt_apply,
    reject_customer_receipt_approval_apply,
};
use super::customer_receipt_submit::{authorized_receipt, persist_receipt_submit, submit_receipt};
use super::dto::{
    CancelCustomerReceiptApprovalRequest, CommitCustomerReceiptRequest, CreateCustomerReceiptRequest,
    CustomerReceiptView, SubmitCustomerReceiptRequest,
};
use crate::{Error, Result};

impl ReceivableProcess {
    // -----------------------------------------------------------------------
    // 客户回款单
    // -----------------------------------------------------------------------

    /// 登记客户回款草稿，并在同一事务绑定已发布审批定义。
    ///
    /// 回款单号全局唯一（`uk_customer_receipts_no` 唯一索引）构成幂等去重。
    /// 绑定失败必须回滚业务实体，不得把绑定推迟到提交。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建回款单视图。
    ///
    /// # 错误
    /// * `ConflictError` - 回款单号重复或流程未配置
    pub async fn create_customer_receipt(
        &self,
        req: CreateCustomerReceiptRequest,
        actor: &AuditActor,
    ) -> Result<CustomerReceiptView> {
        req.validate()?;
        let receipt = CustomerReceipt::new(
            CustomerReceiptId::new(next_id()),
            CustomerReceiptData {
                receipt_no: req.receipt_no,
                counterparty_party_id: req.counterparty_party_id,
                customer_id: req.customer_id,
                received_at: req.received_at,
                amount: req.amount,
                bank_reference: req.bank_reference,
            },
            actor.id(),
        )?;
        persist_created_customer_receipt(&self.db, &self.rbac, receipt, actor.clone()).await
    }

    /// 原子创建或提交客户回款并启动审批。
    ///
    /// 新回款的单据注册与定义绑定、回款实体、冻结核销分配、审批运行事实、
    /// 不可变快照、入口任务和审计全部位于同一事务。已有草稿用乐观锁校验后
    /// 走同一启动事务，前端不得再执行“先创建草稿、再提交”。
    ///
    /// # 参数
    /// * `req` - 新回款或已有草稿身份、冻结分配与幂等键
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回进入审批后的回款单视图。
    ///
    /// # 错误
    /// * `ValidationError` - 参数组合或分配不合法
    /// * `ConflictError` - 草稿版本、状态、绑定或审批定义冲突
    /// * `NotFound` - 已有草稿不存在
    pub async fn commit_customer_receipt(
        &self,
        req: CommitCustomerReceiptRequest,
        actor: &AuditActor,
    ) -> Result<CustomerReceiptView> {
        req.validate()?;
        let command_receipt = CommandReceipt::from_payload(
            "customer-receipt-commit-",
            actor.id(),
            "customer_receipt.commit",
            "customer_receipt",
            &req.idempotency_key,
            &req,
        )?;
        if let Some(view) = self.replay_committed_receipt(&command_receipt, actor).await? {
            return Ok(view);
        }
        let prepared = req.prepare()?;
        let pending_commit = prepare_customer_receipt_commit_candidate(prepared, actor.id())?;
        let allocations = pending_commit.allocations.clone();
        let adapter = customer_receipt_adapter()?;
        let transaction_result = run_commit_transaction(
            &self.db,
            &self.rbac,
            CommitTransactionRequest {
                resubmission_reason: req.resubmission_reason,
                pending: pending_commit,
                allocations,
                idempotency_key: req.idempotency_key,
                owner_role: adapter.owner_role,
                actor: actor.clone(),
                command_receipt: command_receipt.clone(),
            },
        )
        .await;

        match transaction_result {
            Ok(view) => Ok(view),
            Err(error) => self.recover_committed_receipt(&command_receipt, actor, error).await,
        }
    }

    /// 幂等重放已提交回款：同一新快照核对回执、当前提交资格并读取原结果。
    ///
    /// # 参数
    /// * `command_receipt` - 提交幂等收据
    ///
    /// # 返回
    /// 已提交时返回其视图，否则返回 `None`。
    async fn replay_committed_receipt(
        &self,
        command_receipt: &CommandReceipt,
        actor: &AuditActor,
    ) -> Result<Option<CustomerReceiptView>> {
        let db = self.db.clone();
        let authorization = self.rbac.authorization.clone();
        let actor = actor.clone();
        let command_receipt = command_receipt.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let port =
                        ReceiptCommitRecovery { db: &db, authorization: &authorization, actor: &actor };
                    replay_receipt_command(&port, &command_receipt, None, executor).await
                })
            })
            .await
    }

    /// 事务失败后以幂等收据有界回读已提交回款，避免重复提交产生两条事实。
    ///
    /// # 参数
    /// * `command_receipt` - 提交幂等收据
    /// * `error` - 事务原始错误
    ///
    /// # 返回
    /// 已提交时返回其视图，未提交时返回原始错误。
    async fn recover_committed_receipt(
        &self,
        command_receipt: &CommandReceipt,
        actor: &AuditActor,
        error: Error,
    ) -> Result<CustomerReceiptView> {
        let probe = self.replay_committed_receipt(command_receipt, actor).await;
        recover_receipt_command(probe, error)
    }

    /// 提交客户回款并调用统一 `start_approval`。
    ///
    /// 按合同 §4.4.1 冻结 `approval_subject_version` 与 `subject_snapshot`，
    /// 单据进入 `IN_APPROVAL`。定义与审批人取自已绑定事实，不接受客户端选择。
    ///
    /// # 参数
    /// * `id` - 回款单主键
    /// * `req` - 提交请求（版本、幂等键与冻结分配）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回提交后的回款单视图。
    ///
    /// # 错误
    /// * `NotFound` - 回款单不存在
    /// * `ConflictError` - 非草稿、无绑定或并发冲突
    pub async fn submit_customer_receipt(
        &self,
        id: &str,
        req: SubmitCustomerReceiptRequest,
        actor: &AuditActor,
    ) -> Result<CustomerReceiptView> {
        req.validate()?;
        let command = submit_receipt(id, &req, actor.id())?;
        let process = Self::new(self.db.clone(), self.rbac.clone());
        let request_actor = actor.clone();
        let request_id = id.to_owned();
        let request_command = command.clone();
        let result = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    if let Some(view) = process
                        .replay_receipt_submit(&request_id, &request_command, &request_actor, executor)
                        .await?
                    {
                        return Ok(view);
                    }
                    let adapter = customer_receipt_adapter()?;
                    let mut receipt =
                        authorized_receipt(&process.db, &process.rbac, &request_actor, &request_id, executor)
                            .await?;
                    ensure_expected_version(receipt.base.version, req.expected_version)?;
                    let allocations = convert_allocations(&req.allocations)?;
                    start_customer_receipt_approval(&mut receipt, allocations)?;
                    process
                        .dispatch_customer_receipt_start(
                            &request_id,
                            receipt,
                            &req,
                            &request_actor,
                            adapter,
                            request_command,
                            executor,
                        )
                        .await
                })
            })
            .await;
        match result {
            Err(error) if error.command_may_have_committed() => {
                self.recover_customer_receipt_submit(id, &command, actor, error).await
            },
            result => result,
        }
    }

    /// 撤回客户回款审批，成功后回到草稿且 `subject_version` 不回退。
    ///
    /// 作为合同 `cancel_action`，供业务撤回与管理员受阻取消共用。
    ///
    /// # 参数
    /// * `id` - 回款单主键
    /// * `req` - 撤回请求（原因必填）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回撤回后的回款单视图。
    ///
    /// # 错误
    /// * `NotFound` - 回款单不存在
    /// * `ConflictError` - 非审批中、已最终通过或并发冲突
    pub async fn cancel_customer_receipt_approval(
        &self,
        id: &str,
        req: CancelCustomerReceiptApprovalRequest,
        actor: &AuditActor,
    ) -> Result<CustomerReceiptView> {
        req.validate()?;
        let process = Self::new(self.db.clone(), self.rbac.clone());
        let actor = actor.clone();
        let id = id.to_owned();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    process
                        .rbac
                        .authorization
                        .require(
                            &actor,
                            &receivable::CUSTOMER_RECEIPT_CANCEL_APPROVAL,
                            AccessTarget::Operation,
                            executor,
                        )
                        .await?;
                    let mut receipt = process.load_customer_receipt(&id, executor).await?;
                    ensure_expected_version(receipt.base.version, req.expected_version)?;
                    process
                        .persist_cancelled_customer_receipt(&id, &mut receipt, &req, &actor, executor)
                        .await
                })
            })
            .await
    }

    /// 客户端直接过账失败关闭。最终动作只能由审批运行时调用。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 不返回成功值。
    ///
    /// # 错误
    /// 恒返回 `ConflictError`。
    pub fn reject_client_post() -> Result<CustomerReceiptView> {
        Err(Error::ConflictError("客户回款过账只能由审批最终通过动作执行，客户端不得直接过账".to_string()))
    }

    /// 从绑定读取定义并持久化启动事实。
    ///
    /// # 错误
    /// 无绑定、定义缺失或写入失败时返回错误。
    async fn dispatch_customer_receipt_start(
        &self,
        id: &str,
        receipt: CustomerReceipt,
        req: &SubmitCustomerReceiptRequest,
        actor: &AuditActor,
        adapter: adapter::CustomerReceiptAdapter,
        command: CommandReceipt,
        executor: &mut dyn Executor,
    ) -> Result<CustomerReceiptView> {
        let input = self.prepare_receipt_submit_input(id, receipt, req, actor, adapter, executor).await?;
        persist_receipt_submit(&self.db, &self.rbac, input, command, executor).await
    }

    /// 加载撤回运行事实并写回草稿。
    ///
    /// # 错误
    /// 无绑定、实例终态或写入失败时返回错误。
    async fn persist_cancelled_customer_receipt(
        &self,
        id: &str,
        receipt: &mut CustomerReceipt,
        req: &CancelCustomerReceiptApprovalRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<CustomerReceiptView> {
        let adapter = customer_receipt_adapter()?;
        let binding = find_approval_binding(&self.db, id, executor).await.map_err(crate::Error::from)?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        let subject = customer_receipt_subject_ref(id)?;
        let runtime =
            load_cancel_runtime(&self.db, &binding, &subject, receipt.approval_subject_version, executor)
                .await?;
        let now = Instant::now();
        let idempotency_key = normalize_idempotency_key(&req.idempotency_key)?;
        let input = build_customer_receipt_cancel_input(
            &runtime,
            &req.reason,
            actor.id(),
            &idempotency_key,
            None,
            now,
        )?;
        let prepared = prepare_cancel(input)?;
        execute_customer_receipt_domain_action(receipt, adapter.cancel_action)?;
        let audit = actor.clone().resource_log(
            "customer_receipt.cancel_approval",
            "customer_receipt",
            id.to_string(),
        )?;
        persist_customer_receipt_cancel(
            &self.db,
            &self.rbac,
            CustomerReceiptCancelPersistInput {
                receipt: receipt.clone(),
                prepared,
                open_tasks: runtime.open_tasks,
                actor: actor.clone(),
                reason: req.reason.clone(),
                now,
                audit,
            },
            executor,
        )
        .await
    }

    /// 按主键读取客户回款单。
    ///
    /// # 错误
    /// 不存在时返回 `NotFound`。
    async fn load_customer_receipt(&self, id: &str, executor: &mut dyn Executor) -> Result<CustomerReceipt> {
        self.db
            .customer_receipts()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("客户回款单不存在".to_string()))
    }

    /// 最终通过过账并核销（§8.3-1 事务不变量）。
    ///
    /// 作为合同 `on_final_approve`，仅 `IN_APPROVAL` 可进入过账。同一事务内：
    /// 校验回款与应收分录同一往来主体、分录开放余额与回款剩余余额；写提交时
    /// 冻结的核销分配（`APPLY`）；按条件原子更新子账已核销进度。
    ///
    /// # 参数
    /// * `id` - 回款单 ID
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回过账后回款单视图。
    ///
    /// # 错误
    /// * `NotFound` - 回款单或应收分录不存在
    /// * `ConflictError` - 非审批中
    /// * `BusinessLogicError` - 跨主体核销、超额核销或重复过账
    pub async fn post_customer_receipt(&self, id: &str, actor: &AuditActor) -> Result<CustomerReceiptView> {
        let db = self.db.clone();

        let client = db.client().clone();
        let actor_owned = actor.clone();
        let receipt_id = id.to_string();
        let authorization = self.rbac.authorization.clone();
        client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    authorization
                        .require(
                            &actor_owned,
                            &receivable::CUSTOMER_RECEIPT_POST,
                            AccessTarget::Operation,
                            executor,
                        )
                        .await?;
                    post_customer_receipt_apply(&db, &receipt_id, &actor_owned, executor).await?;
                    receipt_command_result(
                        &db,
                        &authorization,
                        &actor_owned,
                        &receipt_id,
                        &receivable::CUSTOMER_RECEIPT_POST,
                        AccessTarget::Operation,
                        executor,
                    )
                    .await
                })
            })
            .await
    }
}

/// 新建提交入口在查证快照按当前提交资格读取原结果。
pub(super) struct ReceiptCommitRecovery<'a> {
    pub(super) db: &'a Database,
    pub(super) authorization: &'a AuthorizationService,
    pub(super) actor: &'a AuditActor,
}

#[async_trait]
impl ReceiptReplayPort for ReceiptCommitRecovery<'_> {
    type Output = CustomerReceiptView;

    async fn committed_resource_id(
        &self,
        command: &CommandReceipt,
        executor: &mut dyn Executor,
    ) -> Result<Option<String>> {
        Ok(FinanceCommandReceiptService::new(self.db.clone())
            .committed_resource_id(command, executor)
            .await?)
    }

    /// 在查证回执的原快照中重验提交资格，并读取完整命令结果。
    ///
    /// # 参数
    /// * `id` - 回执确认的回款单 ID。
    /// * `executor` - 原回执查证事务执行器。
    ///
    /// # 返回
    /// 返回当前回款详情视图。
    ///
    /// # 错误
    /// 回款单不存在时返回 `NotFound`；详情读取失败时返回对应错误。
    async fn current_result(&self, id: &str, executor: &mut dyn Executor) -> Result<CustomerReceiptView> {
        receipt_command_result(
            self.db,
            self.authorization,
            self.actor,
            id,
            &receivable::CUSTOMER_RECEIPT_SUBMIT,
            AccessTarget::record(id)?,
            executor,
        )
        .await
    }
}

/// 写入或幂等查证完成后，在原快照重验本次命令资格并投影完整回包。
/// # 参数
/// 数据库、共享授权、当前账号、原单身份、命令权限和目标及原事务执行器。
/// # 返回
/// 通过当前命令资格后的完整回款视图，不追加详情菜单资格。
/// # 错误
/// 原命令资格失效、目标错配或视图读取失败时拒绝。
pub(super) async fn receipt_command_result(
    db: &Database,
    authorization: &AuthorizationService,
    actor: &AuditActor,
    id: &str,
    permission: &Permission,
    target: AccessTarget,
    executor: &mut dyn Executor,
) -> Result<CustomerReceiptView> {
    if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
        return Err(Error::ValidationError("回款命令结果必须使用原授权快照".into()));
    }
    authorization.require(actor, permission, target, executor).await?;
    Ok(ReceivableReadService::new(db.clone()).customer_receipt_view_with_executor(id, executor).await?)
}

#[cfg(test)]
mod result_tests {
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    /// 无原快照或命令资格取证失败时，不得继续读取完整资金视图。
    #[tokio::test]
    async fn command_results_require_original_snapshot_and_current_command_authorization() {
        let db = mongodb::Client::with_uri_str("mongodb://127.0.0.1:1")
            .await
            .unwrap()
            .database("receipt_command_results");
        let authorization = unavailable_authorization();
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        for (permission, target) in [
            (receivable::CUSTOMER_RECEIPT_CREATE, AccessTarget::Operation),
            (receivable::CUSTOMER_RECEIPT_CANCEL_APPROVAL, AccessTarget::Operation),
            (receivable::CUSTOMER_RECEIPT_POST, AccessTarget::Operation),
            (receivable::CUSTOMER_RECEIPT_SUBMIT, AccessTarget::record("receipt").unwrap()),
        ] {
            let no_snapshot = receipt_command_result(
                &db,
                &authorization,
                &actor,
                "receipt",
                &permission,
                target.clone(),
                &mut ReadExecutor::default(),
            )
            .await
            .unwrap_err();
            assert!(matches!(no_snapshot, Error::ValidationError(message)
                if message == "回款命令结果必须使用原授权快照"));
            let unavailable = receipt_command_result(
                &db,
                &authorization,
                &actor,
                "receipt",
                &permission,
                target,
                &mut SnapshotExecutor::default(),
            )
            .await
            .unwrap_err();
            assert!(matches!(unavailable, Error::Internal(message)
                if message == "测试授权设施不可用"));
        }
    }
}
