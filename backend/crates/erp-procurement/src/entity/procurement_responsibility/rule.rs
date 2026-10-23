//! 采购责任规则实体、选择器形状与规范化。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::assignment::{AssignmentStrategy, normalize_pool};
use erp_core::ids::{ProcurementResponsibilityRuleId, ProductCategoryId, SkuId};
use erp_core::validation::normalize_required_text;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::status::EnableStatus;
use crate::entity::facts::ProductKind;

const ACTOR_MAX_LEN: usize = 128;

/// 采购责任规则类型，枚举顺序不表示优先级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProcurementResponsibilityRuleType {
    /// 询价任务整单优先匹配。
    Inquiry,
    /// 精确 SKU。
    Sku,
    /// 分类；解析时允许沿父分类逐级回退。
    Category,
    /// 商品业务类型。
    ProductKind,
    /// 通用兜底负责人池。
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
            Self::Inquiry => "INQUIRY",
            Self::Sku => "SKU",
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
    /// 返回 0 至 4；询价规则仅在询价场景以最高优先级匹配。
    ///
    /// # 错误
    /// 无。
    pub fn priority(self) -> u8 {
        match self {
            Self::Inquiry => 0,
            Self::Sku => 1,
            Self::Category => 2,
            Self::ProductKind => 3,
            Self::DefaultDispatcher => 4,
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
    /// 分类选择器；仅分类规则填写。
    pub category_id: Option<ProductCategoryId>,
    /// 商品业务类型；仅 ProductKind 规则填写。
    pub product_kind: Option<ProductKind>,
    /// 按轮询顺序排列的采购负责人账号。
    pub owner_user_ids: Vec<String>,
    /// 负责人池分配策略。
    pub strategy: AssignmentStrategy,
    /// 启停状态。
    pub status: EnableStatus,
}

/// 采购责任选择器需要存在性校验的目录引用。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcurementResponsibilitySelectorReference<'a> {
    /// 精确 SKU 引用。
    Sku(&'a SkuId),
    /// 商品分类引用。
    Category(&'a ProductCategoryId),
    /// 该规则类型不引用目录实体。
    None,
}

impl ProcurementResponsibilityRuleData {
    /// 返回当前规则选择器需要校验的目录引用。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// SKU 规则返回 SKU，分类规则返回分类，其余规则返回 `None`。
    ///
    /// # 错误
    /// 规则类型与选择器字段形状不一致时返回错误。
    ///
    /// # Panics
    /// 形状校验已通过后对应选择器字段仍缺失时 panic。
    pub fn selector_reference(&self) -> Result<ProcurementResponsibilitySelectorReference<'_>> {
        ensure_selector_shape(self)?;
        Ok(match self.rule_type {
            ProcurementResponsibilityRuleType::Sku => ProcurementResponsibilitySelectorReference::Sku(
                self.sku_id.as_ref().expect("选择器形状已校验"),
            ),
            ProcurementResponsibilityRuleType::Category => {
                ProcurementResponsibilitySelectorReference::Category(
                    self.category_id.as_ref().expect("选择器形状已校验"),
                )
            },
            ProcurementResponsibilityRuleType::ProductKind
            | ProcurementResponsibilityRuleType::DefaultDispatcher
            | ProcurementResponsibilityRuleType::Inquiry => ProcurementResponsibilitySelectorReference::None,
        })
    }
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
    /// 商品业务类型选择器。
    pub product_kind: Option<ProductKind>,
    /// 按轮询顺序排列的采购负责人账号。
    pub owner_user_ids: Vec<String>,
    /// 负责人池分配策略。
    pub strategy: AssignmentStrategy,
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
    /// 选择器与规则类型不一致、账号为空或过长时返回错误。
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
            product_kind: normalized.product_kind,
            owner_user_ids: normalized.owner_user_ids,
            strategy: normalized.strategy,
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
    /// 选择器形状、负责人或操作人不合法时返回错误。
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
        self.product_kind = normalized.product_kind;
        self.owner_user_ids = normalized.owner_user_ids;
        self.strategy = normalized.strategy;
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
    product_kind: Option<ProductKind>,
    owner_user_ids: Vec<String>,
    strategy: AssignmentStrategy,
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
        ensure_selector_shape(&data)?;
        let selector_key = selector_key(&data);
        let owner_user_ids = normalize_pool(data.owner_user_ids)?;
        Ok(Self {
            rule_type: data.rule_type,
            sku_id: data.sku_id,
            category_id: data.category_id,
            product_kind: data.product_kind,
            owner_user_ids,
            strategy: data.strategy,
            status: data.status,
            selector_key,
        })
    }
}

/// 校验规则类型与选择器字段严格一一对应。
///
/// # 参数
/// * `data` - 原始规则数据
///
/// # 返回
/// 字段组合与规则类型一致时返回 `Ok(())`。
///
/// # 错误
/// 任一多余或缺失选择器字段都会返回领域错误。
fn ensure_selector_shape(data: &ProcurementResponsibilityRuleData) -> Result<()> {
    let valid = match data.rule_type {
        ProcurementResponsibilityRuleType::Sku => {
            data.sku_id.is_some() && data.category_id.is_none() && data.product_kind.is_none()
        },
        ProcurementResponsibilityRuleType::Category => {
            data.sku_id.is_none() && data.category_id.is_some() && data.product_kind.is_none()
        },
        ProcurementResponsibilityRuleType::ProductKind => {
            data.sku_id.is_none() && data.category_id.is_none() && data.product_kind.is_some()
        },
        ProcurementResponsibilityRuleType::DefaultDispatcher | ProcurementResponsibilityRuleType::Inquiry => {
            data.sku_id.is_none() && data.category_id.is_none() && data.product_kind.is_none()
        },
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
///
/// # 返回
/// 每个匹配条件唯一对应一个负责人池，人员不参与选择器键。
///
/// # 错误
/// 不返回错误。仅允许在选择器形状校验通过后调用。
///
/// # Panics
/// 形状未校验却缺少对应选择器字段时 panic。
fn selector_key(data: &ProcurementResponsibilityRuleData) -> String {
    match data.rule_type {
        ProcurementResponsibilityRuleType::Sku => {
            format!("sku:{}", data.sku_id.as_ref().expect("形状已校验"))
        },
        ProcurementResponsibilityRuleType::Category => {
            format!("category:{}", data.category_id.as_ref().expect("形状已校验"))
        },
        ProcurementResponsibilityRuleType::ProductKind => {
            format!("product_kind:{}", data.product_kind.expect("形状已校验").as_str())
        },
        ProcurementResponsibilityRuleType::DefaultDispatcher => "default_dispatcher".into(),
        ProcurementResponsibilityRuleType::Inquiry => "inquiry".into(),
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
    use crate::entity::procurement_responsibility::EnableStatus;

    #[test]
    fn entity_normalizes_selector_and_rejects_mixed_shapes() {
        let category = ProcurementResponsibilityRule::new(
            ProcurementResponsibilityRuleId::new("r-1"),
            ProcurementResponsibilityRuleData {
                rule_type: ProcurementResponsibilityRuleType::Category,
                sku_id: None,
                category_id: Some(ProductCategoryId::new("cat-1")),
                product_kind: None,
                owner_user_ids: vec![" buyer-1 ".to_string()],
                strategy: Default::default(),
                status: EnableStatus::Active,
            },
            " admin-1 ",
        )
        .unwrap();
        assert_eq!(category.selector_key, "category:cat-1");
        assert_eq!(category.owner_user_ids, ["buyer-1"]);
        assert_eq!(category.created_by, "admin-1");

        let invalid = ProcurementResponsibilityRuleData {
            rule_type: ProcurementResponsibilityRuleType::Sku,
            sku_id: Some(SkuId::new("sku-1")),
            category_id: Some(ProductCategoryId::new("cat-1")),
            product_kind: None,
            owner_user_ids: vec!["buyer-1".to_string()],
            strategy: Default::default(),
            status: EnableStatus::Active,
        };
        assert!(
            ProcurementResponsibilityRule::new(
                ProcurementResponsibilityRuleId::new("r-2"),
                invalid,
                "admin-1"
            )
            .is_err()
        );
    }

    #[test]
    fn selector_reference_exposes_only_shape_validated_catalog_identity() {
        let sku = ProcurementResponsibilityRuleData {
            rule_type: ProcurementResponsibilityRuleType::Sku,
            sku_id: Some(SkuId::new("sku-1")),
            category_id: None,
            product_kind: None,
            owner_user_ids: vec!["buyer-1".to_string()],
            strategy: Default::default(),
            status: EnableStatus::Active,
        };
        assert!(matches!(
            sku.selector_reference().unwrap(),
            ProcurementResponsibilitySelectorReference::Sku(id) if id.as_ref() == "sku-1"
        ));

        let invalid =
            ProcurementResponsibilityRuleData { category_id: Some(ProductCategoryId::new("cat-1")), ..sku };
        assert!(invalid.selector_reference().is_err());
    }
    #[test]
    fn default_selector_identifies_pool_not_member() {
        let data = |owner: &str| ProcurementResponsibilityRuleData {
            rule_type: ProcurementResponsibilityRuleType::DefaultDispatcher,
            sku_id: None,
            category_id: None,
            product_kind: None,
            owner_user_ids: vec![owner.into()],
            strategy: Default::default(),
            status: EnableStatus::Active,
        };
        let first = ProcurementResponsibilityRule::new(
            ProcurementResponsibilityRuleId::new("a"),
            data(" buyer-1 "),
            "admin",
        )
        .unwrap();
        let second = ProcurementResponsibilityRule::new(
            ProcurementResponsibilityRuleId::new("b"),
            data("buyer-2"),
            "admin",
        )
        .unwrap();
        let same = ProcurementResponsibilityRule::new(
            ProcurementResponsibilityRuleId::new("c"),
            data("buyer-1"),
            "admin",
        )
        .unwrap();
        assert_eq!(first.selector_key, second.selector_key);
        assert_eq!(first.selector_key, same.selector_key);
        assert_eq!(first.owner_user_ids, ["buyer-1"]);
    }
}
