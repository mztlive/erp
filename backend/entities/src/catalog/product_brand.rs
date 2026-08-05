//! `product_brand` 商品品牌（数据模型 §6.3，稳定字典）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::catalog::status::EnableStatus;
use crate::common::stable::StableBase;
use crate::errors::Result;
use crate::ids::ProductBrandId;
use crate::validation::normalize_required_text;

/// 品牌代码最大长度。
const CODE_MAX_LEN: usize = 64;
/// 品牌名称最大长度。
const NAME_MAX_LEN: usize = 128;

/// 商品品牌创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductBrandData {
    /// 稳定品牌代码（唯一，创建后不可修改）。
    pub brand_code: String,
    /// 品牌名称。
    pub name: String,
    /// 启停状态。
    pub status: EnableStatus,
}

/// 商品品牌更新数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ProductBrandUpdate {
    /// 品牌名称；`None` 表示不修改。
    pub name: Option<String>,
    /// 启停状态；`None` 表示不修改。
    pub status: Option<EnableStatus>,
}

/// 商品品牌实体（稳定基础资料，数据模型 §6.3）。
///
/// `StableBase` 是 P0 冻结基元且未派生 `PartialEq`，因此本实体手工实现
/// `PartialEq`/`Eq`（全字段语义相等）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity)]
pub struct ProductBrand {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub stable: StableBase<EnableStatus>,
    /// 稳定品牌代码（创建后不可修改）。
    pub brand_code: String,
    /// 品牌名称。
    pub name: String,
}

impl PartialEq for ProductBrand {
    /// 全字段语义相等。
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.stable.status == other.stable.status
            && self.stable.current_revision_id == other.stable.current_revision_id
            && self.stable.created_by == other.stable.created_by
            && self.stable.updated_by == other.stable.updated_by
            && self.brand_code == other.brand_code
            && self.name == other.name
    }
}

impl Eq for ProductBrand {}

impl ProductBrand {
    /// 创建商品品牌。
    ///
    /// 完成 brand_code/name 的校验与规范化（去首尾空白、非空、长度上限）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::ProductBrandId`）
    /// * `data` - 创建数据
    /// * `created_by` - 创建人（账号或系统身份）
    ///
    /// # 返回
    /// 返回新建的品牌实体。
    ///
    /// # 错误
    /// 当 brand_code/name 为空或超长时返回错误。
    pub fn new(id: ProductBrandId, data: ProductBrandData, created_by: impl Into<String>) -> Result<Self> {
        let brand_code =
            normalize_required_text(data.brand_code, "品牌代码不能为空", CODE_MAX_LEN, "品牌代码过长")?;
        let name = normalize_required_text(data.name, "品牌名称不能为空", NAME_MAX_LEN, "品牌名称过长")?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            stable: StableBase::new(data.status, created_by),
            brand_code,
            name,
        })
    }

    /// 更新商品品牌。
    ///
    /// 复用 `new` 的校验规则；`brand_code` 是稳定代码，不允许在通用更新中修改。
    ///
    /// # 参数
    /// * `update` - 更新数据
    /// * `updated_by` - 本次更新执行人
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当更新字段校验失败时返回错误。
    pub fn update(&mut self, update: ProductBrandUpdate, updated_by: impl Into<String>) -> Result<()> {
        if let Some(name) = update.name {
            self.name = normalize_required_text(name, "品牌名称不能为空", NAME_MAX_LEN, "品牌名称过长")?;
        }
        if let Some(status) = update.status {
            self.stable.status = status;
        }
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 判断品牌是否处于启用状态。
    ///
    /// # 返回
    /// 状态为 `Active` 时返回 `true`。
    pub fn is_active(&self) -> bool {
        self.stable.status().is_active()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::state::{assert_adjacency_closed, ensure_transition};
    use crate::ids::ProductBrandId;

    fn data() -> ProductBrandData {
        ProductBrandData {
            brand_code: " BR-001 ".to_string(),
            name: " 山姆自营 ".to_string(),
            status: EnableStatus::Active,
        }
    }

    /// happy path：字段 trim 规范化，状态与审计人落位。
    #[test]
    fn new_trims_and_normalizes_fields() {
        let brand = ProductBrand::new(ProductBrandId::new("brand-1"), data(), "admin-1").unwrap();

        assert_eq!(brand.brand_code, "BR-001");
        assert_eq!(brand.name, "山姆自营");
        assert_eq!(brand.stable.status(), EnableStatus::Active);
        assert!(brand.is_active());
    }

    /// 失败路径：必填空与超长各一条。
    #[test]
    fn new_rejects_empty_and_overlong_fields() {
        let empty_code = ProductBrandData {
            brand_code: "   ".to_string(),
            ..data()
        };
        assert!(ProductBrand::new(ProductBrandId::new("brand-1"), empty_code, "admin-1").is_err());

        let overlong_name = ProductBrandData {
            name: "n".repeat(129),
            ..data()
        };
        assert!(ProductBrand::new(ProductBrandId::new("brand-1"), overlong_name, "admin-1").is_err());
    }

    /// update 修改名称与状态并 touch 审计人。
    #[test]
    fn update_applies_fields_and_preserves_code() {
        let mut brand = ProductBrand::new(ProductBrandId::new("brand-1"), data(), "admin-1").unwrap();

        brand
            .update(
                ProductBrandUpdate {
                    name: Some(" 新品牌 ".to_string()),
                    status: Some(EnableStatus::Disabled),
                },
                "admin-2",
            )
            .unwrap();

        assert_eq!(brand.name, "新品牌");
        assert!(!brand.is_active());
        assert_eq!(brand.brand_code, "BR-001");
        assert_eq!(brand.stable.updated_by, "admin-2");
    }

    /// 状态机：合法迁移通过，邻接矩阵对称闭合。
    #[test]
    fn status_transitions_follow_document_state() {
        assert!(ensure_transition(EnableStatus::Active, EnableStatus::Disabled).is_ok());
        assert_adjacency_closed(&[EnableStatus::Active, EnableStatus::Disabled]);
    }
}
