//! 人工目录匹配命令，不修改公司主档或正式供给。
use erp_core::money::UnitPrice;
use serde::{Deserialize, Serialize};

use crate::entity::supplier_catalog::SupplierCatalogItem;
use crate::ports::connector::offer::CatalogAvailability;
use crate::{Error, Result};

/// 目录选品状态依据正式供给关联，不把旧人工 SKU 匹配当作已选品。
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CatalogSelectionStatus {
    #[default]
    All,
    Unselected,
    Selected,
}

/// 实际执行的同步方式，不以适配器声明冒充完整变更覆盖。
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CatalogSyncMode {
    #[default]
    Full,
    IncrementalPartial,
    IncrementalComplete,
}

/// 列表、总数及批量选品共用的来源目录筛选。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogFilter {
    pub keyword: Option<String>,
    pub category_id: Option<String>,
    pub brand_id: Option<String>,
    pub price_min: Option<UnitPrice>,
    pub price_max: Option<UnitPrice>,
    pub availability: Option<CatalogAvailability>,
    pub selection_status: Option<CatalogSelectionStatus>,
}

impl CatalogFilter {
    /// 验证可复用筛选边界。
    /// # 参数
    /// 无；读取当前筛选条件。
    /// # 返回
    /// 金额闭区间及文本长度合法时返回成功。
    /// # 错误
    /// 负金额、倒置区间或过长文本时拒绝。
    pub fn validate(&self) -> Result<()> {
        for value in [&self.keyword, &self.category_id, &self.brand_id].into_iter().flatten() {
            if value.len() > 200 {
                return Err(Error::ValidationError("目录筛选文本过长".into()));
            }
        }
        if self.price_min.is_some_and(|value| value.to_decimal().is_sign_negative())
            || self.price_max.is_some_and(|value| value.to_decimal().is_sign_negative())
            || matches!((self.price_min, self.price_max), (Some(min), Some(max)) if min > max)
        {
            return Err(Error::ValidationError("采购报价区间无效".into()));
        }
        Ok(())
    }
}
/// 人工确认来源文案变化未改变实际销售项；禁止通过此命令换 SKU 或单位。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewCatalogIdentityRequest {
    pub expected_version: u64,
    pub expected_sku_revision_id: String,
    pub expected_offering_version: u64,
    pub source_specification: String,
    pub source_unit: Option<String>,
    pub confirmed_base_unit_id: String,
    pub confirm_same_identity: bool,
    pub reason: String,
    pub idempotency_key: String,
}

/// 已授权且用于人工核对的当前来源与公司身份快照。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogIdentityReviewView {
    pub item_id: String,
    pub item_version: u64,
    pub source_specification: String,
    pub source_unit: Option<String>,
    pub company_sku_id: String,
    pub company_sku_no: String,
    pub company_specification: String,
    pub company_unit_id: String,
    pub company_unit_code: String,
    pub company_unit_name: String,
    pub sku_revision_id: String,
    pub offering_version: u64,
    pub requires_identity_review: bool,
}

impl CatalogIdentityReviewView {
    /// 核对人工看到的公司版本与单位仍对应当前资料。
    /// # 参数
    /// 已明确填写的人工核对请求。
    /// # 返回
    /// 版本及公司基础单位相同则成功。
    /// # 错误
    /// 版本变化或试图更换单位时拒绝。
    pub fn ensure_current(&self, request: &ReviewCatalogIdentityRequest) -> Result<()> {
        if self.sku_revision_id != request.expected_sku_revision_id
            || self.offering_version != request.expected_offering_version
        {
            return Err(Error::ConflictError("公司商品或供给已变化，请重新核对".into()));
        }
        if self.company_unit_id != request.confirmed_base_unit_id {
            return Err(Error::ValidationError("核对操作不得更换公司SKU单位".into()));
        }
        Ok(())
    }
}

/// 目录展示保留原始单位，并提供可用于选品的单位建议。
#[derive(Debug, Serialize)]
pub struct CatalogItemView {
    #[serde(flatten)]
    pub item: SupplierCatalogItem,
    pub suggested_unit: Option<String>,
    pub unit_inferred: bool,
}

impl From<SupplierCatalogItem> for CatalogItemView {
    fn from(item: SupplierCatalogItem) -> Self {
        let suggested_unit = item.suggested_unit();
        let unit_inferred =
            suggested_unit.is_some() && item.unit.as_deref().is_none_or(|unit| unit.trim().is_empty());
        Self { item, suggested_unit, unit_inferred }
    }
}
