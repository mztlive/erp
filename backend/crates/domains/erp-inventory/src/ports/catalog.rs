//! SKU 身份与版本展示事实的消费端口。

use std::collections::HashMap;

use async_trait::async_trait;
use persistence_core::Executor;

use crate::error::{Error, Result};

/// Minimal SKU identity used to hydrate inventory list/detail views.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkuFact {
    /// Stable SKU id.
    pub id: String,
    /// SKU number.
    pub sku_no: String,
    /// Current revision id used to resolve name and specification.
    pub current_revision_id: Option<String>,
}

/// Minimal SKU revision display fact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkuRevisionFact {
    /// Revision id.
    pub id: String,
    /// Current SKU name.
    pub name: String,
    /// Optional specification summary.
    pub specification: Option<String>,
}

/// 库存用来读取 SKU 身份、且不依赖 `erp-catalog` 的端口。
#[async_trait]
pub trait CatalogFactsPort: Send + Sync {
    /// 按 SKU 编码、当前名称或规格做字面量匹配。
    ///
    /// # 参数
    /// * `_q` - 待匹配的字面量。
    /// * `_executor` - 调用方选择的数据访问执行器。
    ///
    /// # 返回
    /// 覆盖实现在成功时返回命中的 SKU 标识；无命中时为空集合。
    ///
    /// # 错误
    /// 默认实现固定返回 `Error::Internal`（商品搜索端口未接线）。覆盖实现在商品查询失败时返回对应错误。
    async fn matching_sku_ids(
        &self,
        _q: &str,
        _executor: &mut dyn Executor,
    ) -> Result<Vec<erp_core::ids::SkuId>> {
        Err(Error::Internal("商品搜索端口未接线".to_string()))
    }

    /// 按标识返回 SKU 事实。
    ///
    /// # 参数
    /// * `ids` - SKU 标识。
    /// * `executor` - 调用方选择的数据访问执行器。
    ///
    /// # 返回
    /// 返回以 SKU 标识为键的事实映射。
    ///
    /// # 错误
    /// 适配器查询失败时返回对应错误。
    async fn skus_by_ids(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, SkuFact>>;

    /// 按标识返回 SKU 版本展示事实。
    ///
    /// # 参数
    /// * `ids` - SKU 版本标识。
    /// * `executor` - 调用方选择的数据访问执行器。
    ///
    /// # 返回
    /// 返回以版本标识为键的展示事实映射。
    ///
    /// # 错误
    /// 适配器查询失败时返回对应错误。
    async fn sku_revisions_by_ids(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, SkuRevisionFact>>;
}

/// Fail-closed catalog facts port used when composition has not injected an adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedCatalogFacts;

#[async_trait]
impl CatalogFactsPort for FailClosedCatalogFacts {
    async fn skus_by_ids(
        &self,
        _ids: &[String],
        _executor: &mut dyn Executor,
    ) -> Result<HashMap<String, SkuFact>> {
        Err(Error::Internal("商品事实端口未接线".to_string()))
    }

    async fn sku_revisions_by_ids(
        &self,
        _ids: &[String],
        _executor: &mut dyn Executor,
    ) -> Result<HashMap<String, SkuRevisionFact>> {
        Err(Error::Internal("商品事实端口未接线".to_string()))
    }
}
