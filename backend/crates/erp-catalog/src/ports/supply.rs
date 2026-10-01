//! 商品消费的跨域列表与精确可售资格查询合同。
use async_trait::async_trait;
use erp_core::common::time::BusinessDate;
use persistence_core::{Executor, PageResult, Result};

use crate::repository::{ProductFilter, ProductRow, SellableSkuFilter, SellableSkuRow};

/// 保持原查询形状、仓储错误与调用方 Executor；实现由组合层注入。
#[async_trait]
pub trait CatalogSupplyQueryPort: Send + Sync {
    /// 返回采购责任解析前已满足完整商品条件的候选身份。
    ///
    /// # 参数
    /// * `filter` - 已规范化且包含授权范围的全部商品筛选；忽略页码与页大小
    /// * `executor` - 与授权相同的数据执行器
    ///
    /// # 返回
    /// 返回至多 10001 个稳定主键；调用方必须整体拒绝超过 10000 个候选。
    ///
    /// # 错误
    /// 聚合或身份投影读取失败时返回仓储错误。
    async fn product_candidate_ids(
        &self,
        filter: &ProductFilter,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>>;
    /// 分页读取商品和当前 SKU 供给覆盖。
    async fn product_page(
        &self,
        filter: &ProductFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<ProductRow>>;
    /// 分页读取符合原资格条件的可售 SKU。
    async fn search_sellable_skus(
        &self,
        filter: &SellableSkuFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SellableSkuRow>>;
    /// 原样复验精确 SKU 与修订引用，不扩大为完整列表查询。
    async fn find_sellable_sku_refs(
        &self,
        refs: &[(String, String)],
        date: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SellableSkuRow>>;
    /// 按稳定 SKU 身份取出当前可售修订。
    async fn find_sellable_skus_by_ids(
        &self,
        sku_ids: &[String],
        date: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SellableSkuRow>>;
}
