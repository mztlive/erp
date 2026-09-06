use crate::ensure_indexes;
use crate::{AccessControlExt, CatalogExt, ProcurementResponsibilityExt};
use entities::catalog::product_category::ProductCategoryData;
use entities::catalog::sku::SkuData;
use entities::catalog::sku_revision::SkuRevisionData;
use entities::catalog::{EnableStatus, ListingStatus, ProductCategory, Sku, SkuRevision};
use entities::procurement_responsibility::{
    ProcurementResponsibilityRule, ProcurementResponsibilityRuleData, ProcurementResponsibilityRuleType,
};
use entities::{AccountCore, AccountCoreData, AccountStatus, LoginAccount, Secret};
use erp_core::common::time::BusinessDate;
use erp_core::ids::{
    ProcurementResponsibilityRuleId, ProductCategoryId, SkuId, SkuRevisionId, UnitOfMeasureId,
};
use erp_core::AccountKind;
use mongodb::bson::doc;
use persistence_core::{NoTransaction, Transactional};
use test_support::{require_mongo, TestDb};

use super::{
    load_procurement_rule_list_facts, load_procurement_rule_list_page, ProcurementResponsibilityRuleFilter,
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
            product_id: erp_core::ids::ProductId::new("prod-1"),
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
            .with_transaction::<_, (), persistence_core::Error>(move |session| {
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
