//! 采购责任规则实体、选择器形状与规范化。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::catalog::{EnableStatus, ProductKind};
use crate::errors::{Error, Result};
use crate::ids::{ProcurementResponsibilityRuleId, ProductCategoryId, SkuId};
use crate::validation::{normalize_optional_text, normalize_required_text};

const SERVICE_REGION_MAX_LEN: usize = 128;
const ACTOR_MAX_LEN: usize = 128;
const OWNER_MAX_LEN: usize = 128;

/// 采购责任规则类型，枚举顺序不表示优先级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProcurementResponsibilityRuleType {
    /// 精确 SKU。
    Sku,
    /// 精确分类与服务区域。
    CategoryServiceRegion,
    /// 分类；解析时允许沿父分类逐级回退。
    Category,
    /// 商品业务类型。
    ProductKind,
    /// 唯一具体默认调度人。
    DefaultDispatcher,
}

impl ProcurementResponsibilityRuleType {
    /// 返回稳定持久化代码。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 `SCREAMING_SNAKE_CASE` 规则类型代码。
    ///
    /// # 错误
    /// 无。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sku => "SKU",
            Self::CategoryServiceRegion => "CATEGORY_SERVICE_REGION",
            Self::Category => "CATEGORY",
            Self::ProductKind => "PRODUCT_KIND",
            Self::DefaultDispatcher => "DEFAULT_DISPATCHER",
        }
    }

    /// 返回面向管理端的优先级序号。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 1 至 5，数值越小优先级越高。
    ///
    /// # 错误
    /// 无。
    pub fn priority(self) -> u8 {
        match self {
            Self::Sku => 1,
            Self::CategoryServiceRegion => 2,
            Self::Category => 3,
            Self::ProductKind => 4,
            Self::DefaultDispatcher => 5,
        }
    }
}

/// 采购责任规则创建或整项更新数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProcurementResponsibilityRuleData {
    /// 规则类型。
    pub rule_type: ProcurementResponsibilityRuleType,
    /// SKU 选择器；仅 SKU 规则填写。
    pub sku_id: Option<SkuId>,
    /// 分类选择器；仅分类相关规则填写。
    pub category_id: Option<ProductCategoryId>,
    /// 服务区域；仅分类 + 服务区域规则填写。
    pub service_region: Option<String>,
    /// 商品业务类型；仅 ProductKind 规则填写。
    pub product_kind: Option<ProductKind>,
    /// 具体采购负责人账号 ID。
    pub owner_user_id: String,
    /// 启停状态。
    pub status: EnableStatus,
}

/// 可维护的采购责任规则。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct ProcurementResponsibilityRule {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 规则类型。
    pub rule_type: ProcurementResponsibilityRuleType,
    /// SKU 选择器。
    pub sku_id: Option<SkuId>,
    /// 分类选择器。
    pub category_id: Option<ProductCategoryId>,
    /// 规范化服务区域。
    pub service_region: Option<String>,
    /// 商品业务类型选择器。
    pub product_kind: Option<ProductKind>,
    /// 具体采购负责人账号 ID。
    pub owner_user_id: String,
    /// 启停状态。
    pub status: EnableStatus,
    /// 由选择器确定性形成的唯一键。
    pub selector_key: String,
    /// 创建人。
    pub created_by: String,
    /// 最近更新人。
    pub updated_by: String,
}

impl ProcurementResponsibilityRule {
    /// 创建采购责任规则。
    ///
    /// # 参数
    /// * `id` - 规则主键
    /// * `data` - 类型、选择器、具体负责人和状态
    /// * `created_by` - 已认证创建人
    ///
    /// # 返回
    /// 返回选择器已规范化且形状合法的新规则。
    ///
    /// # 错误
    /// 选择器与规则类型不一致、区域或账号为空/过长时返回错误。
    pub fn new(
        id: ProcurementResponsibilityRuleId,
        data: ProcurementResponsibilityRuleData,
        created_by: impl Into<String>,
    ) -> Result<Self> {
        let created_by = normalize_actor(created_by.into())?;
        let normalized = NormalizedRuleData::try_from(data)?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            rule_type: normalized.rule_type,
            sku_id: normalized.sku_id,
            category_id: normalized.category_id,
            service_region: normalized.service_region,
            product_kind: normalized.product_kind,
            owner_user_id: normalized.owner_user_id,
            status: normalized.status,
            selector_key: normalized.selector_key,
            updated_by: created_by.clone(),
            created_by,
        })
    }

    /// 整项更新采购责任规则。
    ///
    /// # 参数
    /// * `data` - 新的类型、选择器、具体负责人和状态
    /// * `updated_by` - 已认证更新人
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 选择器形状、区域、负责人或操作人不合法时返回错误。
    pub fn update(
        &mut self,
        data: ProcurementResponsibilityRuleData,
        updated_by: impl Into<String>,
    ) -> Result<()> {
        let updated_by = normalize_actor(updated_by.into())?;
        let normalized = NormalizedRuleData::try_from(data)?;
        self.rule_type = normalized.rule_type;
        self.sku_id = normalized.sku_id;
        self.category_id = normalized.category_id;
        self.service_region = normalized.service_region;
        self.product_kind = normalized.product_kind;
        self.owner_user_id = normalized.owner_user_id;
        self.status = normalized.status;
        self.selector_key = normalized.selector_key;
        self.updated_by = updated_by;
        Ok(())
    }

    /// 判断规则当前是否参与责任解析。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 启用规则返回 `true`。
    ///
    /// # 错误
    /// 无。
    pub fn is_active(&self) -> bool {
        self.status.is_active()
    }
}

/// 已规范化并通过选择器形状校验的规则数据。
struct NormalizedRuleData {
    rule_type: ProcurementResponsibilityRuleType,
    sku_id: Option<SkuId>,
    category_id: Option<ProductCategoryId>,
    service_region: Option<String>,
    product_kind: Option<ProductKind>,
    owner_user_id: String,
    status: EnableStatus,
    selector_key: String,
}

impl TryFrom<ProcurementResponsibilityRuleData> for NormalizedRuleData {
    type Error = Error;

    /// 校验并规范化规则输入。
    ///
    /// # 参数
    /// * `data` - 未规范化规则数据
    ///
    /// # 返回
    /// 返回带稳定选择器键的合法数据。
    ///
    /// # 错误
    /// 选择器字段组合与规则类型不一致时返回错误。
    fn try_from(data: ProcurementResponsibilityRuleData) -> Result<Self> {
        let service_region = normalize_service_region(data.service_region.clone())?;
        ensure_selector_shape(&data, service_region.as_deref())?;
        let selector_key = selector_key(&data, service_region.as_deref());
        let owner_user_id = normalize_required_text(
            data.owner_user_id,
            "采购负责人不能为空",
            OWNER_MAX_LEN,
            "采购负责人过长",
        )?;
        Ok(Self {
            rule_type: data.rule_type,
            sku_id: data.sku_id,
            category_id: data.category_id,
            service_region,
            product_kind: data.product_kind,
            owner_user_id,
            status: data.status,
            selector_key,
        })
    }
}

/// 规范化服务区域代码。
///
/// # 参数
/// * `value` - 可选区域原值
///
/// # 返回
/// 空白转 `None`，其他值 trim 后转 ASCII 大写。
///
/// # 错误
/// 区域超过长度上限时返回错误。
pub fn normalize_service_region(value: Option<String>) -> Result<Option<String>> {
    Ok(
        normalize_optional_text(value, "服务区域", SERVICE_REGION_MAX_LEN)?
            .map(|region| region.to_ascii_uppercase()),
    )
}

/// 校验规则类型与选择器字段严格一一对应。
///
/// # 参数
/// * `data` - 原始规则数据
/// * `service_region` - 已规范化的可选服务区域
///
/// # 返回
/// 字段组合与规则类型一致时返回 `Ok(())`。
///
/// # 错误
/// 任一多余或缺失选择器字段都会返回领域错误。
fn ensure_selector_shape(
    data: &ProcurementResponsibilityRuleData,
    service_region: Option<&str>,
) -> Result<()> {
    let valid = match data.rule_type {
        ProcurementResponsibilityRuleType::Sku => {
            data.sku_id.is_some()
                && data.category_id.is_none()
                && service_region.is_none()
                && data.product_kind.is_none()
        }
        ProcurementResponsibilityRuleType::CategoryServiceRegion => {
            data.sku_id.is_none()
                && data.category_id.is_some()
                && service_region.is_some()
                && data.product_kind.is_none()
        }
        ProcurementResponsibilityRuleType::Category => {
            data.sku_id.is_none()
                && data.category_id.is_some()
                && service_region.is_none()
                && data.product_kind.is_none()
        }
        ProcurementResponsibilityRuleType::ProductKind => {
            data.sku_id.is_none()
                && data.category_id.is_none()
                && service_region.is_none()
                && data.product_kind.is_some()
        }
        ProcurementResponsibilityRuleType::DefaultDispatcher => {
            data.sku_id.is_none()
                && data.category_id.is_none()
                && service_region.is_none()
                && data.product_kind.is_none()
        }
    };
    if !valid {
        return Err(Error::from("采购责任规则类型与选择器字段不一致"));
    }
    Ok(())
}

/// 构造启用规则唯一索引使用的稳定选择器键。
///
/// # 参数
/// * `data` - 已通过形状校验的规则数据
/// * `service_region` - 已规范化的可选服务区域
///
/// # 返回
/// 返回不包含负责人且唯一表示规则层与选择器的字符串。
///
/// # 错误
/// 无；仅允许在选择器形状校验通过后调用。
fn selector_key(data: &ProcurementResponsibilityRuleData, service_region: Option<&str>) -> String {
    match data.rule_type {
        ProcurementResponsibilityRuleType::Sku => {
            format!("sku:{}", data.sku_id.as_ref().expect("形状已校验"))
        }
        ProcurementResponsibilityRuleType::CategoryServiceRegion => format!(
            "category_region:{}:{}",
            data.category_id.as_ref().expect("形状已校验"),
            service_region.expect("形状已校验")
        ),
        ProcurementResponsibilityRuleType::Category => {
            format!("category:{}", data.category_id.as_ref().expect("形状已校验"))
        }
        ProcurementResponsibilityRuleType::ProductKind => {
            format!("product_kind:{}", data.product_kind.expect("形状已校验").as_str())
        }
        ProcurementResponsibilityRuleType::DefaultDispatcher => "default_dispatcher".to_string(),
    }
}

/// 规范化维护操作人身份。
///
/// # 参数
/// * `value` - 已认证操作人账号 ID
///
/// # 返回
/// 返回去除首尾空白的非空账号 ID。
///
/// # 错误
/// 操作人为空或超过长度上限时返回错误。
fn normalize_actor(value: String) -> Result<String> {
    normalize_required_text(value, "操作人不能为空", ACTOR_MAX_LEN, "操作人过长")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_normalizes_selector_and_rejects_mixed_shapes() {
        let region = ProcurementResponsibilityRule::new(
            ProcurementResponsibilityRuleId::new("r-1"),
            ProcurementResponsibilityRuleData {
                rule_type: ProcurementResponsibilityRuleType::CategoryServiceRegion,
                sku_id: None,
                category_id: Some(ProductCategoryId::new("cat-1")),
                service_region: Some(" north ".to_string()),
                product_kind: None,
                owner_user_id: " buyer-1 ".to_string(),
                status: EnableStatus::Active,
            },
            " admin-1 ",
        )
        .unwrap();
        assert_eq!(region.service_region.as_deref(), Some("NORTH"));
        assert_eq!(region.selector_key, "category_region:cat-1:NORTH");
        assert_eq!(region.owner_user_id, "buyer-1");
        assert_eq!(region.created_by, "admin-1");

        let invalid = ProcurementResponsibilityRuleData {
            rule_type: ProcurementResponsibilityRuleType::Sku,
            sku_id: Some(SkuId::new("sku-1")),
            category_id: Some(ProductCategoryId::new("cat-1")),
            service_region: None,
            product_kind: None,
            owner_user_id: "buyer-1".to_string(),
            status: EnableStatus::Active,
        };
        assert!(ProcurementResponsibilityRule::new(
            ProcurementResponsibilityRuleId::new("r-2"),
            invalid,
            "admin-1"
        )
        .is_err());
    }
}
