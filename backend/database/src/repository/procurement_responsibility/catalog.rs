use entities::catalog::ProductCategory;
use entities::procurement_responsibility::ProcurementCatalogBundle;
use erp_core::ids::{ProductCategoryId, ProductRevisionId, SkuId};

use super::ids::unique_ids;
use crate::CatalogExt;
use persistence_core::Executor;
use persistence_core::Result;

/// 批量加载采购责任目录所需的最小持久化事实.
///
/// # 参数
/// * `db` - MongoDB 数据库句柄
/// * `sku_ids` - 待解析的 SKU 集合，已去重并保持调用方顺序
/// * `executor` - 数据访问执行器，由 Service 决定事务边界；事务内重验必须复用调用方 executor
///
/// # 返回
/// 返回包含 SKU、商品、当前修订及全部父分类的最小事实集合；缺失由 Entity 层校验。
///
/// # 错误
/// MongoDB 查询或反序列化失败时返回错误；不负责缺失校验，软删除已通过 Repository 查询过滤。
///
/// # 约束
/// 查询次数与输入规模无关：SKU、商品、修订各一次批量读取，分类按深度分层批量读取；不得出现逐 SKU N+1。
pub async fn load_procurement_catalog_bundle(
    db: &mongodb::Database,
    sku_ids: &[SkuId],
    executor: &mut dyn Executor,
) -> Result<ProcurementCatalogBundle> {
    use std::collections::HashMap;

    if sku_ids.is_empty() {
        return Ok(ProcurementCatalogBundle {
            skus: HashMap::new(),
            products: HashMap::new(),
            revisions: HashMap::new(),
            categories: HashMap::new(),
        });
    }
    let sku_list = db.skus().find_by_ids(sku_ids, executor).await?;
    let mut skus = HashMap::with_capacity(sku_list.len());
    for sku in sku_list {
        skus.insert(sku.base.id.clone(), sku);
    }
    let product_ids = unique_ids(skus.values().map(|sku| sku.product_id.clone()));
    let product_list = db.products().find_by_ids(&product_ids, executor).await?;
    let mut products = HashMap::with_capacity(product_list.len());
    for product in product_list {
        products.insert(product.base.id.clone(), product);
    }
    let revision_ids = unique_ids(products.values().filter_map(|product| {
        product
            .stable
            .current_revision_id
            .as_deref()
            .map(ProductRevisionId::new)
    }));
    let revision_list = db
        .product_revisions()
        .find_by_ids(&revision_ids, executor)
        .await?;
    let mut revisions = HashMap::with_capacity(revision_list.len());
    for revision in revision_list {
        revisions.insert(revision.base.id.clone(), revision);
    }
    let initial_category_ids = unique_ids(revisions.values().map(|revision| revision.category_id.clone()));
    let categories = load_category_ancestors(db, initial_category_ids, executor).await?;
    Ok(ProcurementCatalogBundle {
        skus,
        products,
        revisions,
        categories,
    })
}

/// 分层批量加载当前分类及全部父分类.
///
/// # 参数
/// * `db` - MongoDB 数据库句柄
/// * `initial_ids` - 商品当前修订直接引用的分类 ID
/// * `executor` - 数据访问执行器，可加入销售形式化事务
///
/// # 返回
/// 返回当前分类和全部可达父分类的 ID 映射。
///
/// # 错误
/// MongoDB 查询失败时返回错误；缺失由 Entity 层分类链构造时校验，环由 Entity 检测。
///
/// # 约束
/// 按层级批量读取，每层一次查询，查询次数随分类深度线性增长但与输入规模无关。
async fn load_category_ancestors(
    db: &mongodb::Database,
    initial_ids: Vec<ProductCategoryId>,
    executor: &mut dyn Executor,
) -> Result<std::collections::HashMap<String, ProductCategory>> {
    use std::collections::HashMap;

    let mut categories = HashMap::new();
    let mut pending = initial_ids;
    while !pending.is_empty() {
        let rows = db.product_categories().find_by_ids(&pending, executor).await?;
        let mut row_map = HashMap::with_capacity(rows.len());
        for row in rows {
            row_map.insert(row.base.id.clone(), row);
        }
        // Deterministic parent collection: iterate sorted keys to guarantee stable per-depth $in order.
        let mut sorted_keys: Vec<String> = row_map.keys().cloned().collect();
        sorted_keys.sort();
        let mut next: Vec<ProductCategoryId> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for key in sorted_keys {
            if let Some(category) = row_map.get(&key) {
                if let Some(parent) = category.parent_category_id.clone() {
                    let parent_key = parent.to_string();
                    if !categories.contains_key(&parent_key)
                        && !row_map.contains_key(&parent_key)
                        && seen.insert(parent_key.clone())
                    {
                        next.push(parent);
                    }
                }
            }
        }
        next = unique_ids(next.into_iter());
        categories.extend(row_map);
        pending = next;
    }
    Ok(categories)
}
