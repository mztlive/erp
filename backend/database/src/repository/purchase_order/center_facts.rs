//! 采购单对象中心事实 Bundle（PROC-R06）。
//!
//! 一次批量返回单个采购单对象中心所需的全部当前指针事实：采购单、供应商
//! 当前法定名称、来源销售单业务单号、负责人展示名、当前生效版本与版本行、
//! 当前提交与提交行、生效版本销售分配、变更历史与应付汇总。指针选择、缺失
//! 校验、内容优先级、金额格式化与 View 映射由 Service 负责；审批运行时仍由
//! 审批 Repository 提供，本模块不读取审批定义、实例与历史，不做任何审批
//! 政策判断。

use entities::ids::{
    PurchaseOrderRevisionLineId, PurchaseOrderSubmissionId, SalesOrderId, SupplierAccountId,
};
use entities::payable::PayableAccount;
use entities::purchase_order::{
    PurchaseChangeOrder, PurchaseLineSalesAllocation, PurchaseOrder, PurchaseOrderRevision,
    PurchaseOrderRevisionLine, PurchaseOrderSubmission, PurchaseOrderSubmissionLine,
};
use mongodb::Database;

use crate::executor::Executor;
use crate::repository::extensions::{
    AccessControlExt, PayableExt, PurchaseOrderExt, SalesOrderExt, SupplierExt,
};
use crate::Result;

/// 采购单对象中心事实 Bundle。
///
/// 所有映射均以持久化原值返回；`None`/缺键表示关联不存在，由 Service 按
/// 完整性错误或约定回退解释，本层不做业务回退。
#[derive(Debug, Clone, Default)]
pub struct PurchaseOrderCenterFacts {
    /// 采购单主表。
    pub order: Option<PurchaseOrder>,
    /// 供应商当前法定名称。
    pub supplier_name: Option<String>,
    /// 来源销售单业务单号。
    pub sales_order_no: Option<String>,
    /// 负责人展示名。
    pub owner_name: Option<String>,
    /// 当前生效版本头。
    pub current_revision: Option<PurchaseOrderRevision>,
    /// 当前生效版本行。
    pub revision_lines: Vec<PurchaseOrderRevisionLine>,
    /// 当前提交头。
    pub current_submission: Option<PurchaseOrderSubmission>,
    /// 当前提交行。
    pub submission_lines: Vec<PurchaseOrderSubmissionLine>,
    /// 生效版本销售分配。
    pub allocations: Vec<PurchaseLineSalesAllocation>,
    /// 本采购单的变更单。
    pub changes: Vec<PurchaseChangeOrder>,
    /// 应付往来子账。
    pub payable: Option<PayableAccount>,
}

/// 批量加载采购单对象中心事实。
///
/// # 参数
/// * `db` - MongoDB 数据库句柄
/// * `order_id` - 采购单主键
/// * `executor` - 数据访问执行器，由 Service 决定事务边界；事务内重验必须复用调用方 executor
///
/// # 返回
/// 返回单个采购单的全部当前指针事实；采购单不存在时 `order` 为 `None`，由
/// Service 映射为 `NotFound`。关联缺失以 `None`/空集合表达，由 Service 按
/// 完整性错误或约定回退解释。
///
/// # 错误
/// MongoDB 查询或反序列化失败时返回错误；不负责缺失校验，软删除已由基类
/// 查询过滤。
///
/// # 约束
/// 查询次数固定有界：主表、供应商名称、销售单、负责人、当前版本与版本行、
/// 当前提交与提交行、销售分配、变更历史与应付汇总各至多一次批量读取，不随
/// 行数增长。只沿 `current_submission_id` 或 `current_revision_id` 读取，
/// 历史提交与历史版本不进入结果；不得读取审批运行时，不得做事务或审批政策
/// 判断。
pub async fn load_purchase_order_center_facts(
    db: &Database,
    order_id: &str,
    executor: &mut dyn Executor,
) -> Result<PurchaseOrderCenterFacts> {
    let Some(order) = db.purchase_orders().find_by_id(order_id, executor).await? else {
        return Ok(PurchaseOrderCenterFacts::default());
    };
    let supplier_names = db
        .supplier()
        .current_legal_names_by_account_ids(std::slice::from_ref(&order.supplier_id), executor)
        .await?;
    let supplier_name = supplier_names.get(&order.supplier_id.to_string()).cloned();
    let sales_id = order.sales_order_id.clone();
    let sales_orders = db
        .sales_orders()
        .find_orders_by_ids(std::slice::from_ref(&sales_id), executor)
        .await?;
    let sales_order_no = sales_orders.into_iter().find_map(|item| {
        if item.base.id == sales_id.to_string() {
            Some(item.order_no.clone())
        } else {
            None
        }
    });
    let owner_name = load_owner_name(db, &order, executor).await?;
    let current_revision = match &order.stable.current_revision_id {
        Some(revision_id) => {
            db.purchase_order_revisions()
                .find_by_id(revision_id, executor)
                .await?
        }
        None => None,
    };
    let revision_lines = match &current_revision {
        Some(revision) => {
            db.purchase_order()
                .list_revision_lines(
                    &entities::ids::PurchaseOrderRevisionId::new(revision.base.id.clone()),
                    executor,
                )
                .await?
        }
        None => Vec::new(),
    };
    let current_submission = match &order.current_submission_id {
        Some(submission_id) => {
            db.purchase_order_submissions()
                .find_by_id(submission_id, executor)
                .await?
        }
        None => None,
    };
    let submission_lines = match &current_submission {
        Some(submission) => {
            db.purchase_order()
                .list_submission_lines(
                    &PurchaseOrderSubmissionId::new(submission.base.id.clone()),
                    executor,
                )
                .await?
        }
        None => Vec::new(),
    };
    let allocations = load_allocations(db, &revision_lines, executor).await?;
    let changes = db
        .purchase_order()
        .list_changes_by_order(&order.base.id.clone().into(), executor)
        .await?;
    let payable = db
        .payable_accounts()
        .find_by_purchase_order(&order.base.id.clone().into(), executor)
        .await?;
    Ok(PurchaseOrderCenterFacts {
        order: Some(order),
        supplier_name,
        sales_order_no,
        owner_name,
        current_revision,
        revision_lines,
        current_submission,
        submission_lines,
        allocations,
        changes,
        payable,
    })
}

/// 批量加载负责人展示名。
///
/// # 参数
/// * `db` - MongoDB 数据库句柄
/// * `order` - 已加载的采购单主表
/// * `executor` - 数据访问执行器，由 Service 决定事务边界
///
/// # 返回
/// 返回负责人展示名；责任人缺失或空白时返回 `None`，由 Service 按实体校验
/// 或约定回退解释。
///
/// # 错误
/// MongoDB 查询或反序列化失败时返回错误。
///
/// # 约束
/// 只查询账号集合，不做 RBAC 或跨聚合业务判断。
async fn load_owner_name(
    db: &Database,
    order: &PurchaseOrder,
    executor: &mut dyn Executor,
) -> Result<Option<String>> {
    let Some(owner) = order
        .owner_user_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
    else {
        return Ok(None);
    };
    let names = db
        .accounts()
        .names_by_ids(std::slice::from_ref(&owner.to_string()), executor)
        .await?;
    Ok(names.get(owner).cloned())
}

/// 批量加载生效版本销售分配。
///
/// # 参数
/// * `db` - MongoDB 数据库句柄
/// * `revision_lines` - 当前生效版本行
/// * `executor` - 数据访问执行器，由 Service 决定事务边界
///
/// # 返回
/// 返回当前版本行关联的全部销售分配；无版本行时返回空集合。
///
/// # 错误
/// MongoDB 查询或反序列化失败时返回错误。
///
/// # 约束
/// 一次 `$in` 批量取回，不得出现逐行 N+1。
async fn load_allocations(
    db: &Database,
    revision_lines: &[PurchaseOrderRevisionLine],
    executor: &mut dyn Executor,
) -> Result<Vec<PurchaseLineSalesAllocation>> {
    let line_ids = revision_lines
        .iter()
        .map(|line| PurchaseOrderRevisionLineId::new(line.base.id.clone()))
        .collect::<Vec<_>>();
    if line_ids.is_empty() {
        return Ok(Vec::new());
    }
    db.purchase_line_sales_allocations()
        .find_by_purchase_revision_line_ids(&line_ids, executor)
        .await
}

#[allow(dead_code)]
/// 保持类型引用稳定。
fn _keep_ids(_sales: Option<SalesOrderId>, _supplier: Option<SupplierAccountId>) {}

#[cfg(test)]
mod tests {
    use super::PurchaseOrderCenterFacts;

    /// 空 Bundle 默认全部为空，由 Service 映射为缺失语义。
    #[test]
    fn default_bundle_is_empty() {
        let facts = PurchaseOrderCenterFacts::default();
        assert!(facts.order.is_none());
        assert!(facts.revision_lines.is_empty());
        assert!(facts.allocations.is_empty());
    }
}

#[cfg(test)]
mod isolation_tests {
    use entities::ids::{PurchaseOrderId, SalesOrderId, SupplierAccountId};
    use entities::purchase_order::{
        FulfillmentResponsibility, PurchaseOrder, PurchaseOrderData, PurchaseOrderStatus, PurchaseType,
    };
    use test_support::{require_mongo, TestDb};

    use crate::ensure_indexes;
    use crate::repository::extensions::PurchaseOrderExt;
    use crate::{NoTransaction, Transactional};

    use super::load_purchase_order_center_facts;

    /// 构造最小采购单。
    fn order(id: &str, submission: Option<&str>, revision: Option<&str>) -> PurchaseOrder {
        let mut created = PurchaseOrder::new(
            PurchaseOrderId::new(id),
            PurchaseOrderData {
                purchase_no: format!("PO-{id}"),
                sales_order_id: SalesOrderId::new("so-missing"),
                sales_order_revision_id: entities::ids::SalesOrderRevisionId::new("rev-1"),
                creation_basis_id: "basis-1".to_string(),
                supplier_id: SupplierAccountId::new("sup-missing"),
                purchase_type: PurchaseType::Physical,
                payment_term_code: "NET-30".to_string(),
                fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
                owner_user_id: "buyer-1".to_string(),
                target_warehouse_id: Some(entities::ids::WarehouseId::new("wh-1")),
            },
            "buyer-1",
        )
        .expect("采购单构造失败");
        created.current_submission_id = submission.map(str::to_string);
        created.stable.current_revision_id = revision.map(str::to_string);
        created
    }

    /// 采购单不存在时返回空 Bundle，由 Service 映射为 NotFound。
    ///
    /// # 参数
    /// 无，内部创建隔离库。
    ///
    /// # 返回
    /// 断言 `order` 为空且其余事实全部为空。
    ///
    /// # 错误
    /// MongoDB 连接或事实加载失败时测试失败。
    ///
    /// # 约束
    /// Exact 维度：缺失事实由 Service 校验，Repository 只负责读取整形。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn missing_order_returns_empty_bundle() {
        require_mongo!(async {
            let fixture = TestDb::new("proc_po_center_missing")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            let facts = load_purchase_order_center_facts(fixture.db(), "po-missing", &mut NoTransaction)
                .await
                .expect("事实加载失败");
            assert!(facts.order.is_none());
            assert!(facts.supplier_name.is_none());
            assert!(facts.sales_order_no.is_none());
            assert!(facts.current_revision.is_none());
            assert!(facts.revision_lines.is_empty());
            assert!(facts.current_submission.is_none());
            assert!(facts.submission_lines.is_empty());
            assert!(facts.allocations.is_empty());
            assert!(facts.changes.is_empty());
            assert!(facts.payable.is_none());
        });
    }

    /// 只沿当前指针读取，缺失关联以空值表达。
    ///
    /// # 参数
    /// 无，内部创建隔离库并写入无关联的最小采购单。
    ///
    /// # 返回
    /// 断言主表存在且缺失关联为空，历史事实不进入结果。
    ///
    /// # 错误
    /// MongoDB 连接、夹具写入或事实加载失败时测试失败。
    ///
    /// # 约束
    /// 正式分配、应付摘要与变更历史缺失时为空集合或空值，不得失败。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn loads_order_with_missing_associations_as_empty() {
        require_mongo!(async {
            let fixture = TestDb::new("proc_po_center_minimal")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            fixture
                .db()
                .purchase_orders()
                .create(&order("po-1", None, None), &mut NoTransaction)
                .await
                .expect("采购单写入失败");
            let facts = load_purchase_order_center_facts(fixture.db(), "po-1", &mut NoTransaction)
                .await
                .expect("事实加载失败");
            assert!(facts.order.is_some());
            assert!(facts.supplier_name.is_none());
            assert!(
                facts.sales_order_no.is_none(),
                "缺失销售单以空值表达，由 Service 报完整性错误"
            );
            assert!(facts.current_revision.is_none());
            assert!(facts.revision_lines.is_empty());
            assert!(facts.allocations.is_empty());
            assert!(facts.payable.is_none());
            assert!(facts.order.as_ref().expect("主表存在").stable.status == PurchaseOrderStatus::Draft);
        });
    }

    /// 事务内调用复用调用方 session，读取自身未提交写入。
    ///
    /// # 参数
    /// 无，内部创建隔离库。
    ///
    /// # 返回
    /// 断言事务内可读到同一 session 刚写入的采购单。
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
            let fixture = TestDb::new("proc_po_center_txn")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            let db = fixture.db().clone();
            let client = db.client().clone();
            client
                .with_transaction::<_, (), crate::errors::Error>(move |session| {
                    let db = db.clone();
                    Box::pin(async move {
                        db.purchase_orders()
                            .create(&order("po-txn", None, None), session)
                            .await?;
                        let facts = load_purchase_order_center_facts(&db, "po-txn", session).await?;
                        assert!(facts.order.is_some(), "事务内应能 read-your-writes");
                        Ok(())
                    })
                })
                .await
                .expect("事务内事实加载失败");
        });
    }
}
