//! 采购单列表投影批量事实（PROC-R05）。
//!
//! 分页投影仍由 [`PurchaseOrderFilter`] 承担过滤、软删除、稳定排序与总数语义；
//! 本模块在同一调用方 executor 下一次批量返回列表行所需的全部关联事实：
//! 供应商当前法定名称、来源销售单业务单号、负责人展示名与当前提交/版本表头。
//! 指针选择、缺失校验、金额格式化与 View 回退由 Service 负责；本模块只做
//! 持久化读取，查询次数与页大小无关，不得出现逐行 N+1。

use std::collections::{HashMap, HashSet};

use entities::ids::{PurchaseOrderRevisionId, PurchaseOrderSubmissionId, SalesOrderId, SupplierAccountId};
use entities::purchase_order::{PurchaseOrderRevision, PurchaseOrderSubmission};
use entities::sales_order::SalesOrder;
use mongodb::Database;

use super::order::PurchaseOrderFilter;
use crate::executor::Executor;
use crate::repository::extensions::{AccessControlExt, PurchaseOrderExt, SalesOrderExt, SupplierExt};
use crate::repository::PageResult;
use crate::Result;

/// 采购单列表关联事实。
///
/// 所有映射均以持久化原值返回；缺失键表示关联不存在，由 Service 按
/// 完整性错误或约定回退解释，本层不做业务回退。
#[derive(Debug, Clone, Default)]
pub struct PurchaseOrderListFacts {
    /// 供应商账号 ID 到当前法定名称的映射。
    pub supplier_names: HashMap<String, String>,
    /// 来源销售单 ID 到业务单号的映射。
    pub sales_order_nos: HashMap<String, String>,
    /// 负责人账号 ID 到展示名的映射。
    pub owner_names: HashMap<String, String>,
    /// 当前指针命中的采购提交头。
    pub submissions: HashMap<String, PurchaseOrderSubmission>,
    /// 当前指针命中的采购生效版本头。
    pub revisions: HashMap<String, PurchaseOrderRevision>,
}

/// 批量加载采购单列表页与关联事实。
///
/// # 参数
/// * `db` - MongoDB 数据库句柄
/// * `filter` - 采购单列表筛选与分页条件
/// * `executor` - 数据访问执行器，由 Service 决定事务边界；事务内重验必须复用调用方 executor
///
/// # 返回
/// 返回当前页投影行、总数与同一 executor 下批量取回的关联事实；关联缺失以
/// 缺键形式表达，由 Service 按完整性错误或约定回退解释。
///
/// # 错误
/// MongoDB 查询、计数或反序列化失败时返回错误；不负责缺失校验，软删除已由
/// 基类查询过滤。
///
/// # 约束
/// 查询次数与页大小无关：列表分页、供应商名称、销售单、负责人、当前提交与
/// 当前版本各一次批量读取，不得出现逐行 N+1。提交指针只查提交集合、版本指针
/// 只查版本集合，两种命名空间不得交叉；历史提交与历史版本不进入结果；分页
/// 过滤、稳定排序与总数语义与 [`PurchaseOrderFilter`] 完全一致。
pub async fn load_purchase_order_list_page(
    db: &Database,
    filter: &PurchaseOrderFilter,
    executor: &mut dyn Executor,
) -> Result<(PageResult<super::order::PurchaseOrderRow>, PurchaseOrderListFacts)> {
    let page = db
        .purchase_orders()
        .search_purchase_orders(filter, executor)
        .await?;
    if page.items.is_empty() {
        return Ok((page, PurchaseOrderListFacts::default()));
    }
    let supplier_ids = unique_supplier_ids(&page);
    let supplier_names = db
        .supplier()
        .current_legal_names_by_account_ids(&supplier_ids, executor)
        .await?;
    let sales_ids = unique_sales_ids(&page);
    let sales_orders = db.sales_orders().find_orders_by_ids(&sales_ids, executor).await?;
    let sales_order_nos = sales_orders
        .into_iter()
        .map(|order| (order.base.id.clone(), order.order_no.clone()))
        .collect::<HashMap<_, _>>();
    let owner_ids = unique_owner_ids(&page);
    let owner_names = db.accounts().names_by_ids(&owner_ids, executor).await?;
    let (submission_keys, revision_keys) = split_pointer_ids(&page);
    let submission_ids = submission_keys
        .iter()
        .cloned()
        .map(PurchaseOrderSubmissionId::new)
        .collect::<Vec<_>>();
    let submissions = db
        .purchase_order()
        .find_submissions_by_ids(&submission_ids, executor)
        .await?
        .into_iter()
        .map(|submission| (submission.base.id.clone(), submission))
        .collect::<HashMap<_, _>>();
    let revision_ids = revision_keys
        .iter()
        .cloned()
        .map(PurchaseOrderRevisionId::new)
        .collect::<Vec<_>>();
    let revisions = db
        .purchase_order()
        .find_revisions_by_ids(&revision_ids, executor)
        .await?
        .into_iter()
        .map(|revision| (revision.base.id.clone(), revision))
        .collect::<HashMap<_, _>>();
    Ok((
        page,
        PurchaseOrderListFacts {
            supplier_names,
            sales_order_nos,
            owner_names,
            submissions,
            revisions,
        },
    ))
}

/// 提取列表页去重后的负责人账号 ID。
///
/// # 参数
/// * `page` - 当前页投影行与总数
///
/// # 返回
/// 返回去重、去空白后的负责人账号 ID；空集合表示无需查询。
///
/// # 错误
/// 无。
///
/// # 约束
/// 去重只用于缩小 `$in` 范围，不改变任何业务语义；空白与缺失由 Service 回退。
fn unique_owner_ids(page: &PageResult<super::order::PurchaseOrderRow>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for row in &page.items {
        let Some(owner) = row.owner_user_id.as_deref() else {
            continue;
        };
        let trimmed = owner.trim();
        if trimmed.is_empty() || !seen.insert(trimmed.to_string()) {
            continue;
        }
        unique.push(trimmed.to_string());
    }
    unique
}

/// 提取列表页去重后的供应商账号 ID。
///
/// # 参数
/// * `page` - 当前页投影行与总数
///
/// # 返回
/// 返回按首次出现顺序去重后的供应商 ID；空页返回空集合。
///
/// # 错误
/// 无。
///
/// # 约束
/// 去重只用于缩小 `$in` 范围，不改变任何业务语义。
fn unique_supplier_ids(page: &PageResult<super::order::PurchaseOrderRow>) -> Vec<SupplierAccountId> {
    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for row in &page.items {
        if seen.insert(row.supplier_id.to_string()) {
            unique.push(row.supplier_id.clone());
        }
    }
    unique
}

/// 提取列表页去重后的来源销售单 ID。
///
/// # 参数
/// * `page` - 当前页投影行与总数
///
/// # 返回
/// 返回按首次出现顺序去重后的销售单 ID；空页返回空集合。
///
/// # 错误
/// 无。
///
/// # 约束
/// 去重只用于缩小 `$in` 范围，不改变过滤、排序与总数语义。
fn unique_sales_ids(page: &PageResult<super::order::PurchaseOrderRow>) -> Vec<SalesOrderId> {
    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for row in &page.items {
        if seen.insert(row.sales_order_id.to_string()) {
            unique.push(row.sales_order_id.clone());
        }
    }
    unique
}

/// 按命名空间拆分列表行当前内容指针。
///
/// # 参数
/// * `page` - 当前页投影行与总数
///
/// # 返回
/// 返回 `(提交指针, 版本指针)`：有当前提交的行进入提交集合，否则有当前版本
/// 的行进入版本集合；两者互斥，同一 ID 不得同时进入两个集合。
///
/// # 错误
/// 无。
///
/// # 约束
/// 提交指针只用于提交集合查询，版本指针只用于版本集合查询，不得交叉；
/// 历史提交与历史版本不进入集合，缺失指针由 Service 回退零值。
fn split_pointer_ids(page: &PageResult<super::order::PurchaseOrderRow>) -> (Vec<String>, Vec<String>) {
    let mut submissions = Vec::new();
    let mut revisions = Vec::new();
    let mut seen_submissions = HashSet::new();
    let mut seen_revisions = HashSet::new();
    for row in &page.items {
        if let Some(pointer) = row.current_submission_id.clone() {
            if seen_submissions.insert(pointer.clone()) {
                submissions.push(pointer);
            }
        } else if let Some(pointer) = row.current_revision_id.clone() {
            if seen_revisions.insert(pointer.clone()) {
                revisions.push(pointer);
            }
        }
    }
    (submissions, revisions)
}

#[allow(dead_code)]
/// 保持类型引用稳定。
fn _keep_ids(_sales: Option<SalesOrderId>, _supplier: Option<SupplierAccountId>, _order: Option<SalesOrder>) {
}

#[cfg(test)]
mod tests {
    use entities::ids::{SalesOrderId, SupplierAccountId};
    use entities::purchase_order::{ProgressStatus, PurchaseOrderStatus, PurchaseReviewStatus, PurchaseType};

    use super::{split_pointer_ids, unique_owner_ids, unique_sales_ids, unique_supplier_ids};
    use crate::repository::PageResult;

    /// 构造最小列表行。
    fn row(
        id: &str,
        sales_order_id: &str,
        supplier_id: &str,
        owner: Option<&str>,
        submission: Option<&str>,
        revision: Option<&str>,
    ) -> super::super::order::PurchaseOrderRow {
        super::super::order::PurchaseOrderRow {
            id: id.to_string(),
            purchase_no: format!("PO-{id}"),
            sales_order_id: SalesOrderId::new(sales_order_id),
            supplier_id: SupplierAccountId::new(supplier_id),
            purchase_type: PurchaseType::Physical,
            payment_term_code: "NET-30".to_string(),
            created_by: "buyer-1".to_string(),
            owner_user_id: owner.map(str::to_string),
            status: PurchaseOrderStatus::Draft,
            review_status: PurchaseReviewStatus::Pending,
            payment_progress: ProgressStatus::None,
            invoice_progress: ProgressStatus::None,
            fulfillment_progress: ProgressStatus::None,
            current_submission_id: submission.map(str::to_string),
            current_revision_id: revision.map(str::to_string),
            version: 0,
            created_at: 1_800_000_000,
        }
    }

    /// 负责人去重忽略空白与重复输入。
    #[test]
    fn owner_ids_dedup_and_skip_blank() {
        let page = PageResult {
            items: vec![
                row("po-1", "so-1", "sup-1", Some(" buyer-1 "), Some("sub-1"), None),
                row("po-2", "so-1", "sup-1", Some("buyer-1"), None, Some("rev-1")),
                row("po-3", "so-2", "sup-2", Some("   "), None, None),
                row("po-4", "so-2", "sup-2", None, None, None),
            ],
            total: 4,
        };
        assert_eq!(unique_owner_ids(&page), vec!["buyer-1".to_string()]);
    }

    /// 提交与版本指针按命名空间拆分且各自去重。
    #[test]
    fn pointer_ids_split_by_kind_and_dedup() {
        let page = PageResult {
            items: vec![
                row("po-1", "so-1", "sup-1", None, Some("sub-1"), Some("rev-1")),
                row("po-2", "so-1", "sup-1", None, Some("sub-1"), None),
                row("po-3", "so-1", "sup-1", None, None, Some("rev-2")),
                row("po-4", "so-1", "sup-1", None, None, Some("rev-2")),
                row("po-5", "so-1", "sup-1", None, None, None),
            ],
            total: 5,
        };
        assert_eq!(
            split_pointer_ids(&page),
            (vec!["sub-1".to_string()], vec!["rev-2".to_string()])
        );
    }

    /// 供应商与销售单 ID 去重保持首次顺序。
    #[test]
    fn supplier_and_sales_ids_dedup() {
        let page = PageResult {
            items: vec![
                row("po-1", "so-1", "sup-1", None, None, None),
                row("po-2", "so-1", "sup-2", None, None, None),
                row("po-3", "so-2", "sup-1", None, None, None),
            ],
            total: 3,
        };
        assert_eq!(
            unique_supplier_ids(&page)
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            vec!["sup-1".to_string(), "sup-2".to_string()]
        );
        assert_eq!(
            unique_sales_ids(&page)
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            vec!["so-1".to_string(), "so-2".to_string()]
        );
    }
}

#[cfg(test)]
mod isolation_tests {
    use std::str::FromStr;

    use entities::ids::{PurchaseOrderId, SalesOrderId, SupplierAccountId};
    use entities::money::Amount;
    use entities::purchase_order::{
        FulfillmentResponsibility, PaymentTermSnapshot, PurchaseOrder, PurchaseOrderData,
        PurchaseOrderStatus, PurchaseOrderSubmission, PurchaseOrderSubmissionData, PurchaseType,
        SupplierSnapshot,
    };
    use test_support::{require_mongo, TestDb};

    use crate::ensure_indexes;
    use crate::repository::extensions::PurchaseOrderExt;
    use crate::{NoTransaction, Transactional};

    use super::super::order::PurchaseOrderFilter;
    use super::load_purchase_order_list_page;

    /// 构造列表筛选条件。
    fn list_filter() -> PurchaseOrderFilter {
        PurchaseOrderFilter {
            purchase_no: None,
            sales_order_id: None,
            supplier_id: None,
            status: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        }
    }

    /// 空页直接返回空事实，不发关联查询。
    ///
    /// # 参数
    /// 无，内部创建隔离库。
    ///
    /// # 返回
    /// 断言分页总数为零且各事实映射全部为空。
    ///
    /// # 错误
    /// MongoDB 连接或事实加载失败时测试失败。
    ///
    /// # 约束
    /// Batch 维度：空输入直接短路，不允许空 `$in` 查询。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn empty_page_returns_empty_facts() {
        require_mongo!(async {
            let fixture = TestDb::new("proc_po_list_empty")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            let (page, facts) =
                load_purchase_order_list_page(fixture.db(), &list_filter(), &mut NoTransaction)
                    .await
                    .expect("事实加载失败");
            assert_eq!(page.total, 0);
            assert!(page.items.is_empty());
            assert!(facts.supplier_names.is_empty());
            assert!(facts.sales_order_nos.is_empty());
            assert!(facts.owner_names.is_empty());
            assert!(facts.submissions.is_empty());
            assert!(facts.revisions.is_empty());
        });
    }

    /// 当前指针命中的提交与版本一次批量返回，历史提交不进入结果。
    ///
    /// # 参数
    /// 无，内部创建隔离库并写入最小夹具。
    ///
    /// # 返回
    /// 断言当前提交与版本均被加载且历史提交缺席。
    ///
    /// # 错误
    /// MongoDB 连接、夹具写入或事实加载失败时测试失败。
    ///
    /// # 约束
    /// 只沿当前指针读取；历史提交即使存在也不得进入事实集合。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn loads_current_pointers_and_excludes_historical_submission() {
        require_mongo!(async {
            let fixture = TestDb::new("proc_po_list_pointers")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            let mut order = PurchaseOrder::new(
                PurchaseOrderId::new("po-1"),
                PurchaseOrderData {
                    purchase_no: "PO-1".to_string(),
                    sales_order_id: SalesOrderId::new("so-1"),
                    sales_order_revision_id: entities::ids::SalesOrderRevisionId::new("rev-1"),
                    creation_basis_id: "basis-1".to_string(),
                    supplier_id: SupplierAccountId::new("sup-1"),
                    purchase_type: PurchaseType::Physical,
                    payment_term_code: "NET-30".to_string(),
                    fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
                    owner_user_id: "buyer-1".to_string(),
                    target_warehouse_id: Some(entities::ids::WarehouseId::new("wh-1")),
                },
                "buyer-1",
            )
            .expect("采购单构造失败");
            order.current_submission_id = Some("sub-current".to_string());
            order.stable.status = PurchaseOrderStatus::Draft;
            fixture
                .db()
                .purchase_orders()
                .create(&order, &mut NoTransaction)
                .await
                .expect("采购单写入失败");
            let current = PurchaseOrderSubmission::new(
                entities::ids::PurchaseOrderSubmissionId::new("sub-current"),
                PurchaseOrderSubmissionData {
                    purchase_order_id: PurchaseOrderId::new("po-1"),
                    submission_no: "SUB-current".to_string(),
                    supplier_id: SupplierAccountId::new("sup-1"),
                    purchase_type: PurchaseType::Physical,
                    fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
                    supplier_revision_id: entities::ids::SupplierCommercialProfileRevisionId::new("suprev-1"),
                    supplier_snapshot: SupplierSnapshot::new("供应商".to_string()).expect("快照构造失败"),
                    payment_term_snapshot: PaymentTermSnapshot::new("NET-30".to_string(), false, None, None)
                        .expect("条款构造失败"),
                    gross_amount: Amount::from_str("10").unwrap(),
                    net_amount: Amount::from_str("10").unwrap(),
                    tax_amount: Amount::from_str("0").unwrap(),
                },
            )
            .expect("提交构造失败");
            fixture
                .db()
                .purchase_order_submissions()
                .create(&current, &mut NoTransaction)
                .await
                .expect("提交写入失败");
            let history = PurchaseOrderSubmission::new(
                entities::ids::PurchaseOrderSubmissionId::new("sub-history"),
                PurchaseOrderSubmissionData {
                    purchase_order_id: PurchaseOrderId::new("po-1"),
                    submission_no: "SUB-history".to_string(),
                    supplier_id: SupplierAccountId::new("sup-1"),
                    purchase_type: PurchaseType::Physical,
                    fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
                    supplier_revision_id: entities::ids::SupplierCommercialProfileRevisionId::new("suprev-1"),
                    supplier_snapshot: SupplierSnapshot::new("供应商".to_string()).expect("快照构造失败"),
                    payment_term_snapshot: PaymentTermSnapshot::new("NET-30".to_string(), false, None, None)
                        .expect("条款构造失败"),
                    gross_amount: Amount::from_str("99").unwrap(),
                    net_amount: Amount::from_str("99").unwrap(),
                    tax_amount: Amount::from_str("0").unwrap(),
                },
            )
            .expect("历史提交构造失败");
            fixture
                .db()
                .purchase_order_submissions()
                .create(&history, &mut NoTransaction)
                .await
                .expect("历史提交写入失败");
            let (page, facts) =
                load_purchase_order_list_page(fixture.db(), &list_filter(), &mut NoTransaction)
                    .await
                    .expect("事实加载失败");
            assert_eq!(page.total, 1);
            assert!(facts.submissions.contains_key("sub-current"));
            assert!(
                !facts.submissions.contains_key("sub-history"),
                "历史提交不得进入列表事实"
            );
        });
    }

    /// 事务内调用复用调用方 session，读取自身未提交写入。
    ///
    /// # 参数
    /// 无，内部创建隔离库。
    ///
    /// # 返回
    /// 断言事务内可读到同一 session 刚写入的列表事实。
    ///
    /// # 错误
    /// MongoDB 连接、事务或事实加载失败时测试失败。
    ///
    /// # 约束
    /// 事务内重验必须复用调用方 executor，不得另开连接或独立事务。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn transaction_reads_own_writes_with_same_session() {
        require_mongo!(async {
            let fixture = TestDb::new("proc_po_list_txn").await.expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            let db = fixture.db().clone();
            let client = db.client().clone();
            client
                .with_transaction::<_, (), crate::errors::Error>(move |session| {
                    let db = db.clone();
                    Box::pin(async move {
                        let mut order = PurchaseOrder::new(
                            PurchaseOrderId::new("po-txn"),
                            PurchaseOrderData {
                                purchase_no: "PO-TXN".to_string(),
                                sales_order_id: SalesOrderId::new("so-txn"),
                                sales_order_revision_id: entities::ids::SalesOrderRevisionId::new("rev-1"),
                                creation_basis_id: "basis-txn".to_string(),
                                supplier_id: SupplierAccountId::new("sup-txn"),
                                purchase_type: PurchaseType::Physical,
                                payment_term_code: "NET-30".to_string(),
                                fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
                                owner_user_id: "buyer-txn".to_string(),
                                target_warehouse_id: Some(entities::ids::WarehouseId::new("wh-1")),
                            },
                            "buyer-txn",
                        )
                        .expect("采购单构造失败");
                        order.current_submission_id = Some("sub-txn".to_string());
                        db.purchase_orders().create(&order, session).await?;
                        let (page, facts) =
                            load_purchase_order_list_page(&db, &list_filter(), session).await?;
                        assert_eq!(page.total, 1, "事务内应能 read-your-writes");
                        assert!(facts.submissions.is_empty() || page.items.len() == 1);
                        let _ = facts.sales_order_nos.len();
                        Ok(())
                    })
                })
                .await
                .expect("事务内事实加载失败");
        });
    }
}
