use mongodb::bson::{doc, Bson, Document};

use super::super::extensions::{CatalogExt, SupplierOfferingExt};

/// `product_revision` 集合名（单一来源：`CatalogExt` 关联常量）。
pub(super) const PRODUCT_REVISIONS: &str = <mongodb::Database as CatalogExt>::PRODUCT_REVISIONS;
/// `sku` 集合名（单一来源：`CatalogExt` 关联常量）。
pub(super) const SKUS: &str = <mongodb::Database as CatalogExt>::SKUS;
/// `sku_revision` 集合名（单一来源：`CatalogExt` 关联常量）。
pub(super) const SKU_REVISIONS: &str = <mongodb::Database as CatalogExt>::SKU_REVISIONS;
/// `supplier_offering` 集合名（公司商品池资格依赖的供给稳定身份）。
pub(super) const SUPPLIER_OFFERINGS: &str = <mongodb::Database as SupplierOfferingExt>::SUPPLIER_OFFERINGS;

/// 构造 ID 集合批量匹配条件。
///
/// # 参数
/// * `field` - 匹配字段名
/// * `values` - 待匹配的 ID 字符串集合
///
/// # 返回
/// 返回批量查询条件文档。
pub(super) fn in_filter(field: &str, values: impl IntoIterator<Item = String>) -> Document {
    let values: Vec<Bson> = values.into_iter().map(Bson::String).collect();
    doc! { field: { "$in": values } }
}

/// 构建排序文档。
///
/// # 参数
/// * `field` - 已通过白名单校验的排序字段
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回排序条件文档。
pub(super) fn sort_doc(field: &str, sort_ascending: bool) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    doc! { field: direction }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_doc_applies_direction() {
        assert_eq!(sort_doc("created_at", false), doc! { "created_at": -1 });
        assert_eq!(sort_doc("sku_no", true), doc! { "sku_no": 1 });
    }
}
