//! 采购单对象中心事实 Bundle（PROC-R06）。
//!
//! 一次批量返回单个采购单对象中心所需的全部当前指针事实：采购单、供应商
//! 当前法定名称、来源销售单业务单号、负责人展示名、当前生效版本与版本行、
//! 当前提交与提交行、生效版本销售分配、变更历史与应付汇总。指针选择、缺失
//! 校验、内容优先级、金额格式化与 View 映射由 Service 负责；审批运行时仍由
//! 审批 Repository 提供，本模块不读取审批定义、实例与历史，不做任何审批
//! 政策判断。

use erp_core::ids::{
    PurchaseOrderRevisionId, PurchaseOrderRevisionLineId, PurchaseOrderSubmissionId, SalesOrderId,
    SupplierAccountId,
};
use erp_core::money::Amount;
use erp_finance::entity::payable::PayableAccountStatus;
use erp_finance::repository::PayableExt;
use erp_finance::repository::prelude::*;
use erp_identity::AccessControlExt;
use erp_identity::repository::prelude::*;
use erp_procurement::entity::purchase_order::{
    PurchaseChangeOrder, PurchaseLineSalesAllocation, PurchaseOrder, PurchaseOrderRevision,
    PurchaseOrderRevisionLine, PurchaseOrderSubmission, PurchaseOrderSubmissionLine,
};
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::repository::prelude::*;
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Result};

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
    pub payable: Option<PurchasePayableFact>,
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
    if executor.session().is_none() {
        return load_parallel_facts(db, order).await;
    }
    load_serial_facts(db, order, executor).await
}

/// 非事务展示查询并行装配，各分支使用独立执行器.
///
/// # 参数
/// * `db` - MongoDB 数据库句柄
/// * `order` - 已加载的采购单主表
///
/// # 返回
/// 返回全部当前指针事实；语义与串行路径一致.
///
/// # 错误
/// MongoDB 查询或反序列化失败时返回错误.
///
/// # 约束
/// 独立事实并行 join，两条链内串行、链间并行；不读取审批运行时.
async fn load_parallel_facts(db: &Database, order: PurchaseOrder) -> Result<PurchaseOrderCenterFacts> {
    let supplier_id = order.supplier_id.clone();
    let sales_id = order.sales_order_id.clone();
    let owner = order.owner_user_id.clone();
    let order_id = erp_core::ids::PurchaseOrderId::new(order.base.id.clone());
    let revision_id = order.stable.current_revision_id.clone();
    let submission_id = order.current_submission_id.clone();
    let (supplier_name, sales_order_no, owner_name, changes, payable, revision_bundle, submission_bundle) = tokio::try_join!(
        load_supplier_name_nt(db, supplier_id),
        load_sales_no_nt(db, sales_id),
        load_owner_name_nt(db, owner),
        load_changes_nt(db, order_id.clone()),
        load_payable_nt(db, order_id),
        load_revision_chain_nt(db, revision_id),
        load_submission_chain_nt(db, submission_id),
    )?;
    Ok(PurchaseOrderCenterFacts {
        order: Some(order),
        supplier_name,
        sales_order_no,
        owner_name,
        current_revision: revision_bundle.0,
        revision_lines: revision_bundle.1,
        current_submission: submission_bundle.0,
        submission_lines: submission_bundle.1,
        allocations: revision_bundle.2,
        changes,
        payable,
    })
}

/// 事务内串行装配，复用调用方执行器保证读己写.
///
/// # 参数
/// * `db` - MongoDB 数据库句柄
/// * `order` - 已加载的采购单主表
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 返回全部当前指针事实；缺失以空值表达.
///
/// # 错误
/// MongoDB 查询或反序列化失败时返回错误.
async fn load_serial_facts(
    db: &Database,
    order: PurchaseOrder,
    executor: &mut dyn Executor,
) -> Result<PurchaseOrderCenterFacts> {
    let supplier_name = load_supplier_name_with(db, &order.supplier_id, executor).await?;
    let sales_order_no = load_sales_no_with(db, &order.sales_order_id, executor).await?;
    let owner_name = load_owner_name(db, &order, executor).await?;
    let (current_revision, revision_lines, allocations) =
        load_revision_chain_with(db, order.stable.current_revision_id.clone(), executor).await?;
    let (current_submission, submission_lines) =
        load_submission_chain_with(db, order.current_submission_id.clone(), executor).await?;
    let changes = db.purchase_order().list_changes_by_order(&order.base.id.clone().into(), executor).await?;
    let payable =
        db.payable_accounts().find_by_purchase_order(&order.base.id.clone().into(), executor).await?;
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
        payable: payable.map(|account| PurchasePayableFact {
            status: account.stable.status,
            open_total: account.open_total,
            settled_total: account.settled_total,
            invoiced_total: account.invoiced_total,
        }),
    })
}

/// 非事务加载供应商法定名称，独立分支可并行.
///
/// # 参数
/// * `db` - MongoDB 数据库句柄
/// * `supplier_id` - 供应商账号主键
///
/// # 返回
/// 返回当前法定名称；缺失时返回 `None`.
///
/// # 错误
/// MongoDB 查询或反序列化失败时返回错误.
async fn load_supplier_name_nt(db: &Database, supplier_id: SupplierAccountId) -> Result<Option<String>> {
    load_supplier_name_with(db, &supplier_id, &mut NoTransaction).await
}

/// 非事务加载来源销售单号，独立分支可并行.
///
/// # 参数
/// * `db` - MongoDB 数据库句柄
/// * `sales_id` - 来源销售单主键
///
/// # 返回
/// 返回业务单号；缺失时返回 `None`，由 Service 报完整性错误.
///
/// # 错误
/// MongoDB 查询或反序列化失败时返回错误.
async fn load_sales_no_nt(db: &Database, sales_id: SalesOrderId) -> Result<Option<String>> {
    load_sales_no_with(db, &sales_id, &mut NoTransaction).await
}

/// 非事务加载负责人展示名，独立分支可并行.
///
/// # 参数
/// * `db` - MongoDB 数据库句柄
/// * `owner` - 负责人账号，未指定或空白时直接返回空
///
/// # 返回
/// 返回展示名；缺失时返回 `None`.
///
/// # 错误
/// MongoDB 查询或反序列化失败时返回错误.
async fn load_owner_name_nt(db: &Database, owner: Option<String>) -> Result<Option<String>> {
    let Some(owner) = owner.as_deref().map(str::trim).filter(|id| !id.is_empty()) else {
        return Ok(None);
    };
    let names =
        db.accounts().names_by_ids(std::slice::from_ref(&owner.to_string()), &mut NoTransaction).await?;
    Ok(names.get(owner).cloned())
}

/// 非事务加载变更历史，独立分支可并行.
///
/// # 参数
/// * `db` - MongoDB 数据库句柄
/// * `order_id` - 采购单主键
///
/// # 返回
/// 返回本采购单的全部变更单.
///
/// # 错误
/// MongoDB 查询或反序列化失败时返回错误.
async fn load_changes_nt(
    db: &Database,
    order_id: erp_core::ids::PurchaseOrderId,
) -> Result<Vec<PurchaseChangeOrder>> {
    db.purchase_order().list_changes_by_order(&order_id, &mut NoTransaction).await
}

/// 非事务加载应付汇总，独立分支可并行.
///
/// # 参数
/// * `db` - MongoDB 数据库句柄
/// * `order_id` - 采购单主键
///
/// # 返回
/// 返回应付余额；账户不存在时返回 `None`.
///
/// # 错误
/// MongoDB 查询或反序列化失败时返回错误.
async fn load_payable_nt(
    db: &Database,
    order_id: erp_core::ids::PurchaseOrderId,
) -> Result<Option<PurchasePayableFact>> {
    let payable = db.payable_accounts().find_by_purchase_order(&order_id, &mut NoTransaction).await?;
    Ok(payable.map(|account| PurchasePayableFact {
        status: account.stable.status,
        open_total: account.open_total,
        settled_total: account.settled_total,
        invoiced_total: account.invoiced_total,
    }))
}

/// 非事务加载版本链，链内串行、链间并行.
///
/// # 参数
/// * `db` - MongoDB 数据库句柄
/// * `revision_id` - 当前生效版本指针；为空时返回空链
///
/// # 返回
/// 返回版本头、版本行与销售分配.
///
/// # 错误
/// MongoDB 查询或反序列化失败时返回错误.
async fn load_revision_chain_nt(
    db: &Database,
    revision_id: Option<String>,
) -> Result<(Option<PurchaseOrderRevision>, Vec<PurchaseOrderRevisionLine>, Vec<PurchaseLineSalesAllocation>)>
{
    let Some(revision_id) = revision_id else {
        return Ok((None, Vec::new(), Vec::new()));
    };
    let current_revision = db.purchase_order_revisions().find_by_id(&revision_id, &mut NoTransaction).await?;
    let Some(revision) = current_revision else {
        return Ok((None, Vec::new(), Vec::new()));
    };
    let lines = db
        .purchase_order()
        .list_revision_lines(&PurchaseOrderRevisionId::new(revision.base.id.clone()), &mut NoTransaction)
        .await?;
    let allocations = load_allocations_nt(db, &lines).await?;
    Ok((Some(revision), lines, allocations))
}

/// 非事务加载提交链，链内串行、链间并行.
///
/// # 参数
/// * `db` - MongoDB 数据库句柄
/// * `submission_id` - 当前提交指针；为空时返回空链
///
/// # 返回
/// 返回提交头与提交行.
///
/// # 错误
/// MongoDB 查询或反序列化失败时返回错误.
async fn load_submission_chain_nt(
    db: &Database,
    submission_id: Option<String>,
) -> Result<(Option<PurchaseOrderSubmission>, Vec<PurchaseOrderSubmissionLine>)> {
    let Some(submission_id) = submission_id else {
        return Ok((None, Vec::new()));
    };
    let current_submission =
        db.purchase_order_submissions().find_by_id(&submission_id, &mut NoTransaction).await?;
    let Some(submission) = current_submission else {
        return Ok((None, Vec::new()));
    };
    let lines = db
        .purchase_order()
        .list_submission_lines(
            &PurchaseOrderSubmissionId::new(submission.base.id.clone()),
            &mut NoTransaction,
        )
        .await?;
    Ok((Some(submission), lines))
}

/// 事务内加载供应商法定名称.
///
/// # 参数
/// * `db` - MongoDB 数据库句柄
/// * `supplier_id` - 供应商账号主键
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 返回当前法定名称；缺失时返回 `None`.
///
/// # 错误
/// MongoDB 查询或反序列化失败时返回错误.
async fn load_supplier_name_with(
    db: &Database,
    supplier_id: &SupplierAccountId,
    executor: &mut dyn Executor,
) -> Result<Option<String>> {
    let names = super::supplier_names::current_legal_names_by_account_ids(
        db,
        std::slice::from_ref(supplier_id),
        executor,
    )
    .await?;
    Ok(names.get(&supplier_id.to_string()).cloned())
}

/// 事务内加载来源销售单号.
///
/// # 参数
/// * `db` - MongoDB 数据库句柄
/// * `sales_id` - 来源销售单主键
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 返回业务单号；缺失时返回 `None`，由 Service 报完整性错误.
///
/// # 错误
/// MongoDB 查询或反序列化失败时返回错误.
async fn load_sales_no_with(
    db: &Database,
    sales_id: &SalesOrderId,
    executor: &mut dyn Executor,
) -> Result<Option<String>> {
    let text = sales_id.to_string();
    let orders = db.sales_orders().find_orders_by_ids(std::slice::from_ref(sales_id), executor).await?;
    Ok(orders.into_iter().find_map(
        |item| {
            if item.base.id == text { Some(item.order_no.clone()) } else { None }
        },
    ))
}

/// 事务内加载版本链，链内串行.
///
/// # 参数
/// * `db` - MongoDB 数据库句柄
/// * `revision_id` - 当前生效版本指针；为空时返回空链
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 返回版本头、版本行与销售分配.
///
/// # 错误
/// MongoDB 查询或反序列化失败时返回错误.
async fn load_revision_chain_with(
    db: &Database,
    revision_id: Option<String>,
    executor: &mut dyn Executor,
) -> Result<(Option<PurchaseOrderRevision>, Vec<PurchaseOrderRevisionLine>, Vec<PurchaseLineSalesAllocation>)>
{
    let Some(revision_id) = revision_id else {
        return Ok((None, Vec::new(), Vec::new()));
    };
    let current_revision = db.purchase_order_revisions().find_by_id(&revision_id, executor).await?;
    let Some(revision) = current_revision else {
        return Ok((None, Vec::new(), Vec::new()));
    };
    let lines = db
        .purchase_order()
        .list_revision_lines(&PurchaseOrderRevisionId::new(revision.base.id.clone()), executor)
        .await?;
    let allocations = load_allocations(db, &lines, executor).await?;
    Ok((Some(revision), lines, allocations))
}

/// 事务内加载提交链，链内串行.
///
/// # 参数
/// * `db` - MongoDB 数据库句柄
/// * `submission_id` - 当前提交指针；为空时返回空链
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 返回提交头与提交行.
///
/// # 错误
/// MongoDB 查询或反序列化失败时返回错误.
async fn load_submission_chain_with(
    db: &Database,
    submission_id: Option<String>,
    executor: &mut dyn Executor,
) -> Result<(Option<PurchaseOrderSubmission>, Vec<PurchaseOrderSubmissionLine>)> {
    let Some(submission_id) = submission_id else {
        return Ok((None, Vec::new()));
    };
    let current_submission = db.purchase_order_submissions().find_by_id(&submission_id, executor).await?;
    let Some(submission) = current_submission else {
        return Ok((None, Vec::new()));
    };
    let lines = db
        .purchase_order()
        .list_submission_lines(&PurchaseOrderSubmissionId::new(submission.base.id.clone()), executor)
        .await?;
    Ok((Some(submission), lines))
}

/// 非事务批量加载生效版本销售分配.
///
/// # 参数
/// * `db` - MongoDB 数据库句柄
/// * `revision_lines` - 当前生效版本行
///
/// # 返回
/// 返回当前版本行关联的全部销售分配；无版本行时返回空集合.
///
/// # 错误
/// MongoDB 查询或反序列化失败时返回错误.
async fn load_allocations_nt(
    db: &Database,
    revision_lines: &[PurchaseOrderRevisionLine],
) -> Result<Vec<PurchaseLineSalesAllocation>> {
    let line_ids = revision_lines
        .iter()
        .map(|line| erp_core::ids::PurchaseOrderRevisionLineId::new(line.base.id.clone()))
        .collect::<Vec<_>>();
    if line_ids.is_empty() {
        return Ok(Vec::new());
    }
    db.purchase_line_sales_allocations()
        .find_by_purchase_revision_line_ids(&line_ids, &mut NoTransaction)
        .await
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
    let Some(owner) = order.owner_user_id.as_deref().map(str::trim).filter(|id| !id.is_empty()) else {
        return Ok(None);
    };
    let names = db.accounts().names_by_ids(std::slice::from_ref(&owner.to_string()), executor).await?;
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
    db.purchase_line_sales_allocations().find_by_purchase_revision_line_ids(&line_ids, executor).await
}

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
    use erp_core::ids::{PurchaseOrderId, SalesOrderId, SupplierAccountId};
    use erp_procurement::entity::purchase_order::{
        FulfillmentResponsibility, PurchaseOrder, PurchaseOrderData, PurchaseOrderStatus, PurchaseType,
    };
    use erp_procurement::repository::PurchaseOrderExt;
    use persistence_core::{NoTransaction, Transactional};
    use test_support::{TestDb, require_mongo};

    use super::load_purchase_order_center_facts;
    use crate::test_indexes::ensure_indexes;

    /// 构造最小采购单。
    fn order(id: &str, submission: Option<&str>, revision: Option<&str>) -> PurchaseOrder {
        let mut created = PurchaseOrder::new(
            PurchaseOrderId::new(id),
            PurchaseOrderData {
                business_org_unit_id: "org-procurement".to_string(),
                purchase_no: format!("PO-{id}"),
                sales_order_id: SalesOrderId::new("so-missing"),
                sales_order_revision_id: erp_core::ids::SalesOrderRevisionId::new("rev-1"),
                creation_basis_id: "basis-1".to_string(),
                supplier_id: SupplierAccountId::new("sup-missing"),
                purchase_type: PurchaseType::Physical,
                payment_term_code: "NET-30".to_string(),
                fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
                owner_user_id: "buyer-1".to_string(),
                target_warehouse_id: Some(erp_core::ids::WarehouseId::new("wh-1")),
            },
            "buyer-1",
            crate::purchase_center::test_support::payment_term_fact,
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
            let fixture = TestDb::new("proc_po_center_missing").await.expect("测试数据库创建失败");
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
            let fixture = TestDb::new("proc_po_center_minimal").await.expect("测试数据库创建失败");
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
            assert!(facts.sales_order_no.is_none(), "缺失销售单以空值表达，由 Service 报完整性错误");
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
            let fixture = TestDb::new("proc_po_center_txn").await.expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            let db = fixture.db().clone();
            let client = db.client().clone();
            client
                .with_transaction::<_, (), persistence_core::Error>(move |executor| {
                    let db = db.clone();
                    Box::pin(async move {
                        db.purchase_orders().create(&order("po-txn", None, None), executor).await?;
                        let facts = load_purchase_order_center_facts(&db, "po-txn", executor).await?;
                        assert!(facts.order.is_some(), "事务内应能 read-your-writes");
                        Ok(())
                    })
                })
                .await
                .expect("事务内事实加载失败");
        });
    }
}

/// 采购中心消费的应付余额；账户不存在由外层 Option 表达。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PurchasePayableFact {
    /// 与核销金额在同一财务写入中更新的当前状态。
    pub status: PayableAccountStatus,
    /// 原应付账户未结余额。
    pub open_total: Amount,
    /// 原应付账户已付款分摊累计。
    pub settled_total: Amount,
    /// 原应付账户已开票分摊累计。
    pub invoiced_total: Amount,
}
