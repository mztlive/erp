//! `product_category_attribute` 分类-属性适用关系（数据模型 §6.3）。
//!
//! 多对多适用关系：`(category_id, attribute_id, required_flag, sort_order)` 组合唯一
//! （唯一约束跨行，属 P3/索引校验）；本实体只保证单行不变式。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::errors::{Error, Result};
use crate::ids::{ProductCategoryAttributeId, ProductCategoryId, SkuAttributeId};

/// 分类-属性适用关系创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductCategoryAttributeData {
    /// 商品分类。
    pub category_id: ProductCategoryId,
    /// 规格属性。
    pub attribute_id: SkuAttributeId,
    /// 是否必填。
    pub required_flag: bool,
    /// 展示排序。
    pub sort_order: i32,
}

/// 分类-属性适用关系更新数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ProductCategoryAttributeUpdate {
    /// 是否必填；`None` 表示不修改。
    pub required_flag: Option<bool>,
    /// 展示排序；`None` 表示不修改。
    pub sort_order: Option<i32>,
}

/// 分类-属性适用关系实体（数据模型 §6.3 关系行表，只用 `BaseModel` 持久化元数据）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct ProductCategoryAttribute {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 商品分类。
    pub category_id: ProductCategoryId,
    /// 规格属性。
    pub attribute_id: SkuAttributeId,
    /// 是否必填。
    pub required_flag: bool,
    /// 展示排序。
    pub sort_order: i32,
}

impl ProductCategoryAttribute {
    /// 创建分类-属性适用关系。
    ///
    /// 完成 `sort_order` 非负校验；`category_id`/`attribute_id` 作为关系身份
    /// 创建后不可修改（组合唯一由 P3/索引校验）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::ProductCategoryAttributeId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的适用关系实体。
    ///
    /// # 错误
    /// 当 sort_order 为负数时返回错误。
    pub fn new(id: ProductCategoryAttributeId, data: ProductCategoryAttributeData) -> Result<Self> {
        ensure_non_negative_sort_order(data.sort_order)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            category_id: data.category_id,
            attribute_id: data.attribute_id,
            required_flag: data.required_flag,
            sort_order: data.sort_order,
        })
    }

    /// 更新分类-属性适用关系。
    ///
    /// 复用 `new` 的校验规则；`category_id`/`attribute_id` 是关系身份，
    /// 不允许在通用更新中修改。
    ///
    /// # 参数
    /// * `update` - 更新数据
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当 sort_order 为负数时返回错误。
    pub fn update(&mut self, update: ProductCategoryAttributeUpdate) -> Result<()> {
        if let Some(required_flag) = update.required_flag {
            self.required_flag = required_flag;
        }
        if let Some(sort_order) = update.sort_order {
            ensure_non_negative_sort_order(sort_order)?;
            self.sort_order = sort_order;
        }
        Ok(())
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
    use crate::ids::ProductCategoryId;

    fn data() -> ProductCategoryAttributeData {
        ProductCategoryAttributeData {
            category_id: ProductCategoryId::new("cat-1"),
            attribute_id: SkuAttributeId::new("attr-1"),
            required_flag: true,
            sort_order: 1,
        }
    }

    /// happy path：关系落位。
    #[test]
    fn new_normalizes_relation() {
        let relation =
            ProductCategoryAttribute::new(ProductCategoryAttributeId::new("rel-1"), data()).unwrap();

        assert_eq!(relation.category_id, ProductCategoryId::new("cat-1"));
        assert_eq!(relation.attribute_id, SkuAttributeId::new("attr-1"));
        assert!(relation.required_flag);
        assert_eq!(relation.sort_order, 1);
    }

    /// 失败路径：越界（负排序）被拒绝。
    #[test]
    fn new_rejects_negative_sort_order() {
        let negative = ProductCategoryAttributeData {
            sort_order: -1,
            ..data()
        };
        assert!(ProductCategoryAttribute::new(ProductCategoryAttributeId::new("rel-1"), negative).is_err());
    }

    /// update 修改必填标记与排序，身份字段保持不变。
    #[test]
    fn update_applies_fields_and_preserves_identity() {
        let mut relation =
            ProductCategoryAttribute::new(ProductCategoryAttributeId::new("rel-1"), data()).unwrap();

        relation
            .update(ProductCategoryAttributeUpdate {
                required_flag: Some(false),
                sort_order: Some(4),
            })
            .unwrap();

        assert!(!relation.required_flag);
        assert_eq!(relation.sort_order, 4);
        assert_eq!(relation.category_id, ProductCategoryId::new("cat-1"));

        assert!(relation
            .update(ProductCategoryAttributeUpdate {
                sort_order: Some(-1),
                ..Default::default()
            })
            .is_err());
    }
}
