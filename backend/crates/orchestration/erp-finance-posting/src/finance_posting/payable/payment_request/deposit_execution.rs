//! 原指定出纳明确执行余额，按服务器冻结预算取得执行组专属资金保障。

use application_core::{AuditActor, CommandReceipt};
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_core::common::time::Instant;
use erp_finance::dto::payable::payment_request::ExecutePurchasePaymentRequestDeposit;
use erp_finance::entity::payable::{PaymentRequestExecutionMethod, PurchasePaymentRequest};
use erp_finance::entity::supplier_deposit::{
    DepositAuthorizationMode, DepositReservationBasis, ManualDepositAuthorizationData,
    SupplierDepositAuthorization, SupplierDepositReservation,
};
use erp_finance::repository::supplier_deposit_query::{
    automatic_authorizations, reservation_for_execution, reservations_for_manual_command,
};
use erp_finance::service::command_receipt::FinanceCommandReceiptService;
use erp_finance::service::supplier_deposit::SupplierDepositService;
use erp_party::SensitiveDataCodec;
use erp_supply::api_order::{
    ApiOrderExecution, ApiOrderStage, active_for_purchase, execution_budget as read_execution_budget,
};
use erp_supply::entity::execution_budget::ApprovedExecutionBudget;
use erp_supply::service::supplier_api::SupplierApiService;
use erp_workflow::WorkItemExt;
use erp_workflow::service::work_item::expected_task_version;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::{PurchasePaymentRequestProcess, ensure_version, execution, load, source};
use crate::finance_posting::task_access::{TaskPurpose, require_task};
use crate::{Error, Result};

fn execution_budget(
    execution: &ApiOrderExecution,
    codec: &SensitiveDataCodec,
) -> Result<ApprovedExecutionBudget> {
    let encrypted = execution
        .request_encrypted
        .as_ref()
        .ok_or_else(|| Error::BusinessLogicError("执行组尚未冻结批准预算".into()))?;
    Ok(read_execution_budget(execution, &codec.decrypt(encrypted)?)?)
}

const ACTION: &str = "purchase_payment_request.deposit.execute";

struct DepositExecutionWrite<'a> {
    db: &'a Database,
    process: &'a PurchasePaymentRequestProcess,
    request_id: &'a str,
    input: &'a ExecutePurchasePaymentRequestDeposit,
    actor: &'a AuditActor,
    codec: &'a SensitiveDataCodec,
    command: &'a CommandReceipt,
}

impl PurchasePaymentRequestProcess {
    /// 由当前指定出纳按原批准申请明确使用预存款，不生成银行付款。
    /// # 参数
    /// * `id` / `req` - 原申请、任务和所见版本及稳定操作键。
    /// * `actor` - 当前有付款权限的指定出纳。
    /// * `codec` - 读取服务器加密执行预算的共享加密器。
    /// # 返回
    /// 返回原命令取得的执行组占用；占用仍不计为已付。
    /// # 错误
    /// 非本人任务、来源、预算、账户、资金或批准范围不符合时整个事务拒绝。
    pub async fn execute_deposit(
        &self,
        id: &str,
        req: ExecutePurchasePaymentRequestDeposit,
        actor: &AuditActor,
        codec: &SensitiveDataCodec,
    ) -> Result<Vec<SupplierDepositReservation>> {
        let command = CommandReceipt::from_resource_parts(
            "payment-request-deposit-",
            actor.id(),
            ACTION,
            "purchase_payment_request",
            id,
            &req.idempotency_key,
            [req.work_item_id.to_string(), req.task_version.clone(), req.expected_version.to_string()],
        )?;

        let db = self.db.clone();
        let process = Self::new(db.clone(), self.rbac.clone());
        let id = id.to_owned();
        let actor = actor.clone();
        let codec = codec.clone();
        let recovery = (id.clone(), req.clone(), actor.clone(), command.clone(), codec.clone());
        let result = db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    DepositExecutionWrite {
                        db: &db,
                        process: &process,
                        request_id: &id,
                        input: &req,
                        actor: &actor,
                        codec: &codec,
                        command: &command,
                    }
                    .apply(executor)
                    .await
                })
            })
            .await;

        match result {
            Ok(value) => Ok(value),
            Err(original) => self.recover_deposit_execution(recovery, original).await,
        }
    }

    async fn recover_deposit_execution(
        &self,
        recovery: (
            String,
            ExecutePurchasePaymentRequestDeposit,
            AuditActor,
            CommandReceipt,
            SensitiveDataCodec,
        ),
        original: Error,
    ) -> Result<Vec<SupplierDepositReservation>> {
        let (id, req, actor, command, codec) = recovery;
        let db = self.db.clone();
        let process = Self::new(db.clone(), self.rbac.clone());
        let result = db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    DepositExecutionWrite {
                        db: &db,
                        process: &process,
                        request_id: &id,
                        input: &req,
                        actor: &actor,
                        codec: &codec,
                        command: &command,
                    }
                    .replay(executor)
                    .await
                })
            })
            .await;

        match result {
            Ok(Some(value)) => Ok(value),
            Ok(None) => Err(original),
            Err(_) if matches!(original, Error::OutcomeUnknown(_)) => Err(original),
            Err(error) => Err(error),
        }
    }
}

impl DepositExecutionWrite<'_> {
    async fn apply(&self, executor: &mut dyn Executor) -> Result<Vec<SupplierDepositReservation>> {
        if let Some(previous) = self.replay(executor).await? {
            return Ok(previous);
        }

        let request = load(self.db, self.request_id, executor).await?;
        ensure_version(&request, self.input.expected_version)?;
        let task_version = expected_task_version(&self.input.task_version)?;
        let mut task = execution::authorize_task(
            self.db,
            &self.process.rbac,
            &request,
            &self.input.work_item_id,
            task_version,
            self.actor,
            executor,
        )
        .await?;
        source::revalidate(self.db, &request, executor).await?;

        let PaymentRequestExecutionMethod::SupplierDeposit { account_id } = &request.data.execution_method
        else {
            return Err(Error::ValidationError("原申请未选择供应商预存款方式".into()));
        };
        request.require_deposit_account(account_id)?;
        let reservations = self.reserve_groups(&request, account_id, task_version, executor).await?;
        if reservations.is_empty() {
            return Err(Error::BusinessLogicError(
                "本申请没有已补齐履约资料并绑定该账户的供应商执行组".into(),
            ));
        }

        task.record_activity(Instant::now())?;
        self.db.work_items().update(&mut task, executor).await?;
        let audit = self.actor.clone().resource_log(
            ACTION,
            "purchase_payment_request",
            self.request_id.to_owned(),
        )?;
        persist_log(self.db, &audit, executor).await?;
        FinanceCommandReceiptService::new(self.db.clone())
            .save_resource(self.command, request.base.id, audit.base.id.clone(), executor)
            .await?;
        Ok(reservations)
    }

    async fn replay(&self, executor: &mut dyn Executor) -> Result<Option<Vec<SupplierDepositReservation>>> {
        let Some(id) = FinanceCommandReceiptService::new(self.db.clone())
            .committed_resource_id(self.command, executor)
            .await?
        else {
            return Ok(None);
        };
        if id != self.request_id {
            return Err(Error::ConflictError("原余额执行回执不属于本付款申请".into()));
        }

        let task = self
            .db
            .work_items()
            .find_by_id(self.input.work_item_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::NotFound("原余额执行任务不存在".into()))?;
        if !execution::matches_payment_result_task(&task, self.request_id) {
            return Err(Error::Forbidden("原余额执行任务不属于当前出纳".into()));
        }
        require_task(&self.process.rbac, self.actor, &task, TaskPurpose::Payment, executor).await?;

        let request = load(self.db, self.request_id, executor).await?;
        let PaymentRequestExecutionMethod::SupplierDeposit { account_id } = &request.data.execution_method
        else {
            return Err(Error::ConflictError("原余额执行缺少账户依据".into()));
        };

        let prefix = format!("{}:", self.command.id());
        let reservations =
            reservations_for_manual_command(self.db, account_id, self.request_id, &prefix, executor).await?;
        if reservations.is_empty() || reservations.iter().any(|row| row.actor_id != self.actor.id()) {
            return Err(Error::Internal("原余额执行回执缺少可核实的资金占用".into()));
        }
        Ok(Some(reservations))
    }

    async fn reserve_groups(
        &self,
        request: &PurchasePaymentRequest,
        account_id: &str,
        task_version: u64,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierDepositReservation>> {
        let mut result = Vec::new();
        for line in &request.lines {
            let groups = active_for_purchase(self.db, &line.purchase_order_id, executor).await?;
            if groups.len() > 1000 {
                return Err(Error::ValidationError("采购执行组过多，请分组核查后执行余额".into()));
            }
            for group in groups
                .into_iter()
                .filter(|group| group.purchase_order_revision_id == line.purchase_revision_id)
            {
                if let Some(reservation) =
                    self.reserve_group(request, account_id, task_version, &group, executor).await?
                {
                    result.push(reservation);
                    if result.len() > 1000 {
                        return Err(Error::ValidationError(
                            "本次余额执行组超过一千项，请按批准来源分组执行".into(),
                        ));
                    }
                }
            }
        }
        Ok(result)
    }

    async fn reserve_group(
        &self,
        request: &PurchasePaymentRequest,
        account_id: &str,
        task_version: u64,
        group: &ApiOrderExecution,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierDepositReservation>> {
        if !self.connection_matches(request, account_id, group, executor).await? {
            return Ok(None);
        }

        let budget = execution_budget(group, self.codec)?;
        let service = SupplierDepositService::new(self.db.clone());
        self.ensure_unfunded(request, account_id, group, &budget, executor).await?;
        let grant = self.execution_authorization(account_id, task_version, group, &budget, executor).await?;
        let basis = self.reservation_basis(request, group, &budget, grant)?;
        Ok(Some(
            service
                .reserve(
                    account_id,
                    &format!("{}:{}", self.command.id(), group.base.id),
                    basis,
                    self.actor.id(),
                    Instant::now(),
                    executor,
                )
                .await?,
        ))
    }

    async fn connection_matches(
        &self,
        request: &PurchasePaymentRequest,
        account_id: &str,
        group: &ApiOrderExecution,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let connection =
            SupplierApiService::new(self.db.clone()).load_connection(&group.connection_id, executor).await?;
        if connection.supplier_deposit_account_id.as_deref() != Some(account_id) {
            return Ok(false);
        }
        let snapshot = request.require_deposit_account(account_id)?;
        if group.supplier_id != request.supplier_id
            || connection.supplier_id.as_ref() != request.supplier_id
            || connection.supplier_deposit_external_account_ref.as_deref()
                != Some(snapshot.identity.external_account_ref.as_str())
        {
            return Err(Error::ConflictError("供应商执行连接与批准余额账户归属不一致".into()));
        }
        Ok(true)
    }

    async fn ensure_unfunded(
        &self,
        request: &PurchasePaymentRequest,
        account_id: &str,
        group: &ApiOrderExecution,
        budget: &ApprovedExecutionBudget,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if let Some(existing) = reservation_for_execution(self.db, &group.base.id, executor).await? {
            if existing.account_id != account_id
                || existing.basis.payment_request_id != request.base.id
                || existing.amount != budget.approved_total
            {
                return Err(Error::ConflictError("执行组已有其他资金依据，请核实原占用".into()));
            }
            return Err(Error::ConflictError("执行组已有资金占用，请查询原执行结果".into()));
        }
        if group.original_result_query_allowed() || group.stage == ApiOrderStage::Stopped {
            return Err(Error::BusinessLogicError("供应商执行已发送或停止，请先核实原资金结果".into()));
        }
        Ok(())
    }

    fn reservation_basis(
        &self,
        request: &PurchasePaymentRequest,
        group: &ApiOrderExecution,
        budget: &ApprovedExecutionBudget,
        grant: SupplierDepositAuthorization,
    ) -> Result<DepositReservationBasis> {
        let line = request
            .lines
            .iter()
            .find(|line| {
                line.purchase_order_id == group.purchase_order_id
                    && line.purchase_revision_id == group.purchase_order_revision_id
            })
            .ok_or_else(|| Error::ConflictError("执行组不属于原逐采购批准".into()))?;
        let basis = DepositReservationBasis {
            payment_request_id: request.base.id.clone(),
            payment_request_version: request.submission_version,
            payable_account_id: line.payable_account_id.clone(),
            purchase_order_id: group.purchase_order_id.clone(),
            purchase_revision_id: group.purchase_order_revision_id.clone(),
            execution_group_id: group.base.id.clone(),
            approved_goods_amount: budget.approved_goods,
            approved_freight: budget.approved_freight,
            authorization_id: grant.base.id,
            authorization_version: grant.grant_version,
        };
        if basis.total()? != budget.approved_total {
            return Err(Error::ConflictError("执行组商品及运费预算不守恒".into()));
        }
        Ok(basis)
    }

    async fn execution_authorization(
        &self,
        account_id: &str,
        task_version: u64,
        group: &ApiOrderExecution,
        budget: &ApprovedExecutionBudget,
        executor: &mut dyn Executor,
    ) -> Result<SupplierDepositAuthorization> {
        let now = Instant::now();
        let grants = automatic_authorizations(self.db, account_id, now, executor).await?;
        for grant in grants {
            let mut candidate = grant.clone();
            if grant.mode == DepositAuthorizationMode::Automatic
                && candidate
                    .commit(
                        account_id,
                        &group.purchase_order_id,
                        budget.approved_total,
                        grant.grant_version,
                        now,
                    )
                    .is_ok()
            {
                return Ok(grant);
            }
        }

        let grant = SupplierDepositAuthorization::manual(
            format!("manual-{}-{}", self.command.id(), group.base.id),
            ManualDepositAuthorizationData {
                account_id: account_id.into(),
                purchase_order_id: group.purchase_order_id.clone(),
                work_item_id: self.input.work_item_id.to_string(),
                task_version,
                actor_id: self.actor.id().into(),
                approval_reference: "原批准采购付款申请的本次出纳余额执行".into(),
                amount: budget.approved_total,
                now,
            },
        )?;
        Ok(SupplierDepositService::new(self.db.clone()).authorize(grant, executor).await?)
    }
}
