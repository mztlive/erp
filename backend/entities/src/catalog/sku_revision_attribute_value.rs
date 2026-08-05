//! `sku_revision_attribute_value` SKU 修订规格属性值（数据模型 §6.3，关系行表）。
//!
//! `(sku_revision_id, sku_attribute_id)` 唯一（唯一约束跨行，属 P3/索引校验）；
//! 枚举值必须属于对应属性（需字典查询，属 P3）。
//! 本实体保证单行不变式：枚举值与文本值只能使用一种（二选一）。
//! `identity_position` 是规范化排序位置，跨行必须构成 `0..n` 完整排列，
//! 由 [`crate::catalog::specification::validate_identity_positions`] 判定（P3 汇总后校验）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::errors::{Error, Result};
use crate::ids::{SkuAttributeId, SkuAttributeValueId, SkuRevisionAttributeValueId, SkuRevisionId};
use crate::validation::normalize_optional_text;

/// 规范文本属性值最大长度。
const TEXT_VALUE_MAX_LEN: usize = 512;

/// SKU 修订规格属性值创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkuRevisionAttributeValueData {
    /// 所属 SKU 修订。
    pub sku_revision_id: SkuRevisionId,
    /// 规格属性。
    pub sku_attribute_id: SkuAttributeId,
    /// 受控枚举属性值（使用枚举值时必填，文本值时空）。
    pub sku_attribute_value_id: Option<SkuAttributeValueId>,
    /// 规范文本属性值（使用文本值时必填，枚举值时空）。
    pub normalized_text_value: Option<String>,
    /// 规范化排序位置（签名内排序位，`0..n` 完整排列）。
    pub identity_position: u32,
}

/// SKU 修订规格属性值实体（数据模型 §6.3 关系行表，只用 `BaseModel` 持久化元数据）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SkuRevisionAttributeValue {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属 SKU 修订。
    pub sku_revision_id: SkuRevisionId,
    /// 规格属性。
    pub sku_attribute_id: SkuAttributeId,
    /// 受控枚举属性值。
    pub sku_attribute_value_id: Option<SkuAttributeValueId>,
    /// 规范文本属性值。
    pub normalized_text_value: Option<String>,
    /// 规范化排序位置。
    pub identity_position: u32,
}

impl SkuRevisionAttributeValue {
    /// 创建 SKU 修订规格属性值。
    ///
    /// 完成文本值的可选校验与规范化，并强制「枚举值与文本值只能使用一种」
    /// （数据模型 §6.3）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SkuRevisionAttributeValueId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的属性值关系实体。
    ///
    /// # 错误
    /// 当枚举值与文本值同时出现或同时缺失、文本值超长时返回错误。
    pub fn new(id: SkuRevisionAttributeValueId, data: SkuRevisionAttributeValueData) -> Result<Self> {
        let normalized_text_value =
            normalize_optional_text(data.normalized_text_value, "规范文本值", TEXT_VALUE_MAX_LEN)?;
        match (&data.sku_attribute_value_id, &normalized_text_value) {
            (Some(_), Some(_)) => return Err(Error::from("枚举值与文本值只能使用一种")),
            (None, None) => return Err(Error::from("枚举值与文本值必须提供一种")),
            _ => {}
        }

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            sku_revision_id: data.sku_revision_id,
            sku_attribute_id: data.sku_attribute_id,
            sku_attribute_value_id: data.sku_attribute_value_id,
            normalized_text_value,
            identity_position: data.identity_position,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::SkuRevisionId;

    fn data() -> SkuRevisionAttributeValueData {
        SkuRevisionAttributeValueData {
            sku_revision_id: SkuRevisionId::new("rev-1"),
            sku_attribute_id: SkuAttributeId::new("attr-1"),
            sku_attribute_value_id: Some(SkuAttributeValueId::new("val-1")),
            normalized_text_value: None,
            identity_position: 0,
        }
    }

    /// happy path：枚举值形态落位。
    #[test]
    fn new_accepts_enum_value() {
        let row = SkuRevisionAttributeValue::new(SkuRevisionAttributeValueId::new("row-1"), data()).unwrap();

        assert_eq!(
            row.sku_attribute_value_id,
            Some(SkuAttributeValueId::new("val-1"))
        );
        assert!(row.normalized_text_value.is_none());
        assert_eq!(row.identity_position, 0);
    }

    /// happy path：文本值形态 trim 规范化。
    #[test]
    fn new_accepts_normalized_text_value() {
        let text = SkuRevisionAttributeValueData {
            sku_attribute_value_id: None,
            normalized_text_value: Some(" 红色 ".to_string()),
            ..data()
        };
        let row = SkuRevisionAttributeValue::new(SkuRevisionAttributeValueId::new("row-1"), text).unwrap();

        assert!(row.sku_attribute_value_id.is_none());
        assert_eq!(row.normalized_text_value.as_deref(), Some("红色"));
    }

    /// 失败路径：关联不一致——枚举值与文本值同时出现或同时缺失均被拒绝。
    #[test]
    fn new_rejects_both_or_neither_value_kinds() {
        let both = SkuRevisionAttributeValueData {
            sku_attribute_value_id: Some(SkuAttributeValueId::new("val-1")),
            normalized_text_value: Some(" 红色 ".to_string()),
            ..data()
        };
        assert!(SkuRevisionAttributeValue::new(SkuRevisionAttributeValueId::new("row-1"), both).is_err());

        let neither = SkuRevisionAttributeValueData {
            sku_attribute_value_id: None,
            normalized_text_value: None,
            ..data()
        };
        assert!(SkuRevisionAttributeValue::new(SkuRevisionAttributeValueId::new("row-1"), neither).is_err());
    }

    /// 失败路径：文本值超长被拒绝。
    #[test]
    fn new_rejects_overlong_text_value() {
        let overlong = SkuRevisionAttributeValueData {
            sku_attribute_value_id: None,
            normalized_text_value: Some("t".repeat(513)),
            ..data()
        };
        assert!(SkuRevisionAttributeValue::new(SkuRevisionAttributeValueId::new("row-1"), overlong).is_err());
    }
}
