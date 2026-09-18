use application_core::normalized_text;
use erp_core::common::time::BusinessDate;
use erp_core::ids::SkuId;
use erp_core::money::Amount;
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::common::{PageParams, paging_params};
use crate::entity::catalog::{EnableStatus, SkuRevision};
use crate::error::Result;
use crate::repository::SkuRevisionRow;

/// SKU 修订列表允许的排序字段白名单。
pub(crate) const SKU_REVISION_SORT_FIELDS: &[&str] = &["created_at", "revision_no"];

/// SKU 修订响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SkuRevisionView {
    /// 实体主键。
    pub id: String,
    /// 所属稳定 SKU。
    pub sku_id: String,
    /// 修订序号。
    pub revision_no: u32,
    /// 公司审核后的 SKU 名称。
    pub name: String,
    /// 公司审核后的 SKU 描述。
    pub description: Option<String>,
    /// 公司审核后的规格或服务内容。
    pub specification: Option<String>,
    /// 条码原值。
    pub barcode: Option<String>,
    /// 来源 SKU 主图（已归档受控文件，D05）。
    pub source_main_image_asset_id: Option<String>,
    /// 重量（千克）。
    pub weight_kg: Option<erp_core::money::Quantity>,
    /// 体积（立方米）。
    pub volume_m3: Option<erp_core::money::Quantity>,
    /// 修订启停状态。
    pub status: EnableStatus,
    /// 公司对销售可见的含税价格（字符串形态）。
    pub sales_visible_price_gross: Option<Amount>,
    /// 市场参考价。
    pub market_price: Option<Amount>,
    /// 生效开始日。
    pub effective_from: BusinessDate,
    /// 生效结束日；空表示长期。
    pub effective_to: Option<BusinessDate>,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 乐观锁版本。
    pub version: u64,
}

impl From<SkuRevision> for SkuRevisionView {
    /// 从实体构造响应视图。
    ///
    /// # 参数
    /// * `revision` - SKU 修订实体
    ///
    /// # 返回
    /// 返回响应视图。
    fn from(revision: SkuRevision) -> Self {
        Self {
            id: revision.base.id,
            sku_id: revision.sku_id.to_string(),
            revision_no: revision.revision.revision_no,
            name: revision.name,
            description: revision.description,
            specification: revision.specification,
            barcode: revision.barcode,
            source_main_image_asset_id: revision.source_main_image_asset_id.as_ref().map(|id| id.to_string()),
            weight_kg: revision.weight_kg,
            volume_m3: revision.volume_m3,
            status: revision.status,
            sales_visible_price_gross: revision.sales_visible_price_gross,
            market_price: revision.market_price,
            effective_from: revision.effective_from,
            effective_to: revision.effective_to,
            created_at: revision.base.created_at,
            version: revision.base.version,
        }
    }
}

impl From<SkuRevisionRow> for SkuRevisionView {
    /// 从投影行构造响应视图，与 `From<SkuRevision>` 同语义（erp-catalog-002）。
    ///
    /// # 参数
    /// * `row` - SKU 修订列表投影行
    ///
    /// # 返回
    /// 返回响应视图。
    fn from(row: SkuRevisionRow) -> Self {
        Self {
            id: row.id,
            sku_id: row.sku_id,
            revision_no: row.revision_no,
            name: row.name,
            description: row.description,
            specification: row.specification,
            barcode: row.barcode,
            source_main_image_asset_id: row.source_main_image_asset_id,
            weight_kg: row.weight_kg,
            volume_m3: row.volume_m3,
            status: row.status,
            sales_visible_price_gross: row.sales_visible_price_gross,
            market_price: row.market_price,
            effective_from: row.effective_from,
            effective_to: row.effective_to,
            created_at: row.created_at,
            version: row.version,
        }
    }
}

/// SKU 修订列表查询参数。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
pub struct SkuRevisionListParams {
    /// 所属稳定 SKU 筛选。
    pub sku_id: Option<SkuId>,
    /// 名称字面量筛选（忽略大小写）。
    pub name: Option<String>,
    /// 条码精确筛选（内部按 trim 规范化）。
    pub barcode: Option<String>,
    /// 修订启停状态筛选。
    pub status: Option<EnableStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`revision_no`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的 SKU 修订列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SkuRevisionListQuery {
    /// 所属稳定 SKU 筛选。
    pub sku_id: Option<String>,
    /// 名称筛选。
    pub name: Option<String>,
    /// 条码筛选。
    pub barcode: Option<String>,
    /// 修订启停状态筛选。
    pub status: Option<EnableStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl SkuRevisionListParams {
    /// 归一化 SKU 修订列表查询参数。
    ///
    /// 文本筛选去首尾空白、分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<SkuRevisionListQuery> {
        Ok(SkuRevisionListQuery {
            sku_id: self.sku_id.as_ref().map(|id| id.to_string()),
            name: normalized_text(self.name.as_deref()),
            barcode: normalized_text(self.barcode.as_deref()),
            status: self.status,
            paging: paging_params(
                self.page,
                self.page_size,
                &self.sort_by,
                &self.sort_dir,
                SKU_REVISION_SORT_FIELDS,
            )?,
        })
    }
}
