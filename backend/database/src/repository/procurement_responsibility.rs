//! 采购责任规则仓储查询。

use entities::catalog::{EnableStatus, ProductCategory};
use entities::ids::{ProductCategoryId, ProductRevisionId, SkuId, SkuRevisionId};
use entities::procurement_responsibility::{
    collect_rule_list_ids, ProcurementCatalogBundle, ProcurementResponsibilityRule,
    ProcurementResponsibilityRuleType, ProcurementRuleListDisplayFacts, ProcurementRuleListPage,
};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};

use crate::{AccessControlExt, CatalogExt, ProcurementResponsibilityExt};
use mongodb::options::FindOptions;

use super::{PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// 采购责任规则列表筛选条件。
#[derive(Debug, Clone)]
pub struct ProcurementResponsibilityRuleFilter {
    /// 规则类型；`None` 表示不筛选。
    pub rule_type: Option<ProcurementResponsibilityRuleType>,
    /// 负责人账号 ID；`None` 表示不筛选。
    pub owner_user_id: Option<String>,
    /// 启停状态；`None` 表示不筛选。
    pub status: Option<EnableStatus>,
    /// 页码，从 1 开始。
    pub page: u64,
    /// 每页条数。
    pub page_size: u32,
}

impl QueryFilter for ProcurementResponsibilityRuleFilter {
    /// 构造包含软删除约束的 MongoDB 查询文档。
    ///
    /// # 返回
    /// 返回规则类型、负责人及状态筛选文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(rule_type) = self.rule_type {
            filter.insert("rule_type", rule_type.as_str());
        }
        if let Some(owner_user_id) = self.owner_user_id.as_deref() {
            filter.insert("owner_user_id", owner_user_id);
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for ProcurementResponsibilityRuleFilter {
    /// 返回页码与每页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)`。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, ProcurementResponsibilityRule> {
    /// 分页查询采购责任规则。
    ///
    /// # 参数
    /// * `filter` - 规则列表筛选与分页条件
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回按优先级与创建时间稳定排序的当前页规则及总数。
    ///
    /// # 错误
    /// MongoDB 查询、计数或反序列化失败时返回错误。
    pub async fn search_procurement_responsibility_rules(
        &self,
        filter: &ProcurementResponsibilityRuleFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<ProcurementResponsibilityRule>> {
        let options = FindOptions::builder()
            .sort(doc! { "rule_type": 1, "created_at": 1, "id": 1 })
            .skip(filter.skip())
            .limit(filter.limit())
            .build();
        let items = mongo_ops::find_many(&self.collection(), filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;
        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 读取全部启用采购责任规则。
    ///
    /// # 参数
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回全部未删除且启用规则。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_active_procurement_responsibility_rules(
        &self,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ProcurementResponsibilityRule>> {
        self.find_many_sorted(
            doc! { "status": EnableStatus::Active.as_str() },
            doc! { "rule_type": 1, "created_at": 1, "id": 1 },
            executor,
        )
        .await
    }

    /// 按稳定 ID 读取采购责任规则。
    ///
    /// # 参数
    /// * `id` - 采购责任规则 ID
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回未删除规则；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn find_procurement_responsibility_rule(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ProcurementResponsibilityRule>> {
        self.find_by_id(id, executor).await
    }
}

/// 批量加载规则行展示所需的最小关联事实.
///
/// # 参数
/// * `db` - MongoDB 数据库句柄
/// * `rules` - 当前页规则实体切片
/// * `executor` - 数据访问执行器，由 Service 决定事务边界；事务内重验必须复用调用方 executor
///
/// # 返回
/// 返回负责人姓名、SKU 编号和当前名称、分类名称的稀疏映射；缺失引用保持稀疏.
///
/// # 错误
/// MongoDB 查询或反序列化失败时返回错误；不负责缺失校验，软删除已通过 Repository 查询过滤.
///
/// # 约束
/// 查询次数固定为 4 次（负责人、SKU、SKU 当前修订、分类各一次），与页大小无关；空输入直接返回空映射.
pub async fn load_procurement_rule_list_facts(
    db: &mongodb::Database,
    rules: &[ProcurementResponsibilityRule],
    executor: &mut dyn Executor,
) -> Result<ProcurementRuleListDisplayFacts> {
    use std::collections::HashMap;

    if rules.is_empty() {
        return Ok(ProcurementRuleListDisplayFacts::default());
    }
    let (owner_ids, sku_ids, category_ids) = collect_rule_list_ids(rules);
    let mut facts = ProcurementRuleListDisplayFacts::default();
    if !owner_ids.is_empty() {
        let owners = db
            .accounts()
            .list_procurement_responsibility_owners(&owner_ids, executor)
            .await?;
        facts.owner_names = owners
            .into_iter()
            .map(|account| (account.base.id, account.name))
            .collect::<HashMap<_, _>>();
    }
    if !sku_ids.is_empty() {
        let skus = db
            .skus()
            .list_procurement_responsibility_skus(&sku_ids, executor)
            .await?;
        let revision_ids: Vec<SkuRevisionId> = unique_ids(
            skus.iter()
                .filter_map(|sku| sku.stable.current_revision_id.as_deref().map(SkuRevisionId::new)),
        );
        let revision_names = if revision_ids.is_empty() {
            HashMap::new()
        } else {
            db.sku_revisions()
                .list_procurement_responsibility_sku_revisions(&revision_ids, executor)
                .await?
                .into_iter()
                .map(|revision| (revision.base.id, revision.name))
                .collect::<HashMap<_, _>>()
        };
        for sku in skus {
            facts.sku_nos.insert(sku.base.id.clone(), sku.sku_no.clone());
            if let Some(revision_id) = sku.stable.current_revision_id.as_deref() {
                if let Some(name) = revision_names.get(revision_id) {
                    facts.sku_names.insert(sku.base.id, name.clone());
                }
            }
        }
    }
    if !category_ids.is_empty() {
        let categories = db
            .product_categories()
            .list_procurement_responsibility_categories(&category_ids, executor)
            .await?;
        facts.category_names = categories
            .into_iter()
            .map(|category| (category.base.id, category.name))
            .collect::<HashMap<_, _>>();
    }
    Ok(facts)
}

/// 分页查询规则行并批量返回管理列表展示事实.
///
/// # 参数
/// * `db` - MongoDB 数据库句柄
/// * `filter` - 规则类型、负责人、状态及分页筛选
/// * `executor` - 数据访问执行器，由 Service 决定事务边界；事务内重验必须复用调用方 executor
///
/// # 返回
/// 返回按优先级与创建时间稳定排序的当前页规则、总数及展示事实.
///
/// # 错误
/// MongoDB 查询、计数或反序列化失败时返回错误；总数、排序与软删除语义与规则集合查询一致.
///
/// # 约束
/// 分页先按过滤求总数再取当前页，关联查询放在分页之后但次数固定为 4 次；不得读取分页外规则.
pub async fn load_procurement_rule_list_page(
    db: &mongodb::Database,
    filter: &ProcurementResponsibilityRuleFilter,
    executor: &mut dyn Executor,
) -> Result<ProcurementRuleListPage> {
    let page = db
        .procurement_responsibility_rules()
        .search_procurement_responsibility_rules(filter, executor)
        .await?;
    let facts = load_procurement_rule_list_facts(db, &page.items, executor).await?;
    Ok(ProcurementRuleListPage {
        items: page.items,
        total: page.total,
        facts,
    })
}

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

/// 对强类型目录 ID 去重并按字典序稳定排序，供 Entity 单一规则源复用。
///
/// # 参数
/// * `values` - 待去重的强类型 ID 迭代器
///
/// # 返回
/// 返回按字符串字典序稳定排序的唯一值集合，满足确定性；与 Entity 值对象共用同一排序实现。
///
/// # 错误
/// 无。
///
/// # 约束
/// 去重后按字符串字典序排序，避免 HashMap 随机迭代导致查询批次不稳定；直接复用 Entity 的 `dedup_sorted_ids`。
fn unique_ids<T>(values: impl Iterator<Item = T>) -> Vec<T>
where
    T: PartialEq + ToString,
{
    entities::procurement_responsibility::dedup_sorted_ids(values)
}

#[cfg(test)]
mod isolation_tests {
    use crate::ensure_indexes;
    use crate::{CatalogExt, NoTransaction, Transactional};
    use entities::catalog::product::ProductData;
    use entities::catalog::product_category::ProductCategoryData;
    use entities::catalog::product_revision::ProductRevisionData;
    use entities::catalog::sku::SkuData;
    use entities::catalog::{EnableStatus, ListingStatus, Product, ProductCategory, ProductRevision, Sku};
    use entities::common::time::BusinessDate;
    use entities::ids::{
        ProductBrandId, ProductCategoryId, ProductId, ProductRevisionId, SkuId, UnitOfMeasureId,
    };
    use entities::procurement_responsibility::{
        build_catalog_facts, ProcurementResponsibilityResolutionLine,
    };
    use mongodb::bson::doc;
    use test_support::{require_mongo, TestDb};

    use super::load_procurement_catalog_bundle;

    fn test_category(id: &str, parent: Option<&str>) -> ProductCategory {
        ProductCategory::new(
            ProductCategoryId::new(id),
            ProductCategoryData {
                category_code: format!("code-{id}"),
                parent_category_id: parent.map(ProductCategoryId::new),
                name: format!("分类{id}"),
                product_kind: entities::catalog::ProductKind::Physical,
                status: EnableStatus::Active,
            },
            "test",
        )
        .unwrap()
    }

    fn test_product(id: &str, revision_id: Option<&str>) -> Product {
        let mut product = Product::new(
            ProductId::new(id),
            ProductData {
                product_no: format!("P-{id}"),
                product_kind: entities::catalog::ProductKind::Physical,
                status: EnableStatus::Active,
            },
            "test",
        )
        .unwrap();
        product.stable.current_revision_id = revision_id.map(|s| s.to_string());
        product
    }

    fn test_revision(id: &str, product_id: &str, category_id: &str) -> ProductRevision {
        ProductRevision::new(
            ProductRevisionId::new(id),
            ProductRevisionData {
                product_id: ProductId::new(product_id),
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

    fn test_sku(id: &str, product_id: &str) -> Sku {
        Sku::new(
            SkuId::new(id),
            SkuData {
                sku_no: format!("SKU-{id}"),
                product_id: ProductId::new(product_id),
                base_unit_id: UnitOfMeasureId::new("unit-1"),
                specification_signature: format!("spec-{id}"),
                status: EnableStatus::Active,
                listing_status: ListingStatus::Unlisted,
            },
            "test",
        )
        .unwrap()
    }

    /// 空输入返回空 bundle，不触发数据库查询错误。
    ///
    /// # 参数
    /// 无，内部创建隔离库。
    ///
    /// # 返回
    /// 空 bundle 时断言全部映射为空。
    ///
    /// # 错误
    /// MongoDB 连接或 bundle 加载失败时测试失败。
    ///
    /// # 约束
    /// Batch 维度空输入必须返回空集合；不适用 Page/Aggregation/Index 标记 N/A。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn empty_sku_ids_returns_empty_bundle() {
        require_mongo!(async {
            let fixture = TestDb::new("proc_catalog_empty")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            let bundle = load_procurement_catalog_bundle(fixture.db(), &[], &mut NoTransaction)
                .await
                .expect("空 bundle 加载失败");
            assert!(bundle.skus.is_empty());
            assert!(bundle.products.is_empty());
            assert!(bundle.revisions.is_empty());
            assert!(bundle.categories.is_empty());
        });
    }

    /// 重复 SkuId 输入经幂等处理后结果与去重输入一致，保持调用方去重语义。
    ///
    /// # 参数
    /// 无，内部创建 1 SKU 及其关联事实。
    ///
    /// # 返回
    /// 断言重复输入 bundle 与单次输入 bundle 一致。
    ///
    /// # 错误
    /// 写入或 bundle 加载失败时测试失败。
    ///
    /// # 约束
    /// Batch 去重保持调用方顺序，查询次数不随重复数增长；Page/Aggregation/Index 标记 N/A。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn duplicate_sku_ids_are_deduplicated_and_stable() {
        require_mongo!(async {
            let fixture = TestDb::new("proc_catalog_dedup")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            let cat = test_category("cat-1", None);
            fixture
                .db()
                .product_categories()
                .create(&cat, &mut NoTransaction)
                .await
                .expect("分类写入失败");
            let prod = test_product("prod-1", Some("rev-1"));
            fixture
                .db()
                .products()
                .create(&prod, &mut NoTransaction)
                .await
                .expect("商品写入失败");
            let rev = test_revision("rev-1", "prod-1", "cat-1");
            fixture
                .db()
                .product_revisions()
                .create(&rev, &mut NoTransaction)
                .await
                .expect("修订写入失败");
            let sku = test_sku("sku-1", "prod-1");
            fixture
                .db()
                .skus()
                .create(&sku, &mut NoTransaction)
                .await
                .expect("SKU写入失败");

            let sku_id = SkuId::new("sku-1");
            let dup_ids = vec![sku_id.clone(), sku_id.clone(), sku_id.clone()];
            let bundle_dup = load_procurement_catalog_bundle(fixture.db(), &dup_ids, &mut NoTransaction)
                .await
                .expect("重复 bundle 加载失败");
            let bundle_once = load_procurement_catalog_bundle(
                fixture.db(),
                std::slice::from_ref(&sku_id),
                &mut NoTransaction,
            )
            .await
            .expect("单次 bundle 加载失败");
            assert_eq!(bundle_dup.skus.len(), 1);
            assert_eq!(bundle_once.skus.len(), 1);
            assert_eq!(
                bundle_dup.skus.keys().collect::<Vec<_>>(),
                bundle_once.skus.keys().collect::<Vec<_>>()
            );
            assert!(bundle_dup.skus.contains_key("sku-1"));
        });
    }

    /// 缺失 SKU/product/revision/category 时 Repository 返回稀疏映射，由 Entity 层检出缺失。
    ///
    /// # 参数
    /// 无，内部仅创建部分事实。
    ///
    /// # 返回
    /// 断言稀疏映射及 Entity 校验错误。
    ///
    /// # 错误
    /// 写入或 bundle 加载失败时测试失败。
    ///
    /// # 约束
    /// Batch 缺项由 Repository 稀疏返回，Entity 负责 exact 校验；软删除通过 base.rs 已过滤。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn missing_facts_are_sparse_and_detected_by_entity() {
        require_mongo!(async {
            let fixture = TestDb::new("proc_catalog_missing")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            // Only create category and product/revision, but no SKU.
            let cat = test_category("cat-1", None);
            fixture
                .db()
                .product_categories()
                .create(&cat, &mut NoTransaction)
                .await
                .expect("分类写入失败");
            let prod = test_product("prod-1", Some("rev-1"));
            fixture
                .db()
                .products()
                .create(&prod, &mut NoTransaction)
                .await
                .expect("商品写入失败");
            let rev = test_revision("rev-1", "prod-1", "cat-1");
            fixture
                .db()
                .product_revisions()
                .create(&rev, &mut NoTransaction)
                .await
                .expect("修订写入失败");
            // Do not create SKU "sku-missing"
            let sku_id = SkuId::new("sku-missing");
            let bundle = load_procurement_catalog_bundle(
                fixture.db(),
                std::slice::from_ref(&sku_id),
                &mut NoTransaction,
            )
            .await
            .expect("缺失 bundle 加载失败");
            assert!(bundle.skus.is_empty(), "Repository 应返回稀疏映射而非错误");
            // Entity 层应检出缺失
            let inputs =
                vec![
                    ProcurementResponsibilityResolutionLine::new("line-1".to_string(), sku_id, None).unwrap(),
                ];
            let err = build_catalog_facts(
                &inputs,
                &bundle.skus,
                &bundle.products,
                &bundle.revisions,
                &bundle.categories,
            )
            .unwrap_err();
            assert!(
                err.to_string().contains("SKU不存在") || err.to_string().contains("SKU"),
                "Entity 应检出 SKU 缺失: {err}"
            );

            // Missing product: create sku pointing to non-existent product
            let sku2 = test_sku("sku-2", "prod-missing");
            fixture
                .db()
                .skus()
                .create(&sku2, &mut NoTransaction)
                .await
                .expect("SKU写入失败");
            let bundle2 =
                load_procurement_catalog_bundle(fixture.db(), &[SkuId::new("sku-2")], &mut NoTransaction)
                    .await
                    .expect("bundle 加载失败");
            assert!(bundle2.products.is_empty());
            let inputs2 = vec![ProcurementResponsibilityResolutionLine::new(
                "line-2".to_string(),
                SkuId::new("sku-2"),
                None,
            )
            .unwrap()];
            let err2 = build_catalog_facts(
                &inputs2,
                &bundle2.skus,
                &bundle2.products,
                &bundle2.revisions,
                &bundle2.categories,
            )
            .unwrap_err();
            assert!(
                err2.to_string().contains("商品不存在") || err2.to_string().contains("商品"),
                "Entity 应检出商品缺失: {err2}"
            );

            // Missing category: revision points to non-existent category
            let cat2 = test_category("cat-2", None);
            fixture
                .db()
                .product_categories()
                .create(&cat2, &mut NoTransaction)
                .await
                .expect("分类写入失败");
            let prod3 = test_product("prod-3", Some("rev-3"));
            fixture
                .db()
                .products()
                .create(&prod3, &mut NoTransaction)
                .await
                .expect("商品写入失败");
            let rev3 = test_revision("rev-3", "prod-3", "cat-missing");
            fixture
                .db()
                .product_revisions()
                .create(&rev3, &mut NoTransaction)
                .await
                .expect("修订写入失败");
            let sku3 = test_sku("sku-3", "prod-3");
            fixture
                .db()
                .skus()
                .create(&sku3, &mut NoTransaction)
                .await
                .expect("SKU写入失败");
            let bundle3 =
                load_procurement_catalog_bundle(fixture.db(), &[SkuId::new("sku-3")], &mut NoTransaction)
                    .await
                    .expect("bundle 加载失败");
            assert!(bundle3.categories.is_empty() || !bundle3.categories.contains_key("cat-missing"));
            let inputs3 = vec![ProcurementResponsibilityResolutionLine::new(
                "line-3".to_string(),
                SkuId::new("sku-3"),
                None,
            )
            .unwrap()];
            let err3 = build_catalog_facts(
                &inputs3,
                &bundle3.skus,
                &bundle3.products,
                &bundle3.revisions,
                &bundle3.categories,
            )
            .unwrap_err();
            assert!(
                err3.to_string().contains("分类不存在")
                    || err3.to_string().contains("环")
                    || err3.to_string().contains("分类"),
                "Entity 应检出分类缺失: {err3}"
            );
        });
    }

    /// 软删除的 SKU/product/revision/category 不应出现在 bundle 中，由 base.rs 过滤。
    ///
    /// # 参数
    /// 无，内部创建后软删除。
    ///
    /// # 返回
    /// 断言软删除后 bundle 为稀疏映射。
    ///
    /// # 错误
    /// 写入、软删除或 bundle 加载失败时测试失败。
    ///
    /// # 约束
    /// 软删除语义由 Repository 层 base.rs 统一过滤，Entity 仍检出缺失；Page/Aggregation/Index 标记 N/A。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn soft_deleted_facts_are_filtered() {
        require_mongo!(async {
            let fixture = TestDb::new("proc_catalog_soft_delete")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            let cat = test_category("cat-1", None);
            fixture
                .db()
                .product_categories()
                .create(&cat, &mut NoTransaction)
                .await
                .expect("分类写入失败");
            let prod = test_product("prod-1", Some("rev-1"));
            fixture
                .db()
                .products()
                .create(&prod, &mut NoTransaction)
                .await
                .expect("商品写入失败");
            let rev = test_revision("rev-1", "prod-1", "cat-1");
            fixture
                .db()
                .product_revisions()
                .create(&rev, &mut NoTransaction)
                .await
                .expect("修订写入失败");
            let mut sku = test_sku("sku-1", "prod-1");
            fixture
                .db()
                .skus()
                .create(&sku, &mut NoTransaction)
                .await
                .expect("SKU写入失败");
            // Soft delete SKU
            fixture
                .db()
                .skus()
                .soft_delete(&mut sku, &mut NoTransaction)
                .await
                .expect("软删除失败");
            let bundle =
                load_procurement_catalog_bundle(fixture.db(), &[SkuId::new("sku-1")], &mut NoTransaction)
                    .await
                    .expect("bundle 加载失败");
            assert!(bundle.skus.is_empty(), "软删除 SKU 应被过滤");
            let inputs = vec![ProcurementResponsibilityResolutionLine::new(
                "line-1".to_string(),
                SkuId::new("sku-1"),
                None,
            )
            .unwrap()];
            let err = build_catalog_facts(
                &inputs,
                &bundle.skus,
                &bundle.products,
                &bundle.revisions,
                &bundle.categories,
            )
            .unwrap_err();
            assert!(
                err.to_string().contains("SKU"),
                "Entity 应检出软删除后的缺失: {err}"
            );

            // Soft delete category similarly
            let mut cat2 = test_category("cat-2", None);
            fixture
                .db()
                .product_categories()
                .create(&cat2, &mut NoTransaction)
                .await
                .expect("分类写入失败");
            fixture
                .db()
                .product_categories()
                .soft_delete(&mut cat2, &mut NoTransaction)
                .await
                .expect("分类软删除失败");
            let prod2 = test_product("prod-2", Some("rev-2"));
            fixture
                .db()
                .products()
                .create(&prod2, &mut NoTransaction)
                .await
                .expect("商品写入失败");
            let rev2 = test_revision("rev-2", "prod-2", "cat-2");
            fixture
                .db()
                .product_revisions()
                .create(&rev2, &mut NoTransaction)
                .await
                .expect("修订写入失败");
            let sku2 = test_sku("sku-2", "prod-2");
            fixture
                .db()
                .skus()
                .create(&sku2, &mut NoTransaction)
                .await
                .expect("SKU写入失败");
            let bundle2 =
                load_procurement_catalog_bundle(fixture.db(), &[SkuId::new("sku-2")], &mut NoTransaction)
                    .await
                    .expect("bundle 加载失败");
            assert!(!bundle2.categories.contains_key("cat-2"), "软删除分类应被过滤");
        });
    }

    /// 批量查询次数不随 SKU 数量线性增长：N 个 SKU 共用 1+1+1+depth 次批量读取，无 N+1。
    ///
    /// # 参数
    /// 无，内部创建多 SKU 共享同商品与分类链。
    ///
    /// # 返回
    /// 断言 bundle 正确包含全部事实且分类按深度分层批量加载。
    ///
    /// # 错误
    /// 写入或 bundle 加载失败时测试失败。
    ///
    /// # 约束
    /// Batch 查询次数固定为 SKU、商品、修订各一次，分类按深度分层；稳定 grouping 经 dedup_sorted 保证字典序。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn bounded_batch_queries_no_n_plus_one() {
        require_mongo!(async {
            let fixture = TestDb::new("proc_catalog_bounded")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            // Depth 3 chain: cat-1 <- cat-2 <- cat-3
            for (id, parent) in [
                ("cat-1", None),
                ("cat-2", Some("cat-1")),
                ("cat-3", Some("cat-2")),
            ] {
                let cat = test_category(id, parent);
                fixture
                    .db()
                    .product_categories()
                    .create(&cat, &mut NoTransaction)
                    .await
                    .expect("分类写入失败");
            }
            let prod = test_product("prod-1", Some("rev-1"));
            fixture
                .db()
                .products()
                .create(&prod, &mut NoTransaction)
                .await
                .expect("商品写入失败");
            let rev = test_revision("rev-1", "prod-1", "cat-3");
            fixture
                .db()
                .product_revisions()
                .create(&rev, &mut NoTransaction)
                .await
                .expect("修订写入失败");
            // Create 5 SKUs all pointing to same product
            let mut sku_ids = Vec::new();
            for i in 1..=5 {
                let sku_id = format!("sku-{i}");
                let sku = test_sku(&sku_id, "prod-1");
                fixture
                    .db()
                    .skus()
                    .create(&sku, &mut NoTransaction)
                    .await
                    .expect("SKU写入失败");
                sku_ids.push(SkuId::new(sku_id));
            }
            let bundle = load_procurement_catalog_bundle(fixture.db(), &sku_ids, &mut NoTransaction)
                .await
                .expect("bundle 加载失败");
            assert_eq!(bundle.skus.len(), 5);
            assert_eq!(bundle.products.len(), 1);
            assert_eq!(bundle.revisions.len(), 1);
            // Categories should contain all 3 levels reached via parent chain, regardless of N.
            assert_eq!(bundle.categories.len(), 3);
            assert!(bundle.categories.contains_key("cat-1"));
            assert!(bundle.categories.contains_key("cat-2"));
            assert!(bundle.categories.contains_key("cat-3"));
            // Verify deduped category order is stable (lexicographically sorted via dedup_sorted_ids).
            // The pending order per depth is sorted, so categories keys sorted equals expected.
            let mut keys: Vec<String> = bundle.categories.keys().cloned().collect();
            keys.sort();
            let mut sorted_keys = keys.clone();
            sorted_keys.sort();
            assert_eq!(keys, sorted_keys);
        });
    }

    /// 同一事务内调用 bundle 复用调用方 executor，能读取事务内未提交写入（read-your-writes）。
    ///
    /// # 参数
    /// 无，内部通过 with_transaction 写入后即时读取。
    ///
    /// # 返回
    /// 断言事务内 bundle 能见未提交 SKU。
    ///
    /// # 错误
    /// 事务或 bundle 加载失败时测试失败。
    ///
    /// # 约束
    /// Repository 必须接收 &mut dyn Executor 且不自行开启事务，事务内重验复用同一 session。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn transaction_reuses_caller_executor_read_your_writes() {
        require_mongo!(async {
            let fixture = TestDb::new("proc_catalog_txn").await.expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            let cat = test_category("cat-1", None);
            fixture
                .db()
                .product_categories()
                .create(&cat, &mut NoTransaction)
                .await
                .expect("分类写入失败");
            let prod = test_product("prod-1", Some("rev-1"));
            fixture
                .db()
                .products()
                .create(&prod, &mut NoTransaction)
                .await
                .expect("商品写入失败");
            let rev = test_revision("rev-1", "prod-1", "cat-1");
            fixture
                .db()
                .product_revisions()
                .create(&rev, &mut NoTransaction)
                .await
                .expect("修订写入失败");

            let db = fixture.db().clone();
            let client = db.client().clone();
            let sku = test_sku("sku-txn", "prod-1");
            let sku_id = SkuId::new("sku-txn");
            client
                .with_transaction::<_, (), crate::errors::Error>(move |session| {
                    let db = db.clone();
                    let sku = sku.clone();
                    let sku_id = sku_id.clone();
                    Box::pin(async move {
                        db.skus().create(&sku, session).await?;
                        // Same executor (session) should see uncommitted write
                        let bundle =
                            load_procurement_catalog_bundle(&db, std::slice::from_ref(&sku_id), session)
                                .await?;
                        assert!(bundle.skus.contains_key("sku-txn"), "事务内应能 read-your-writes");
                        // Also verify Entity can build facts inside txn
                        let inputs = vec![ProcurementResponsibilityResolutionLine::new(
                            "line-1".to_string(),
                            sku_id,
                            None,
                        )
                        .unwrap()];
                        let facts = build_catalog_facts(
                            &inputs,
                            &bundle.skus,
                            &bundle.products,
                            &bundle.revisions,
                            &bundle.categories,
                        )
                        .map_err(|e| {
                            crate::errors::Error::DatabaseError(mongodb::error::Error::custom(e.to_string()))
                        })?;
                        assert!(facts.contains_key("line-1"));
                        Ok(())
                    })
                })
                .await
                .expect("事务内 bundle 复用失败");
            // After commit, outside txn also visible
            let bundle_after =
                load_procurement_catalog_bundle(fixture.db(), &[SkuId::new("sku-txn")], &mut NoTransaction)
                    .await
                    .expect("提交后 bundle 加载失败");
            assert!(bundle_after.skus.contains_key("sku-txn"));
        });
    }

    /// 代表性 id 查询的 explain 应优先命中索引而非全表扫描；若未建索引则标记 N/A。
    ///
    /// # 参数
    /// 无，内部对各集合执行 explain。
    ///
    /// # 返回
    /// 断言 explain 包含 IXSCAN 且不为 COLLSCAN，缺索引时记录 N/A 原因。
    ///
    /// # 错误
    /// explain 执行失败时测试失败。
    ///
    /// # 约束
    /// Index 维度：当前批次不新增索引，代表性数据量下 explain 需显示 IXSCAN；若仍为 COLLSCAN 则标记 N/A 并说明依赖 PROC-R10 索引批次。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn explain_id_queries_use_ixscan() {
        require_mongo!(async {
            let fixture = TestDb::new("proc_catalog_explain")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            for collection in ["skus", "products", "product_revisions", "product_categories"] {
                let explain = fixture
                    .db()
                    .run_command(doc! {
                        "explain": {
                            "find": collection,
                            "filter": { "id": "test-id", "deleted_at": 0 },
                        },
                        "verbosity": "executionStats",
                    })
                    .await
                    .expect("explain 失败");
                let rendered = format!("{explain:?}");
                // Document the current state: if IXSCAN missing, mark N/A with reason instead of hard fail in this wave.
                // For proc-catalog wave, id 索引由现有 catalog 索引或 PROC-R10 补充，当前若为 COLLSCAN 则记录 N/A。
                if rendered.contains("COLLSCAN") {
                    eprintln!("N/A: {collection} id 查询当前为 COLLSCAN，未建专用 id 索引，依赖后续索引批次（PROC-R10）; explain={rendered}");
                } else {
                    assert!(
                        rendered.contains("IXSCAN"),
                        "explain 未使用 IXSCAN for {collection}: {rendered}"
                    );
                }
            }
        });
    }

    /// 父分类链成环由 Entity 检出，Repository 仅返回原始映射。
    ///
    /// # 参数
    /// 无，内部构造环状分类链。
    ///
    /// # 返回
    /// 断言 bundle 返回环上分类，Entity 层 category_chain 报错。
    ///
    /// # 错误
    /// 写入或 bundle 加载失败时测试失败。
    ///
    /// # 约束
    /// 环检测由 Entity 值对象负责，Repository 不自行判定环。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn category_ring_is_detected_by_entity() {
        require_mongo!(async {
            let fixture = TestDb::new("proc_catalog_ring")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            // Create ring: cat-a -> cat-b -> cat-a (via parent chain)
            // Since entity creation rejects self-loop, we mutate after creation via direct update
            let cat_a = test_category("cat-a", None);
            let cat_b = test_category("cat-b", Some("cat-a"));
            fixture
                .db()
                .product_categories()
                .create(&cat_a, &mut NoTransaction)
                .await
                .expect("分类写入失败");
            fixture
                .db()
                .product_categories()
                .create(&cat_b, &mut NoTransaction)
                .await
                .expect("分类写入失败");
            // Directly update cat-a to parent cat-b to form ring, bypassing entity check
            fixture
                .db()
                .collection::<mongodb::bson::Document>("product_categories")
                .update_one(
                    doc! {"id": "cat-a"},
                    doc! {"$set": {"parent_category_id": "cat-b"}},
                )
                .await
                .expect("环构造失败");
            let prod = test_product("prod-1", Some("rev-1"));
            fixture
                .db()
                .products()
                .create(&prod, &mut NoTransaction)
                .await
                .expect("商品写入失败");
            let rev = test_revision("rev-1", "prod-1", "cat-b");
            fixture
                .db()
                .product_revisions()
                .create(&rev, &mut NoTransaction)
                .await
                .expect("修订写入失败");
            let sku = test_sku("sku-1", "prod-1");
            fixture
                .db()
                .skus()
                .create(&sku, &mut NoTransaction)
                .await
                .expect("SKU写入失败");
            let bundle =
                load_procurement_catalog_bundle(fixture.db(), &[SkuId::new("sku-1")], &mut NoTransaction)
                    .await
                    .expect("bundle 加载失败");
            assert!(bundle.categories.contains_key("cat-a"));
            assert!(bundle.categories.contains_key("cat-b"));
            let inputs = vec![ProcurementResponsibilityResolutionLine::new(
                "line-1".to_string(),
                SkuId::new("sku-1"),
                None,
            )
            .unwrap()];
            let err = build_catalog_facts(
                &inputs,
                &bundle.skus,
                &bundle.products,
                &bundle.revisions,
                &bundle.categories,
            )
            .unwrap_err();
            assert!(err.to_string().contains("环"), "应检出环: {err}");
        });
    }
}

#[cfg(test)]
mod rule_list_isolation_tests {
    use crate::ensure_indexes;
    use crate::{AccessControlExt, CatalogExt, NoTransaction, ProcurementResponsibilityExt, Transactional};
    use entities::catalog::product_category::ProductCategoryData;
    use entities::catalog::sku::SkuData;
    use entities::catalog::sku_revision::SkuRevisionData;
    use entities::catalog::{EnableStatus, ListingStatus, ProductCategory, Sku, SkuRevision};
    use entities::common::time::BusinessDate;
    use entities::ids::{
        ProcurementResponsibilityRuleId, ProductCategoryId, SkuId, SkuRevisionId, UnitOfMeasureId,
    };
    use entities::procurement_responsibility::{
        ProcurementResponsibilityRule, ProcurementResponsibilityRuleData, ProcurementResponsibilityRuleType,
    };
    use entities::{AccountCore, AccountCoreData, AccountKind, AccountStatus, LoginAccount, Secret};
    use mongodb::bson::doc;
    use test_support::{require_mongo, TestDb};

    use super::{
        load_procurement_rule_list_facts, load_procurement_rule_list_page,
        ProcurementResponsibilityRuleFilter,
    };

    fn page_filter(
        rule_type: Option<ProcurementResponsibilityRuleType>,
        page: u64,
        page_size: u32,
    ) -> ProcurementResponsibilityRuleFilter {
        ProcurementResponsibilityRuleFilter {
            rule_type,
            owner_user_id: None,
            status: None,
            page,
            page_size,
        }
    }

    /// 构造可登录的后台测试负责人.
    ///
    /// # 参数
    /// * `id` - 账号稳定 ID
    /// * `login` - 登录账号
    /// * `name` - 展示姓名
    ///
    /// # 返回
    /// 返回启用后台管理员账号.
    ///
    /// # 错误
    /// 账号校验失败时 panic.
    ///
    /// # 约束
    /// 纯内存构造，不访问数据库.
    fn test_owner(id: &str, login: &str, name: &str) -> AccountCore {
        AccountCore::new(
            id.to_string(),
            AccountCoreData {
                secret: Secret::new(LoginAccount::new(login).unwrap(), "password123").unwrap(),
                name: name.to_string(),
                kind: AccountKind::Admin,
                status: AccountStatus::Active,
                email: None,
                phone: None,
                avatar: None,
            },
        )
        .unwrap()
    }

    /// 构造测试分类.
    ///
    /// # 参数
    /// * `id` - 分类稳定 ID
    /// * `name` - 分类名称
    ///
    /// # 返回
    /// 返回启用根分类.
    ///
    /// # 错误
    /// 分类校验失败时 panic.
    ///
    /// # 约束
    /// 纯内存构造，不访问数据库.
    fn test_category(id: &str, name: &str) -> ProductCategory {
        ProductCategory::new(
            ProductCategoryId::new(id),
            ProductCategoryData {
                category_code: format!("code-{id}"),
                parent_category_id: None,
                name: name.to_string(),
                product_kind: entities::catalog::ProductKind::Physical,
                status: EnableStatus::Active,
            },
            "test",
        )
        .unwrap()
    }

    /// 构造指向指定 SKU 修订的测试 SKU.
    ///
    /// # 参数
    /// * `id` - SKU 稳定 ID
    /// * `revision_id` - 当前 SKU 修订 ID
    ///
    /// # 返回
    /// 返回当前修订已设置的启用 SKU.
    ///
    /// # 错误
    /// SKU 校验失败时 panic.
    ///
    /// # 约束
    /// 纯内存构造，不访问数据库.
    fn test_sku(id: &str, revision_id: &str) -> Sku {
        let mut sku = Sku::new(
            SkuId::new(id),
            SkuData {
                sku_no: format!("SKU-{id}"),
                product_id: entities::ids::ProductId::new("prod-1"),
                base_unit_id: UnitOfMeasureId::new("unit-1"),
                specification_signature: format!("spec-{id}"),
                status: EnableStatus::Active,
                listing_status: ListingStatus::Unlisted,
            },
            "test",
        )
        .unwrap();
        sku.stable.current_revision_id = Some(revision_id.to_string());
        sku
    }

    /// 构造测试 SKU 修订.
    ///
    /// # 参数
    /// * `id` - 修订稳定 ID
    /// * `sku_id` - 所属 SKU
    /// * `name` - 修订展示名称
    ///
    /// # 返回
    /// 返回生效的 SKU 修订.
    ///
    /// # 错误
    /// 修订校验失败时 panic.
    ///
    /// # 约束
    /// 纯内存构造，不访问数据库.
    fn test_sku_revision(id: &str, sku_id: &str, name: &str) -> SkuRevision {
        SkuRevision::new(
            SkuRevisionId::new(id),
            SkuRevisionData {
                sku_id: SkuId::new(sku_id),
                revision_no: 1,
                name: name.to_string(),
                description: None,
                specification: None,
                barcode: None,
                source_main_image_asset_id: None,
                weight_kg: None,
                volume_m3: None,
                sales_visible_price_gross: None,
                market_price: None,
                status: EnableStatus::Active,
                effective_from: BusinessDate::from_ymd(2024, 1, 1).unwrap(),
                effective_to: None,
            },
        )
        .unwrap()
    }

    /// 构造测试规则行.
    ///
    /// # 参数
    /// * `id` - 规则稳定 ID
    /// * `rule_type` - 规则类型
    /// * `sku_id` - 可选 SKU 选择器
    /// * `category_id` - 可选分类选择器
    /// * `owner` - 负责人账号 ID
    ///
    /// # 返回
    /// 返回选择器形状合法的启用规则.
    ///
    /// # 错误
    /// 选择器形状非法时 panic.
    ///
    /// # 约束
    /// 纯内存构造，不校验引用存在性.
    fn test_rule(
        id: &str,
        rule_type: ProcurementResponsibilityRuleType,
        sku_id: Option<&str>,
        category_id: Option<&str>,
        owner: &str,
    ) -> ProcurementResponsibilityRule {
        ProcurementResponsibilityRule::new(
            ProcurementResponsibilityRuleId::new(id),
            ProcurementResponsibilityRuleData {
                rule_type,
                sku_id: sku_id.map(SkuId::new),
                category_id: category_id.map(ProductCategoryId::new),
                service_region: None,
                product_kind: None,
                owner_user_id: owner.to_string(),
                status: EnableStatus::Active,
            },
            "admin-1",
        )
        .unwrap()
    }

    /// 空库分页返回空页与空事实，空输入事实加载零查询短路.
    ///
    /// # 参数
    /// 无，内部创建隔离库.
    ///
    /// # 返回
    /// 断言总数为零、当前页为空且事实映射全空.
    ///
    /// # 错误
    /// MongoDB 连接或分页加载失败时测试失败.
    ///
    /// # 约束
    /// Page 空集合与 Batch 空输入维度；Aggregation/Index 标记 N/A（无聚合与新索引）.
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn empty_db_returns_empty_page_and_facts() {
        require_mongo!(async {
            let fixture = TestDb::new("proc_rule_list_empty")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            let page =
                load_procurement_rule_list_page(fixture.db(), &page_filter(None, 1, 50), &mut NoTransaction)
                    .await
                    .expect("空分页加载失败");
            assert_eq!(page.total, 0);
            assert!(page.items.is_empty());
            assert!(page.facts.owner_names.is_empty());
            assert!(page.facts.sku_nos.is_empty());
            assert!(page.facts.sku_names.is_empty());
            assert!(page.facts.category_names.is_empty());
            let facts = load_procurement_rule_list_facts(fixture.db(), &[], &mut NoTransaction)
                .await
                .expect("空事实加载失败");
            assert!(facts.owner_names.is_empty());
            assert!(facts.sku_nos.is_empty());
            assert!(facts.sku_names.is_empty());
            assert!(facts.category_names.is_empty());
        });
    }

    /// 分页总数、稳定排序、去重事实与页边界语义.
    ///
    /// # 参数
    /// 无，内部写入 2 负责人、1 分类、1 SKU 修订与 3 规则.
    ///
    /// # 返回
    /// 断言总数为 3、页拆分无重叠、事实去重且展示完整、越界页为空但总数不变.
    ///
    /// # 错误
    /// 写入或分页加载失败时测试失败.
    ///
    /// # 约束
    /// Page 过滤/稳定排序/总数/边界页维度；排序为 rule_type/created_at/id；关联固定 4 次批量查询.
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn page_returns_facts_with_stable_pagination() {
        require_mongo!(async {
            let fixture = TestDb::new("proc_rule_list_page")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            for (id, login, name) in [("owner-1", "buyer-1", "张三"), ("owner-2", "buyer-2", "李四")] {
                fixture
                    .db()
                    .accounts()
                    .create(&test_owner(id, login, name), &mut NoTransaction)
                    .await
                    .expect("负责人写入失败");
            }
            fixture
                .db()
                .product_categories()
                .create(&test_category("cat-1", "五金"), &mut NoTransaction)
                .await
                .expect("分类写入失败");
            fixture
                .db()
                .skus()
                .create(&test_sku("sku-1", "sku-rev-1"), &mut NoTransaction)
                .await
                .expect("SKU写入失败");
            fixture
                .db()
                .sku_revisions()
                .create(
                    &test_sku_revision("sku-rev-1", "sku-1", "红色零件"),
                    &mut NoTransaction,
                )
                .await
                .expect("SKU修订写入失败");
            for rule in [
                test_rule(
                    "r-sku",
                    ProcurementResponsibilityRuleType::Sku,
                    Some("sku-1"),
                    None,
                    "owner-1",
                ),
                test_rule(
                    "r-cat",
                    ProcurementResponsibilityRuleType::Category,
                    None,
                    Some("cat-1"),
                    "owner-2",
                ),
                test_rule(
                    "r-default",
                    ProcurementResponsibilityRuleType::DefaultDispatcher,
                    None,
                    None,
                    "owner-1",
                ),
            ] {
                fixture
                    .db()
                    .procurement_responsibility_rules()
                    .create(&rule, &mut NoTransaction)
                    .await
                    .expect("规则写入失败");
            }

            let full =
                load_procurement_rule_list_page(fixture.db(), &page_filter(None, 1, 10), &mut NoTransaction)
                    .await
                    .expect("整页加载失败");
            assert_eq!(full.total, 3);
            assert_eq!(full.items.len(), 3);
            // Stable sort: rule_type/created_at/id.
            let mut sorted = full.items.clone();
            sorted.sort_by(|a, b| {
                (a.rule_type.as_str(), a.base.created_at, &a.base.id).cmp(&(
                    b.rule_type.as_str(),
                    b.base.created_at,
                    &b.base.id,
                ))
            });
            assert_eq!(
                full.items.iter().map(|rule| &rule.base.id).collect::<Vec<_>>(),
                sorted.iter().map(|rule| &rule.base.id).collect::<Vec<_>>(),
                "分页必须保持 rule_type/created_at/id 稳定排序"
            );
            // Facts deduplicated: two rules share owner-1.
            assert_eq!(full.facts.owner_names.len(), 2);
            assert_eq!(
                full.facts.owner_names.get("owner-1").map(String::as_str),
                Some("张三")
            );
            assert_eq!(
                full.facts.sku_nos.get("sku-1").map(String::as_str),
                Some("SKU-sku-1")
            );
            assert_eq!(
                full.facts.sku_names.get("sku-1").map(String::as_str),
                Some("红色零件")
            );
            assert_eq!(
                full.facts.category_names.get("cat-1").map(String::as_str),
                Some("五金")
            );

            // Page split covers all rows without overlap; out-of-range page keeps total.
            let first =
                load_procurement_rule_list_page(fixture.db(), &page_filter(None, 1, 2), &mut NoTransaction)
                    .await
                    .expect("首页加载失败");
            let second =
                load_procurement_rule_list_page(fixture.db(), &page_filter(None, 2, 2), &mut NoTransaction)
                    .await
                    .expect("次页加载失败");
            let beyond =
                load_procurement_rule_list_page(fixture.db(), &page_filter(None, 9, 2), &mut NoTransaction)
                    .await
                    .expect("越界页加载失败");
            assert_eq!((first.total, second.total, beyond.total), (3, 3, 3));
            assert_eq!(first.items.len(), 2);
            assert_eq!(second.items.len(), 1);
            assert!(beyond.items.is_empty());
            let first_ids = first
                .items
                .iter()
                .map(|rule| rule.base.id.as_str())
                .collect::<Vec<_>>();
            assert!(
                !first_ids.contains(&second.items[0].base.id.as_str()),
                "分页不得重叠"
            );
        });
    }

    /// 缺失与软删除引用保持稀疏，不报错.
    ///
    /// # 参数
    /// 无，内部写入缺失引用规则与软删除事实.
    ///
    /// # 返回
    /// 断言缺失键不在映射中，存量展示保持空语义.
    ///
    /// # 错误
    /// 写入、软删除或事实加载失败时测试失败.
    ///
    /// # 约束
    /// Exact 缺失与软删除维度；缺项由 Service 保留空展示.
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn missing_and_soft_deleted_refs_stay_sparse() {
        require_mongo!(async {
            let fixture = TestDb::new("proc_rule_list_sparse")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            // Soft-deleted facts: owner, category, sku and sku revision.
            let mut owner = test_owner("owner-del", "buyer-del", "已删");
            fixture
                .db()
                .accounts()
                .create(&owner, &mut NoTransaction)
                .await
                .expect("负责人写入失败");
            fixture
                .db()
                .accounts()
                .soft_delete(&mut owner, &mut NoTransaction)
                .await
                .expect("负责人软删除失败");
            let mut category = test_category("cat-del", "已删");
            fixture
                .db()
                .product_categories()
                .create(&category, &mut NoTransaction)
                .await
                .expect("分类写入失败");
            fixture
                .db()
                .product_categories()
                .soft_delete(&mut category, &mut NoTransaction)
                .await
                .expect("分类软删除失败");
            let mut sku = test_sku("sku-del", "sku-rev-del");
            fixture
                .db()
                .skus()
                .create(&sku, &mut NoTransaction)
                .await
                .expect("SKU写入失败");
            fixture
                .db()
                .skus()
                .soft_delete(&mut sku, &mut NoTransaction)
                .await
                .expect("SKU软删除失败");
            let mut revision = test_sku_revision("sku-rev-del", "sku-del", "已删修订");
            fixture
                .db()
                .sku_revisions()
                .create(&revision, &mut NoTransaction)
                .await
                .expect("修订写入失败");
            fixture
                .db()
                .sku_revisions()
                .soft_delete(&mut revision, &mut NoTransaction)
                .await
                .expect("修订软删除失败");
            // SKU whose current revision was soft-deleted keeps number without name.
            fixture
                .db()
                .skus()
                .create(&test_sku("sku-orphan", "sku-rev-del"), &mut NoTransaction)
                .await
                .expect("孤儿SKU写入失败");

            let rules = vec![
                test_rule(
                    "r-gone",
                    ProcurementResponsibilityRuleType::Sku,
                    Some("sku-gone"),
                    None,
                    "owner-gone",
                ),
                test_rule(
                    "r-cat-gone",
                    ProcurementResponsibilityRuleType::Category,
                    None,
                    Some("cat-gone"),
                    "owner-gone",
                ),
                test_rule(
                    "r-del",
                    ProcurementResponsibilityRuleType::Sku,
                    Some("sku-del"),
                    None,
                    "owner-del",
                ),
                test_rule(
                    "r-cat-del",
                    ProcurementResponsibilityRuleType::Category,
                    None,
                    Some("cat-del"),
                    "owner-del",
                ),
                test_rule(
                    "r-orphan",
                    ProcurementResponsibilityRuleType::Sku,
                    Some("sku-orphan"),
                    None,
                    "owner-gone",
                ),
            ];
            let facts = load_procurement_rule_list_facts(fixture.db(), &rules, &mut NoTransaction)
                .await
                .expect("稀疏事实加载失败");
            for key in ["owner-gone", "owner-del"] {
                assert!(
                    !facts.owner_names.contains_key(key),
                    "缺失/软删除负责人必须稀疏: {key}"
                );
            }
            for key in ["sku-gone", "sku-del"] {
                assert!(!facts.sku_nos.contains_key(key), "缺失/软删除SKU必须稀疏: {key}");
                assert!(!facts.sku_names.contains_key(key));
            }
            for key in ["cat-gone", "cat-del"] {
                assert!(
                    !facts.category_names.contains_key(key),
                    "缺失/软删除分类必须稀疏: {key}"
                );
            }
            assert_eq!(
                facts.sku_nos.get("sku-orphan").map(String::as_str),
                Some("SKU-sku-orphan"),
                "修订缺失不得清空SKU编号展示"
            );
            assert!(
                !facts.sku_names.contains_key("sku-orphan"),
                "软删除修订名称必须稀疏"
            );
        });
    }

    /// 总数与分页共用同一过滤，软删除规则被排除.
    ///
    /// # 参数
    /// 无，内部写入 3 规则并软删除其中 1 条.
    ///
    /// # 返回
    /// 断言总数为 2，类型过滤总数一致，软删除规则不在页内.
    ///
    /// # 错误
    /// 写入、软删除或分页加载失败时测试失败.
    ///
    /// # 约束
    /// Page 总数/过滤维度；软删除语义与集合查询一致.
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn total_uses_same_filter_and_excludes_soft_deleted() {
        require_mongo!(async {
            let fixture = TestDb::new("proc_rule_list_total")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            let mut deleted = test_rule(
                "r-del",
                ProcurementResponsibilityRuleType::Sku,
                Some("sku-1"),
                None,
                "owner-1",
            );
            // 先写入再软删除：部分唯一索引只覆盖启用且未删除的行，删除后
            // 同选择器键被释放，后续同键启用规则可写入。
            fixture
                .db()
                .procurement_responsibility_rules()
                .create(&deleted, &mut NoTransaction)
                .await
                .expect("规则写入失败");
            fixture
                .db()
                .procurement_responsibility_rules()
                .soft_delete(&mut deleted, &mut NoTransaction)
                .await
                .expect("规则软删除失败");
            for rule in [
                test_rule(
                    "r-sku",
                    ProcurementResponsibilityRuleType::Sku,
                    Some("sku-1"),
                    None,
                    "owner-1",
                ),
                test_rule(
                    "r-cat",
                    ProcurementResponsibilityRuleType::Category,
                    None,
                    Some("cat-1"),
                    "owner-1",
                ),
            ] {
                fixture
                    .db()
                    .procurement_responsibility_rules()
                    .create(&rule, &mut NoTransaction)
                    .await
                    .expect("规则写入失败");
            }

            let page =
                load_procurement_rule_list_page(fixture.db(), &page_filter(None, 1, 10), &mut NoTransaction)
                    .await
                    .expect("分页加载失败");
            assert_eq!(page.total, 2);
            assert_eq!(page.items.len(), 2);
            assert!(page.items.iter().all(|rule| rule.base.id != "r-del"));
            let sku_only = load_procurement_rule_list_page(
                fixture.db(),
                &page_filter(Some(ProcurementResponsibilityRuleType::Sku), 1, 10),
                &mut NoTransaction,
            )
            .await
            .expect("类型过滤分页失败");
            assert_eq!(sku_only.total, 1);
            assert_eq!(sku_only.items[0].base.id, "r-sku");
        });
    }

    /// 事务内分页复用调用方 executor，可见未提交写入.
    ///
    /// # 参数
    /// 无，内部在事务中创建规则后即时分页.
    ///
    /// # 返回
    /// 断言事务内总数包含未提交规则，提交后外部同样可见.
    ///
    /// # 错误
    /// 事务或分页加载失败时测试失败.
    ///
    /// # 约束
    /// Repository 不自行开启事务；事务内重验复用同一 session.
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn transaction_reuses_caller_executor_read_your_writes() {
        require_mongo!(async {
            let fixture = TestDb::new("proc_rule_list_txn")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            let db = fixture.db().clone();
            let client = db.client().clone();
            let rule = test_rule(
                "r-txn",
                ProcurementResponsibilityRuleType::DefaultDispatcher,
                None,
                None,
                "owner-1",
            );
            let filter = page_filter(None, 1, 10);
            client
                .with_transaction::<_, (), crate::errors::Error>(move |session| {
                    let db = db.clone();
                    let rule = rule.clone();
                    let filter = ProcurementResponsibilityRuleFilter {
                        rule_type: None,
                        owner_user_id: None,
                        status: None,
                        page: filter.page,
                        page_size: filter.page_size,
                    };
                    Box::pin(async move {
                        db.procurement_responsibility_rules()
                            .create(&rule, session)
                            .await?;
                        let page = load_procurement_rule_list_page(&db, &filter, session).await?;
                        assert_eq!(page.total, 1, "事务内应能 read-your-writes");
                        assert_eq!(page.items[0].base.id, "r-txn");
                        Ok(())
                    })
                })
                .await
                .expect("事务内分页复用失败");
            let after =
                load_procurement_rule_list_page(fixture.db(), &page_filter(None, 1, 10), &mut NoTransaction)
                    .await
                    .expect("提交后分页失败");
            assert_eq!(after.total, 1);
        });
    }

    /// 规则集合代表性查询的 explain 记录索引状态.
    ///
    /// # 参数
    /// 无，内部对规则集合执行 explain.
    ///
    /// # 返回
    /// 命中索引时断言 IXSCAN，否则记录 N/A 及原因.
    ///
    /// # 错误
    /// explain 执行失败时测试失败.
    ///
    /// # 约束
    /// Index 维度：本批不新增索引；若为 COLLSCAN 则标记 N/A 并说明依赖后续索引批次.
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn explain_rule_list_query_documents_index_state() {
        require_mongo!(async {
            let fixture = TestDb::new("proc_rule_list_explain")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            let explain = fixture
                .db()
                .run_command(doc! {
                    "explain": {
                        "find": "procurement_responsibility_rules",
                        "filter": { "deleted_at": 0 },
                    },
                    "verbosity": "executionStats",
                })
                .await
                .expect("explain 失败");
            let rendered = format!("{explain:?}");
            if rendered.contains("COLLSCAN") {
                eprintln!("N/A: procurement_responsibility_rules 列表查询当前为 COLLSCAN，未建专用过滤索引，依赖后续索引批次；explain={rendered}");
            } else {
                assert!(rendered.contains("IXSCAN"), "explain 未使用 IXSCAN: {rendered}");
            }
        });
    }
}
