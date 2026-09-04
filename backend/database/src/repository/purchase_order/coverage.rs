//! 采购覆盖事实批量加载（PROC-R07）。
//!
//! 一次批量返回销售当前目标行与全部当前覆盖来源：销售当前版本头与公共/子类型
//! 行、SKU 与商品类型、未作废且参与覆盖的采购单、草稿类当前提交行、正式类
//! 当前版本行、正式销售分配与现有库存预占。指针选择、完整性校验、覆盖累计与
//! 超覆盖拒绝由 `entities::purchase_order::coverage` 领域值对象负责；本模块只
//! 做持久化读取，查询次数与输入规模无关，不得出现逐行 N+1。

use std::collections::HashMap;

use entities::ids::{
    PurchaseOrderRevisionId, PurchaseOrderRevisionLineId, PurchaseOrderSubmissionId, SalesOrderId,
    SalesOrderRevisionId, SalesOrderRevisionLineId,
};
use entities::purchase_order::ProcurementCoverageFacts;
use entities::purchase_order::{PurchaseLineType, PurchaseOrderStatus};
use entities::sales_order::LineType;
use mongodb::Database;

use crate::executor::Executor;
use crate::repository::extensions::{CatalogExt, InventoryExt, PurchaseOrderExt, SalesOrderExt};
use crate::Result;

/// 批量加载采购覆盖计算所需的最小持久化事实。
///
/// # 参数
/// * `db` - MongoDB 数据库句柄
/// * `revision_id` - 销售单当前版本身份（Service 已校验指针存在）
/// * `sales_order_id` - 来源销售单稳定身份
/// * `executor` - 数据访问执行器，由 Service 决定事务边界；事务内重验必须复用调用方 executor
///
/// # 返回
/// 返回包含当前销售目标行与全部当前覆盖来源的事实集合；当前版本文档缺失时
/// 返回空事实集合，由 Entity 层校验。
///
/// # 错误
/// MongoDB 查询或反序列化失败时返回错误；不负责缺失校验，软删除与作废采购单
/// 已通过 Repository 查询过滤。
///
/// # 约束
/// 查询次数与输入规模无关：销售版本行、商品子类型行、SKU、商品、覆盖采购单、
/// 当前提交行、当前版本行、正式分配与现有库存预占各一次批量读取，不得出现
/// 逐行 N+1。草稿类状态只沿当前提交指针、正式状态只沿当前版本指针读取；
/// 现有库存预占只按 `source_type = EXISTING_STOCK` 读取，采购入库预占不进入
/// 结果，避免与采购入库覆盖重复累计。
pub async fn load_procurement_coverage_facts(
    db: &Database,
    revision_id: &SalesOrderRevisionId,
    sales_order_id: &SalesOrderId,
    executor: &mut dyn Executor,
) -> Result<ProcurementCoverageFacts> {
    let Some(revision) = db
        .sales_order_revisions()
        .find_by_id(revision_id.as_ref(), executor)
        .await?
    else {
        return Ok(ProcurementCoverageFacts::default());
    };
    let revision_key = SalesOrderRevisionId::new(revision.base.id.clone());
    let revision_lines = db
        .sales_order_revision_lines()
        .list_lines_by_revision(&revision_key, executor)
        .await?;
    let goods_ids = revision_lines
        .iter()
        .filter(|line| line.line_type == LineType::GoodsService)
        .map(|line| SalesOrderRevisionLineId::new(line.base.id.clone()))
        .collect::<Vec<_>>();
    let goods_lines = db
        .sales_order_goods_service_line_revisions()
        .list_by_revision_line_ids(&goods_ids, executor)
        .await?;
    let sku_ids = goods_lines
        .iter()
        .map(|line| line.sku_id.clone())
        .collect::<Vec<_>>();
    let skus = db
        .skus()
        .find_by_ids(&sku_ids, executor)
        .await?
        .into_iter()
        .map(|sku| (sku.base.id.clone(), sku))
        .collect::<HashMap<_, _>>();
    let product_ids = skus
        .values()
        .map(|sku| sku.product_id.clone())
        .collect::<Vec<_>>();
    let products = db
        .products()
        .find_by_ids(&product_ids, executor)
        .await?
        .into_iter()
        .map(|product| (product.base.id.clone(), product))
        .collect::<HashMap<_, _>>();
    let purchase_orders = db
        .purchase_orders()
        .find_covering_by_sales_order(sales_order_id, executor)
        .await?;
    let submission_ids = current_pointer_ids(&purchase_orders);
    let submission_lines = db
        .purchase_order_submission_lines()
        .find_lines_by_submission_ids(&submission_ids, executor)
        .await?;
    let purchase_revision_ids = current_revision_pointer_ids(&purchase_orders);
    let purchase_revision_lines = db
        .purchase_order_revision_lines()
        .find_lines_by_revision_ids(&purchase_revision_ids, executor)
        .await?;
    let purchase_line_ids = purchase_revision_lines
        .iter()
        .filter(|line| line.line_type == PurchaseLineType::ItemService)
        .map(|line| PurchaseOrderRevisionLineId::new(line.base.id.clone()))
        .collect::<Vec<_>>();
    let allocations = db
        .purchase_line_sales_allocations()
        .find_by_purchase_revision_line_ids(&purchase_line_ids, executor)
        .await?;
    let target_sales_line_ids = revision_lines
        .iter()
        .filter(|line| line.line_type == LineType::GoodsService)
        .map(|line| line.sales_order_line_id.clone())
        .collect::<Vec<_>>();
    let reservations = db
        .inventory()
        .existing_stock_reservations_for_sales_lines(&target_sales_line_ids, executor)
        .await?;
    Ok(ProcurementCoverageFacts {
        revision: Some(revision),
        revision_lines,
        goods_lines,
        skus,
        products,
        purchase_orders,
        submission_lines,
        purchase_revision_lines,
        allocations,
        reservations,
    })
}

/// 提取草稿类采购单的当前提交指针（读取整形，缺指针的采购单不进入集合）。
///
/// # 参数
/// * `orders` - 参与覆盖的采购单
///
/// # 返回
/// 返回草稿、旧待财务与审批中采购的当前提交 ID。
///
/// # 错误
/// 无。
///
/// # 约束
/// 指针缺失校验由 Entity 层负责，本函数只负责读取整形。
fn current_pointer_ids(orders: &[entities::purchase_order::PurchaseOrder]) -> Vec<PurchaseOrderSubmissionId> {
    orders
        .iter()
        .filter(|order| {
            matches!(
                order.stable.status,
                PurchaseOrderStatus::Draft
                    | PurchaseOrderStatus::PendingFinanceReview
                    | PurchaseOrderStatus::InApproval
            )
        })
        .filter_map(|order| {
            order
                .current_submission_id
                .as_ref()
                .map(|id| PurchaseOrderSubmissionId::new(id.clone()))
        })
        .collect()
}

/// 提取正式采购单的当前版本指针（读取整形，缺指针的采购单不进入集合）。
///
/// # 参数
/// * `orders` - 参与覆盖的采购单
///
/// # 返回
/// 返回生效、部分执行与已完成采购的当前版本 ID。
///
/// # 错误
/// 无。
///
/// # 约束
/// 指针缺失校验由 Entity 层负责，本函数只负责读取整形。
fn current_revision_pointer_ids(
    orders: &[entities::purchase_order::PurchaseOrder],
) -> Vec<PurchaseOrderRevisionId> {
    orders
        .iter()
        .filter(|order| {
            matches!(
                order.stable.status,
                PurchaseOrderStatus::Effective
                    | PurchaseOrderStatus::PartiallyExecuted
                    | PurchaseOrderStatus::Completed
            )
        })
        .filter_map(|order| {
            order
                .stable
                .current_revision_id
                .as_ref()
                .map(|id| PurchaseOrderRevisionId::new(id.clone()))
        })
        .collect()
}

#[cfg(test)]
mod isolation_tests {
    use std::str::FromStr;

    use entities::catalog::product::ProductData;
    use entities::catalog::sku::SkuData;
    use entities::catalog::{EnableStatus, ListingStatus, Product, Sku};
    use entities::common::time::Instant;
    use entities::ids::{
        ProductId, PurchaseLineSalesAllocationId, PurchaseOrderId, PurchaseOrderRevisionId,
        PurchaseOrderRevisionLineId, PurchaseOrderSubmissionId, PurchaseOrderSubmissionLineId, SalesOrderId,
        SalesOrderLineId, SalesOrderRevisionId, SalesOrderRevisionLineId, SkuId, StockReservationId,
        SupplierAccountId, SupplierCommercialProfileRevisionId, UnitOfMeasureId, WarehouseId,
    };
    use entities::inventory::stock_reservation::{
        ReservationStatus, StockReservation, StockReservationData, StockReservationSourceType,
    };
    use entities::money::{Amount, Quantity, Rate};
    use entities::purchase_order::{
        FulfillmentResponsibility, PaymentTermSnapshot, PurchaseLineType, PurchaseOrder, PurchaseOrderData,
        PurchaseOrderRevision, PurchaseOrderRevisionData, PurchaseOrderRevisionLine,
        PurchaseOrderRevisionLineData, PurchaseOrderStatus, PurchaseOrderSubmission,
        PurchaseOrderSubmissionData, PurchaseOrderSubmissionLine, PurchaseOrderSubmissionLineData,
        PurchaseType, SupplierSnapshot,
    };
    use entities::sales_order::revision::{
        SalesOrderGoodsServiceLineRevision, SalesOrderGoodsServiceLineRevisionData, SalesOrderRevision,
        SalesOrderRevisionData, SalesOrderRevisionLine, SalesOrderRevisionLineData,
    };
    use entities::sales_order::snapshot::HeaderSnapshotData;
    use entities::sales_order::{LineType, RevisionSource};
    use test_support::{require_mongo, TestDb};

    use crate::ensure_indexes;
    use crate::repository::extensions::{CatalogExt, InventoryExt, PurchaseOrderExt, SalesOrderExt};
    use crate::{NoTransaction, Transactional};

    use super::load_procurement_coverage_facts;

    /// 构造销售当前版本头。
    fn test_revision(id: &str) -> SalesOrderRevision {
        SalesOrderRevision::new(
            SalesOrderRevisionId::new(id),
            SalesOrderRevisionData {
                sales_order_id: SalesOrderId::new("so-1"),
                revision_no: 1,
                revision_source: RevisionSource::ErpApproval,
                previous_revision_id: None,
                content_hash: format!("hash-{id}"),
                customer_revision_id: None,
                contract_revision_id: None,
                snapshot: HeaderSnapshotData {
                    customer_name: "客户".to_string(),
                    contract_no: None,
                    settlement_party_name: None,
                    payment_term_code: "NET-30".to_string(),
                    payment_term_name: "净30天".to_string(),
                    invoice_type: "增值税专用发票".to_string(),
                    tax_point: "13".to_string(),
                },
                project_name: None,
                business_remark: None,
                voucher_category_sku_id: None,
                voucher_expiry_at: None,
                gross_amount: Amount::from_str("100").unwrap(),
                net_amount: Amount::from_str("100").unwrap(),
                tax_amount: Amount::from_str("0").unwrap(),
                effective_at: Instant::from_unix_secs(1_800_000_000),
                recorded_at: Instant::from_unix_secs(1_800_000_000),
            },
        )
        .unwrap()
    }

    /// 构造销售当前版本公共行。
    fn test_revision_line(id: &str, stable_line_id: &str) -> SalesOrderRevisionLine {
        SalesOrderRevisionLine::new(
            SalesOrderRevisionLineId::new(id),
            SalesOrderRevisionLineData {
                sales_order_revision_id: SalesOrderRevisionId::new("rev-1"),
                sales_order_line_id: SalesOrderLineId::new(stable_line_id),
                line_no: 1,
                line_type: LineType::GoodsService,
                gross_amount: Amount::from_str("10").unwrap(),
                net_amount: Amount::from_str("10").unwrap(),
                tax_amount: Amount::from_str("0").unwrap(),
                sales_tax_rate: Rate::from_str("0").unwrap(),
                item_name_snapshot: "商品".to_string(),
                spec_snapshot: Some("规格".to_string()),
                unit_snapshot: Some("件".to_string()),
            },
        )
        .unwrap()
    }

    /// 构造销售当前版本商品/服务子类型行。
    fn test_goods_line(revision_line_id: &str) -> SalesOrderGoodsServiceLineRevision {
        SalesOrderGoodsServiceLineRevision::new(
            entities::ids::SalesOrderGoodsServiceLineRevisionId::new(format!("goods-{revision_line_id}")),
            SalesOrderGoodsServiceLineRevisionData {
                revision_line_id: SalesOrderRevisionLineId::new(revision_line_id),
                sku_id: SkuId::new("sku-1"),
                sku_revision_id: entities::ids::SkuRevisionId::new("skur-1"),
                welfare_scenario: None,
                service_region: None,
                fulfillment_due_at: Instant::from_unix_secs(1_800_000_000),
                quantity: Quantity::from_str("10").unwrap(),
                base_unit_code: "件".to_string(),
                unit_price_gross: entities::money::UnitPrice::from_str("5").unwrap(),
            },
        )
        .unwrap()
    }

    /// 构造 SKU 事实。
    fn test_sku(id: &str) -> Sku {
        Sku::new(
            SkuId::new(id),
            SkuData {
                sku_no: format!("SKU-{id}"),
                product_id: ProductId::new(format!("product-{id}")),
                base_unit_id: UnitOfMeasureId::new("unit-1"),
                specification_signature: format!("spec-{id}"),
                status: EnableStatus::Active,
                listing_status: ListingStatus::Listed,
            },
            "test",
        )
        .unwrap()
    }

    /// 构造商品事实。
    fn test_product(id: &str) -> Product {
        Product::new(
            ProductId::new(id),
            ProductData {
                product_no: format!("P-{id}"),
                product_kind: entities::catalog::ProductKind::Physical,
                status: EnableStatus::Active,
            },
            "test",
        )
        .unwrap()
    }

    /// 构造指定状态和当前指针的采购单。
    fn purchase_order(
        id: &str,
        status: PurchaseOrderStatus,
        submission_id: Option<&str>,
        revision_id: Option<&str>,
    ) -> PurchaseOrder {
        let mut order = PurchaseOrder::new(
            PurchaseOrderId::new(id),
            PurchaseOrderData {
                purchase_no: format!("PO-{id}"),
                sales_order_id: SalesOrderId::new("so-1"),
                sales_order_revision_id: SalesOrderRevisionId::new("rev-1"),
                creation_basis_id: format!("basis-{id}"),
                supplier_id: SupplierAccountId::new("sup-1"),
                purchase_type: PurchaseType::Physical,
                payment_term_code: "NET-30".to_string(),
                fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
                owner_user_id: "buyer-1".to_string(),
                target_warehouse_id: Some(WarehouseId::new("warehouse-1")),
            },
            "buyer-1",
        )
        .unwrap();
        order.stable.status = status;
        order.current_submission_id = submission_id.map(str::to_string);
        order.stable.current_revision_id = revision_id.map(str::to_string);
        order
    }

    /// 构造草稿类采购提交。
    fn submission(id: &str, order_id: &str) -> PurchaseOrderSubmission {
        PurchaseOrderSubmission::new(
            PurchaseOrderSubmissionId::new(id),
            PurchaseOrderSubmissionData {
                purchase_order_id: PurchaseOrderId::new(order_id),
                submission_no: format!("SUB-{id}"),
                supplier_id: SupplierAccountId::new("sup-1"),
                purchase_type: PurchaseType::Physical,
                fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
                supplier_revision_id: SupplierCommercialProfileRevisionId::new("suprev-1"),
                supplier_snapshot: SupplierSnapshot::new("供应商".to_string()).unwrap(),
                payment_term_snapshot: PaymentTermSnapshot::new("NET-30".to_string(), false, None, None)
                    .unwrap(),
                gross_amount: Amount::from_str("10").unwrap(),
                net_amount: Amount::from_str("10").unwrap(),
                tax_amount: Amount::from_str("0").unwrap(),
            },
        )
        .unwrap()
    }

    /// 构造草稿类采购提交行。
    fn submission_line(id: &str, submission_id: &str, stable_line_id: &str) -> PurchaseOrderSubmissionLine {
        PurchaseOrderSubmissionLine::new(
            PurchaseOrderSubmissionLineId::new(id),
            PurchaseOrderSubmissionLineData {
                purchase_order_submission_id: PurchaseOrderSubmissionId::new(submission_id),
                line_no: 1,
                line_type: PurchaseLineType::ItemService,
                procurement_confirmation_line_id: Some(entities::ids::ProcurementConfirmationLineId::new(
                    "pcl-1",
                )),
                sku_id: Some(SkuId::new("sku-1")),
                sku_revision_id: Some(entities::ids::SkuRevisionId::new("skur-1")),
                product_name_snapshot: Some("商品".to_string()),
                specification_snapshot: Some("规格".to_string()),
                quantity: Some(Quantity::from_str("2").unwrap()),
                base_unit_code: Some("件".to_string()),
                unit_cost_gross: Some(entities::money::UnitPrice::from_str("5").unwrap()),
                gross_amount: Amount::from_str("10").unwrap(),
                net_amount: Amount::from_str("10").unwrap(),
                tax_amount: Amount::from_str("0").unwrap(),
                input_tax_rate: Some(Rate::from_str("0").unwrap()),
                expected_delivery_date: None,
                sales_order_line_id: Some(SalesOrderLineId::new(stable_line_id)),
                sales_order_revision_line_id: Some(SalesOrderRevisionLineId::new("sorl-1")),
                sales_order_submission_line_id: None,
                allocated_quantity: Some(Quantity::from_str("2").unwrap()),
            },
        )
        .unwrap()
    }

    /// 构造正式采购版本。
    fn purchase_revision(id: &str, order_id: &str) -> PurchaseOrderRevision {
        PurchaseOrderRevision::new(
            PurchaseOrderRevisionId::new(id),
            PurchaseOrderRevisionData {
                purchase_order_id: PurchaseOrderId::new(order_id),
                revision_no: 1,
                supplier_revision_id: SupplierCommercialProfileRevisionId::new("suprev-1"),
                supplier_snapshot: SupplierSnapshot::new("供应商".to_string()).unwrap(),
                payment_term_snapshot: PaymentTermSnapshot::new("NET-30".to_string(), false, None, None)
                    .unwrap(),
                gross_amount: Amount::from_str("15").unwrap(),
                net_amount: Amount::from_str("15").unwrap(),
                tax_amount: Amount::from_str("0").unwrap(),
                effective_at: Instant::from_unix_secs(1_800_000_000),
            },
        )
        .unwrap()
    }

    /// 构造正式采购版本行。
    fn purchase_revision_line(
        id: &str,
        revision_id: &str,
        stable_line_id: &str,
    ) -> PurchaseOrderRevisionLine {
        PurchaseOrderRevisionLine::new(
            PurchaseOrderRevisionLineId::new(id),
            PurchaseOrderRevisionLineData {
                purchase_order_revision_id: PurchaseOrderRevisionId::new(revision_id),
                line_no: 1,
                line_type: PurchaseLineType::ItemService,
                procurement_confirmation_line_id: Some(entities::ids::ProcurementConfirmationLineId::new(
                    "pcl-1",
                )),
                sku_id: Some(SkuId::new("sku-1")),
                sku_revision_id: Some(entities::ids::SkuRevisionId::new("skur-1")),
                product_name_snapshot: Some("商品".to_string()),
                specification_snapshot: Some("规格".to_string()),
                quantity: Some(Quantity::from_str("3").unwrap()),
                base_unit_code: Some("件".to_string()),
                unit_cost_gross: Some(entities::money::UnitPrice::from_str("5").unwrap()),
                gross_amount: Amount::from_str("15").unwrap(),
                net_amount: Amount::from_str("15").unwrap(),
                tax_amount: Amount::from_str("0").unwrap(),
                input_tax_rate: Some(Rate::from_str("0").unwrap()),
                expected_delivery_date: None,
                sales_order_line_id: Some(SalesOrderLineId::new(stable_line_id)),
                sales_order_revision_line_id: Some(SalesOrderRevisionLineId::new("sorl-1")),
                allocated_quantity: Some(Quantity::from_str("3").unwrap()),
            },
        )
        .unwrap()
    }

    /// 构造正式采购分配。
    fn allocation(id: &str, purchase_line_id: &str) -> entities::purchase_order::PurchaseLineSalesAllocation {
        entities::purchase_order::PurchaseLineSalesAllocation::new(
            PurchaseLineSalesAllocationId::new(id),
            entities::purchase_order::PurchaseLineSalesAllocationData {
                purchase_order_revision_line_id: PurchaseOrderRevisionLineId::new(purchase_line_id),
                sales_order_revision_line_id: SalesOrderRevisionLineId::new("sorl-1"),
                allocated_quantity: Quantity::from_str("3").unwrap(),
                allocated_cost_gross: Amount::from_str("15").unwrap(),
                allocated_cost_net: Amount::from_str("15").unwrap(),
            },
        )
        .unwrap()
    }

    /// 构造现有库存预占。
    fn reservation(id: &str) -> StockReservation {
        StockReservation::new(
            StockReservationId::new(id),
            StockReservationData {
                warehouse_id: WarehouseId::new("warehouse-1"),
                sku_id: SkuId::new("sku-1"),
                sales_order_line_id: SalesOrderLineId::new("sol-1"),
                source_type: StockReservationSourceType::ExistingStock,
                purchase_line_sales_allocation_id: None,
                source_receipt_line_id: None,
                source_allocation_id: Some("allocation-1".to_string()),
                reserved_quantity: Quantity::from_str("1").unwrap(),
                consumed_quantity: Quantity::from_str("1").unwrap(),
                released_quantity: Quantity::from_str("0").unwrap(),
                status: ReservationStatus::PartiallyConsumed,
            },
        )
        .unwrap()
    }

    /// 写入一套完整覆盖夹具：销售当前版本目标、草稿提交、正式版本、分配与现有库存预占。
    async fn seed_coverage_fixture(db: &mongodb::Database) {
        db.sales_order_revisions()
            .create(&test_revision("rev-1"), &mut NoTransaction)
            .await
            .expect("销售版本写入失败");
        db.sales_order_revision_lines()
            .create(&test_revision_line("sorl-1", "sol-1"), &mut NoTransaction)
            .await
            .expect("销售版本行写入失败");
        db.sales_order_goods_service_line_revisions()
            .create(&test_goods_line("sorl-1"), &mut NoTransaction)
            .await
            .expect("商品子类型行写入失败");
        db.skus()
            .create(&test_sku("sku-1"), &mut NoTransaction)
            .await
            .expect("SKU 写入失败");
        db.products()
            .create(&test_product("product-sku-1"), &mut NoTransaction)
            .await
            .expect("商品写入失败");
        db.purchase_orders()
            .create(
                &purchase_order("po-draft", PurchaseOrderStatus::Draft, Some("sub-1"), None),
                &mut NoTransaction,
            )
            .await
            .expect("草稿采购单写入失败");
        db.purchase_order_submissions()
            .create(&submission("sub-1", "po-draft"), &mut NoTransaction)
            .await
            .expect("采购提交写入失败");
        db.purchase_order_submission_lines()
            .create(&submission_line("subl-1", "sub-1", "sol-1"), &mut NoTransaction)
            .await
            .expect("采购提交行写入失败");
        db.purchase_orders()
            .create(
                &purchase_order(
                    "po-effective",
                    PurchaseOrderStatus::Effective,
                    Some("sub-history"),
                    Some("rev-po-1"),
                ),
                &mut NoTransaction,
            )
            .await
            .expect("正式采购单写入失败");
        db.purchase_order_revisions()
            .create(&purchase_revision("rev-po-1", "po-effective"), &mut NoTransaction)
            .await
            .expect("采购版本写入失败");
        db.purchase_order_revision_lines()
            .create(
                &purchase_revision_line("porl-1", "rev-po-1", "sol-1"),
                &mut NoTransaction,
            )
            .await
            .expect("采购版本行写入失败");
        db.purchase_line_sales_allocations()
            .create(&allocation("alloc-1", "porl-1"), &mut NoTransaction)
            .await
            .expect("销售分配写入失败");
        db.stock_reservations()
            .create(&reservation("rsv-1"), &mut NoTransaction)
            .await
            .expect("库存预占写入失败");
    }

    /// 当前销售版本文档缺失时返回空事实集合，不触发后续查询。
    ///
    /// # 参数
    /// 无，内部创建隔离库。
    ///
    /// # 返回
    /// 断言 revision 为空且其余事实集合全部为空。
    ///
    /// # 错误
    /// MongoDB 连接或事实加载失败时测试失败。
    ///
    /// # 约束
    /// Exact 维度：缺失事实由 Entity 层校验，Repository 只负责读取整形。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn missing_revision_doc_returns_empty_facts() {
        require_mongo!(async {
            let fixture = TestDb::new("proc_coverage_missing_revision")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            let facts = load_procurement_coverage_facts(
                fixture.db(),
                &SalesOrderRevisionId::new("rev-missing"),
                &SalesOrderId::new("so-1"),
                &mut NoTransaction,
            )
            .await
            .expect("事实加载失败");
            assert!(facts.revision.is_none());
            assert!(facts.revision_lines.is_empty());
            assert!(facts.goods_lines.is_empty());
            assert!(facts.skus.is_empty());
            assert!(facts.products.is_empty());
            assert!(facts.purchase_orders.is_empty());
            assert!(facts.submission_lines.is_empty());
            assert!(facts.purchase_revision_lines.is_empty());
            assert!(facts.allocations.is_empty());
            assert!(facts.reservations.is_empty());
        });
    }

    /// 完整夹具下一次性返回当前销售目标与全部当前覆盖来源。
    ///
    /// # 参数
    /// 无，内部创建隔离库并写入完整夹具。
    ///
    /// # 返回
    /// 断言各事实集合均包含夹具写入的当前指针数据。
    ///
    /// # 错误
    /// MongoDB 连接、夹具写入或事实加载失败时测试失败。
    ///
    /// # 约束
    /// Batch 维度：一次调用返回全部来源，查询次数与行数无关。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn loads_current_target_and_all_coverage_sources() {
        require_mongo!(async {
            let fixture = TestDb::new("proc_coverage_facts")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            seed_coverage_fixture(fixture.db()).await;

            let facts = load_procurement_coverage_facts(
                fixture.db(),
                &SalesOrderRevisionId::new("rev-1"),
                &SalesOrderId::new("so-1"),
                &mut NoTransaction,
            )
            .await
            .expect("事实加载失败");
            assert!(facts.revision.is_some(), "当前销售版本应被加载");
            assert_eq!(facts.revision_lines.len(), 1, "销售公共行应被加载");
            assert_eq!(facts.goods_lines.len(), 1, "商品子类型行应被加载");
            assert_eq!(facts.skus.len(), 1, "SKU 应被加载");
            assert_eq!(facts.products.len(), 1, "商品应被加载");
            assert_eq!(facts.purchase_orders.len(), 2, "草稿与正式采购单都应被加载");
            assert_eq!(facts.submission_lines.len(), 1, "当前提交行应被加载");
            assert_eq!(facts.purchase_revision_lines.len(), 1, "当前版本行应被加载");
            assert_eq!(facts.allocations.len(), 1, "正式分配应被加载");
            assert_eq!(facts.reservations.len(), 1, "现有库存预占应被加载");
            assert_eq!(
                facts.submission_lines[0]
                    .sales_order_line_id
                    .as_ref()
                    .unwrap()
                    .to_string(),
                "sol-1"
            );
        });
    }

    /// 作废采购单与其历史提交/版本行不进入事实集合。
    ///
    /// # 参数
    /// 无，内部创建隔离库并写入含作废单的夹具。
    ///
    /// # 返回
    /// 断言作废采购单不参与覆盖事实。
    ///
    /// # 错误
    /// MongoDB 连接、夹具写入或事实加载失败时测试失败。
    ///
    /// # 约束
    /// 软删除与作废过滤由 Repository 查询条件保证，历史提交不进入当前指针读取。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn voided_orders_and_historical_pointers_are_excluded() {
        require_mongo!(async {
            let fixture = TestDb::new("proc_coverage_voided")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            seed_coverage_fixture(fixture.db()).await;
            // 作废采购单：即使携带指针也不进入覆盖事实。
            fixture
                .db()
                .purchase_orders()
                .create(
                    &purchase_order(
                        "po-voided",
                        PurchaseOrderStatus::Voided,
                        Some("sub-voided"),
                        Some("rev-po-voided"),
                    ),
                    &mut NoTransaction,
                )
                .await
                .expect("作废采购单写入失败");
            // 历史提交：已不再是当前指针，其行不得进入事实集合。
            fixture
                .db()
                .purchase_order_submissions()
                .create(&submission("sub-history", "po-effective"), &mut NoTransaction)
                .await
                .expect("历史提交写入失败");
            fixture
                .db()
                .purchase_order_submission_lines()
                .create(
                    &submission_line("subl-history", "sub-history", "sol-history"),
                    &mut NoTransaction,
                )
                .await
                .expect("历史提交行写入失败");

            let facts = load_procurement_coverage_facts(
                fixture.db(),
                &SalesOrderRevisionId::new("rev-1"),
                &SalesOrderId::new("so-1"),
                &mut NoTransaction,
            )
            .await
            .expect("事实加载失败");
            assert_eq!(facts.purchase_orders.len(), 2, "作废采购单不得参与覆盖");
            assert_eq!(facts.submission_lines.len(), 1, "历史提交行不得进入当前覆盖");
            assert_eq!(
                facts.purchase_revision_lines.len(),
                1,
                "历史版本行不得进入当前覆盖"
            );
        });
    }

    /// 事务内调用复用调用方 session，读取自身未提交写入。
    ///
    /// # 参数
    /// 无，内部创建隔离库。
    ///
    /// # 返回
    /// 断言事务内可读到同一 session 刚写入的覆盖事实。
    ///
    /// # 错误
    /// MongoDB 连接、夹具写入、事务或事实加载失败时测试失败。
    ///
    /// # 约束
    /// 事务内重验必须复用调用方 executor，不得另开连接或独立事务。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn transaction_reads_own_writes_with_same_session() {
        require_mongo!(async {
            let fixture = TestDb::new("proc_coverage_txn")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            seed_coverage_fixture(fixture.db()).await;

            let db = fixture.db().clone();
            let client = db.client().clone();
            client
                .with_transaction::<_, (), crate::errors::Error>(move |session| {
                    let db = db.clone();
                    Box::pin(async move {
                        // 同一 session 内写入新的草稿提交行并推进当前提交指针，
                        // 事实加载必须立即可见。
                        db.purchase_order_submissions()
                            .create(&submission("sub-txn", "po-draft"), session)
                            .await?;
                        db.purchase_order_submission_lines()
                            .create(&submission_line("subl-txn", "sub-txn", "sol-1"), session)
                            .await?;
                        let mut txn_order =
                            purchase_order("po-draft", PurchaseOrderStatus::Draft, Some("sub-txn"), None);
                        db.purchase_orders().update(&mut txn_order, session).await?;
                        let facts = load_procurement_coverage_facts(
                            &db,
                            &SalesOrderRevisionId::new("rev-1"),
                            &SalesOrderId::new("so-1"),
                            session,
                        )
                        .await?;
                        assert!(
                            facts
                                .submission_lines
                                .iter()
                                .any(|line| line.base.id == "subl-txn"),
                            "事务内应能 read-your-writes"
                        );
                        Ok(())
                    })
                })
                .await
                .expect("事务内事实加载失败");
        });
    }
}
