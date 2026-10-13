//! 销售变更生效时撤回受影响的审批中采购（第一期 §6.3“销售变更对采购的影响”第 4 条，
//! 审批授权与执行合同 §3 第 10 条）。
//!
//! 撤回与销售变更生效处于同一事务；撤回人记为触发生效的审批人，原因说明来源销售
//! 变更。该撤回不属于管理代办，原提交人随后按“修改原单”刷新依据并重新提交。

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_core::common::time::Instant;
use erp_procurement::entity::purchase_order::{PurchaseOrder, PurchaseOrderStatus};
use erp_read_models::purchase_center::source_impact::PurchaseSourceState;
use erp_workflow::entity::work_item::WorkItem;
use erp_workflow::service::approval::execution::{
    DocumentCancelCommand, PlannedWrites, PreparedExecution, prepare_document_cancel,
};
use erp_workflow::service::document_registry::find_approval_binding;
use mongodb::Database;
use persistence_core::Executor;

use super::adapter::{
    execute_purchase_order_domain_action, purchase_order_adapter, purchase_order_subject_ref,
    require_frozen_binding,
};
use super::cancel_approval::{
    CancelPosting, build_purchase_order_cancel_input, load_cancel_runtime, post_cancel,
};
use crate::Result;

/// 一次销售变更生效的撤回上下文。
pub struct SalesChangeWithdrawal<'a> {
    /// 生效的销售变更单。
    pub sales_change_order_id: &'a str,
    /// 来源销售单号，用于撤回原因。
    pub sales_order_no: &'a str,
    /// 触发生效的审批人。
    pub actor: &'a AuditActor,
}

impl SalesChangeWithdrawal<'_> {
    /// 撤回全部受影响的审批中采购。
    ///
    /// # 参数
    /// * `db` - 业务数据库
    /// * `state` - 生效写入后按新销售版本计算的覆盖与受影响采购
    /// * `executor` - 销售变更生效事务执行器
    ///
    /// # 返回
    /// 全部撤回写入成功时返回 `Ok(())`。
    ///
    /// # 错误
    /// 审批绑定、运行时、版本 CAS 或写入失败时返回错误，调用方整体回滚。
    pub async fn withdraw_affected(
        &self,
        db: &Database,
        state: &PurchaseSourceState,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let reason = format!("销售单 {} 的变更已生效，采购依据待更新", self.sales_order_no);
        for purchase in in_approval_affected(state) {
            self.withdraw(db, purchase, &reason, executor).await?;
        }
        Ok(())
    }

    /// 撤回一张审批中采购并回到原草稿。
    async fn withdraw(
        &self,
        db: &Database,
        purchase: &PurchaseOrder,
        reason: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let command = DocumentCancelCommand::new(
            purchase_order_subject_ref(&purchase.base.id)?,
            purchase.approval_subject_version,
            purchase.base.version,
            reason,
            self.actor.id(),
            &format!("sales-change:{}:{}", self.sales_change_order_id, purchase.base.id),
        )?;
        let now = Instant::now();
        let Some((writes, open_tasks)) = self.prepare(db, purchase, &command, now, executor).await? else {
            return Ok(());
        };
        let mut order = purchase.clone();
        let submission_id = order.current_submission_id.clone().unwrap_or_default();
        let adapter = purchase_order_adapter()?;
        execute_purchase_order_domain_action(
            &mut order,
            adapter.cancel_action,
            &submission_id,
            self.actor.id(),
        )?;
        let closed_tasks = WorkItem::close_all_for_approval_cancellation(
            open_tasks,
            self.actor.id(),
            command.reason(),
            now,
        )?;
        let audit = self.actor.clone().resource_log(
            "purchase_order.cancel_approval",
            "purchase_order",
            purchase.base.id.clone(),
        )?;
        let mut posting = CancelPosting {
            db,
            writes: &writes,
            closed_tasks: &closed_tasks,
            order: &mut order,
            audit: &audit,
        };
        post_cancel(&mut posting, executor).await
    }

    /// 读取审批运行时并形成撤回计划；无需写入时返回 `None`。
    async fn prepare(
        &self,
        db: &Database,
        purchase: &PurchaseOrder,
        command: &DocumentCancelCommand,
        now: Instant,
        executor: &mut dyn Executor,
    ) -> Result<Option<(PlannedWrites, Vec<WorkItem>)>> {
        let binding = find_approval_binding(db, &purchase.base.id, executor).await?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        let runtime =
            load_cancel_runtime(db, &binding, command.subject(), purchase.approval_subject_version, executor)
                .await?;
        let input = build_purchase_order_cancel_input(
            &runtime,
            command.reason(),
            self.actor.id(),
            command.idempotency_key(),
            None,
            now,
        )?;
        Ok(match prepare_document_cancel(input, command.expected_document_version())? {
            PreparedExecution::Apply(writes) => Some((*writes, runtime.open_tasks)),
            _ => None,
        })
    }
}

/// 受影响且处于审批中的采购，顺序与事实一致。
///
/// # 参数
/// * `state` - 覆盖与受影响采购
///
/// # 返回
/// 返回需要撤回的采购。
///
/// # 错误
/// 无。
fn in_approval_affected(state: &PurchaseSourceState) -> Vec<&PurchaseOrder> {
    state
        .facts
        .purchase_orders
        .iter()
        .filter(|purchase| purchase.stable.status == PurchaseOrderStatus::InApproval)
        .filter(|purchase| state.impact_of(&purchase.base.id).is_some())
        .collect()
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::ids::{
        PurchaseOrderId, SalesOrderId, SalesOrderRevisionId, SupplierAccountId, WarehouseId,
    };
    use erp_core::money::Quantity;
    use erp_procurement::entity::facts::{
        FactIdentity, PaymentTermFact, SalesCustomerSnapshotFact, SalesRevisionFact,
    };
    use erp_procurement::entity::purchase_order::{
        FulfillmentResponsibility, ProcurementCoverageFacts, ProcurementCoverageSummary, PurchaseOrder,
        PurchaseOrderData, PurchaseOrderStatus, PurchaseSourceImpact, PurchaseType, SalesProcurementCoverage,
        SourceImpactKind, SourceImpactReason,
    };
    use erp_read_models::purchase_center::source_impact::PurchaseSourceState;

    use super::in_approval_affected;

    fn purchase(id: &str, status: PurchaseOrderStatus) -> PurchaseOrder {
        let mut order = PurchaseOrder::new(
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
        .unwrap();
        order.stable.status = status;
        order
    }

    fn impact(id: &str) -> PurchaseSourceImpact {
        PurchaseSourceImpact {
            purchase_order_id: id.into(),
            reasons: vec![SourceImpactReason {
                sales_order_line_id: "sol-1".into(),
                line_no: 1,
                item_name: "礼盒".into(),
                kind: SourceImpactKind::Zeroed { baseline_quantity: Quantity::from_str("5").unwrap() },
            }],
        }
    }

    /// 只撤回受影响且处于审批中的采购；草稿、已生效和未受影响采购不撤回。
    #[test]
    fn only_affected_in_approval_purchases_are_withdrawn() {
        let zero = Quantity::from_str("0").unwrap();
        let state = PurchaseSourceState {
            facts: ProcurementCoverageFacts {
                purchase_orders: vec![
                    purchase("approval", PurchaseOrderStatus::InApproval),
                    purchase("draft", PurchaseOrderStatus::Draft),
                    purchase("effective", PurchaseOrderStatus::Effective),
                    purchase("untouched", PurchaseOrderStatus::InApproval),
                ],
                ..ProcurementCoverageFacts::default()
            },
            coverage: SalesProcurementCoverage {
                revision: SalesRevisionFact {
                    base: FactIdentity { id: "rev-2".into() },
                    customer_snapshot: SalesCustomerSnapshotFact { customer_name: "客户".into() },
                    contract_snapshot: None,
                },
                lines: Vec::new(),
                summary: ProcurementCoverageSummary::new(zero, zero).unwrap(),
            },
            impacts: vec![impact("approval"), impact("draft"), impact("effective")],
        };

        let ids =
            in_approval_affected(&state).into_iter().map(|order| order.base.id.as_str()).collect::<Vec<_>>();

        assert_eq!(ids, vec!["approval"]);
    }
}
