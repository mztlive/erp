//! 生产装配的通用资金调查器；账务接收与新增下单资格分开。

use std::sync::Arc;

use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_finance::repository::SupplierDepositExt;
use erp_finance::repository::funds_evidence::funds_evidence_by_transaction;
use erp_integration::entity::supplier_callback::SupplierRefreshHint;
use erp_integration::entity::supplier_message::SupplierPulledMessage;
use erp_supply::connector_registry::SupplierConnectorRegistry;
use erp_supply::entity::supplier_api::SupplierApiConnection;
use erp_supply::ports::connector::funds::SupplierFundsDirection;
use erp_supply::ports::connector::order::OrderKey;
use erp_supply::ports::supplier_api_gateway::SupplierApiGateway;
use erp_supply::repository::{
    SupplierApiExt, SupplierFulfillmentExt, SupplierFulfillmentOrderRepositoryExt, supplier_catalog,
};
use mongodb::Database;
use persistence_core::NoTransaction;
use tokio::sync::Mutex;

use super::convert;
use super::process::{FundsInvestigationProcess, FundsInvestigationRun};
use super::store::MongoFundsInvestigationStore;
use crate::supplier_deposits::OrdersPaymentTasks;
use crate::supplier_messages::SupplierMessageFundsInvestigation;
use crate::{Error, Result};

/// 每轮最多20个连接、每账号一页50条，保持跨轮分页位置。
pub struct SupplierFundsInvestigationWorker {
    db: Database,
    registry: Arc<SupplierConnectorRegistry>,
    cursor: Mutex<String>,
    payment_tasks: Arc<dyn OrdersPaymentTasks>,
}

impl SupplierFundsInvestigationWorker {
    /// 复用应用数据库及实际协议注册表。
    /// # 参数
    /// * `db` / `registry` - 由组合根注入的同一数据源和协议实现。
    /// # 返回
    /// 返回未发起外部调用的可共享worker。
    /// # 错误
    /// 无。
    pub fn new(
        db: Database,
        registry: Arc<SupplierConnectorRegistry>,
        payment_tasks: Arc<dyn OrdersPaymentTasks>,
    ) -> Self {
        Self { db, registry, cursor: Mutex::new(String::new()), payment_tasks }
    }

    /// 有界恢复资金证据及读取明细，账户停用或授权撤销仍记录既有真实事实。
    /// # 参数
    /// 无；连接及外部资金账号全部从服务端装配取得。
    /// # 返回
    /// 返回本轮实际完成财务应用的证据数。
    /// # 错误
    /// 连接分页读取失败时返回错误；单账号异常保留证据并继续其他账号。
    pub async fn run_once(&self) -> Result<usize> {
        let mut cursor = self.cursor.lock().await;
        let kinds = self.registry.catalog_connector_types();
        let connections =
            supplier_catalog::connection_page(&self.db, &cursor, &kinds, &mut NoTransaction).await?;
        if connections.is_empty() {
            cursor.clear();
            return Ok(0);
        }
        let mut applied = 0;
        for connection in connections {
            *cursor = connection.base.id.clone();
            match self.run_connection(&connection).await {
                Ok(report) => applied += report.applied,
                Err(_) => {
                    tracing::warn!(connection_id = %connection.base.id, "供应商资金调查仍有未决证据，未覆盖账面或释放占用")
                },
            }
        }
        Ok(applied)
    }

    async fn run_connection(&self, connection: &SupplierApiConnection) -> Result<FundsInvestigationRun> {
        let connector = self
            .registry
            .fulfillment_connector(connection)
            .map_err(|error| Error::BusinessLogicError(error.summary))?;
        let Some(source) = connector.funds_source() else {
            return Ok(FundsInvestigationRun::default());
        };
        if connector.connection_id().as_ref() != connection.base.id {
            return Err(Error::Forbidden("资金能力不属于当前连接".into()));
        }
        let Some(store) = MongoFundsInvestigationStore::claim(
            self.db.clone(),
            connection,
            source.account_reference(),
            self.payment_tasks.clone(),
        )
        .await?
        else {
            return Ok(FundsInvestigationRun::default());
        };
        let result = FundsInvestigationProcess { source, store: &store }.run(Instant::now()).await;
        store.release().await?;
        result
    }

    async fn message_order(
        &self,
        connection: &SupplierApiConnection,
        hint: &SupplierRefreshHint,
    ) -> Result<String> {
        let key = match hint.target_kind.as_str() {
            "external_order" => return Ok(hint.object_id.clone()),
            "merchant_order" => OrderKey::Merchant(hint.object_id.clone()),
            _ => return Err(Error::BusinessLogicError("资金消息缺少可核验原订单".into())),
        };
        self.db
            .supplier_fulfillment_orders()
            .find_for_connection(&connection.base.id.clone().into(), &key, &mut NoTransaction)
            .await?
            .and_then(|order| order.external_order_no)
            .ok_or_else(|| Error::BusinessLogicError("原消息供应商订单仍待查询".into()))
    }

    async fn exact_message_conclusion(
        &self,
        connection: &SupplierApiConnection,
        transaction: &str,
        order: &str,
        effect: SupplierFundsDirection,
    ) -> Result<()> {
        let account_id = connection
            .supplier_deposit_account_id
            .as_deref()
            .ok_or_else(|| Error::BusinessLogicError("资金消息缺少财务账户绑定".into()))?;
        let account = self
            .db
            .supplier_deposit_accounts()
            .find_by_id(account_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("资金账号不存在".into()))?;
        if account.reconciliation_required {
            return Err(Error::BusinessLogicError("资金账户仍有待核对项目，消息继续保留".into()));
        }
        let record = funds_evidence_by_transaction(&self.db, account_id, transaction, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::BusinessLogicError("本次消息资金交易仍待核实".into()))?;
        record.ensure_message_conclusion(&account, transaction, order, convert::direction(effect))?;
        Ok(())
    }
}

#[async_trait]
impl SupplierMessageFundsInvestigation for SupplierFundsInvestigationWorker {
    async fn investigate(&self, message: &SupplierPulledMessage, hint: &SupplierRefreshHint) -> Result<()> {
        let connection = self
            .db
            .supplier_api_connections()
            .find_by_id(message.connection_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("资金消息原连接不存在".into()))?;
        let connector = self
            .registry
            .fulfillment_connector(&connection)
            .map_err(|error| Error::BusinessLogicError(error.summary))?;
        if connector.messages().is_none_or(|source| source.account_scope() != message.account_scope) {
            return Err(Error::Forbidden("资金消息账号不属于原技术连接".into()));
        }
        let source = connector
            .funds_source()
            .ok_or_else(|| Error::BusinessLogicError("资金只读协议尚未核实，原消息保留待查".into()))?;
        self.run_connection(&connection).await?;
        let effect = source
            .message_effect(&message.message_type)
            .ok_or_else(|| Error::BusinessLogicError("该消息未证明资金最终结果，继续保留原线索".into()))?;
        let transaction = hint
            .funds_transaction_id
            .as_deref()
            .ok_or_else(|| Error::BusinessLogicError("消息尚未提供可精确关联的资金交易，继续调查".into()))?;
        let order = self.message_order(&connection, hint).await?;
        self.exact_message_conclusion(&connection, transaction, &order, effect).await
    }
}
