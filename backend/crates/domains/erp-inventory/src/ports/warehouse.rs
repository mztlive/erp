//! 仓库身份与版本展示事实的消费端口。

use std::collections::HashMap;

use async_trait::async_trait;
use persistence_core::Executor;

use crate::error::{Error, Result};

/// Minimal warehouse identity used to hydrate inventory list/detail views.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WarehouseFact {
    /// Stable warehouse id.
    pub id: String,
    /// Warehouse code.
    pub warehouse_code: String,
    /// Current revision id used to resolve the display name.
    pub current_revision_id: Option<String>,
}

/// Minimal warehouse revision display fact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WarehouseRevisionFact {
    /// Revision id.
    pub id: String,
    /// Current warehouse name.
    pub name: String,
}

/// 库存用来读取仓库身份、且不依赖 `erp-warehouse` 的端口。
#[async_trait]
pub trait WarehouseFactsPort: Send + Sync {
    /// 判断仓库标识是否存在。
    ///
    /// # 参数
    /// * `id` - 仓库标识。
    /// * `executor` - 调用方选择的数据访问执行器。
    ///
    /// # 返回
    /// 存在时为 `true`，不存在时为 `false`。
    ///
    /// # 错误
    /// 适配器查询失败时返回对应错误。
    async fn warehouse_exists(&self, id: &str, executor: &mut dyn Executor) -> Result<bool>;

    /// 按标识返回仓库事实。
    ///
    /// # 参数
    /// * `ids` - 仓库标识。
    /// * `executor` - 调用方选择的数据访问执行器。
    ///
    /// # 返回
    /// 返回以仓库标识为键的事实映射。
    ///
    /// # 错误
    /// 适配器查询失败时返回对应错误。
    async fn warehouses_by_ids(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, WarehouseFact>>;

    /// 按标识返回仓库版本展示事实。
    ///
    /// # 参数
    /// * `ids` - 仓库版本标识。
    /// * `executor` - 调用方选择的数据访问执行器。
    ///
    /// # 返回
    /// 返回以版本标识为键的展示事实映射。
    ///
    /// # 错误
    /// 适配器查询失败时返回对应错误。
    async fn warehouse_revisions_by_ids(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, WarehouseRevisionFact>>;
}

/// Fail-closed warehouse facts port used when composition has not injected an adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedWarehouseFacts;

#[async_trait]
impl WarehouseFactsPort for FailClosedWarehouseFacts {
    async fn warehouse_exists(&self, _id: &str, _executor: &mut dyn Executor) -> Result<bool> {
        Err(Error::Internal("仓库事实端口未接线".to_string()))
    }

    async fn warehouses_by_ids(
        &self,
        _ids: &[String],
        _executor: &mut dyn Executor,
    ) -> Result<HashMap<String, WarehouseFact>> {
        Err(Error::Internal("仓库事实端口未接线".to_string()))
    }

    async fn warehouse_revisions_by_ids(
        &self,
        _ids: &[String],
        _executor: &mut dyn Executor,
    ) -> Result<HashMap<String, WarehouseRevisionFact>> {
        Err(Error::Internal("仓库事实端口未接线".to_string()))
    }
}
