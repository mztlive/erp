//! 客户回款单查询、创建、提交审批、撤回与过账编排。

use application_core::{AuditActor, CommandReceipt};
use erp_audit::{AuditActorLogs, CommandReceiptServiceExt as _};
use erp_core::common::time::Instant;
use erp_core::ids::CustomerReceiptId;
use erp_finance::entity::receivable::{CustomerReceipt, CustomerReceiptData};
use erp_finance::repository::ReceivableExt;
use erp_finance::service::receivable::mapping::ensure_expected_version;
use erp_workflow::service::approval::execution::idempotency::normalize_idempotency_key;
use erp_workflow::service::approval::execution::{command_recovery_delay, prepare_cancel};
use erp_workflow::service::document_registry::find_approval_binding;
use id_generator::next_id;
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use super::ReceivableProcess;
use super::adapter::{
    self, RECENT_HISTORY_LIMIT, build_customer_receipt_snapshot, customer_receipt_adapter,
    customer_receipt_object_readable, customer_receipt_responsible_org_id, customer_receipt_start_command,
    customer_receipt_subject_ref, execute_customer_receipt_domain_action, require_frozen_binding,
    start_approval_command_kind, start_customer_receipt_approval,
};
use super::cancel_approval::{
    CustomerReceiptCancelPersistInput, build_customer_receipt_cancel_input, load_cancel_runtime,
    persist_customer_receipt_cancel,
};
use super::customer_receipt_posting::{
    CommitTransactionRequest, persist_created_customer_receipt, prepare_customer_receipt_commit_candidate,
    prepare_dispatch_start, run_commit_transaction,
};
pub use super::customer_receipt_posting::{
    cancel_customer_receipt_approval_apply, post_customer_receipt_apply,
};
use super::dto::{
    CancelCustomerReceiptApprovalRequest, CommitCustomerReceiptRequest, CreateCustomerReceiptRequest,
    CustomerReceiptView, SubmitCustomerReceiptRequest,
};
use super::start_approval::{
    CustomerReceiptStartPersistInput, persist_customer_receipt_start,
    replay_customer_receipt_start_with_executor,
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
        persist_created_customer_receipt(
            &self.db,
            &self.rbac,
            std::sync::Arc::clone(&self.object_read),
            receipt.clone(),
            actor.clone(),
        )
        .await?;
        self.read.customer_receipt_detail(&receipt.base.id).await.map_err(crate::Error::from)
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
        if let Some(view) = self.replay_committed_receipt(&command_receipt).await? {
            return Ok(view);
        }
        let prepared = req.prepare()?;
        let pending_commit = prepare_customer_receipt_commit_candidate(prepared, actor.id())?;
        let allocations = pending_commit.allocations.clone();
        let adapter = customer_receipt_adapter()?;
        let transaction_result = run_commit_transaction(
            &self.db,
            &self.rbac,
            std::sync::Arc::clone(&self.object_read),
            CommitTransactionRequest {
                pending: pending_commit,
                allocations,
                idempotency_key: req.idempotency_key,
                owner_role: adapter.owner_role,
                actor: actor.clone(),
                command_receipt: command_receipt.clone(),
            },
        )
        .await;

        let committed = match transaction_result {
            Ok(committed) => committed,
            Err(error) => return self.recover_committed_receipt(&command_receipt, error).await,
        };

        self.read.customer_receipt_detail(&committed.base.id).await.map_err(crate::Error::from)
    }

    /// 幂等重放已提交回款：收据已落盘时直接返回其视图，不再进入事务。
    ///
    /// # 参数
    /// * `command_receipt` - 提交幂等收据
    ///
    /// # 返回
    /// 已提交时返回其视图，否则返回 `None`。
    async fn replay_committed_receipt(
        &self,
        command_receipt: &CommandReceipt,
    ) -> Result<Option<CustomerReceiptView>> {
        match command_receipt.committed_resource_id(&self.db).await? {
            Some(receipt_id) => {
                Ok(Some(self.read.customer_receipt_detail(&receipt_id).await.map_err(crate::Error::from)?))
            },
            None => Ok(None),
        }
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
        error: Error,
    ) -> Result<CustomerReceiptView> {
        match command_receipt.committed_resource_id(&self.db).await? {
            Some(receipt_id) => {
                self.read.customer_receipt_detail(&receipt_id).await.map_err(crate::Error::from)
            },
            None => Err(error),
        }
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
        let adapter = customer_receipt_adapter()?;
        let mut receipt = self.load_customer_receipt(id).await?;
        ensure_expected_version(receipt.base.version, req.expected_version)?;
        let allocations =
            erp_finance::service::receivable::customer_receipt_commit::convert_allocations(&req.allocations)?;
        start_customer_receipt_approval(&mut receipt, allocations)?;
        self.dispatch_customer_receipt_start(id, receipt, req.idempotency_key, actor, adapter).await
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
        let mut receipt = self.load_customer_receipt(id).await?;
        ensure_expected_version(receipt.base.version, req.expected_version)?;
        self.persist_cancelled_customer_receipt(id, &mut receipt, &req, actor).await?;
        self.read.customer_receipt_detail(id).await.map_err(crate::Error::from)
    }

    /// 客户端直接过账失败关闭。最终动作只能由审批运行时调用。
    ///
    /// # 返回
    /// 恒返回冲突。
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
        idempotency_key: String,
        actor: &AuditActor,
        adapter: adapter::CustomerReceiptAdapter,
    ) -> Result<CustomerReceiptView> {
        let subject = customer_receipt_subject_ref(id)?;
        let binding =
            find_approval_binding(&self.db, id, &mut NoTransaction).await.map_err(crate::Error::from)?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        let now = Instant::now();
        let snapshot = build_customer_receipt_snapshot(&receipt, actor.id(), now)?;
        let start = customer_receipt_start_command(
            id,
            receipt.approval_subject_version,
            actor.id(),
            &idempotency_key,
        );
        let _ = (start_approval_command_kind(&start), RECENT_HISTORY_LIMIT);
        let organization_id = customer_receipt_responsible_org_id(&receipt)?;
        let _ = customer_receipt_object_readable(&organization_id, actor.id())?;
        let prepared = prepare_dispatch_start(
            &self.db,
            &binding,
            &subject,
            receipt.approval_subject_version,
            &organization_id,
            actor.id(),
            &idempotency_key,
            now,
        )
        .await?;
        let recovery_subject_version = receipt.approval_subject_version;
        let persisted = persist_customer_receipt_start(
            &self.db,
            CustomerReceiptStartPersistInput {
                receipt,
                actor: actor.clone(),
                id: id.to_string(),
                snapshot_payload: snapshot,
                prepared,
                owner_role: adapter.owner_role,
                organization_id,
                now,
            },
        )
        .await;
        if let Err(error) = persisted {
            if !error.command_may_have_committed() {
                return Err(error);
            }
            self.recover_customer_receipt_start(id, recovery_subject_version, &idempotency_key, actor, error)
                .await?;
        }
        self.read.customer_receipt_detail(id).await.map_err(crate::Error::from)
    }

    /// receipt 唯一竞争、瞬态事务或提交结果未知后，以 fresh session 有界回读。
    async fn recover_customer_receipt_start(
        &self,
        receipt_id: &str,
        subject_version: u32,
        idempotency_key: &str,
        actor: &AuditActor,
        original_error: Error,
    ) -> Result<String> {
        const RECOVERY_ATTEMPTS: usize = 8;
        for attempt in 0..RECOVERY_ATTEMPTS {
            let db = self.db.clone();
            let receipt_id = receipt_id.to_string();
            let idempotency_key = idempotency_key.to_string();
            let actor_id = actor.id().to_string();
            let recovered = self
                .db
                .client()
                .with_transaction(move |executor| {
                    Box::pin(async move {
                        let receipt = db
                            .customer_receipts()
                            .find_by_id(&receipt_id, executor)
                            .await?
                            .ok_or_else(|| Error::NotFound("客户回款单不存在".to_string()))?;
                        let organization_id = customer_receipt_responsible_org_id(&receipt)?;
                        let _ = customer_receipt_object_readable(&organization_id, &actor_id)?;
                        let binding = find_approval_binding(&db, &receipt_id, executor)
                            .await
                            .map_err(crate::Error::from)?;
                        let binding = require_frozen_binding(binding.as_ref())?;
                        let subject = customer_receipt_subject_ref(&receipt_id)?;
                        replay_customer_receipt_start_with_executor(
                            &db,
                            &subject,
                            subject_version,
                            &idempotency_key,
                            binding,
                            &actor_id,
                            executor,
                        )
                        .await
                    })
                })
                .await;
            match recovered {
                Ok(Some(instance_id)) => return Ok(instance_id),
                Ok(None) => {},
                Err(error) if error.command_may_have_committed() => {},
                Err(error) => return Err(error),
            }
            if attempt + 1 < RECOVERY_ATTEMPTS {
                tokio::time::sleep(command_recovery_delay(attempt)).await;
            }
        }
        Err(original_error)
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
    ) -> Result<()> {
        let adapter = customer_receipt_adapter()?;
        let binding =
            find_approval_binding(&self.db, id, &mut NoTransaction).await.map_err(crate::Error::from)?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        let subject = customer_receipt_subject_ref(id)?;
        let runtime =
            load_cancel_runtime(&self.db, &binding, &subject, receipt.approval_subject_version).await?;
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
            CustomerReceiptCancelPersistInput {
                receipt: receipt.clone(),
                prepared,
                open_tasks: runtime.open_tasks,
                actor_id: actor.id().to_string(),
                reason: req.reason.clone(),
                now,
                audit,
            },
        )
        .await
    }

    /// 按主键读取客户回款单。
    ///
    /// # 错误
    /// 不存在时返回 `NotFound`。
    async fn load_customer_receipt(&self, id: &str) -> Result<CustomerReceipt> {
        self.db
            .customer_receipts()
            .find_by_id(id, &mut NoTransaction)
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
        let _object_read = std::sync::Arc::clone(&self.object_read);
        let client = db.client().clone();
        let actor_owned = actor.clone();
        let receipt_id = id.to_string();
        let detail_id = receipt_id.clone();
        client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    post_customer_receipt_apply(&db, &receipt_id, &actor_owned, executor).await
                })
            })
            .await?;

        self.read.customer_receipt_detail(&detail_id).await.map_err(crate::Error::from)
    }
}
