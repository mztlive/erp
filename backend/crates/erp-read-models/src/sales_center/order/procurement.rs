//! 销售详情将已证明的历史采购依据单独展示；采购命令继续使用严格当前版本覆盖。

use std::collections::{HashMap, HashSet};

use erp_core::ids::{PurchaseOrderRevisionId, SalesOrderId, SalesOrderRevisionId};
use erp_core::money::{Quantity, Rate};
use erp_procurement::entity::purchase_order::{
    PaymentSubjectSnapshot, ProcurementCoverageFacts, PurchaseLineSalesAllocation, PurchaseLineType,
    PurchaseOrder, PurchaseOrderRevision, PurchaseOrderRevisionLine, PurchaseOrderStatus,
    SalesProcurementCoverage, build_procurement_coverage,
};
use erp_procurement::repository::PurchaseOrderExt;
use erp_sales::entity::sales_order::{LineType, SalesOrder, SalesOrderRevision, SalesOrderRevisionLine};
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction};
use rust_decimal::Decimal;

use super::SalesOrderReadService;
use super::dto::{SalesProcurementCoverageLineView, SalesProcurementCoverageView};
use crate::purchase_center::repository::load_procurement_coverage_facts;
use crate::{Error, Result};

#[derive(Clone)]
struct SalesVersionIdentity {
    order_id: String,
    revision_no: u32,
    party_id: String,
    party_name: Option<String>,
}

#[derive(Clone)]
struct SalesLineIdentity {
    revision_id: String,
    stable_line_id: String,
    is_goods_service: bool,
}

#[derive(Clone)]
struct PurchaseVersionIdentity {
    order_id: String,
    subject: PaymentSubjectSnapshot,
}

#[derive(Clone, Default)]
struct HistoricalSources {
    sales_versions: HashMap<String, SalesVersionIdentity>,
    sales_lines: HashMap<String, SalesLineIdentity>,
    purchase_versions: HashMap<String, PurchaseVersionIdentity>,
}

struct OutdatedPurchases {
    count: usize,
    quantity: Quantity,
}

/// 为采购正式变更后的任务同步读取当前合格覆盖，保留尚未更新的历史采购。
/// # 参数
/// `db` 为业务数据库；`order` 为调用方已加载的销售单；`executor` 为同一原子用例执行器。
/// # 返回
/// 返回只包含当前正式销售版本合格分配的逐行覆盖；历史来源经证明后单独排除。
/// # 错误
/// 正式版本缺失、历史身份无法证明、当前覆盖异常或读取失败时拒绝。
/// # 约束
/// 不修改采购、分配或付款申请；新增采购仍由创建入口的严格来源版本资格校验阻断。
pub async fn load_current_procurement_coverage_for_sync(
    db: &Database,
    order: &SalesOrder,
    executor: &mut dyn Executor,
) -> Result<SalesProcurementCoverage> {
    let revision_id = order
        .current_revision_id()
        .ok_or_else(|| Error::BusinessLogicError("销售单缺少当前版本，无法计算采购剩余量".into()))?;
    let facts = load_procurement_coverage_facts(
        db,
        &SalesOrderRevisionId::new(revision_id),
        &SalesOrderId::new(&order.base.id),
        executor,
    )
    .await?;
    let sources = historical_sources(db, &facts, revision_id, executor).await?;
    current_coverage(facts, &order.base.id, revision_id, &sources).map(|(coverage, _)| coverage)
}

impl SalesOrderReadService {
    /// 展示当前合格覆盖及仍引用历史正式销售版本的采购数量。
    /// # 参数
    /// `order` 是已授权销售单；读取复用展示查询执行器。
    /// # 返回
    /// 历史采购不计入当前合格覆盖，单独标记并阻断新增采购资格。
    /// # 错误
    /// 当前或历史正式身份缺失、关系不符、数量异常及仓储失败时拒绝。
    pub(super) async fn sales_procurement_coverage(
        &self,
        order: &SalesOrder,
    ) -> Result<SalesProcurementCoverageView> {
        let Some(revision_id) = order.current_revision_id() else {
            return empty_coverage();
        };
        let facts = load_procurement_coverage_facts(
            &self.db,
            &SalesOrderRevisionId::new(revision_id),
            &SalesOrderId::new(&order.base.id),
            &mut NoTransaction,
        )
        .await?;
        let sources = historical_sources(&self.db, &facts, revision_id, &mut NoTransaction).await?;
        display_coverage(facts, &order.base.id, revision_id, &sources)
    }
}

/// 一次批量读取历史正式头、销售行和当前采购版本，不按采购逐条查询。
async fn historical_sources(
    db: &Database,
    facts: &ProcurementCoverageFacts,
    current_revision: &str,
    executor: &mut dyn Executor,
) -> Result<HistoricalSources> {
    let orders = outdated_orders(facts, current_revision);
    if orders.is_empty() {
        return Ok(HistoricalSources::default());
    }
    let mut sales_ids =
        orders.iter().map(|order| order.sales_order_revision_id.to_string()).collect::<HashSet<_>>();
    sales_ids.insert(current_revision.to_owned());
    let sales_ids = sales_ids.into_iter().collect::<Vec<_>>();
    let purchase_ids = orders
        .iter()
        .filter_map(|order| order.stable.current_revision_id.as_ref().map(PurchaseOrderRevisionId::new))
        .collect::<Vec<_>>();
    let sales_versions = db.sales_order_revisions().find_revisions_by_ids(&sales_ids, executor).await?;
    let sales_lines = db
        .sales_order_revision_lines()
        .list_lines_by_revisions(
            &sales_ids.iter().map(SalesOrderRevisionId::new).collect::<Vec<_>>(),
            executor,
        )
        .await?;
    let purchase_versions = db.purchase_order().find_revisions_by_ids(&purchase_ids, executor).await?;
    Ok(HistoricalSources::from_records(sales_versions, sales_lines, purchase_versions))
}

impl HistoricalSources {
    fn from_records(
        sales_versions: Vec<SalesOrderRevision>,
        sales_lines: Vec<SalesOrderRevisionLine>,
        purchase_versions: Vec<PurchaseOrderRevision>,
    ) -> Self {
        Self {
            sales_versions: sales_versions
                .into_iter()
                .filter(|version| version.base.deleted_at == 0)
                .map(|version| {
                    (
                        version.base.id,
                        SalesVersionIdentity {
                            order_id: version.sales_order_id.to_string(),
                            revision_no: version.revision.revision_no,
                            party_id: version.settlement_party_id.to_string(),
                            party_name: version
                                .settlement_party_snapshot
                                .map(|snapshot| snapshot.settlement_party_name),
                        },
                    )
                })
                .collect(),
            sales_lines: sales_lines
                .into_iter()
                .filter(|line| line.base.deleted_at == 0)
                .map(|line| {
                    (
                        line.base.id,
                        SalesLineIdentity {
                            revision_id: line.sales_order_revision_id.to_string(),
                            stable_line_id: line.sales_order_line_id.to_string(),
                            is_goods_service: line.line_type == LineType::GoodsService,
                        },
                    )
                })
                .collect(),
            purchase_versions: purchase_versions
                .into_iter()
                .filter(|version| version.base.deleted_at == 0)
                .map(|version| {
                    (
                        version.base.id,
                        PurchaseVersionIdentity {
                            order_id: version.purchase_order_id.to_string(),
                            subject: version.payment_subject,
                        },
                    )
                })
                .collect(),
        }
    }
}

fn outdated_orders<'a>(
    facts: &'a ProcurementCoverageFacts,
    current_revision: &str,
) -> Vec<&'a PurchaseOrder> {
    facts
        .purchase_orders
        .iter()
        .filter(|order| {
            matches!(
                order.stable.status,
                PurchaseOrderStatus::Effective
                    | PurchaseOrderStatus::PartiallyExecuted
                    | PurchaseOrderStatus::Completed
            ) && order.sales_order_revision_id.as_ref() != current_revision
        })
        .collect()
}

fn display_coverage(
    facts: ProcurementCoverageFacts,
    sales_id: &str,
    current_revision: &str,
    sources: &HistoricalSources,
) -> Result<SalesProcurementCoverageView> {
    let (coverage, outdated) = current_coverage(facts, sales_id, current_revision, sources)?;
    Ok(SalesProcurementCoverageView {
        lines: coverage
            .lines
            .into_iter()
            .map(|line| SalesProcurementCoverageLineView {
                sales_order_line_id: line.revision_line.sales_order_line_id.to_string(),
                line_no: line.revision_line.line_no,
                item_name: line.revision_line.item_name_snapshot,
                specification: line.revision_line.spec_snapshot,
                unit: line.revision_line.unit_snapshot,
                total_quantity: line.summary.total_quantity,
                covered_quantity: line.summary.covered_quantity,
                remaining_quantity: line.summary.remaining_quantity,
            })
            .collect(),
        total_quantity: coverage.summary.total_quantity,
        covered_quantity: coverage.summary.covered_quantity,
        remaining_quantity: coverage.summary.remaining_quantity,
        progress: coverage.summary.progress,
        source_changed: outdated.count != 0,
        outdated_purchase_quantity: outdated.quantity,
        outdated_purchase_count: outdated.count,
    })
}

fn current_coverage(
    mut facts: ProcurementCoverageFacts,
    sales_id: &str,
    current_revision: &str,
    sources: &HistoricalSources,
) -> Result<(SalesProcurementCoverage, OutdatedPurchases)> {
    let orders = outdated_orders(&facts, current_revision);
    let outdated_purchase_count = orders.len();
    let old_order_ids = orders.iter().map(|order| order.base.id.clone()).collect::<HashSet<_>>();
    let mut old_revisions = HashSet::new();
    for order in orders {
        old_revisions.insert(verify_historical_order(order, sales_id, current_revision, sources)?.to_owned());
    }
    let old_lines = facts
        .purchase_revision_lines
        .iter()
        .filter(|line| old_revisions.contains(line.purchase_order_revision_id.as_ref()))
        .collect::<Vec<_>>();
    let outdated_purchase_quantity =
        historical_quantity(&old_lines, &facts.allocations, &old_revisions, sources)?;
    let old_line_ids = old_lines.iter().map(|line| line.base.id.clone()).collect::<HashSet<_>>();
    facts.purchase_orders.retain(|order| !old_order_ids.contains(order.base.id.as_str()));
    facts
        .purchase_revision_lines
        .retain(|line| !old_revisions.contains(line.purchase_order_revision_id.as_ref()));
    facts
        .allocations
        .retain(|allocation| !old_line_ids.contains(allocation.purchase_order_revision_line_id.as_ref()));
    let coverage = build_procurement_coverage(facts).map_err(Error::Logic)?;
    Ok((coverage, OutdatedPurchases { count: outdated_purchase_count, quantity: outdated_purchase_quantity }))
}

fn verify_historical_order<'a>(
    order: &'a PurchaseOrder,
    sales_id: &str,
    current_revision: &str,
    sources: &HistoricalSources,
) -> Result<&'a str> {
    let revision_id = order.stable.current_revision_id.as_deref().ok_or_else(invalid_history)?;
    let purchase = sources.purchase_versions.get(revision_id).ok_or_else(invalid_history)?;
    let original =
        sources.sales_versions.get(order.sales_order_revision_id.as_ref()).ok_or_else(invalid_history)?;
    let current = sources.sales_versions.get(current_revision).ok_or_else(invalid_history)?;
    let subject = &purchase.subject;
    if order.sales_order_id.as_ref() != sales_id
        || purchase.order_id != order.base.id
        || original.order_id != sales_id
        || current.order_id != sales_id
        || original.revision_no == 0
        || original.revision_no >= current.revision_no
        || original.party_id.trim().is_empty()
        || original.party_name.as_ref().is_none_or(|name| name.trim().is_empty())
        || subject.sales_order_id.as_ref() != sales_id
        || subject.sales_order_revision_id != order.sales_order_revision_id
        || subject.settlement_party_id.as_ref() != original.party_id
        || original.party_name.as_deref() != Some(subject.settlement_party_name.as_str())
    {
        return Err(invalid_history());
    }
    Ok(revision_id)
}

fn historical_quantity(
    lines: &[&PurchaseOrderRevisionLine],
    allocations: &[PurchaseLineSalesAllocation],
    revision_ids: &HashSet<String>,
    sources: &HistoricalSources,
) -> Result<Quantity> {
    if revision_ids.iter().any(|id| !lines.iter().any(|line| line.purchase_order_revision_id.as_ref() == id))
    {
        return Err(invalid_history());
    }
    let mut total = Decimal::ZERO;
    let mut by_line = HashMap::<&str, Vec<&PurchaseLineSalesAllocation>>::new();
    for allocation in allocations {
        by_line.entry(allocation.purchase_order_revision_line_id.as_ref()).or_default().push(allocation);
    }
    for line in lines.iter().filter(|line| line.line_type == PurchaseLineType::ItemService) {
        let matching = by_line.get(line.base.id.as_str()).ok_or_else(invalid_history)?;
        let [allocation] = matching.as_slice() else {
            return Err(invalid_history());
        };
        let purchase = sources
            .purchase_versions
            .get(line.purchase_order_revision_id.as_ref())
            .ok_or_else(invalid_history)?;
        let source_line_id = line.sales_order_revision_line_id.as_ref().ok_or_else(invalid_history)?;
        let source_line = sources.sales_lines.get(source_line_id.as_ref()).ok_or_else(invalid_history)?;
        if !source_line.is_goods_service
            || source_line.revision_id != purchase.subject.sales_order_revision_id.as_ref()
            || line.sales_order_line_id.as_ref().map(|id| id.as_ref())
                != Some(source_line.stable_line_id.as_str())
            || allocation.sales_order_revision_line_id != *source_line_id
            || line.allocated_quantity != Some(allocation.allocated_quantity)
            || line.quantity != Some(allocation.allocated_quantity)
            || allocation.allocated_quantity.to_decimal() <= Decimal::ZERO
        {
            return Err(invalid_history());
        }
        total = total.checked_add(allocation.allocated_quantity.to_decimal()).ok_or_else(invalid_history)?;
    }
    Quantity::try_from(total).map_err(Into::into)
}

fn empty_coverage() -> Result<SalesProcurementCoverageView> {
    let zero = Quantity::try_from(Decimal::ZERO)?;
    Ok(SalesProcurementCoverageView {
        lines: Vec::new(),
        total_quantity: zero,
        covered_quantity: zero,
        remaining_quantity: zero,
        progress: Rate::try_from(Decimal::ZERO)?,
        source_changed: false,
        outdated_purchase_quantity: zero,
        outdated_purchase_count: 0,
    })
}

fn invalid_history() -> Error {
    Error::ConflictError("销售历史采购的正式版本、主体或分配依据不完整，请核对相关资料".into())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::common::time::Instant;
    use erp_core::ids::{
        ProcurementConfirmationLineId, ProductId, PurchaseLineSalesAllocationId, PurchaseOrderId,
        PurchaseOrderRevisionId, PurchaseOrderRevisionLineId, SalesOrderLineId, SalesOrderRevisionLineId,
        SkuId, SkuRevisionId, SupplierAccountId, WarehouseId,
    };
    use erp_core::money::{Amount, UnitPrice};
    use erp_procurement::entity::facts::{
        CurrentRevisionFact, FactIdentity, PaymentTermFact, ProductFact, ProductKind,
        SalesCustomerSnapshotFact, SalesGoodsLineFact, SalesLineType, SalesRevisionFact,
        SalesRevisionLineFact, SkuFact,
    };
    use erp_procurement::entity::purchase_order::{
        FulfillmentResponsibility, PurchaseLineSalesAllocationData, PurchaseOrderData,
        PurchaseOrderRevisionLineData, PurchaseType,
    };

    use super::*;

    fn quantity(value: &str) -> Quantity {
        Quantity::from_str(value).unwrap()
    }

    fn purchase(source_revision: &str) -> PurchaseOrder {
        let mut order = PurchaseOrder::new(
            PurchaseOrderId::new("purchase"),
            PurchaseOrderData {
                business_org_unit_id: "procurement-org".into(),
                purchase_no: "PO-1".into(),
                sales_order_id: SalesOrderId::new("sales"),
                sales_order_revision_id: SalesOrderRevisionId::new(source_revision),
                creation_basis_id: "basis".into(),
                supplier_id: SupplierAccountId::new("supplier"),
                purchase_type: PurchaseType::Physical,
                payment_term_code: "POSTPAY_NET30".into(),
                fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
                owner_user_id: "buyer".into(),
                target_warehouse_id: Some(WarehouseId::new("warehouse")),
            },
            "buyer",
            |_| Ok(PaymentTermFact::new("POSTPAY_NET30").with_days_after_delivery(30)),
        )
        .unwrap();
        order.stable.status = PurchaseOrderStatus::Effective;
        order.stable.current_revision_id = Some("purchase-version".into());
        order
    }

    fn purchase_line(source_line: &str) -> PurchaseOrderRevisionLine {
        PurchaseOrderRevisionLine::new(
            PurchaseOrderRevisionLineId::new("purchase-line"),
            PurchaseOrderRevisionLineData {
                purchase_order_revision_id: PurchaseOrderRevisionId::new("purchase-version"),
                line_no: 1,
                line_type: PurchaseLineType::ItemService,
                procurement_confirmation_line_id: Some(ProcurementConfirmationLineId::new(
                    "confirmation-line",
                )),
                sku_id: Some(SkuId::new("sku")),
                sku_revision_id: Some(SkuRevisionId::new("sku-version")),
                supplier_offering_source: None,
                delivery_allocations: Vec::new(),
                product_name_snapshot: Some("商品".into()),
                specification_snapshot: Some("规格".into()),
                quantity: Some(quantity("2")),
                base_unit_code: Some("件".into()),
                unit_cost_gross: Some(UnitPrice::from_str("100").unwrap()),
                gross_amount: Amount::from_str("200").unwrap(),
                net_amount: Amount::from_str("200").unwrap(),
                tax_amount: Amount::zero(),
                input_tax_rate: Some(Rate::from_str("0").unwrap()),
                expected_delivery_date: None,
                sales_order_line_id: Some(SalesOrderLineId::new("stable-line")),
                sales_order_revision_line_id: Some(SalesOrderRevisionLineId::new(source_line)),
                allocated_quantity: Some(quantity("2")),
            },
        )
        .unwrap()
    }

    fn facts(source_revision: &str, source_line: &str) -> ProcurementCoverageFacts {
        ProcurementCoverageFacts {
            revision: Some(SalesRevisionFact {
                base: FactIdentity { id: "sales-v2".into() },
                customer_snapshot: SalesCustomerSnapshotFact { customer_name: "客户".into() },
                contract_snapshot: None,
            }),
            revision_lines: vec![SalesRevisionLineFact {
                base: FactIdentity { id: "current-line".into() },
                sales_order_line_id: SalesOrderLineId::new("stable-line"),
                line_no: 1,
                line_type: SalesLineType::GoodsService,
                item_name_snapshot: "商品".into(),
                spec_snapshot: None,
                unit_snapshot: Some("件".into()),
            }],
            goods_lines: vec![SalesGoodsLineFact {
                assigned_delivery_allocations: Vec::new(),
                delivery_allocations: Vec::new(),

                revision_line_id: SalesOrderRevisionLineId::new("current-line"),
                sku_id: SkuId::new("sku"),
                sku_revision_id: SkuRevisionId::new("sku-version"),
                quantity: quantity("2"),
                base_unit_code: "件".into(),
                fulfillment_due_at: Instant::from_unix_secs(1_800_000_000),
            }],
            skus: HashMap::from([(
                "sku".into(),
                SkuFact { base: FactIdentity { id: "sku".into() }, product_id: ProductId::new("product") },
            )]),
            products: HashMap::from([(
                "product".into(),
                ProductFact {
                    base: FactIdentity { id: "product".into() },
                    stable: CurrentRevisionFact::default(),
                    product_kind: ProductKind::Physical,
                },
            )]),
            purchase_orders: vec![purchase(source_revision)],
            purchase_revision_lines: vec![purchase_line(source_line)],
            allocations: vec![
                PurchaseLineSalesAllocation::new(
                    PurchaseLineSalesAllocationId::new("allocation"),
                    PurchaseLineSalesAllocationData {
                        purchase_order_revision_line_id: PurchaseOrderRevisionLineId::new("purchase-line"),
                        sales_order_revision_line_id: SalesOrderRevisionLineId::new(source_line),
                        allocated_quantity: quantity("2"),
                        allocated_cost_gross: Amount::from_str("200").unwrap(),
                        allocated_cost_net: Amount::from_str("200").unwrap(),
                    },
                )
                .unwrap(),
            ],
            ..Default::default()
        }
    }

    fn sources() -> HistoricalSources {
        HistoricalSources {
            sales_versions: HashMap::from([
                (
                    "sales-v1".into(),
                    SalesVersionIdentity {
                        order_id: "sales".into(),
                        revision_no: 1,
                        party_id: "company".into(),
                        party_name: Some("我方公司".into()),
                    },
                ),
                (
                    "sales-v2".into(),
                    SalesVersionIdentity {
                        order_id: "sales".into(),
                        revision_no: 2,
                        party_id: "company".into(),
                        party_name: Some("我方公司".into()),
                    },
                ),
            ]),
            sales_lines: HashMap::from([(
                "original-line".into(),
                SalesLineIdentity {
                    revision_id: "sales-v1".into(),
                    stable_line_id: "stable-line".into(),
                    is_goods_service: true,
                },
            )]),
            purchase_versions: HashMap::from([(
                "purchase-version".into(),
                PurchaseVersionIdentity {
                    order_id: "purchase".into(),
                    subject: PaymentSubjectSnapshot::new(
                        "company".to_string().into(),
                        "我方公司".into(),
                        SalesOrderId::new("sales"),
                        SalesOrderRevisionId::new("sales-v1"),
                    )
                    .unwrap(),
                },
            )]),
        }
    }

    #[test]
    fn price_only_sales_revision_preserves_old_allocation_and_marks_purchase_change() {
        // 价格不参与采购数量事实：仅公共版本行身份更新，稳定行、SKU、数量均保持不变。
        let original = facts("sales-v1", "original-line");
        let error = build_procurement_coverage(original.clone()).unwrap_err();
        assert_eq!(error.to_string(), "采购正式分配未绑定销售当前版本行");
        let view = display_coverage(original.clone(), "sales", "sales-v2", &sources()).unwrap();
        assert_eq!(view.total_quantity, quantity("2"));
        assert_eq!(view.covered_quantity, quantity("0"));
        assert_eq!(view.remaining_quantity, quantity("2"));
        assert!(view.source_changed);
        assert_eq!(view.lines[0].covered_quantity, quantity("0"));
        assert_eq!(view.lines[0].remaining_quantity, quantity("2"));
        assert_eq!(view.outdated_purchase_count, 1);
        assert_eq!(view.outdated_purchase_quantity, quantity("2"));
        assert_eq!(
            view.source_change_blocker(),
            Some("销售正式版本已变化，请先完成相关采购正式变更，再继续供给分配")
        );
        assert_eq!(original.purchase_orders[0].sales_order_revision_id.as_ref(), "sales-v1");
        assert_eq!(
            original.purchase_revision_lines[0].sales_order_revision_line_id.as_ref().unwrap().as_ref(),
            "original-line"
        );
        assert_eq!(original.allocations[0].sales_order_revision_line_id.as_ref(), "original-line");
        assert!(build_procurement_coverage(original).is_err(), "命令严格校验不得被展示投影改变");
    }

    #[test]
    fn coverage_lines_distinguish_partial_and_uncovered_sales_items() {
        let mut input = facts("sales-v2", "current-line");
        input.goods_lines[0].quantity = quantity("3");
        let mut second_line = input.revision_lines[0].clone();
        second_line.base.id = "second-line".into();
        second_line.sales_order_line_id = SalesOrderLineId::new("second-stable-line");
        second_line.line_no = 2;
        second_line.item_name_snapshot = "另一商品".into();
        second_line.spec_snapshot = Some("盒装".into());
        let mut second_goods = input.goods_lines[0].clone();
        second_goods.revision_line_id = SalesOrderRevisionLineId::new("second-line");
        second_goods.quantity = quantity("4");
        input.revision_lines.push(second_line);
        input.goods_lines.push(second_goods);
        let view = display_coverage(input, "sales", "sales-v2", &HistoricalSources::default()).unwrap();
        assert_eq!(view.remaining_quantity, quantity("5"));
        assert_eq!(view.lines.len(), 2);
        let first = &view.lines[0];
        assert_eq!(first.sales_order_line_id, "stable-line");
        assert_eq!(first.total_quantity, quantity("3"));
        assert_eq!(first.covered_quantity, quantity("2"));
        assert_eq!(first.remaining_quantity, quantity("1"));
        let second = &view.lines[1];
        assert_eq!(second.sales_order_line_id, "second-stable-line");
        assert_eq!(second.line_no, 2);
        assert_eq!(second.item_name, "另一商品");
        assert_eq!(second.specification.as_deref(), Some("盒装"));
        assert_eq!(second.unit.as_deref(), Some("件"));
        assert_eq!(second.total_quantity, quantity("4"));
        assert_eq!(second.covered_quantity, quantity("0"));
        assert_eq!(second.remaining_quantity, quantity("4"));
    }

    #[test]
    fn missing_sales_revision_has_no_coverage_lines() {
        let view = empty_coverage().unwrap();
        assert!(view.lines.is_empty());
        assert_eq!(view.remaining_quantity, quantity("0"));
    }

    #[test]
    fn current_native_coverage_keeps_original_quantity_and_rejects_tampering() {
        let original = facts("sales-v2", "current-line");
        let native = build_procurement_coverage(original.clone()).unwrap();
        let view =
            display_coverage(original.clone(), "sales", "sales-v2", &HistoricalSources::default()).unwrap();
        assert_eq!(view.total_quantity, native.summary.total_quantity);
        assert_eq!(view.covered_quantity, native.summary.covered_quantity);
        assert_eq!(view.remaining_quantity, native.summary.remaining_quantity);
        assert_eq!(view.progress, native.summary.progress);
        assert_eq!(view.lines[0].remaining_quantity, quantity("0"));
        assert!(!view.source_changed);
        assert_eq!(view.outdated_purchase_quantity, quantity("0"));
        assert_eq!(view.outdated_purchase_count, 0);
        assert!(view.source_change_blocker().is_none());
        let mut wrong_line = original.clone();
        wrong_line.allocations[0].sales_order_revision_line_id =
            SalesOrderRevisionLineId::new("original-line");
        assert!(display_coverage(wrong_line, "sales", "sales-v2", &sources()).is_err());
        let mut over_covered = original;
        over_covered.goods_lines[0].quantity = quantity("1");
        assert!(display_coverage(over_covered, "sales", "sales-v2", &sources()).is_err());
    }

    #[test]
    fn mixed_current_and_historical_purchases_keep_distinct_coverage_quantities() {
        let mut input = facts("sales-v1", "original-line");
        let mut native = facts("sales-v2", "current-line");
        native.purchase_orders[0].base.id = "current-purchase".into();
        native.purchase_orders[0].stable.current_revision_id = Some("current-purchase-version".into());
        native.purchase_revision_lines[0].base.id = "current-purchase-line".into();
        native.purchase_revision_lines[0].purchase_order_revision_id =
            PurchaseOrderRevisionId::new("current-purchase-version");
        native.allocations[0].base.id = "current-allocation".into();
        native.allocations[0].purchase_order_revision_line_id =
            PurchaseOrderRevisionLineId::new("current-purchase-line");
        input.purchase_orders.extend(native.purchase_orders);
        input.purchase_revision_lines.extend(native.purchase_revision_lines);
        input.allocations.extend(native.allocations);
        let view = display_coverage(input, "sales", "sales-v2", &sources()).unwrap();
        assert_eq!(view.covered_quantity, quantity("2"));
        assert_eq!(view.remaining_quantity, quantity("0"));
        assert_eq!(view.outdated_purchase_quantity, quantity("2"));
        assert_eq!(view.outdated_purchase_count, 1);
        assert!(view.source_change_blocker().is_some());
    }

    #[test]
    fn task_sync_allows_formal_purchase_changes_one_by_one_without_rebinding_other_orders() {
        let mut first = facts("sales-v1", "original-line");
        let mut second = facts("sales-v1", "original-line");
        second.purchase_orders[0].base.id = "purchase-2".into();
        second.purchase_orders[0].stable.current_revision_id = Some("purchase-version-2".into());
        second.purchase_revision_lines[0].base.id = "purchase-line-2".into();
        second.purchase_revision_lines[0].purchase_order_revision_id =
            PurchaseOrderRevisionId::new("purchase-version-2");
        second.allocations[0].base.id = "allocation-2".into();
        second.allocations[0].purchase_order_revision_line_id =
            PurchaseOrderRevisionLineId::new("purchase-line-2");
        first.goods_lines[0].quantity = quantity("4");
        first.purchase_orders.extend(second.purchase_orders);
        first.purchase_revision_lines.extend(second.purchase_revision_lines);
        first.allocations.extend(second.allocations);
        let original = first.clone();
        let mut history = sources();
        let mut second_version = history.purchase_versions["purchase-version"].clone();
        second_version.order_id = "purchase-2".into();
        history.purchase_versions.insert("purchase-version-2".into(), second_version);

        let (before, outdated) = current_coverage(first.clone(), "sales", "sales-v2", &history).unwrap();
        assert_eq!(before.summary.covered_quantity, quantity("0"));
        assert_eq!(before.summary.remaining_quantity, quantity("4"));
        assert_eq!(outdated.count, 2);
        assert_eq!(outdated.quantity, quantity("4"));

        // 每个状态代表一张采购正式变更已保存的新当前版本，未变更的第二张保持原身份。
        for index in 0..2 {
            let updated_revision = format!("updated-purchase-version-{index}");
            first.purchase_orders[index].sales_order_revision_id = SalesOrderRevisionId::new("sales-v2");
            first.purchase_orders[index].stable.current_revision_id = Some(updated_revision.clone());
            first.purchase_revision_lines[index].purchase_order_revision_id =
                PurchaseOrderRevisionId::new(updated_revision);
            first.purchase_revision_lines[index].sales_order_revision_line_id =
                Some(SalesOrderRevisionLineId::new("current-line"));
            first.allocations[index].sales_order_revision_line_id =
                SalesOrderRevisionLineId::new("current-line");
            let (current, outdated) = current_coverage(first.clone(), "sales", "sales-v2", &history).unwrap();
            let expected_current = if index == 0 { quantity("2") } else { quantity("4") };
            let expected_remaining = if index == 0 { quantity("2") } else { quantity("0") };
            assert_eq!(current.summary.covered_quantity, expected_current);
            assert_eq!(current.summary.remaining_quantity, expected_remaining);
            assert_eq!(outdated.count, 1 - index);
            assert_eq!(outdated.quantity, expected_remaining);
            if index == 0 {
                assert_eq!(first.purchase_orders[1].sales_order_revision_id.as_ref(), "sales-v1");
                assert_eq!(first.allocations[1].sales_order_revision_line_id.as_ref(), "original-line");
                assert!(build_procurement_coverage(first.clone()).is_err());
            }
        }
        assert_eq!(original.purchase_orders[0].sales_order_revision_id.as_ref(), "sales-v1");
        assert_eq!(original.purchase_orders[1].sales_order_revision_id.as_ref(), "sales-v1");
        assert!(
            original
                .allocations
                .iter()
                .all(|allocation| allocation.sales_order_revision_line_id.as_ref() == "original-line")
        );
    }

    #[test]
    fn historical_display_rejects_unknown_formal_versions_and_cross_bound_identities() {
        for case in 0..14 {
            let mut input = facts("sales-v1", "original-line");
            let mut history = sources();
            match case {
                0 => {
                    history.sales_versions.remove("sales-v1");
                },
                1 => {
                    history.sales_versions.remove("sales-v2");
                },
                2 => {
                    history.purchase_versions.clear();
                },
                3 => {
                    history.sales_lines.clear();
                },
                4 => {
                    history.sales_versions.get_mut("sales-v1").unwrap().order_id = "other-sales".into();
                },
                5 => {
                    history.sales_versions.get_mut("sales-v1").unwrap().revision_no = 2;
                },
                6 => {
                    history
                        .purchase_versions
                        .get_mut("purchase-version")
                        .unwrap()
                        .subject
                        .sales_order_revision_id = SalesOrderRevisionId::new("sales-v2");
                },
                7 => {
                    history.sales_lines.get_mut("original-line").unwrap().revision_id = "sales-v2".into();
                },
                8 => {
                    history.sales_lines.get_mut("original-line").unwrap().stable_line_id =
                        "other-stable-line".into();
                },
                9 => {
                    input.allocations[0].sales_order_revision_line_id =
                        SalesOrderRevisionLineId::new("current-line");
                },
                10 => {
                    input.allocations.push(input.allocations[0].clone());
                },
                11 => {
                    input.purchase_revision_lines.clear();
                },
                12 => {
                    input.purchase_orders[0].stable.current_revision_id = None;
                },
                13 => {
                    history.sales_versions.get_mut("sales-v1").unwrap().party_name = None;
                },
                _ => unreachable!(),
            }
            assert!(display_coverage(input, "sales", "sales-v2", &history).is_err(), "case {case}");
        }
    }
}
