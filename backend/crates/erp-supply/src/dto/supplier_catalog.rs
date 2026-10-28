//! 人工目录匹配命令，不修改公司主档或正式供给。
use std::collections::BTreeSet;

use erp_core::money::UnitPrice;
use serde::{Deserialize, Deserializer, Serialize, de};

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
    #[serde(default, deserialize_with = "deserialize_ids")]
    pub category_ids: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_ids")]
    pub brand_ids: Vec<String>,
    /// 仅来源明确提供可靠分类树时允许，当前目录没有该事实。
    #[serde(default)]
    pub include_descendants: bool,
    #[serde(default)]
    pub only_unadopted: bool,
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
        for value in [&self.keyword, &self.category_id, &self.brand_id]
            .into_iter()
            .flatten()
            .chain(self.category_ids.iter())
            .chain(self.brand_ids.iter())
        {
            if value.len() > 200 {
                return Err(Error::ValidationError("目录筛选文本过长".into()));
            }
        }
        if self.category_ids.len() > 500 || self.brand_ids.len() > 500 {
            return Err(Error::ValidationError("来源分类或品牌最多选择 500 项".into()));
        }
        if self.include_descendants {
            return Err(Error::ValidationError("来源未提供可靠分类层级，不能包含子分类".into()));
        }
        if self.only_unadopted && self.selection_status == Some(CatalogSelectionStatus::Selected) {
            return Err(Error::ValidationError("仅处理未入库项不能同时筛选已入库项".into()));
        }
        if self.price_min.is_some_and(|value| value.to_decimal().is_sign_negative())
            || self.price_max.is_some_and(|value| value.to_decimal().is_sign_negative())
            || matches!((self.price_min, self.price_max), (Some(min), Some(max)) if min > max)
        {
            return Err(Error::ValidationError("采购报价区间无效".into()));
        }
        Ok(())
    }

    /// 合并单值兼容参数和多选来源分类，不按名称推断层级。
    /// # 参数
    /// 无；读取本筛选的来源分类编号。
    /// # 返回
    /// 去空、去重并稳定排序的分类编号。
    /// # 错误
    /// 不返回错误；边界约束由 `validate` 执行。
    pub fn categories(&self) -> Vec<&str> {
        ids(self.category_id.as_deref(), &self.category_ids)
    }

    /// 合并单值兼容参数和多选来源品牌。
    /// # 参数
    /// 无；读取本筛选的来源品牌编号。
    /// # 返回
    /// 去空、去重并稳定排序的品牌编号。
    /// # 错误
    /// 不返回错误；边界约束由 `validate` 执行。
    pub fn brands(&self) -> Vec<&str> {
        ids(self.brand_id.as_deref(), &self.brand_ids)
    }

    /// 检查依赖采购报价的查询资格，防止通过区间结果推算隐藏成本。
    /// # 参数
    /// `can_read_cost` 为当前服务端成本权限判定。
    /// # 返回
    /// 具备成本权限或未使用成本条件时成功。
    /// # 错误
    /// 无成本权限且设置报价边界时拒绝。
    pub fn ensure_cost_access(&self, can_read_cost: bool) -> Result<()> {
        if !can_read_cost && (self.price_min.is_some() || self.price_max.is_some()) {
            return Err(Error::Forbidden("当前账号不能按采购报价筛选供应商商品".into()));
        }
        Ok(())
    }
}

fn ids<'a>(single: Option<&'a str>, many: &'a [String]) -> Vec<&'a str> {
    single
        .into_iter()
        .chain(many.iter().map(String::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn deserialize_ids<'de, D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Values {
        List(Vec<String>),
        Query(String),
    }
    match Values::deserialize(deserializer)? {
        Values::List(values) => Ok(values),
        Values::Query(value) if value.trim_start().starts_with('[') => {
            serde_json::from_str(&value).map_err(de::Error::custom)
        },
        Values::Query(value) => Ok(value.split(',').map(str::to_string).collect()),
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

#[cfg(test)]
mod tests {
    use super::{CatalogFilter, CatalogSelectionStatus};

    #[test]
    fn multi_select_filters_accept_query_strings_and_json_arrays() {
        let filter: CatalogFilter = serde_json::from_value(serde_json::json!({
            "category_id": "legacy", "category_ids": "[\"a,b\",\" second \",\"legacy\"]",
            "brand_ids": ["first", "second", "first", ""]
        }))
        .unwrap();
        filter.validate().unwrap();
        assert_eq!(filter.categories(), ["a,b", "legacy", "second"]);
        assert_eq!(filter.brands(), ["first", "second"]);
        let round_trip: CatalogFilter =
            serde_json::from_value(serde_json::to_value(filter).unwrap()).unwrap();
        assert_eq!(round_trip.categories(), ["a,b", "legacy", "second"]);
    }

    #[test]
    fn selection_without_reliable_hierarchy_is_rejected_without_guessing() {
        let filter = CatalogFilter { include_descendants: true, ..Default::default() };
        assert!(filter.validate().unwrap_err().to_string().contains("来源未提供可靠分类层级"));
    }

    #[test]
    fn only_unadopted_does_not_allow_a_conflicting_adopted_filter() {
        let mut filter = CatalogFilter {
            only_unadopted: true,
            selection_status: Some(CatalogSelectionStatus::Selected),
            ..Default::default()
        };
        assert!(filter.validate().is_err());
        filter.selection_status = Some(CatalogSelectionStatus::All);
        filter.validate().unwrap();
    }

    #[test]
    fn purchase_range_is_nonnegative_inclusive_and_cost_protected() {
        for json in [
            serde_json::json!({"price_min":"0", "price_max":"0"}),
            serde_json::json!({"price_min":"10.1000"}),
            serde_json::json!({"price_max":"100.0001"}),
        ] {
            let filter: CatalogFilter = serde_json::from_value(json).unwrap();
            filter.validate().unwrap();
            filter.ensure_cost_access(true).unwrap();
            assert!(filter.ensure_cost_access(false).is_err());
        }
        for json in [
            serde_json::json!({"price_min":"-0.0001"}),
            serde_json::json!({"price_min":"1.0001", "price_max":"1"}),
        ] {
            let filter: CatalogFilter = serde_json::from_value(json).unwrap();
            assert!(filter.validate().is_err());
        }
        assert!(serde_json::from_value::<CatalogFilter>(serde_json::json!({"price_min":0})).is_err());
        CatalogFilter::default().ensure_cost_access(false).unwrap();
    }

    #[test]
    fn malformed_or_excessive_ids_fail_instead_of_becoming_unrestricted() {
        assert!(
            serde_json::from_value::<CatalogFilter>(serde_json::json!({"category_ids":"[broken"})).is_err()
        );
        assert!(serde_json::from_value::<CatalogFilter>(serde_json::json!({"brand_ids":[1]})).is_err());
        assert!(
            CatalogFilter { category_ids: vec!["c".into(); 501], ..Default::default() }.validate().is_err()
        );
        assert!(CatalogFilter { brand_ids: vec!["c".repeat(201)], ..Default::default() }.validate().is_err());
    }
}
