//! 补充资料先在授权快照取证，外部地址验证后在写事务重新授权。
use std::sync::Arc;

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_core::common::time::Instant;
use erp_permissions::supplier_fulfillment;
use erp_supply::api_order::{ApiOrderExecution, ApiOrderExt, ApiOrderSetupReceipt, ApiOrderStage};
use erp_supply::ports::connector::SupplierConnector;
use erp_supply::ports::connector::address::AddressRequirement;
use erp_supply::ports::connector::order::CoordinateSystem;
use persistence_core::Transactional;

use super::{
    ExecutionSetupRequest, ExecutionSetupView, SupplierApiOrderProcess, apply_setup, validate_request,
};
use crate::{Error, Result};

enum SetupStart {
    Replayed(Box<ExecutionSetupView>),
    Prepared(Box<ApiOrderExecution>, Arc<dyn SupplierConnector>),
}

impl SupplierApiOrderProcess {
    /// 保存经采购确认的配送资料，后台继续执行。
    ///
    /// id 为原订单；request 携带预期执行版本及操作号；actor 为当前操作人。
    ///
    /// 原命令资格下的完整结果；同载荷回放不执行外部验证或重复写入。
    ///
    /// 当前资格、旧版本、外部地址、旧选项或异载荷重试无效时拒绝。
    pub async fn save_execution_setup(
        &self,
        id: &str,
        request: ExecutionSetupRequest,
        actor: &AuditActor,
    ) -> Result<ExecutionSetupView> {
        validate_request(&request)?;
        let (hash, receipt) = request.receipt(id)?;

        let (mut execution, connector) = match self.start_setup(id, &hash, &receipt, actor).await? {
            SetupStart::Replayed(view) => return Ok(*view),
            SetupStart::Prepared(execution, connector) => (*execution, connector),
        };
        if execution.base.version != request.expected_version
            || execution.stage != ApiOrderStage::AwaitingSetup
        {
            return Err(Error::ConflictError(
                "供应商订单的准备资料已更新，请刷新并核对收货和配送资料".into(),
            ));
        }

        self.apply_setup_request(&mut execution, &request, connector.as_ref()).await?;
        execution.setup_idempotency_key = Some(request.idempotency_key);
        execution.setup_payload_hash = Some(hash);
        execution.blocker = None;
        execution.next_run_at = Instant::now();

        self.persist_setup(execution, receipt, actor).await?;
        self.setup_result(id, actor, &supplier_fulfillment::ORDER_SUBMIT).await
    }

    async fn start_setup(
        &self,
        id: &str,
        hash: &str,
        receipt: &ApiOrderSetupReceipt,
        actor: &AuditActor,
    ) -> Result<SetupStart> {
        let (process, id, hash, receipt_id, actor) =
            (self.clone(), id.to_owned(), hash.to_owned(), receipt.base.id.clone(), actor.clone());
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let (access, execution) = process
                        .setup_access(&id, &actor, &supplier_fulfillment::ORDER_SUBMIT, executor)
                        .await?;

                    if let Some(previous) = process
                        .db
                        .supplier_api_order_setup_receipts()
                        .find_by_id(&receipt_id, executor)
                        .await?
                    {
                        previous.ensure_replay(&id, &hash)?;
                        let view = process.setup_projection(&execution, &actor, executor).await?;
                        access.validate(executor).await?;
                        return Ok(SetupStart::Replayed(Box::new(view)));
                    }

                    let connector = process.setup_connector(&execution, executor).await?;
                    access.validate(executor).await?;
                    Ok(SetupStart::Prepared(Box::new(execution), connector))
                })
            })
            .await
    }

    async fn apply_setup_request(
        &self,
        execution: &mut ApiOrderExecution,
        request: &ExecutionSetupRequest,
        connector: &dyn SupplierConnector,
    ) -> Result<()> {
        let requirement = connector.address_requirement();
        if request.geocode.is_some() && requirement != AddressRequirement::Geocoded(CoordinateSystem::Bd09) {
            return Err(Error::ValidationError("当前供应商不使用百度坐标，请按其收货地址要求补齐".into()));
        }

        let mut snapshot = self.decode(execution)?;
        apply_setup(execution, &mut snapshot, request)?;
        if let Some(ticket) = &request.supplier_address_ticket {
            self.apply_supplier_address(execution, &mut snapshot, ticket, connector).await?;
        }

        if execution.stage == ApiOrderStage::PrepareAddress {
            snapshot.recipient.ensure_address_ready(
                requirement,
                &execution.connection_id,
                &execution.base.id,
                execution.address_fingerprint.as_deref().unwrap_or_default(),
            )?;
        }
        self.encode(execution, &snapshot)
    }

    async fn persist_setup(
        &self,
        mut execution: ApiOrderExecution,
        receipt: ApiOrderSetupReceipt,
        actor: &AuditActor,
    ) -> Result<()> {
        let (process, actor) = (self.clone(), actor.clone());
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    process
                        .authorized_setup(
                            &execution.base.id,
                            &actor,
                            &supplier_fulfillment::ORDER_SUBMIT,
                            executor,
                        )
                        .await?;

                    process.db.supplier_api_order_executions().update(&mut execution, executor).await?;
                    process.db.supplier_api_order_setup_receipts().create(&receipt, executor).await?;

                    let audit = actor.resource_log(
                        "supplier_fulfillment.execution_setup",
                        "supplier_fulfillment_order",
                        execution.base.id.clone(),
                    )?;
                    persist_log(&process.db, &audit, executor).await?;
                    Ok::<_, Error>(())
                })
            })
            .await
    }
}
