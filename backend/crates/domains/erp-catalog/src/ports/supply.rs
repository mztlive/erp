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
    ///
    /// # 参数
    /// * `filter` - 已规范化的商品筛选
    /// * `executor` - 调用方数据执行器
    ///
    /// # 返回
    /// 返回当前页商品行及 SKU 供给覆盖。
    ///
    /// # 错误
    /// 分页读取失败时返回仓储错误。
    async fn product_page(
        &self,
        filter: &ProductFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<ProductRow>>;
    /// 分页读取符合原资格条件的可售 SKU。
    ///
    /// # 参数
    /// * `filter` - 已规范化的可售资格筛选
    /// * `executor` - 调用方数据执行器
    ///
    /// # 返回
    /// 返回当前页可售 SKU 行。
    ///
    /// # 错误
    /// 分页读取失败时返回仓储错误。
    async fn search_sellable_skus(
        &self,
        filter: &SellableSkuFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SellableSkuRow>>;
    /// 原样复验精确 SKU 与修订引用，不扩大为完整列表查询。
    ///
    /// # 参数
    /// * `refs` - SKU 与修订的精确引用对
    /// * `date` - 资格业务日期
    /// * `executor` - 调用方数据执行器
    ///
    /// # 返回
    /// 返回复验后仍可售的行。
    ///
    /// # 错误
    /// 精确读取失败时返回仓储错误。
    async fn find_sellable_sku_refs(
        &self,
        refs: &[(String, String)],
        date: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SellableSkuRow>>;
    /// 按稳定 SKU 身份取出当前可售修订。
    ///
    /// # 参数
    /// * `sku_ids` - 稳定 SKU ID
    /// * `date` - 资格业务日期
    /// * `executor` - 调用方数据执行器
    ///
    /// # 返回
    /// 返回该日期下仍可售的当前修订。
    ///
    /// # 错误
    /// 读取失败时返回仓储错误。
    async fn find_sellable_skus_by_ids(
        &self,
        sku_ids: &[String],
        date: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SellableSkuRow>>;
}
