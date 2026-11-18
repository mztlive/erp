//! 从已批准采购冻结接口执行意图，逐步执行真实供应商协议。

mod budget;
mod cancellation;
mod change_guard;
mod confirmation;
mod delivery_drafts;
mod enqueue;
mod execution;
mod funds;
mod payment;
mod preflight;
mod receipt;
mod setup;
mod setup_address;
mod setup_query;
mod setup_tree;
mod snapshot;
mod steps;

use std::sync::Arc;

pub use change_guard::supersede_purchase_executions;
pub use delivery_drafts::create_api_delivery_drafts;
pub use enqueue::enqueue_purchase;
use erp_identity::AuthorizationService;
use erp_party::SensitiveDataCodec;
use erp_supply::api_order::{ApiOrderExecution, ApiOrderExt, ApiOrderStage};
use erp_supply::connector_registry::SupplierConnectorRegistry;
use erp_supply::ports::connector::funds::SupplierOrderFunds;
pub use funds::execution_budget;
use mongodb::Database;
use persistence_core::NoTransaction;
pub use setup::{ExecutionSetupRequest, ExecutionSetupView};
pub use setup_address::{ExecutionAddressCandidates, ExecutionAddressQuery};
pub use setup_query::ExecutionQueryRequest;

use crate::{Error, Result};

/// 供应商 API 正式采购执行器；密文、写意图和结果分别可靠保存。
#[derive(Clone)]
pub struct SupplierApiOrderProcess {
    db: Database,
    runtime: Arc<SupplierConnectorRegistry>,
    codec: Arc<SensitiveDataCodec>,
    funds: Arc<dyn SupplierOrderFunds<Error = Error>>,
    authorization: Arc<AuthorizationService>,
}

impl SupplierApiOrderProcess {
    /// 组合根装配正式采购执行时注入协议运行时、敏感资料加密器和 ERP 资金资格；构造流程本身不外呼。
    ///
    /// `db`、`runtime`、`codec` 与 ERP 资金资格 `funds` 均来自应用组合根。
    ///
    /// 不执行外部调用的流程实例。
    pub fn new(
        db: Database,
        runtime: Arc<SupplierConnectorRegistry>,
        codec: Arc<SensitiveDataCodec>,
        funds: Arc<dyn SupplierOrderFunds<Error = Error>>,
        authorization: Arc<AuthorizationService>,
    ) -> Self {
        Self { db, runtime, codec, funds, authorization }
    }

    /// 后台轮询时扫描一批到期的原采购执行意图；单个执行受阻只延迟该订单，不阻断后续订单。
    ///
    /// 无；到期记录由有界领域查询取得。
    ///
    /// 本轮检查记录数。
    ///
    /// 初始队列读取失败时返回数据库错误。
    pub async fn run_once(&self) -> Result<usize> {
        let rows = erp_supply::api_order::due(&self.db, &mut NoTransaction).await?;
        let count = rows.len();
        for row in rows {
            let id = row.base.id.clone();
            if let Err(error) = self.execute(row).await {
                tracing::warn!(error = %error, "供应商订单执行需重查持久化状态");
                self.defer_current(&id, &error).await;
            }
        }

        Ok(count + self.run_receipts_once().await?)
    }

    async fn defer_current(&self, id: &str, error: &Error) {
        // 重读当前记录只补充原因，不从旧内存恢复步骤，也不重放外部调用。
        if let Ok(mut current) = self.load(id).await {
            if matches!(current.stage, ApiOrderStage::Observe | ApiOrderStage::Stopped) {
                return;
            }
            current.defer(error.supplier_order_message());
            if let Err(error) = self.persist(&mut current).await {
                tracing::warn!(error = %error, "供应商订单阻断原因待持久化，下轮仍查询原状态");
            }
        }
    }

    async fn persist(&self, execution: &mut ApiOrderExecution) -> Result<()> {
        self.db.supplier_api_order_executions().update(execution, &mut NoTransaction).await?;
        Ok(())
    }

    async fn load(&self, id: &str) -> Result<ApiOrderExecution> {
        self.db
            .supplier_api_order_executions()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("此订单未登记供应商自动处理记录，请从采购单核对下单方式".into()))
    }
}

fn missing_capability(label: &str) -> Error {
    Error::BusinessLogicError(format!(
        "当前供应商尚不支持自动{label}，请联系管理员核实处理方式；系统未发送请求"
    ))
}
