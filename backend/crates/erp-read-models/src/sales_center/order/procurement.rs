//! 销售详情展示当前供给覆盖与受销售变更影响的采购。
//!
//! 引用历史销售版本的采购按稳定销售行计入覆盖；只有第一期 §6.3 判定受影响的采购
//! 才标记“采购依据待更新”，已修改待重新提交的草稿单独计数。采购明细只对具备采购
//! 列表权限的账号返回。

use std::collections::HashMap;

use application_core::AuditActor;
use erp_core::ids::SupplierAccountId;
use erp_core::money::{Quantity, Rate};
use erp_identity::AccessControlExt;
use erp_identity::repository::prelude::*;
use erp_procurement::entity::purchase_order::{
    PurchaseOrder, PurchaseSourceUpdate, PurchaseSourceUpdateStatus, SalesProcurementCoverage,
    SourceImpactReason,
};
use erp_procurement::repository::purchase_source_update::list_by_sales_order;
use erp_sales::entity::sales_order::SalesOrder;
use persistence_core::NoTransaction;
use rust_decimal::Decimal;

use super::SalesOrderReadService;
use super::dto::{
    SalesProcurementCoverageLineView, SalesProcurementCoverageView, SalesProcurementSourceUpdateView,
};
use crate::Result;
use crate::purchase_center::repository::supplier_names::current_legal_names_by_account_ids;
use crate::purchase_center::source_impact::{PurchaseSourceState, load_purchase_source_state};
use crate::sales_center::purchase_visibility::can_list_purchases;

/// 销售详情须逐张展示的采购：仍受影响，或草稿已修改待重新提交。
struct PendingPurchase<'a> {
    /// 采购单。
    order: &'a PurchaseOrder,
    /// 当前或最近一次影响原因。
    reasons: &'a [SourceImpactReason],
    /// 是否暂停付款。
    blocks_payment: bool,
    /// 是否等待重新提交审批。
    awaiting_submission: bool,
}

impl SalesOrderReadService {
    /// 展示当前供给覆盖与受影响采购。
    /// # 参数
    /// `order` 是已授权销售单；`actor` 为当前账号，内部调用时为空。
    /// # 返回
    /// 返回逐行覆盖、受影响与待提交张数；具备采购列表权限时附带采购明细。
    /// # 错误
    /// 来源事实不完整、账号或权限查询失败时拒绝。
    pub(super) async fn sales_procurement_coverage(
        &self,
        order: &SalesOrder,
        actor: Option<&AuditActor>,
    ) -> Result<SalesProcurementCoverageView> {
        if order.current_revision_id().is_none() {
            return empty_coverage();
        }
        let state = load_purchase_source_state(&self.db, order, &mut NoTransaction).await?;
        let records = list_by_sales_order(&self.db, &order.base.id, &mut NoTransaction).await?;
        let pending = pending_purchases(&state, &records);
        let awaiting = pending.iter().filter(|purchase| purchase.awaiting_submission).count();
        let updates =
            if !pending.is_empty() && can_list_purchases(&self.db, self.require_rbac()?, actor).await? {
                self.source_update_views(&pending).await?
            } else {
                Vec::new()
            };
        Ok(coverage_view(&state.coverage, (state.impacts.len(), awaiting), updates))
    }

    /// 批量解析待处理采购的供应商与负责人名称。
    async fn source_update_views(
        &self,
        pending: &[PendingPurchase<'_>],
    ) -> Result<Vec<SalesProcurementSourceUpdateView>> {
        let supplier_ids = pending
            .iter()
            .map(|purchase| purchase.order.supplier_id.clone())
            .collect::<Vec<SupplierAccountId>>();
        let suppliers =
            current_legal_names_by_account_ids(&self.db, &supplier_ids, &mut NoTransaction).await?;
        let owner_ids =
            pending.iter().filter_map(|purchase| purchase.order.owner_user_id.clone()).collect::<Vec<_>>();
        let owners = self.db.accounts().names_by_ids(&owner_ids, &mut NoTransaction).await?;
        Ok(pending.iter().map(|purchase| source_update_view(purchase, &suppliers, &owners)).collect())
    }
}

/// 按事实顺序列出受影响采购与等待重新提交的草稿。
///
/// # 参数
/// * `state` - 同一批事实的覆盖与判定
/// * `records` - 本销售单的依据更新记录
///
/// # 返回
/// 返回仍受影响的采购，以及开放记录处于等待提交的采购。
///
/// # 错误
/// 无。
fn pending_purchases<'a>(
    state: &'a PurchaseSourceState,
    records: &'a [PurchaseSourceUpdate],
) -> Vec<PendingPurchase<'a>> {
    state
        .facts
        .purchase_orders
        .iter()
        .filter_map(|order| {
            if let Some(impact) = state.impact_of(&order.base.id) {
                return Some(PendingPurchase {
                    order,
                    reasons: &impact.reasons,
                    blocks_payment: impact.blocks_payment(),
                    awaiting_submission: false,
                });
            }
            records
                .iter()
                .find(|record| {
                    record.purchase_order_id == order.base.id
                        && record.status == PurchaseSourceUpdateStatus::Open
                        && record.awaiting_submission
                })
                .map(|record| PendingPurchase {
                    order,
                    reasons: &record.reasons,
                    blocks_payment: false,
                    awaiting_submission: true,
                })
        })
        .collect()
}

/// 由待处理采购构造明细视图。
fn source_update_view(
    purchase: &PendingPurchase<'_>,
    suppliers: &HashMap<String, String>,
    owners: &HashMap<String, String>,
) -> SalesProcurementSourceUpdateView {
    let order = purchase.order;
    SalesProcurementSourceUpdateView {
        purchase_order_id: order.base.id.clone(),
        purchase_no: order.purchase_no.clone(),
        status: order.stable.status.as_str().to_string(),
        status_label: order.stable.status.label().to_string(),
        supplier_name: suppliers.get(order.supplier_id.as_ref()).cloned(),
        owner_name: order.owner_user_id.as_ref().and_then(|id| owners.get(id)).cloned(),
        blocks_payment: purchase.blocks_payment,
        awaiting_submission: purchase.awaiting_submission,
        reasons: purchase.reasons.to_vec(),
    }
}

/// 由覆盖与受影响采购构造销售详情视图。
///
/// # 参数
/// * `coverage` - 当前覆盖
/// * `counts` - 受影响张数与等待重新提交张数
/// * `source_updates` - 可见的采购明细
///
/// # 返回
/// 返回销售详情覆盖视图。
///
/// # 错误
/// 无。
fn coverage_view(
    coverage: &SalesProcurementCoverage,
    (affected_count, awaiting_count): (usize, usize),
    source_updates: Vec<SalesProcurementSourceUpdateView>,
) -> SalesProcurementCoverageView {
    SalesProcurementCoverageView {
        lines: coverage
            .lines
            .iter()
            .map(|line| SalesProcurementCoverageLineView {
                sales_order_line_id: line.revision_line.sales_order_line_id.to_string(),
                line_no: line.revision_line.line_no,
                item_name: line.revision_line.item_name_snapshot.clone(),
                specification: line.revision_line.spec_snapshot.clone(),
                unit: line.revision_line.unit_snapshot.clone(),
                total_quantity: line.summary.total_quantity,
                covered_quantity: line.summary.covered_quantity,
                remaining_quantity: line.summary.remaining_quantity,
                excess_quantity: line.summary.excess_quantity,
            })
            .collect(),
        total_quantity: coverage.summary.total_quantity,
        covered_quantity: coverage.summary.covered_quantity,
        remaining_quantity: coverage.summary.remaining_quantity,
        excess_quantity: coverage.summary.excess_quantity,
        progress: coverage.summary.progress,
        source_changed: affected_count != 0,
        affected_purchase_count: affected_count,
        awaiting_submission_count: awaiting_count,
        source_updates,
    }
}

/// 销售单尚无当前版本时的空覆盖。
fn empty_coverage() -> Result<SalesProcurementCoverageView> {
    let zero = Quantity::try_from(Decimal::ZERO)?;
    Ok(SalesProcurementCoverageView {
        lines: Vec::new(),
        total_quantity: zero,
        covered_quantity: zero,
        remaining_quantity: zero,
        excess_quantity: zero,
        progress: Rate::try_from(Decimal::ZERO)?,
        source_changed: false,
        affected_purchase_count: 0,
        awaiting_submission_count: 0,
        source_updates: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::str::FromStr;

    use erp_core::common::time::Instant;
    use erp_core::ids::{
        ProductId, PurchaseOrderId, SalesOrderId, SalesOrderLineId, SalesOrderRevisionId,
        SalesOrderRevisionLineId, SkuId, SkuRevisionId, SupplierAccountId, WarehouseId,
    };
    use erp_core::money::Quantity;
    use erp_procurement::entity::facts::{
        CurrentRevisionFact, ExistingStockReservationFact, FactIdentity, PaymentTermFact, ProductFact,
        ProductKind, SalesCustomerSnapshotFact, SalesGoodsLineFact, SalesLineType, SalesRevisionFact,
        SalesRevisionLineFact, SkuFact,
    };
    use erp_procurement::entity::purchase_order::{
        FulfillmentResponsibility, ProcurementCoverageFacts, PurchaseOrder, PurchaseOrderData,
        PurchaseOrderStatus, PurchaseSourceImpact, PurchaseSourceResolution, PurchaseSourceUpdate,
        PurchaseType, SalesProcurementCoverage, SourceImpactKind, SourceImpactReason, SourceUpdateOpening,
        build_procurement_coverage,
    };

    use super::{PendingPurchase, coverage_view, pending_purchases, source_update_view};
    use crate::purchase_center::source_impact::PurchaseSourceState;

    fn qty(value: &str) -> Quantity {
        Quantity::from_str(value).unwrap()
    }

    /// 当前版本 8 件、草稿采购覆盖 10 件的覆盖结果。
    fn coverage_with_excess() -> SalesProcurementCoverage {
        let facts = ProcurementCoverageFacts {
            revision: Some(SalesRevisionFact {
                base: FactIdentity { id: "rev-cur".into() },
                customer_snapshot: SalesCustomerSnapshotFact { customer_name: "客户".into() },
                contract_snapshot: None,
            }),
            revision_lines: vec![SalesRevisionLineFact {
                base: FactIdentity { id: "cur-1".into() },
                sales_order_line_id: SalesOrderLineId::new("sol-1"),
                line_no: 2,
                line_type: SalesLineType::GoodsService,
                item_name_snapshot: "礼盒".into(),
                spec_snapshot: Some("大号".into()),
                unit_snapshot: Some("件".into()),
            }],
            goods_lines: vec![SalesGoodsLineFact {
                revision_line_id: SalesOrderRevisionLineId::new("cur-1"),
                sku_id: SkuId::new("sku-1"),
                sku_revision_id: SkuRevisionId::new("skur-1"),
                quantity: qty("8"),
                base_unit_code: "件".into(),
                fulfillment_due_at: Instant::from_unix_secs(1_800_000_000),
                service_region: None,
                delivery_allocations: Vec::new(),
                assigned_delivery_allocations: Vec::new(),
            }],
            skus: HashMap::from([(
                "sku-1".to_string(),
                SkuFact { base: FactIdentity { id: "sku-1".into() }, product_id: ProductId::new("p-1") },
            )]),
            products: HashMap::from([(
                "p-1".to_string(),
                ProductFact {
                    base: FactIdentity { id: "p-1".into() },
                    stable: CurrentRevisionFact::default(),
                    product_kind: ProductKind::Physical,
                },
            )]),
            reservations: vec![ExistingStockReservationFact {
                sales_order_line_id: SalesOrderLineId::new("sol-1"),
                reserved_quantity: qty("10"),
                consumed_quantity: qty("0"),
            }],
            ..ProcurementCoverageFacts::default()
        };
        build_procurement_coverage(facts).unwrap()
    }

    fn purchase(owner: Option<&str>) -> PurchaseOrder {
        let mut order = PurchaseOrder::new(
            PurchaseOrderId::new("po-1"),
            PurchaseOrderData {
                business_org_unit_id: "org".into(),
                purchase_no: "PO-0001".into(),
                sales_order_id: SalesOrderId::new("so-1"),
                sales_order_revision_id: SalesOrderRevisionId::new("rev-old"),
                creation_basis_id: "basis".into(),
                supplier_id: SupplierAccountId::new("sup-1"),
                purchase_type: PurchaseType::Physical,
                payment_term_code: "POSTPAY_NET30".into(),
                fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
                owner_user_id: "buyer".into(),
                target_warehouse_id: Some(WarehouseId::new("wh-1")),
            },
            "buyer",
            |_| Ok(PaymentTermFact::new("POSTPAY_NET30").with_days_after_delivery(30)),
        )
        .unwrap();
        order.stable.status = PurchaseOrderStatus::InApproval;
        order.owner_user_id = owner.map(str::to_string);
        order
    }

    fn reduced_impact() -> PurchaseSourceImpact {
        PurchaseSourceImpact {
            purchase_order_id: "po-1".into(),
            reasons: vec![SourceImpactReason {
                sales_order_line_id: "sol-1".into(),
                line_no: 2,
                item_name: "礼盒".into(),
                kind: SourceImpactKind::QuantityReduced {
                    baseline_quantity: qty("10"),
                    current_quantity: qty("8"),
                    excess_quantity: qty("2"),
                },
            }],
        }
    }

    /// 覆盖超出逐行与整单展示，受影响张数决定依据待更新标记。
    #[test]
    fn coverage_view_reports_excess_and_affected_count() {
        let view = coverage_view(&coverage_with_excess(), (1, 2), Vec::new());

        assert!(view.source_changed);
        assert_eq!(view.affected_purchase_count, 1);
        assert_eq!(view.awaiting_submission_count, 2);
        assert_eq!(view.lines[0].line_no, 2);
        assert_eq!(view.lines[0].excess_quantity, qty("2"));
        assert_eq!(view.lines[0].remaining_quantity, qty("0"));
        assert_eq!(view.excess_quantity, qty("2"));
        assert!(view.source_updates.is_empty());
    }

    /// 没有受影响采购时不标记依据待更新。
    #[test]
    fn coverage_view_without_impacts_is_not_marked() {
        let view = coverage_view(&coverage_with_excess(), (0, 0), Vec::new());

        assert!(!view.source_changed);
        assert_eq!(view.affected_purchase_count, 0);
    }

    /// 受影响采购视图使用中文状态、供应商与负责人名称，不暴露账号 ID。
    #[test]
    fn source_update_view_resolves_names_and_payment_block() {
        let suppliers = HashMap::from([("sup-1".to_string(), "甲供应商".to_string())]);
        let owners = HashMap::from([("buyer".to_string(), "陈国平".to_string())]);

        let order = purchase(Some("buyer"));
        let impact = reduced_impact();
        let pending = PendingPurchase {
            order: &order,
            reasons: &impact.reasons,
            blocks_payment: impact.blocks_payment(),
            awaiting_submission: false,
        };

        let view = source_update_view(&pending, &suppliers, &owners);

        assert_eq!(view.purchase_no, "PO-0001");
        assert_eq!(view.status, "IN_APPROVAL");
        assert_eq!(view.status_label, "审批中");
        assert_eq!(view.supplier_name.as_deref(), Some("甲供应商"));
        assert_eq!(view.owner_name.as_deref(), Some("陈国平"));
        assert!(view.blocks_payment);
        assert_eq!(view.reasons.len(), 1);
    }

    /// 受影响采购取当前判定；不再受影响的草稿只在开放记录等待提交时列出。
    #[test]
    fn pending_purchases_include_affected_and_awaiting_drafts() {
        let affected = purchase(Some("buyer"));
        let mut awaiting = purchase(Some("buyer"));
        awaiting.base.id = "po-2".into();
        let mut settled = purchase(Some("buyer"));
        settled.base.id = "po-3".into();
        let impact = reduced_impact();
        let opening = |order| SourceUpdateOpening {
            order,
            impact: &impact,
            sales_order_revision_id: "rev-cur",
            trigger_sales_change_order_id: Some("sco-1"),
        };
        let mut awaiting_record = PurchaseSourceUpdate::open("psu-2".into(), opening(&awaiting));
        awaiting_record.await_submission("rev-cur").unwrap();
        let mut resolved = PurchaseSourceUpdate::open("psu-3".into(), opening(&settled));
        resolved.resolve(PurchaseSourceResolution::Voided, None, Instant::from_unix_secs(1)).unwrap();
        let state = PurchaseSourceState {
            facts: ProcurementCoverageFacts {
                purchase_orders: vec![affected, awaiting, settled],
                ..ProcurementCoverageFacts::default()
            },
            coverage: coverage_with_excess(),
            impacts: vec![reduced_impact()],
        };
        let records = [awaiting_record, resolved];

        let pending = pending_purchases(&state, &records);

        assert_eq!(pending.len(), 2);
        assert_eq!((pending[0].order.base.id.as_str(), pending[0].awaiting_submission), ("po-1", false));
        assert!(pending[0].blocks_payment);
        assert_eq!((pending[1].order.base.id.as_str(), pending[1].awaiting_submission), ("po-2", true));
        assert!(!pending[1].blocks_payment);
        assert_eq!(pending[1].reasons.len(), 1);
    }

    /// 负责人缺失或名称不可解析时留空。
    #[test]
    fn source_update_view_leaves_missing_names_empty() {
        let order = purchase(None);
        let pending =
            PendingPurchase { order: &order, reasons: &[], blocks_payment: false, awaiting_submission: true };

        let view = source_update_view(&pending, &HashMap::new(), &HashMap::new());

        assert_eq!(view.supplier_name, None);
        assert_eq!(view.owner_name, None);
        assert!(view.awaiting_submission);
    }
}
