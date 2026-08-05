//! `sku_attribute_value` 规格属性值字典（数据模型 §6.3，稳定字典）。
//!
//! 同一属性下 `value_code` 唯一（唯一约束跨行，属 P3/索引校验）；
//! `sort_order` 仅影响展示，不参与身份。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::catalog::status::EnableStatus;
use crate::common::stable::StableBase;
use crate::errors::{Error, Result};
use crate::ids::{SkuAttributeId, SkuAttributeValueId};
use crate::validation::normalize_required_text;

/// 属性值代码最大长度。
const CODE_MAX_LEN: usize = 64;
/// 展示值最大长度。
const DISPLAY_VALUE_MAX_LEN: usize = 128;

/// 规格属性值创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkuAttributeValueData {
    /// 所属规格属性。
    pub attribute_id: SkuAttributeId,
    /// 稳定属性值代码（同一属性下唯一，创建后不可修改）。
    pub value_code: String,
    /// 展示值。
    pub display_value: String,
    /// 展示排序（仅影响展示，不参与身份）。
    pub sort_order: i32,
    /// 启停状态。
    pub status: EnableStatus,
}

/// 规格属性值更新数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SkuAttributeValueUpdate {
    /// 展示值；`None` 表示不修改。
    pub display_value: Option<String>,
    /// 展示排序；`None` 表示不修改。
    pub sort_order: Option<i32>,
    /// 启停状态；`None` 表示不修改。
    pub status: Option<EnableStatus>,
}

/// 规格属性值实体（稳定基础资料，数据模型 §6.3）。
///
/// `StableBase` 是 P0 冻结基元且未派生 `PartialEq`，因此本实体手工实现
/// `PartialEq`/`Eq`（全字段语义相等）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity)]
pub struct SkuAttributeValue {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub stable: StableBase<EnableStatus>,
    /// 所属规格属性。
    pub attribute_id: SkuAttributeId,
    /// 稳定属性值代码（创建后不可修改）。
    pub value_code: String,
    /// 展示值。
    pub display_value: String,
    /// 展示排序（仅影响展示，不参与身份）。
    pub sort_order: i32,
}

impl PartialEq for SkuAttributeValue {
    /// 全字段语义相等。
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.stable.status == other.stable.status
            && self.stable.current_revision_id == other.stable.current_revision_id
            && self.stable.created_by == other.stable.created_by
            && self.stable.updated_by == other.stable.updated_by
            && self.attribute_id == other.attribute_id
            && self.value_code == other.value_code
            && self.display_value == other.display_value
            && self.sort_order == other.sort_order
    }
}

impl Eq for SkuAttributeValue {}

impl SkuAttributeValue {
    /// 创建规格属性值。
    ///
    /// 完成 value_code/display_value 的校验与规范化（去首尾空白、非空、长度上限），
    /// 并要求 `sort_order` 为非负整数。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SkuAttributeValueId`）
    /// * `data` - 创建数据
    /// * `created_by` - 创建人（账号或系统身份）
    ///
    /// # 返回
    /// 返回新建的属性值实体。
    ///
    /// # 错误
    /// 当 value_code/display_value 为空、超长，或 sort_order 为负数时返回错误。
    pub fn new(
        id: SkuAttributeValueId,
        data: SkuAttributeValueData,
        created_by: impl Into<String>,
    ) -> Result<Self> {
        let value_code = normalize_required_text(
            data.value_code,
            "属性值代码不能为空",
            CODE_MAX_LEN,
            "属性值代码过长",
        )?;
        let display_value = normalize_required_text(
            data.display_value,
            "展示值不能为空",
            DISPLAY_VALUE_MAX_LEN,
            "展示值过长",
        )?;
        ensure_non_negative_sort_order(data.sort_order)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            stable: StableBase::new(data.status, created_by),
            attribute_id: data.attribute_id,
            value_code,
            display_value,
            sort_order: data.sort_order,
        })
    }

    /// 更新规格属性值。
    ///
    /// 复用 `new` 的校验规则；`value_code` 与 `attribute_id` 是稳定身份，
    /// 不允许在通用更新中修改。
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
    pub fn update(&mut self, update: SkuAttributeValueUpdate, updated_by: impl Into<String>) -> Result<()> {
        if let Some(display_value) = update.display_value {
            self.display_value = normalize_required_text(
                display_value,
                "展示值不能为空",
                DISPLAY_VALUE_MAX_LEN,
                "展示值过长",
            )?;
        }
        if let Some(sort_order) = update.sort_order {
            ensure_non_negative_sort_order(sort_order)?;
            self.sort_order = sort_order;
        }
        if let Some(status) = update.status {
            self.stable.status = status;
        }
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 判断属性值是否处于启用状态。
    ///
    /// # 返回
    /// 状态为 `Active` 时返回 `true`。
    pub fn is_active(&self) -> bool {
        self.stable.status().is_active()
    }
}

/// 校验展示排序为非负整数。
///
/// # 参数
/// * `sort_order` - 展示排序
///
/// # 返回
/// 非负时返回 `Ok(())`。
///
/// # 错误
/// 为负数时返回错误。
fn ensure_non_negative_sort_order(sort_order: i32) -> Result<()> {
    if sort_order < 0 {
        return Err(Error::from("展示排序不能为负数"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::state::{assert_adjacency_closed, ensure_transition};
    use crate::ids::SkuAttributeId;

    fn data() -> SkuAttributeValueData {
        SkuAttributeValueData {
            attribute_id: SkuAttributeId::new("attr-1"),
            value_code: " L ".to_string(),
            display_value: " 大号 ".to_string(),
            sort_order: 2,
            status: EnableStatus::Active,
        }
    }

    /// happy path：字段 trim 规范化。
    #[test]
    fn new_trims_and_normalizes_fields() {
        let value = SkuAttributeValue::new(SkuAttributeValueId::new("val-1"), data(), "admin-1").unwrap();

        assert_eq!(value.value_code, "L");
        assert_eq!(value.display_value, "大号");
        assert_eq!(value.attribute_id, SkuAttributeId::new("attr-1"));
        assert_eq!(value.sort_order, 2);
        assert!(value.is_active());
    }

    /// 失败路径：必填空与越界（负排序）各一条。
    #[test]
    fn new_rejects_empty_and_negative_sort_order() {
        let empty_code = SkuAttributeValueData {
            value_code: "  ".to_string(),
            ..data()
        };
        assert!(SkuAttributeValue::new(SkuAttributeValueId::new("val-1"), empty_code, "admin-1").is_err());

        let negative_sort = SkuAttributeValueData {
            sort_order: -1,
            ..data()
        };
        assert!(SkuAttributeValue::new(SkuAttributeValueId::new("val-1"), negative_sort, "admin-1").is_err());
    }

    /// update 修改展示值/排序/状态并 touch 审计人；稳定代码与归属属性不可修改。
    #[test]
    fn update_applies_fields_and_preserves_identity() {
        let mut value = SkuAttributeValue::new(SkuAttributeValueId::new("val-1"), data(), "admin-1").unwrap();

        value
            .update(
                SkuAttributeValueUpdate {
                    display_value: Some(" 加大号 ".to_string()),
                    sort_order: Some(3),
                    status: Some(EnableStatus::Disabled),
                },
                "admin-2",
            )
            .unwrap();

        assert_eq!(value.display_value, "加大号");
        assert_eq!(value.sort_order, 3);
        assert!(!value.is_active());
        assert_eq!(value.value_code, "L");

        let negative_sort = SkuAttributeValueUpdate {
            sort_order: Some(-1),
            ..Default::default()
        };
        assert!(value.update(negative_sort, "admin-2").is_err());
    }

    /// 状态机：合法迁移通过，邻接矩阵对称闭合。
    #[test]
    fn status_transitions_follow_document_state() {
        assert!(ensure_transition(EnableStatus::Active, EnableStatus::Disabled).is_ok());
        assert_adjacency_closed(&[EnableStatus::Active, EnableStatus::Disabled]);
    }
}
