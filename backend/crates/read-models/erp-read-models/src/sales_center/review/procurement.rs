//! 销售变更详情的采购处理进度（第一期 §6.3“销售变更对采购的影响”第 9–11 条）。

use std::collections::HashMap;

use application_core::AuditActor;
use erp_identity::AccessControlExt;
use erp_identity::repository::prelude::*;
use erp_procurement::entity::purchase_order::{
    PurchaseOrder, PurchaseSourceImpact, PurchaseSourceUpdate, PurchaseSourceUpdateStatus,
};
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::repository::purchase_source_update::list_by_trigger_change;
use erp_sales::entity::sales_review::SalesChangeOrderStatus;
use erp_sales::repository::SalesOrderExt;
use persistence_core::NoTransaction;

use super::{
    SalesChangeOrderDetailView, SalesChangeProcurementItemView, SalesChangeProcurementView,
    SalesChangeReadService,
};
use crate::Result;
use crate::purchase_center::repository::supplier_names::current_legal_names_by_account_ids;
use crate::purchase_center::source_impact::predict_change_impacts;
use crate::sales_center::purchase_visibility::can_list_purchases;

/// 采购明细展示所需的单号、供应商与负责人。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct PurchaseLabel {
    /// 采购单号。
    purchase_no: String,
    /// 供应商当前法定名称。
    supplier_name: Option<String>,
    /// 采购负责人姓名。
    owner_name: Option<String>,
}

impl SalesChangeReadService {
    /// 读取本变更的采购处理进度；审批中时按提交内容预判。
    ///
    /// # 参数
    /// * `view` - 已授权的销售变更单详情
    /// * `actor` - 当前账号
    ///
    /// # 返回
    /// 返回张数与完成标记；具备采购列表权限时附带逐张处理情况。
    ///
    /// # 错误
    /// 仓储、预判或授权查询失败时返回错误。
    pub(super) async fn change_procurement(
        &self,
        view: &SalesChangeOrderDetailView,
        actor: &AuditActor,
    ) -> Result<SalesChangeProcurementView> {
        let updates = list_by_trigger_change(&self.db, &view.id, &mut NoTransaction).await?;
        let visible = can_list_purchases(&self.db, self.require_rbac()?, Some(actor)).await?;
        if updates.is_empty() {
            return match (&view.status, &view.current_submission_id) {
                (SalesChangeOrderStatus::InApproval, Some(submission_id)) => {
                    self.predicted_procurement(&view.sales_order_id, submission_id, visible).await
                },
                _ => Ok(SalesChangeProcurementView::default()),
            };
        }
        if !visible {
            return Ok(procurement_view(&updates, None));
        }
        let ids = updates.iter().map(|update| update.purchase_order_id.clone()).collect::<Vec<_>>();
        let purchases = self.db.purchase_orders().list_active_by_ids(&ids, &mut NoTransaction).await?;
        let labels = self.purchase_labels(&purchases).await?;
        Ok(procurement_view(&updates, Some(&labels)))
    }

    /// 按待审批变更的提交内容预判受影响采购。
    async fn predicted_procurement(
        &self,
        sales_order_id: &str,
        submission_id: &str,
        visible: bool,
    ) -> Result<SalesChangeProcurementView> {
        let Some(order) = self.db.sales_orders().find_by_id(sales_order_id, &mut NoTransaction).await? else {
            return Ok(SalesChangeProcurementView::default());
        };
        let (purchases, impacts) =
            predict_change_impacts(&self.db, &order, submission_id, &mut NoTransaction).await?;
        if !visible {
            return Ok(predicted_view(&impacts, None));
        }
        let affected = purchases
            .into_iter()
            .filter(|purchase| impacts.iter().any(|impact| impact.purchase_order_id == purchase.base.id))
            .collect::<Vec<_>>();
        let labels = self.purchase_labels(&affected).await?;
        Ok(predicted_view(&impacts, Some(&labels)))
    }

    /// 批量解析采购单号、供应商与负责人名称。
    ///
    /// # 参数
    /// * `purchases` - 需展示的采购单
    ///
    /// # 返回
    /// 返回采购单 ID 到展示标签的映射。
    ///
    /// # 错误
    /// 供应商或账号名称读取失败时返回错误。
    async fn purchase_labels(&self, purchases: &[PurchaseOrder]) -> Result<HashMap<String, PurchaseLabel>> {
        let supplier_ids = purchases.iter().map(|order| order.supplier_id.clone()).collect::<Vec<_>>();
        let suppliers =
            current_legal_names_by_account_ids(&self.db, &supplier_ids, &mut NoTransaction).await?;
        let owner_ids = purchases.iter().filter_map(|order| order.owner_user_id.clone()).collect::<Vec<_>>();
        let owners = self.db.accounts().names_by_ids(&owner_ids, &mut NoTransaction).await?;
        Ok(purchases
            .iter()
            .map(|order| {
                let label = PurchaseLabel {
                    purchase_no: order.purchase_no.clone(),
                    supplier_name: suppliers.get(order.supplier_id.as_ref()).cloned(),
                    owner_name: order.owner_user_id.as_ref().and_then(|id| owners.get(id)).cloned(),
                };
                (order.base.id.clone(), label)
            })
            .collect())
    }
}

/// 由预判结果构造进度视图，全部视为待处理。
///
/// # 参数
/// * `impacts` - 预判受影响的采购
/// * `labels` - 可见时的采购展示标签；不可见时为空
///
/// # 返回
/// 返回预判张数与可选明细。
///
/// # 错误
/// 无。
fn predicted_view(
    impacts: &[PurchaseSourceImpact],
    labels: Option<&HashMap<String, PurchaseLabel>>,
) -> SalesChangeProcurementView {
    let items = labels
        .map(|labels| {
            impacts
                .iter()
                .map(|impact| {
                    let label = labels.get(&impact.purchase_order_id).cloned().unwrap_or_default();
                    SalesChangeProcurementItemView {
                        purchase_order_id: impact.purchase_order_id.clone(),
                        purchase_no: Some(label.purchase_no).filter(|no| !no.is_empty()),
                        supplier_name: label.supplier_name,
                        owner_name: label.owner_name,
                        open: true,
                        awaiting_submission: false,
                        resolution: None,
                        resolution_label: None,
                        maintain_note: None,
                        reasons: impact.reasons.clone(),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    SalesChangeProcurementView {
        total: impacts.len(),
        open: impacts.len(),
        completed: false,
        predicted: true,
        items,
    }
}

/// 由记录构造处理进度视图。
///
/// # 参数
/// * `updates` - 本变更触发的记录
/// * `labels` - 可见时的采购展示标签；不可见时为空
///
/// # 返回
/// 返回张数、未处理数、完成标记与可选明细。
///
/// # 错误
/// 无。
fn procurement_view(
    updates: &[PurchaseSourceUpdate],
    labels: Option<&HashMap<String, PurchaseLabel>>,
) -> SalesChangeProcurementView {
    let open = updates.iter().filter(|update| update.status == PurchaseSourceUpdateStatus::Open).count();
    let items = labels
        .map(|labels| {
            updates.iter().map(|update| record_item(update, labels.get(&update.purchase_order_id))).collect()
        })
        .unwrap_or_default();
    SalesChangeProcurementView {
        total: updates.len(),
        open,
        completed: !updates.is_empty() && open == 0,
        predicted: false,
        items,
    }
}

/// 由一条记录构造逐张处理情况。
fn record_item(
    update: &PurchaseSourceUpdate,
    label: Option<&PurchaseLabel>,
) -> SalesChangeProcurementItemView {
    let open = update.status == PurchaseSourceUpdateStatus::Open;
    SalesChangeProcurementItemView {
        purchase_order_id: update.purchase_order_id.clone(),
        purchase_no: label.map(|label| label.purchase_no.clone()),
        supplier_name: label.and_then(|label| label.supplier_name.clone()),
        owner_name: label.and_then(|label| label.owner_name.clone()),
        open,
        awaiting_submission: open && update.awaiting_submission,
        resolution: update.resolution,
        resolution_label: update.resolution.map(|resolution| resolution.label().to_string()),
        maintain_note: update.maintain_note.clone(),
        reasons: update.reasons.clone(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::str::FromStr;

    use erp_core::common::time::Instant;
    use erp_core::ids::{
        PurchaseOrderId, SalesOrderId, SalesOrderRevisionId, SupplierAccountId, WarehouseId,
    };
    use erp_core::money::Quantity;
    use erp_procurement::entity::facts::PaymentTermFact;
    use erp_procurement::entity::purchase_order::{
        FulfillmentResponsibility, PurchaseOrder, PurchaseOrderData, PurchaseSourceImpact,
        PurchaseSourceResolution, PurchaseSourceUpdate, PurchaseType, SourceImpactKind, SourceImpactReason,
        SourceUpdateOpening,
    };

    use super::{PurchaseLabel, procurement_view};

    fn purchase(id: &str) -> PurchaseOrder {
        PurchaseOrder::new(
            PurchaseOrderId::new(id),
            PurchaseOrderData {
                business_org_unit_id: "org".into(),
                purchase_no: format!("PO-{id}"),
                sales_order_id: SalesOrderId::new("so-1"),
                sales_order_revision_id: SalesOrderRevisionId::new("rev-1"),
                creation_basis_id: format!("basis-{id}"),
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
        .unwrap()
    }

    fn record(id: &str) -> PurchaseSourceUpdate {
        let order = purchase(id);
        let impact = PurchaseSourceImpact {
            purchase_order_id: id.into(),
            reasons: vec![SourceImpactReason {
                sales_order_line_id: "sol-1".into(),
                line_no: 1,
                item_name: "礼盒".into(),
                kind: SourceImpactKind::Zeroed { baseline_quantity: Quantity::from_str("5").unwrap() },
            }],
        };
        PurchaseSourceUpdate::open(
            format!("psu-{id}"),
            SourceUpdateOpening {
                order: &order,
                impact: &impact,
                sales_order_revision_id: "rev-2",
                trigger_sales_change_order_id: Some("sco-1"),
            },
        )
    }

    /// 全部处理完成才标记完成；无权限时只给张数。
    #[test]
    fn completion_counts_open_records_and_hides_items_without_permission() {
        let mut done = record("a");
        done.resolve(PurchaseSourceResolution::Voided, None, Instant::from_unix_secs(1)).unwrap();
        let pending = record("b");

        let labels = HashMap::from([(
            "a".to_string(),
            PurchaseLabel {
                purchase_no: "PO-a".into(),
                supplier_name: Some("甲供应商".into()),
                owner_name: Some("陈国平".into()),
            },
        )]);

        let partial = procurement_view(&[done.clone(), pending], None);
        let finished = procurement_view(std::slice::from_ref(&done), Some(&labels));

        assert_eq!((partial.total, partial.open, partial.completed), (2, 1, false));
        assert!(partial.items.is_empty());
        assert!(finished.completed);
        assert_eq!(finished.items[0].purchase_no.as_deref(), Some("PO-a"));
        assert_eq!(finished.items[0].supplier_name.as_deref(), Some("甲供应商"));
        assert_eq!(finished.items[0].owner_name.as_deref(), Some("陈国平"));
        assert_eq!(finished.items[0].resolution_label.as_deref(), Some("已作废"));
    }

    /// 草稿已修改待提交时仍计为未处理，并标记等待提交。
    #[test]
    fn awaiting_submission_is_still_open() {
        let mut awaiting = record("a");
        awaiting.await_submission("rev-2").unwrap();

        let view = procurement_view(std::slice::from_ref(&awaiting), Some(&HashMap::new()));

        assert_eq!((view.total, view.open, view.completed), (1, 1, false));
        assert!(view.items[0].awaiting_submission);
        assert_eq!(view.items[0].purchase_no, None);
    }

    /// 没有受影响采购时不标记完成。
    #[test]
    fn no_records_is_not_completed() {
        let view = procurement_view(&[], Some(&HashMap::new()));

        assert!(!view.completed);
        assert_eq!(view.total, 0);
    }

    /// 预判结果全部待处理并标记为预判；无权限时只给张数。
    #[test]
    fn predicted_view_marks_all_open() {
        let order = record("a");
        let impact = PurchaseSourceImpact { purchase_order_id: "a".into(), reasons: order.reasons.clone() };
        let labels = HashMap::from([(
            "a".to_string(),
            PurchaseLabel {
                purchase_no: "PO-a".into(),
                supplier_name: Some("甲供应商".into()),
                owner_name: None,
            },
        )]);

        let visible = super::predicted_view(std::slice::from_ref(&impact), Some(&labels));
        let hidden = super::predicted_view(&[impact], None);

        assert!(visible.predicted);
        assert_eq!((visible.total, visible.open, visible.completed), (1, 1, false));
        assert_eq!(visible.items[0].purchase_no.as_deref(), Some("PO-a"));
        assert_eq!(visible.items[0].supplier_name.as_deref(), Some("甲供应商"));
        assert!(hidden.items.is_empty());
        assert_eq!(hidden.total, 1);
    }
}
