//! 选品册所需的跨域事实，不依赖提供方领域。

use async_trait::async_trait;
use erp_core::common::time::BusinessDate;
use erp_core::money::Amount;

use crate::entity::sales_selection::{
    ImageAssetSnapshot, PoolFilterSnapshot, SpecificationAttributeSnapshot,
};
use crate::Result;

/// 客户身份与展示名称。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionCustomerFact {
    /// 客户稳定身份。
    pub id: String,
    /// 客户编号。
    pub customer_no: String,
    /// 展示名称。
    pub display_name: String,
    /// 是否启用。
    pub active: bool,
}

/// 可售 SKU 事实，不含成本与供应商。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionSkuFact {
    /// 稳定 SKU。
    pub sku_id: String,
    /// 修订。
    pub sku_revision_id: String,
    /// SPU。
    pub product_id: String,
    /// 商品类型代码。
    pub product_kind: String,
    /// 分类；缺失为 `None`。
    pub category_id: Option<String>,
    /// 名称。
    pub name: String,
    /// 结构化规格。
    pub specification_attributes: Vec<SpecificationAttributeSnapshot>,
    /// 单位。
    pub unit: String,
    /// 主图资产。
    pub main_image_asset_id: Option<String>,
    /// 销售可见含税价。
    pub sales_visible_price_gross: Amount,
}

/// 客户事实端口。
#[async_trait]
pub trait SelectionCustomerPort: Send + Sync {
    /// 读取客户编号与展示名称。
    ///
    /// # 参数
    /// * `customer_id` - 客户身份
    ///
    /// # 返回
    /// 返回客户事实。
    ///
    /// # 错误
    /// 不存在或停用时由调用方拒绝。
    async fn customer_fact(&self, customer_id: &str) -> Result<SelectionCustomerFact>;
}

/// 商品池事实端口。
#[async_trait]
pub trait SelectionCatalogPort: Send + Sync {
    /// 按筛选取出可售 SKU，调用方负责上限与排序。
    ///
    /// # 参数
    /// * `filter` - 规范化筛选
    /// * `as_of` - 资格业务日期
    ///
    /// # 返回
    /// 返回资格投影，不含分页截断承诺。
    ///
    /// # 错误
    /// 仓储失败。
    async fn collect_by_filter(
        &self,
        filter: &PoolFilterSnapshot,
        as_of: BusinessDate,
    ) -> Result<Vec<SelectionSkuFact>>;

    /// 按稳定 SKU 身份取出当前可售修订。
    ///
    /// # 参数
    /// * `sku_ids` - 勾选身份
    /// * `as_of` - 资格业务日期
    ///
    /// # 返回
    /// 返回仍具备资格的 SKU；缺失项由调用方列为失效。
    ///
    /// # 错误
    /// 仓储失败。
    async fn collect_by_ids(&self, sku_ids: &[String], as_of: BusinessDate) -> Result<Vec<SelectionSkuFact>>;

    /// 复核精确 SKU 修订是否仍可售。
    ///
    /// # 参数
    /// * `refs` - `(sku_id, sku_revision_id)`
    /// * `as_of` - 资格业务日期
    ///
    /// # 返回
    /// 返回仍合格的引用。
    ///
    /// # 错误
    /// 仓储失败。
    async fn qualified_refs(
        &self,
        refs: &[(String, String)],
        as_of: BusinessDate,
    ) -> Result<Vec<(String, String)>>;
}

/// 图片快照与读取端口。
#[async_trait]
pub trait SelectionImagePort: Send + Sync {
    /// 把 SKU 主图复制为选品快照资产。
    ///
    /// # 参数
    /// * `source_asset_id` - 原资产；空则无图
    /// * `booklet_id` - 选品册
    /// * `batch_id` - 批次
    ///
    /// # 返回
    /// 可读时返回快照引用；不存在或不可读返回 `None`。
    ///
    /// # 错误
    /// 存储失败。
    async fn snapshot_image(
        &self,
        source_asset_id: Option<&str>,
        booklet_id: &str,
        batch_id: &str,
    ) -> Result<Option<ImageAssetSnapshot>>;

    /// 按快照对象键读取内容。
    ///
    /// # 参数
    /// * `storage_object_key` - 快照对象键
    ///
    /// # 返回
    /// 返回字节与内容类型。
    ///
    /// # 错误
    /// 对象不存在或读取失败。
    async fn load_bytes(&self, storage_object_key: &str) -> Result<(Vec<u8>, String)>;
}
