//! 预检成功后的自动资金准备，只使用原批准和真实自动授权。

use application_core::AuditActor;
use erp_core::AccountKind;
use erp_core::common::time::Instant;
use erp_core::money::Amount;
use erp_finance::entity::supplier_deposit::{DepositAutomaticReservationData, SupplierDepositAccount};
use erp_finance::repository::SupplierDepositExt;
use erp_finance::repository::supplier_deposit_automatic::automatic_payment_requests;
use erp_finance::repository::supplier_deposit_query::{automatic_authorizations, reservation_for_execution};
use erp_finance::service::supplier_deposit::SupplierDepositService;
use erp_fulfillment_execution::fulfillment_execution::ensure_po_fulfillable;
use erp_party::SensitiveDataCodec;
use erp_procurement::repository::PurchaseOrderExt;
use erp_supply::api_order::{ApiOrderExecution, ApiOrderExt, ApiOrderStage};
use erp_supply::entity::supplier_api::SupplierApiConnection;
use erp_supply::repository::SupplierApiExt;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::access::audit;
use crate::supplier_api_orders::execution_budget;
use crate::{Error, Result};

const SYSTEM_ACTOR: &str = "system:supplier-deposit-worker";

struct AutomaticPreparation {
    db: Database,
    expected: ApiOrderExecution,
    codec: SensitiveDataCodec,
}

/// 为已通过采购预检的创建步骤取得本执行组全额资金保障。
/// # 参数
/// * `db` / `execution` - 服务器数据源及已预检的冻结执行组。
/// * `codec` - 读取原批准预算的共享解码器。
/// # 返回
/// 原占用有效或本次按唯一批准及自动授权占用成功时返回成功。
/// # 错误
/// 采购或预算变化、批准不唯一、自动授权缺失或资金不足时阻断，不冒用出纳。
pub async fn prepare_automatic_execution_funds(
    db: &Database,
    execution: &ApiOrderExecution,
    codec: &SensitiveDataCodec,
) -> Result<()> {
    let context = AutomaticPreparation { db: db.clone(), expected: execution.clone(), codec: codec.clone() };
    db.client()
        .clone()
        .with_transaction(move |executor| Box::pin(async move { context.apply(executor).await }))
        .await
}

impl AutomaticPreparation {
    async fn apply(&self, executor: &mut dyn Executor) -> Result<()> {
        let execution = self.execution(executor).await?;
        let budget = execution_budget(&execution, &self.codec)?;
        if budget.approved_other != Amount::zero() {
            return Err(Error::BusinessLogicError("自动余额执行尚不支持其他收费项目".into()));
        }

        let mut connection = self.connection(&execution, executor).await?;
        let account = self.account(&execution, &connection, executor).await?;

        let mut purchase = self
            .db
            .purchase_orders()
            .find_by_id(&execution.purchase_order_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("原采购单不存在".into()))?;
        ensure_po_fulfillable(&purchase)?;
        if purchase.stable.current_revision_id.as_deref()
            != Some(execution.purchase_order_revision_id.as_str())
        {
            return Err(Error::ConflictError("原采购生效修订已变化，不能取得旧执行资金占用".into()));
        }

        if self.covered(&execution, &account, budget.approved_total, executor).await? {
            return Ok(());
        }
        if execution.original_result_query_allowed() {
            return Err(Error::BusinessLogicError(
                "本执行组已有可能送达的原写请求，请先调查原资金结果".into(),
            ));
        }

        let data = DepositAutomaticReservationData {
            account_id: account.base.id.clone(),
            purchase_order_id: execution.purchase_order_id.clone(),
            purchase_revision_id: execution.purchase_order_revision_id.clone(),
            execution_group_id: execution.base.id.clone(),
            approved_goods_amount: budget.approved_goods,
            approved_freight: budget.approved_freight,
        };
        self.reserve(&data, executor).await?;
        self.db.purchase_orders().update(&mut purchase, executor).await?;
        self.db.supplier_api_connections().update(&mut connection, executor).await?;
        Ok(())
    }

    async fn covered(
        &self,
        execution: &ApiOrderExecution,
        account: &SupplierDepositAccount,
        total: Amount,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let Some(existing) = reservation_for_execution(&self.db, &execution.base.id, executor).await? else {
            return Ok(false);
        };
        if existing.account_id != account.base.id {
            return Err(Error::ConflictError("本执行组原占用不属于当前连接资金账户".into()));
        }
        SupplierDepositService::new(self.db.clone())
            .execution_coverage(
                &execution.base.id,
                &execution.purchase_order_id,
                &execution.purchase_order_revision_id,
                total,
                Instant::now(),
                executor,
            )
            .await?;
        Ok(true)
    }

    async fn reserve(
        &self,
        data: &DepositAutomaticReservationData,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let requests = automatic_payment_requests(
            &self.db,
            &data.account_id,
            &data.purchase_order_id,
            &data.purchase_revision_id,
            executor,
        )
        .await?;
        let request = data.request(&requests)?;

        let now = Instant::now();
        let grants = automatic_authorizations(&self.db, &data.account_id, now, executor).await?;
        let grant = data.authorization(&grants, now)?;

        let basis = data.basis(request, grant)?;
        let key = format!("automatic-execution:{}", data.execution_group_id);
        SupplierDepositService::new(self.db.clone())
            .reserve(&data.account_id, &key, basis, SYSTEM_ACTOR, now, executor)
            .await?;

        let actor = AuditActor::new(SYSTEM_ACTOR.into(), SYSTEM_ACTOR.into(), AccountKind::Admin)
            .with_actor_name_snapshot(Some("供应商预存款自动执行".into()))?;
        audit(&self.db, &actor, "reserve", &data.account_id, executor).await
    }

    async fn execution(&self, executor: &mut dyn Executor) -> Result<ApiOrderExecution> {
        let execution = self
            .db
            .supplier_api_order_executions()
            .find_by_id(&self.expected.base.id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("原采购执行组不存在".into()))?;
        if execution.stage != ApiOrderStage::Create
            || execution.superseded_by_purchase_revision_id.is_some()
            || execution.connection_id != self.expected.connection_id
            || execution.supplier_id != self.expected.supplier_id
            || execution.purchase_order_id != self.expected.purchase_order_id
            || execution.purchase_order_revision_id != self.expected.purchase_order_revision_id
            || execution.request_encrypted != self.expected.request_encrypted
        {
            return Err(Error::ConflictError("已预检执行组或原预算已变化，请重新核验".into()));
        }
        Ok(execution)
    }

    async fn connection(
        &self,
        execution: &ApiOrderExecution,
        executor: &mut dyn Executor,
    ) -> Result<SupplierApiConnection> {
        let connection = self
            .db
            .supplier_api_connections()
            .find_by_id(&execution.connection_id, executor)
            .await?
            .filter(|connection| connection.supplier_id.as_ref() == execution.supplier_id)
            .ok_or_else(|| Error::NotFound("执行组供应商连接不存在或归属不符".into()))?;
        connection.ensure_active()?;
        Ok(connection)
    }

    async fn account(
        &self,
        execution: &ApiOrderExecution,
        connection: &SupplierApiConnection,
        executor: &mut dyn Executor,
    ) -> Result<SupplierDepositAccount> {
        let id = connection
            .supplier_deposit_account_id
            .as_deref()
            .ok_or_else(|| Error::BusinessLogicError("请先核验并绑定供应商预存款账户".into()))?;
        let account = self
            .db
            .supplier_deposit_accounts()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("连接预存款账户不存在".into()))?;

        let revision = self
            .db
            .purchase_order_revisions()
            .find_by_id(&execution.purchase_order_revision_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("原采购生效修订不存在".into()))?;
        if account.identity.supplier_id != execution.supplier_id
            || account.identity.payer_party_id != revision.payment_subject.settlement_party_id.as_ref()
            || connection.supplier_deposit_external_account_ref.as_ref()
                != Some(&account.identity.external_account_ref)
        {
            return Err(Error::BusinessLogicError(
                "原批准采购主体、供应商或连接资金账号与预存款账户不一致".into(),
            ));
        }
        Ok(account)
    }
}
