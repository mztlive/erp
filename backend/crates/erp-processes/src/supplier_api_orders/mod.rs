//! 从已批准采购冻结接口执行意图，逐步执行真实供应商协议。

mod change_guard;
mod delivery_drafts;
mod enqueue;
mod execution;
mod payment;
mod preflight;
mod setup;
mod snapshot;
mod steps;

use std::sync::Arc;

pub use change_guard::supersede_purchase_executions;
pub use delivery_drafts::create_api_delivery_drafts;
pub use enqueue::enqueue_purchase;
use erp_party::SensitiveDataCodec;
use erp_supply::api_order::{ApiOrderExecution, ApiOrderExt};
use mongodb::Database;
use persistence_core::NoTransaction;
pub use setup::{ExecutionSetupRequest, ExecutionSetupView};

use crate::connectors::dangaoshushu::DangaoshushuRuntime;
use crate::{Error, Result};

/// 供应商 API 正式采购执行器；密文、写意图和结果分别可靠保存。
#[derive(Clone)]
pub struct SupplierApiOrderProcess {
    db: Database,
    runtime: Arc<DangaoshushuRuntime>,
    codec: Arc<SensitiveDataCodec>,
}

impl SupplierApiOrderProcess {
    /// 装配真实连接器运行时与应用敏感资料加密器。
    /// # 参数
    /// `db`、`runtime`、`codec` 来自应用组合根。
    /// # 返回
    /// 不执行外部调用的流程实例。
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database, runtime: Arc<DangaoshushuRuntime>, codec: Arc<SensitiveDataCodec>) -> Self {
        Self { db, runtime, codec }
    }

    /// 处理一批到期意图；单个异常只延迟该订单，后续订单继续执行。
    /// # 参数
    /// 无；到期记录由有界领域查询取得。
    /// # 返回
    /// 本轮检查记录数。
    /// # 错误
    /// 初始队列读取失败时返回数据库错误。
    pub async fn run_once(&self) -> Result<usize> {
        let rows = erp_supply::api_order::due(&self.db, &mut NoTransaction).await?;
        let count = rows.len();
        for row in rows {
            if let Err(error) = self.execute(row).await {
                tracing::warn!(error = %error, "供应商订单执行需重查持久化状态");
            }
        }
        Ok(count)
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
            .ok_or_else(|| Error::NotFound("此订单没有 API 自动执行记录".into()))
    }
}
