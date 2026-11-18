//! `product_category` 商品分类（数据模型 §6.3，树形字典）。
//!
//! W14 以树形维护页管理分类：父子关系不得成环、停用后仍可被历史 SKU 修订引用。
//! 本实体只保证单节点不变式（父分类不得为自身）；成环检测需要整棵树，
//! 属跨聚合校验，留 P3（注释标注数据模型 §6.3 必需约束条目）。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::stable::StableBase;
use erp_core::ids::ProductCategoryId;
use erp_core::validation::normalize_required_text;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use crate::entity::catalog::product_kind::ProductKind;
use crate::entity::catalog::status::EnableStatus;

/// 分类代码最大长度。
const CODE_MAX_LEN: usize = 64;
/// 分类名称最大长度。
const NAME_MAX_LEN: usize = 128;

/// 商品分类创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductCategoryData {
    /// 稳定分类代码（唯一，创建后不可修改）。
    pub category_code: String,
    /// 父分类；空表示根分类。
    pub parent_category_id: Option<ProductCategoryId>,
    /// 分类名称。
    pub name: String,
    /// 分类允许的商品类型；只用于兼容性校验和筛选。
    pub product_kind: ProductKind,
    /// 启停状态。
    pub status: EnableStatus,
}

/// 商品分类更新数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ProductCategoryUpdate {
    /// 分类名称；`None` 表示不修改。
    pub name: Option<String>,
    /// 分类允许的商品类型；`None` 表示不修改。
    pub product_kind: Option<ProductKind>,
    /// 启停状态；`None` 表示不修改。
    pub status: Option<EnableStatus>,
}

/// 商品分类实体（稳定基础资料，数据模型 §6.3）。
///
/// `StableBase` 是 P0 冻结基元且未派生 `PartialEq`，因此本实体手工实现
/// `PartialEq`/`Eq`（全字段语义相等）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity)]
pub struct ProductCategory {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub stable: StableBase<EnableStatus>,
    /// 稳定分类代码（创建后不可修改）。
    pub category_code: String,
    /// 父分类；空表示根分类。
    pub parent_category_id: Option<ProductCategoryId>,
    /// 分类名称。
    pub name: String,
    /// 分类允许的商品类型；只用于兼容性校验和筛选。
    pub product_kind: ProductKind,
}

impl PartialEq for ProductCategory {
    // 逐字段比较商品分类，仅持有相同标识不视为内容相等。
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.stable.status == other.stable.status
            && self.stable.current_revision_id == other.stable.current_revision_id
            && self.stable.created_by == other.stable.created_by
            && self.stable.updated_by == other.stable.updated_by
            && self.category_code == other.category_code
            && self.parent_category_id == other.parent_category_id
            && self.name == other.name
            && self.product_kind == other.product_kind
    }
}

impl Eq for ProductCategory {}

impl ProductCategory {
    /// 创建商品分类。
    ///
    /// 完成 category_code/name 的校验与规范化（去首尾空白、非空、长度上限），并拒绝「父分类为自身」的环。
    ///
    /// 当 category_code/name 为空、超长，或父分类为自身时返回错误。
    ///
    /// - `id`：实体主键（`erp_core::ids::ProductCategoryId`）。
    /// - `created_by`：创建人（账号或系统身份）。
    pub fn new(
        id: ProductCategoryId,
        data: ProductCategoryData,
        created_by: impl Into<String>,
    ) -> Result<Self> {
        let category_code =
            normalize_required_text(data.category_code, "分类代码不能为空", CODE_MAX_LEN, "分类代码过长")?;
        let name = normalize_required_text(data.name, "分类名称不能为空", NAME_MAX_LEN, "分类名称过长")?;
        if data.parent_category_id.as_ref() == Some(&id) {
            return Err(Error::from("商品分类不能选自己作为上级分类，请另选上级分类"));
        }

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            stable: StableBase::new(data.status, created_by),
            category_code,
            parent_category_id: data.parent_category_id,
            name,
            product_kind: data.product_kind,
        })
    }

    /// 更新商品分类。
    ///
    /// 复用 `new` 的校验规则；`category_code` 是稳定代码，不允许在通用更新中修改。
    ///
    /// 当更新字段校验失败时返回错误。
    pub fn update(&mut self, update: ProductCategoryUpdate, updated_by: impl Into<String>) -> Result<()> {
        self.apply_name(update.name)?;
        if let Some(product_kind) = update.product_kind {
            self.product_kind = product_kind;
        }
        self.apply_status(update.status);
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 移动分类到新的父分类。
    ///
    /// 只拒绝「父分类为自身」；跨节点的成环检测需要整棵分类树，由 P3 服务层在事务内完成（数据模型 §6.3：分类父子关系不得形成环）。
    ///
    /// 当父分类为自身时返回错误。
    ///
    /// `parent` 为新父分类；`None` 表示提升为根分类。
    pub fn set_parent(
        &mut self,
        parent: Option<ProductCategoryId>,
        updated_by: impl Into<String>,
    ) -> Result<()> {
        if parent.as_ref().is_some_and(|parent_id| parent_id.as_ref() == self.base.id.as_str()) {
            return Err(Error::from("商品分类不能选自己作为上级分类，请另选上级分类"));
        }
        self.parent_category_id = parent;
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 判断是否为根分类。没有父分类时返回 `true`。
    pub fn is_root(&self) -> bool {
        self.parent_category_id.is_none()
    }

    /// 读取商品分类的启用标记，供调用方过滤停用记录。
    pub fn is_active(&self) -> bool {
        self.stable.status().is_active()
    }

    // 应用名称更新。当名称为空或超长时返回错误。
    //
    // `name` 为可选名称。
    fn apply_name(&mut self, name: Option<String>) -> Result<()> {
        if let Some(name) = name {
            self.name = normalize_required_text(name, "分类名称不能为空", NAME_MAX_LEN, "分类名称过长")?;
        }
        Ok(())
    }

    // 应用状态更新。
    //
    // `status` 为可选状态。
    fn apply_status(&mut self, status: Option<EnableStatus>) {
        if let Some(status) = status {
            self.stable.status = status;
        }
    }
}

#[cfg(test)]
mod tests {
    use erp_core::common::state::{assert_adjacency_closed, ensure_transition};
    use erp_core::ids::ProductCategoryId;

    use super::*;

    fn data() -> ProductCategoryData {
        ProductCategoryData {
            category_code: " CAT-001 ".to_string(),
            parent_category_id: None,
            name: " 食品分类 ".to_string(),
            product_kind: ProductKind::Physical,
            status: EnableStatus::Active,
        }
    }

    // 字段 去除首尾空白、根分类、商品类型与状态按请求保存。
    #[test]
    fn new_trims_and_normalizes_fields() {
        let category = ProductCategory::new(ProductCategoryId::new("cat-1"), data(), "admin-1").unwrap();

        assert_eq!(category.category_code, "CAT-001");
        assert_eq!(category.name, "食品分类");
        assert!(category.is_root());
        assert_eq!(category.product_kind, ProductKind::Physical);
        assert_eq!(category.stable.status(), EnableStatus::Active);
        assert!(category.is_active());
        assert_eq!(category.stable.created_by, "admin-1");
    }

    // 商品分类的必填字段为空或超长时均应拒绝创建。
    #[test]
    fn new_rejects_empty_and_overlong_fields() {
        let empty_code = ProductCategoryData { category_code: "   ".to_string(), ..data() };
        assert!(ProductCategory::new(ProductCategoryId::new("cat-1"), empty_code, "admin-1").is_err());

        let overlong_name = ProductCategoryData { name: "n".repeat(129), ..data() };
        assert!(ProductCategory::new(ProductCategoryId::new("cat-1"), overlong_name, "admin-1").is_err());
    }

    // 父分类为自身（单节点可判定的环）被拒绝。
    #[test]
    fn new_rejects_self_parent() {
        let self_parent =
            ProductCategoryData { parent_category_id: Some(ProductCategoryId::new("cat-1")), ..data() };
        assert!(ProductCategory::new(ProductCategoryId::new("cat-1"), self_parent, "admin-1").is_err());
    }

    // 更新时修改名称/商品类型/状态并更新审计人；稳定代码不可修改。
    #[test]
    fn update_applies_fields_and_touches_auditor() {
        let mut category = ProductCategory::new(ProductCategoryId::new("cat-1"), data(), "admin-1").unwrap();

        category
            .update(
                ProductCategoryUpdate {
                    name: Some(" 新名称 ".to_string()),
                    product_kind: Some(ProductKind::Voucher),
                    status: Some(EnableStatus::Disabled),
                },
                "admin-2",
            )
            .unwrap();

        assert_eq!(category.name, "新名称");
        assert_eq!(category.product_kind, ProductKind::Voucher);
        assert!(!category.is_active());
        assert_eq!(category.stable.updated_by, "admin-2");
        assert_eq!(category.category_code, "CAT-001");
    }

    // 锁定商品分类启停的正向、反向与幂等迁移，防止状态表缺边。
    #[test]
    fn status_transitions_follow_document_state() {
        assert!(ensure_transition(EnableStatus::Active, EnableStatus::Disabled).is_ok());
        assert!(ensure_transition(EnableStatus::Disabled, EnableStatus::Active).is_ok());
        assert_adjacency_closed(&[EnableStatus::Active, EnableStatus::Disabled]);
    }
}
