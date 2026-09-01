//! 采购责任目录事实的值对象层。
//!
//! Repository 仅负责批量拉取 SKU、商品、当前修订与分类的原始持久化事实；
//! 本模块负责目录完整性、当前修订一致性、分类链顺序与成环拒绝的纯业务规则。

use std::collections::{HashMap, HashSet};

use crate::catalog::{Product, ProductCategory, ProductKind, ProductRevision, Sku};
use crate::errors::{Error, Result};
use crate::ids::{ProductCategoryId, ProductRevisionId};

use super::resolution::ProcurementResponsibilityResolutionLine;

/// 单行采购责任目录解析事实。
///
/// 包含从当前分类到根分类的有序链与商品业务类型，仅承载规则解析所需的最小事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcurementCatalogFact {
    /// 当前分类到根分类的有序链。
    pub category_chain: Vec<ProductCategoryId>,
    /// 商品业务类型。
    pub product_kind: ProductKind,
}

/// 采购责任目录批量事实的最小集合.
///
/// 包含解析所需 SKU、商品、当前修订及全部可达分类的原始持久化事实；
/// 完整性、分类链顺序与成环校验由 Entity 值对象负责。
#[derive(Debug, Clone)]
pub struct ProcurementCatalogBundle {
    /// SKU ID 到 SKU 事实的映射。
    pub skus: HashMap<String, Sku>,
    /// 商品 ID 到商品事实的映射。
    pub products: HashMap<String, Product>,
    /// 修订 ID 到商品修订事实的映射。
    pub revisions: HashMap<String, ProductRevision>,
    /// 分类 ID 到商品分类事实的映射，包含全部可达父分类。
    pub categories: HashMap<String, ProductCategory>,
}

/// 提取全部商品当前修订 ID。
///
/// # 参数
/// * `products` - 商品稳定 ID 到商品实体的映射
///
/// # 返回
/// 返回去重后按字典序稳定排序的当前商品修订 ID 集合。
///
/// # 错误
/// 任一商品尚未形成当前修订时返回校验错误。
///
/// # 约束
/// 去重后按 ID 字符串字典序稳定排序，满足确定性顺序；不依赖 HashMap 迭代随机性。
pub fn current_revision_ids(products: &HashMap<String, Product>) -> Result<Vec<ProductRevisionId>> {
    let mut sorted_keys: Vec<&String> = products.keys().collect();
    sorted_keys.sort();
    let ids = sorted_keys
        .into_iter()
        .map(|key| {
            let product = products.get(key).expect("sorted key present");
            product
                .stable
                .current_revision_id
                .as_deref()
                .map(ProductRevisionId::new)
                .ok_or_else(|| Error::from(format!("商品 {} 没有当前修订", product.base.id)))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(unique_values(ids.into_iter()))
}

/// 由批量目录映射构造每行解析事实。
///
/// # 参数
/// * `inputs` - 已校验的采购责任解析行
/// * `skus` - SKU ID 到稳定 SKU 的映射
/// * `products` - 商品 ID 到稳定商品的映射
/// * `revisions` - 商品修订 ID 到当前修订的映射
/// * `categories` - 当前分类与父分类的完整映射
///
/// # 返回
/// 返回行键到分类链和商品类型事实的映射。
///
/// # 错误
/// 分类链缺失或存在环时返回错误；调用方已保证前置映射完整性后仍需再次校验环。
///
/// # 约束
/// 结果以输入行键为键保持确定性；分类链顺序从当前分类到根，不依赖输入乱序。
pub fn build_catalog_facts(
    inputs: &[ProcurementResponsibilityResolutionLine],
    skus: &HashMap<String, Sku>,
    products: &HashMap<String, Product>,
    revisions: &HashMap<String, ProductRevision>,
    categories: &HashMap<String, ProductCategory>,
) -> Result<HashMap<String, ProcurementCatalogFact>> {
    let mut facts = HashMap::with_capacity(inputs.len());
    for input in inputs {
        let sku = skus
            .get(input.sku_id.as_ref())
            .ok_or_else(|| Error::from(format!("SKU不存在或已删除：{}", input.sku_id.as_ref())))?;
        let product = products
            .get(sku.product_id.as_ref())
            .ok_or_else(|| Error::from(format!("商品不存在或已删除：{}", sku.product_id.as_ref())))?;
        let revision_id = product
            .stable
            .current_revision_id
            .as_deref()
            .ok_or_else(|| Error::from(format!("商品 {} 没有当前修订", product.base.id)))?;
        let revision = revisions
            .get(revision_id)
            .ok_or_else(|| Error::from(format!("商品当前修订不存在或已删除：{revision_id}")))?;
        let category_chain = category_chain(&revision.category_id, categories)?;
        facts.insert(
            input.line_key.clone(),
            ProcurementCatalogFact {
                category_chain,
                product_kind: product.product_kind,
            },
        );
    }
    Ok(facts)
}

/// 构造当前分类到根分类的有序链并检测环。
///
/// # 参数
/// * `first` - 商品当前修订直接引用的分类
/// * `categories` - 当前分类与全部父分类映射
///
/// # 返回
/// 返回从当前分类到根分类的有序强类型 ID 链，顺序固定为自底向上。
///
/// # 错误
/// 分类缺失时返回校验错误；父级关系成环（自环或多节点环）时返回冲突错误。
///
/// # 约束
/// 环检测使用访问集合，去重键为分类 ID 字符串；链内不得出现重复。
pub fn category_chain(
    first: &ProductCategoryId,
    categories: &HashMap<String, ProductCategory>,
) -> Result<Vec<ProductCategoryId>> {
    let mut chain = Vec::new();
    let mut seen = HashSet::new();
    let mut current = Some(first.clone());
    while let Some(category_id) = current {
        if !seen.insert(category_id.to_string()) {
            return Err(Error::from("商品分类父级关系存在环"));
        }
        let category = categories
            .get(category_id.as_ref())
            .ok_or_else(|| Error::from(format!("商品分类不存在：{category_id}")))?;
        chain.push(category_id);
        current = category.parent_category_id.clone();
    }
    Ok(chain)
}

/// 对强类型目录 ID 去重并按字典序稳定排序。
///
/// # 参数
/// * `values` - 待去重的强类型 ID 迭代器
///
/// # 返回
/// 返回按 ID 字符串字典序稳定排序的唯一值集合，满足确定性。
///
/// # 错误
/// 无。
///
/// # 约束
/// 去重后按字符串字典序排序，避免 HashMap 随机迭代导致结果不确定。
fn unique_values<T>(values: impl Iterator<Item = T>) -> Vec<T>
where
    T: PartialEq + ToString,
{
    let mut unique = Vec::new();
    for value in values {
        if !unique.contains(&value) {
            unique.push(value);
        }
    }
    unique.sort_by_key(|a| a.to_string());
    unique
}

/// 对强类型目录 ID 去重并按字典序稳定排序的公开复用入口。
///
/// # 参数
/// * `values` - 待去重的强类型 ID 迭代器
///
/// # 返回
/// 返回按字符串字典序稳定排序的唯一值集合，供 Repository 复用单一去重实现。
///
/// # 错误
/// 无。
///
/// # 约束
/// 与 `current_revision_ids` 共用同一去重与排序实现，保证单一规则源。
pub fn dedup_sorted_ids<T>(values: impl Iterator<Item = T>) -> Vec<T>
where
    T: PartialEq + ToString,
{
    unique_values(values)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::product::ProductData;
    use crate::catalog::product_category::ProductCategoryData;
    use crate::catalog::product_revision::ProductRevisionData;
    use crate::catalog::sku::SkuData;
    use crate::catalog::{EnableStatus, ProductKind};
    use crate::common::time::BusinessDate;
    use crate::ids::{ProductBrandId, ProductCategoryId, ProductId, SkuId, UnitOfMeasureId};

    fn test_product(current_revision_id: Option<&str>) -> Product {
        let mut product = Product::new(
            ProductId::new("product-1"),
            ProductData {
                product_no: "P-001".to_string(),
                product_kind: ProductKind::Physical,
                status: EnableStatus::Active,
            },
            "test",
        )
        .unwrap();
        product.stable.current_revision_id = current_revision_id.map(|s| s.to_string());
        product
    }

    fn test_revision(category_id: &str) -> ProductRevision {
        ProductRevision::new(
            crate::ids::ProductRevisionId::new("rev-1"),
            ProductRevisionData {
                product_id: ProductId::new("product-1"),
                revision_no: 1,
                name: "商品".to_string(),
                description: None,
                specification: None,
                category_id: ProductCategoryId::new(category_id),
                brand_id: ProductBrandId::new("brand-1"),
                status: EnableStatus::Active,
                effective_from: BusinessDate::from_ymd(2024, 1, 1).unwrap(),
                effective_to: None,
            },
        )
        .unwrap()
    }

    fn test_category(id: &str, parent: Option<&str>) -> ProductCategory {
        ProductCategory::new(
            ProductCategoryId::new(id),
            ProductCategoryData {
                category_code: format!("code-{id}"),
                parent_category_id: parent.map(ProductCategoryId::new),
                name: format!("分类{id}"),
                product_kind: ProductKind::Physical,
                status: EnableStatus::Active,
            },
            "test",
        )
        .unwrap()
    }

    fn test_sku() -> Sku {
        Sku::new(
            SkuId::new("sku-1"),
            SkuData {
                sku_no: "SKU-001".to_string(),
                product_id: ProductId::new("product-1"),
                base_unit_id: UnitOfMeasureId::new("unit-1"),
                specification_signature: String::new(),
                status: EnableStatus::Active,
                listing_status: crate::catalog::ListingStatus::Unlisted,
            },
            "test",
        )
        .unwrap()
    }

    #[test]
    fn category_chain_single_level_returns_single_node() {
        let mut categories = HashMap::new();
        categories.insert("cat-1".to_string(), test_category("cat-1", None));
        let chain = category_chain(&ProductCategoryId::new("cat-1"), &categories).unwrap();
        assert_eq!(chain, vec![ProductCategoryId::new("cat-1")]);
    }

    #[test]
    fn category_chain_multi_level_returns_leaf_to_root_order() {
        let mut categories = HashMap::new();
        categories.insert("cat-1".to_string(), test_category("cat-1", None));
        categories.insert("cat-2".to_string(), test_category("cat-2", Some("cat-1")));
        categories.insert("cat-3".to_string(), test_category("cat-3", Some("cat-2")));
        let chain = category_chain(&ProductCategoryId::new("cat-3"), &categories).unwrap();
        assert_eq!(
            chain,
            vec![
                ProductCategoryId::new("cat-3"),
                ProductCategoryId::new("cat-2"),
                ProductCategoryId::new("cat-1")
            ]
        );
    }

    #[test]
    fn category_chain_missing_parent_returns_error() {
        let mut categories = HashMap::new();
        categories.insert("cat-2".to_string(), test_category("cat-2", Some("cat-1")));
        let err = category_chain(&ProductCategoryId::new("cat-2"), &categories).unwrap_err();
        assert!(err.to_string().contains("不存在"));
    }

    #[test]
    fn category_chain_self_loop_is_rejected() {
        let mut categories = HashMap::new();
        let mut cat = test_category("cat-1", None);
        cat.parent_category_id = Some(ProductCategoryId::new("cat-1"));
        categories.insert("cat-1".to_string(), cat);
        let err = category_chain(&ProductCategoryId::new("cat-1"), &categories).unwrap_err();
        assert!(err.to_string().contains("环"));
    }

    #[test]
    fn category_chain_multi_node_ring_is_rejected() {
        let mut categories = HashMap::new();
        categories.insert("cat-1".to_string(), test_category("cat-1", Some("cat-3")));
        categories.insert("cat-2".to_string(), test_category("cat-2", Some("cat-1")));
        categories.insert("cat-3".to_string(), test_category("cat-3", Some("cat-2")));
        let err = category_chain(&ProductCategoryId::new("cat-3"), &categories).unwrap_err();
        assert!(err.to_string().contains("环"));
    }

    #[test]
    fn current_revision_ids_fails_when_missing_current_revision() {
        let mut products = HashMap::new();
        products.insert("product-1".to_string(), test_product(None));
        let err = current_revision_ids(&products).unwrap_err();
        assert!(err.to_string().contains("没有当前修订"));
    }

    #[test]
    fn current_revision_ids_deduplicates_and_returns_sorted_order() {
        let mut products = HashMap::new();
        products.insert("product-a".to_string(), test_product(Some("rev-1")));
        let mut prod_b = test_product(Some("rev-1"));
        prod_b.base.id = "product-b".to_string();
        products.insert("product-b".to_string(), prod_b);
        let mut prod_c = test_product(Some("rev-2"));
        prod_c.base.id = "product-c".to_string();
        products.insert("product-c".to_string(), prod_c);
        let ids = current_revision_ids(&products).unwrap();
        assert_eq!(
            ids,
            vec![ProductRevisionId::new("rev-1"), ProductRevisionId::new("rev-2")]
        );
    }

    #[test]
    fn current_revision_ids_is_deterministic_with_shuffled_insertion() {
        // Two different insertion orders should yield the same sorted output.
        let mut products_a = HashMap::new();
        products_a.insert("product-b".to_string(), {
            let mut p = test_product(Some("rev-2"));
            p.base.id = "product-b".to_string();
            p
        });
        products_a.insert("product-a".to_string(), test_product(Some("rev-1")));
        products_a.insert("product-c".to_string(), {
            let mut p = test_product(Some("rev-3"));
            p.base.id = "product-c".to_string();
            p
        });

        let mut products_b = HashMap::new();
        products_b.insert("product-c".to_string(), {
            let mut p = test_product(Some("rev-3"));
            p.base.id = "product-c".to_string();
            p
        });
        products_b.insert("product-a".to_string(), test_product(Some("rev-1")));
        products_b.insert("product-b".to_string(), {
            let mut p = test_product(Some("rev-2"));
            p.base.id = "product-b".to_string();
            p
        });

        let ids_a = current_revision_ids(&products_a).unwrap();
        let ids_b = current_revision_ids(&products_b).unwrap();
        assert_eq!(ids_a, ids_b);
        assert_eq!(
            ids_a,
            vec![
                ProductRevisionId::new("rev-1"),
                ProductRevisionId::new("rev-2"),
                ProductRevisionId::new("rev-3")
            ]
        );
    }

    #[test]
    fn current_revision_ids_dedup_preserves_sorted_stable_order_with_same_revision() {
        let mut products = HashMap::new();
        // Multiple products sharing same revision should deduplicate to single sorted entry.
        for key in ["product-z", "product-a", "product-m"] {
            let mut prod = test_product(Some("rev-shared"));
            prod.base.id = key.to_string();
            products.insert(key.to_string(), prod);
        }
        let mut prod_unique = test_product(Some("rev-alpha"));
        prod_unique.base.id = "product-unique".to_string();
        products.insert("product-unique".to_string(), prod_unique);
        let ids = current_revision_ids(&products).unwrap();
        // Sorted order: rev-alpha < rev-shared lexicographically.
        assert_eq!(
            ids,
            vec![
                ProductRevisionId::new("rev-alpha"),
                ProductRevisionId::new("rev-shared")
            ]
        );
    }

    #[test]
    fn current_revision_ids_empty_products_returns_empty() {
        let products: HashMap<String, Product> = HashMap::new();
        let ids = current_revision_ids(&products).unwrap();
        assert!(ids.is_empty());
    }

    #[test]
    fn build_catalog_facts_is_deterministic_and_preserves_leaf_to_root() {
        let inputs = vec![ProcurementResponsibilityResolutionLine::new(
            "line-1".to_string(),
            SkuId::new("sku-1"),
            None,
        )
        .unwrap()];
        let sku = test_sku();
        let mut skus = HashMap::new();
        skus.insert("sku-1".to_string(), sku);

        let mut products = HashMap::new();
        products.insert("product-1".to_string(), test_product(Some("rev-1")));
        let mut revisions = HashMap::new();
        revisions.insert("rev-1".to_string(), test_revision("cat-2"));
        let mut categories = HashMap::new();
        categories.insert("cat-1".to_string(), test_category("cat-1", None));
        categories.insert("cat-2".to_string(), test_category("cat-2", Some("cat-1")));

        let facts = build_catalog_facts(&inputs, &skus, &products, &revisions, &categories).unwrap();
        let fact = facts.get("line-1").unwrap();
        assert_eq!(
            fact.category_chain,
            vec![ProductCategoryId::new("cat-2"), ProductCategoryId::new("cat-1")]
        );
        assert_eq!(fact.product_kind, ProductKind::Physical);
    }

    #[test]
    fn build_catalog_facts_propagates_category_ring_error() {
        let inputs = vec![ProcurementResponsibilityResolutionLine::new(
            "line-1".to_string(),
            SkuId::new("sku-1"),
            None,
        )
        .unwrap()];
        let sku = test_sku();
        let mut skus = HashMap::new();
        skus.insert("sku-1".to_string(), sku);
        let mut products = HashMap::new();
        products.insert("product-1".to_string(), test_product(Some("rev-1")));
        let mut revisions = HashMap::new();
        revisions.insert("rev-1".to_string(), test_revision("cat-1"));
        let mut categories = HashMap::new();
        // Self-loop is rejected at entity creation, so simulate via manual parent edit that bypasses self-check
        // Create cat-1 as root then mutate to self-loop for chain test
        let mut cat = test_category("cat-1", None);
        cat.parent_category_id = Some(ProductCategoryId::new("cat-1"));
        categories.insert("cat-1".to_string(), cat);
        let err = build_catalog_facts(&inputs, &skus, &products, &revisions, &categories).unwrap_err();
        assert!(err.to_string().contains("环"));
    }
}
