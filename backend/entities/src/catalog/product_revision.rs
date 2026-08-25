//! `product_revision` 商品修订（数据模型 §6.3、§4.4，不可变修订）。
//!
//! 正式版本按 §4.4 内联结构化快照字段（商品名称、描述、规格、分类、品牌），
//! P1 定义并校验、P3 填充；`(product_id, revision_no)` 唯一（唯一约束跨行，
//! 属 P3/索引校验）。修订一经形成不得修改，本实体不提供 `update()`。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::catalog::status::EnableStatus;
use crate::common::revision::RevisionBase;
use crate::common::time::BusinessDate;
use crate::errors::{Error, Result};
use crate::ids::{ProductBrandId, ProductCategoryId, ProductId, ProductRevisionId};
use crate::validation::{normalize_optional_text, normalize_required_text};

/// 商品名称最大长度。
const NAME_MAX_LEN: usize = 128;
/// 描述最大长度。
const DESCRIPTION_MAX_LEN: usize = 512;
/// 规格/服务内容最大长度。
const SPECIFICATION_MAX_LEN: usize = 1024;

/// 商品修订创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductRevisionData {
    /// 所属商品 SPU。
    pub product_id: ProductId,
    /// 修订序号（同一商品内从 1 递增）。
    pub revision_no: u32,
    /// 公司审核后的商品名称（结构化快照）。
    pub name: String,
    /// 公司审核后的描述。
    pub description: Option<String>,
    /// 公司审核后的规格或服务内容。
    pub specification: Option<String>,
    /// ERP 分类。
    pub category_id: ProductCategoryId,
    /// ERP 品牌。
    pub brand_id: ProductBrandId,
    /// 修订启停状态。
    pub status: EnableStatus,
    /// 生效开始日。
    pub effective_from: BusinessDate,
    /// 生效结束日；空表示无限期。
    pub effective_to: Option<BusinessDate>,
}

/// 商品修订实体（不可变修订，数据模型 §6.3、§4.4）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct ProductRevision {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub revision: RevisionBase,
    /// 所属商品 SPU。
    pub product_id: ProductId,
    /// 公司审核后的商品名称（结构化快照）。
    pub name: String,
    /// 公司审核后的描述。
    pub description: Option<String>,
    /// 公司审核后的规格或服务内容。
    pub specification: Option<String>,
    /// ERP 分类。
    pub category_id: ProductCategoryId,
    /// ERP 品牌。
    pub brand_id: ProductBrandId,
    /// 修订启停状态。
    pub status: EnableStatus,
    /// 生效开始日。
    pub effective_from: BusinessDate,
    /// 生效结束日；空表示无限期。
    pub effective_to: Option<BusinessDate>,
}

impl ProductRevision {
    /// 创建商品修订。
    ///
    /// 完成 name/description/specification 的校验与规范化（去首尾空白、非空、
    /// 长度上限），校验修订序号从 1 开始、生效区间不倒挂。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::ProductRevisionId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的商品修订实体。
    ///
    /// # 错误
    /// 当 name 为空/超长、revision_no 为 0，或生效区间倒挂时返回错误。
    pub fn new(id: ProductRevisionId, data: ProductRevisionData) -> Result<Self> {
        let name = normalize_required_text(data.name, "商品名称不能为空", NAME_MAX_LEN, "商品名称过长")?;
        let description = normalize_optional_text(data.description, "商品描述", DESCRIPTION_MAX_LEN)?;
        let specification = normalize_optional_text(data.specification, "商品规格", SPECIFICATION_MAX_LEN)?;
        ensure_revision_no(data.revision_no)?;
        ensure_effective_window(data.effective_from, data.effective_to)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            revision: RevisionBase::new(data.revision_no),
            product_id: data.product_id,
            name,
            description,
            specification,
            category_id: data.category_id,
            brand_id: data.brand_id,
            status: data.status,
            effective_from: data.effective_from,
            effective_to: data.effective_to,
        })
    }

    /// 从当前快照派生一份改名或改描述的后继修订。
    ///
    /// 分类、品牌、规格与状态沿用当前不可变快照，只替换展示文案和生效区间。
    ///
    /// # 参数
    /// * `id` - 新修订主键
    /// * `revision_no` - 同一商品内的下一修订序号
    /// * `name` - 新商品名称
    /// * `description` - 新商品描述
    /// * `effective_from` / `effective_to` - 新修订生效区间
    ///
    /// # 返回
    /// 返回经完整实体校验的新商品修订。
    ///
    /// # 错误
    /// 名称、描述、修订序号或生效区间违反实体不变式时返回错误。
    pub fn content_successor(
        &self,
        id: ProductRevisionId,
        revision_no: u32,
        name: String,
        description: Option<String>,
        effective_from: BusinessDate,
        effective_to: Option<BusinessDate>,
    ) -> Result<Self> {
        Self::new(
            id,
            ProductRevisionData {
                product_id: self.product_id.clone(),
                revision_no,
                name,
                description,
                specification: self.specification.clone(),
                category_id: self.category_id.clone(),
                brand_id: self.brand_id.clone(),
                status: self.status,
                effective_from,
                effective_to,
            },
        )
    }

    /// 从当前快照派生一份停用后继修订。
    ///
    /// 名称、描述、规格、分类、品牌与原结束日保持不变，仅把状态切为停用并设置
    /// 新修订的生效开始日。
    ///
    /// # 参数
    /// * `id` - 新修订主键
    /// * `revision_no` - 同一商品内的下一修订序号
    /// * `effective_from` - 停用修订生效开始日
    ///
    /// # 返回
    /// 返回经完整实体校验的停用商品修订。
    ///
    /// # 错误
    /// 修订序号或生效区间违反实体不变式时返回错误。
    pub fn disabled_successor(
        &self,
        id: ProductRevisionId,
        revision_no: u32,
        effective_from: BusinessDate,
    ) -> Result<Self> {
        Self::new(
            id,
            ProductRevisionData {
                product_id: self.product_id.clone(),
                revision_no,
                name: self.name.clone(),
                description: self.description.clone(),
                specification: self.specification.clone(),
                category_id: self.category_id.clone(),
                brand_id: self.brand_id.clone(),
                status: EnableStatus::Disabled,
                effective_from,
                effective_to: self.effective_to,
            },
        )
    }

    /// 判断修订是否处于启用状态。
    ///
    /// # 返回
    /// 状态为 `Active` 时返回 `true`。
    pub fn is_active(&self) -> bool {
        self.status.is_active()
    }
}

/// 校验修订序号从 1 开始。
///
/// # 参数
/// * `revision_no` - 修订序号
///
/// # 返回
/// 大于等于 1 时返回 `Ok(())`。
///
/// # 错误
/// 为 0 时返回错误。
fn ensure_revision_no(revision_no: u32) -> Result<()> {
    if revision_no == 0 {
        return Err(Error::from("修订序号必须从 1 开始"));
    }
    Ok(())
}

/// 校验生效区间不倒挂。
///
/// # 参数
/// * `effective_from` - 生效开始日
/// * `effective_to` - 生效结束日
///
/// # 返回
/// 结束日晚于开始日（或无限期）时返回 `Ok(())`。
///
/// # 错误
/// 结束日早于或等于开始日时返回错误。
fn ensure_effective_window(effective_from: BusinessDate, effective_to: Option<BusinessDate>) -> Result<()> {
    if let Some(effective_to) = effective_to {
        if effective_to <= effective_from {
            return Err(Error::from("生效结束日必须晚于生效开始日"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::state::{assert_adjacency_closed, ensure_transition};
    use crate::ids::ProductRevisionId;

    fn data() -> ProductRevisionData {
        ProductRevisionData {
            product_id: ProductId::new("prod-1"),
            revision_no: 1,
            name: " 公司认证坚果礼盒 ".to_string(),
            description: Some(" 春节礼盒 ".to_string()),
            specification: None,
            category_id: ProductCategoryId::new("cat-1"),
            brand_id: ProductBrandId::new("brand-1"),
            status: EnableStatus::Active,
            effective_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            effective_to: None,
        }
    }

    /// happy path：快照字段 trim 规范化，修订序号与生效区间落位。
    #[test]
    fn new_trims_and_normalizes_fields() {
        let revision = ProductRevision::new(ProductRevisionId::new("rev-1"), data()).unwrap();

        assert_eq!(revision.name, "公司认证坚果礼盒");
        assert_eq!(revision.description.as_deref(), Some("春节礼盒"));
        assert_eq!(revision.revision.revision_no, 1);
        assert_eq!(revision.category_id, ProductCategoryId::new("cat-1"));
        assert!(revision.is_active());
    }

    /// 失败路径：必填空、超长各一条。
    #[test]
    fn new_rejects_empty_and_overlong_name() {
        let empty = ProductRevisionData {
            name: "  ".to_string(),
            ..data()
        };
        assert!(ProductRevision::new(ProductRevisionId::new("rev-1"), empty).is_err());

        let overlong = ProductRevisionData {
            name: "n".repeat(129),
            ..data()
        };
        assert!(ProductRevision::new(ProductRevisionId::new("rev-1"), overlong).is_err());
    }

    /// 失败路径：越界（修订序号为 0）与关联不一致（生效区间倒挂）各一条。
    #[test]
    fn new_rejects_zero_revision_no_and_reversed_window() {
        let zero_revision = ProductRevisionData {
            revision_no: 0,
            ..data()
        };
        assert!(ProductRevision::new(ProductRevisionId::new("rev-1"), zero_revision).is_err());

        let reversed = ProductRevisionData {
            effective_from: BusinessDate::from_ymd(2026, 3, 1).unwrap(),
            effective_to: Some(BusinessDate::from_ymd(2026, 2, 1).unwrap()),
            ..data()
        };
        assert!(ProductRevision::new(ProductRevisionId::new("rev-1"), reversed).is_err());

        let equal_window = ProductRevisionData {
            effective_to: Some(BusinessDate::from_ymd(2026, 1, 1).unwrap()),
            ..data()
        };
        assert!(ProductRevision::new(ProductRevisionId::new("rev-1"), equal_window).is_err());
    }

    /// 后继修订只替换允许变化的内容并保留稳定快照字段。
    #[test]
    fn content_successor_preserves_stable_snapshot_fields() {
        let current = ProductRevision::new(ProductRevisionId::new("rev-1"), data()).unwrap();
        let successor = current
            .content_successor(
                ProductRevisionId::new("rev-2"),
                2,
                "新名称".to_string(),
                Some("新描述".to_string()),
                BusinessDate::from_ymd(2026, 2, 1).unwrap(),
                None,
            )
            .unwrap();

        assert_eq!(successor.revision.revision_no, 2);
        assert_eq!(successor.name, "新名称");
        assert_eq!(successor.category_id, current.category_id);
        assert_eq!(successor.brand_id, current.brand_id);
        assert_eq!(successor.status, current.status);
    }

    /// 停用后继修订保留当前快照并只切换状态。
    #[test]
    fn disabled_successor_preserves_content_and_disables_status() {
        let current = ProductRevision::new(ProductRevisionId::new("rev-1"), data()).unwrap();
        let successor = current
            .disabled_successor(
                ProductRevisionId::new("rev-2"),
                2,
                BusinessDate::from_ymd(2026, 2, 1).unwrap(),
            )
            .unwrap();

        assert_eq!(successor.name, current.name);
        assert_eq!(successor.description, current.description);
        assert_eq!(successor.status, EnableStatus::Disabled);
        assert_eq!(successor.revision.revision_no, 2);
    }

    /// 状态机：合法迁移通过，邻接矩阵对称闭合。
    #[test]
    fn status_transitions_follow_document_state() {
        assert!(ensure_transition(EnableStatus::Active, EnableStatus::Disabled).is_ok());
        assert!(ensure_transition(EnableStatus::Disabled, EnableStatus::Active).is_ok());
        assert_adjacency_closed(&[EnableStatus::Active, EnableStatus::Disabled]);
    }

    /// 实体 JSON 往返。
    #[test]
    fn revision_roundtrips_through_json() {
        let revision = ProductRevision::new(ProductRevisionId::new("rev-1"), data()).unwrap();
        let json = serde_json::to_string(&revision).unwrap();
        let back: ProductRevision = serde_json::from_str(&json).unwrap();
        assert_eq!(back, revision);
    }
}
