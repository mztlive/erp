//! 采购费用行确认发生：登记凭证、确认事实与实际发生成本同事务写入（§7.5.1）。

use std::collections::HashSet;

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_core::common::time::BusinessDate;
use erp_core::ids::{CostEntryId, FileAssetId, PurchaseOrderId, PurchaseOrderRevisionId};
use erp_finance::entity::cost::PurchaseExpenseCostFact;
use erp_finance::service::cost::{PreparedCostEntry, persist_cost_entry};
use erp_procurement::dto::purchase_order::{
    CONFIRM_EXPENSE_ACTION, ConfirmExpenseOccurrenceRequest, ConfirmExpenseOccurrenceResult,
};
use erp_procurement::entity::purchase_order::{
    ProgressStatus, PurchaseExpenseOccurrence, PurchaseExpenseOccurrenceData, PurchaseOrder,
    PurchaseOrderRevisionLine, PurchaseOrderStatus,
};
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::repository::purchase_expense::{
    find_occurrence, occurrences_by_orders, purchase_expense_occurrences,
};
use erp_procurement::service::purchase_order::expense::find_expense_line;
use erp_read_models::finance::extra_cost::extra_cost_category;
use erp_support::{PendingAttachmentBatch, PendingFileAssetRequest};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::PurchaseOrderProcess;
use crate::adapters::purchase_access;
use crate::attachments::PendingFileAssets;
use crate::audit::persist_log;
use crate::{Error, Result};

/// 事务内确认命令。
struct ConfirmCommand {
    purchase_order_id: String,
    expense_line_id: String,
    occurred_on: BusinessDate,
    evidence: FileAssetId,
    actor: AuditActor,
}

impl PurchaseOrderProcess {
    /// 采购确认一条费用行已发生，形成实际发生成本。
    ///
    /// # 参数
    /// * `purchase_order_id` - 采购单
    /// * `expense_line_id` - 稳定费用行身份
    /// * `req` - 发生日期与凭证引用
    /// * `assets` - 本次已写入对象存储的凭证文件
    /// * `actor` - 已通过 `purchase_order:confirm_expense` 鉴权的操作人
    ///
    /// # 返回
    /// 返回费用行身份与实际成本事实主键。
    ///
    /// # 错误
    /// 凭证缺失、采购单不可见或未生效、费用行不在当前版本或金额为 0、已确认发生、
    /// 发生日期晚于今天或写入失败时返回错误。
    pub async fn confirm_expense_occurrence(
        &self,
        purchase_order_id: &str,
        expense_line_id: &str,
        mut req: ConfirmExpenseOccurrenceRequest,
        assets: Vec<PendingFileAssetRequest>,
        actor: &AuditActor,
    ) -> Result<ConfirmExpenseOccurrenceResult> {
        req.validate()?;
        let pending = PendingFileAssets::prepare(assets, actor)?.shared();
        resolve_evidence(&mut req.evidence_attachment_id, pending.as_ref())?;
        let command = ConfirmCommand {
            purchase_order_id: purchase_order_id.to_string(),
            expense_line_id: expense_line_id.to_string(),
            occurred_on: req.occurred_on,
            evidence: req.evidence_attachment_id,
            actor: actor.clone(),
        };
        let db = self.db.clone();
        let rbac = self.require_rbac()?.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { apply_confirm(&db, &rbac, &command, pending.as_ref(), executor).await })
            })
            .await
    }
}

/// 事务内校验并写入确认事实、实际成本、凭证与审计。
async fn apply_confirm(
    db: &Database,
    rbac: &erp_identity::SharedRbacService,
    command: &ConfirmCommand,
    pending: &dyn PendingAttachmentBatch,
    executor: &mut dyn Executor,
) -> Result<ConfirmExpenseOccurrenceResult> {
    let mut order = purchase_access(db.clone(), rbac.clone())
        .require_object(&command.actor, "update", &command.purchase_order_id, &[], executor)
        .await?;
    let (revision_id, lines) = current_lines(db, &order, executor).await?;
    let line = find_expense_line(&lines, &command.expense_line_id)?;
    ensure_not_confirmed(db, command, executor).await?;
    let occurrence = build_occurrence(&order, &revision_id, line, command)?;
    let cost_entry_id = CostEntryId::new(next_id());
    let (entry, allocation) = cost_fact(&occurrence, cost_entry_id.clone()).into_facts()?;
    let audit = command.actor.clone().resource_log_with_message(
        CONFIRM_EXPENSE_ACTION,
        "purchase_order",
        order.base.id.clone(),
        Some(format!("费用行 {} 确认发生", occurrence.expense.description)),
    )?;
    pending.persist(db, executor).await?;
    purchase_expense_occurrences(db).create(&occurrence, executor).await?;
    crate::sales_inquiry_expense::mark_occurred(db, &order.base.id, &occurrence.expense_line_id, executor)
        .await?;
    persist_cost_entry(db, PreparedCostEntry { entry, allocations: vec![allocation] }, executor).await?;
    refresh_expense_progress(db, &mut order, &lines, command.actor.id(), executor).await?;
    persist_log(db, &audit, executor).await?;
    Ok(ConfirmExpenseOccurrenceResult {
        expense_line_id: occurrence.expense_line_id,
        cost_entry_id: cost_entry_id.to_string(),
        replayed: false,
    })
}

/// 读取已生效采购单的当前版本行。
async fn current_lines(
    db: &Database,
    order: &PurchaseOrder,
    executor: &mut dyn Executor,
) -> Result<(PurchaseOrderRevisionId, Vec<PurchaseOrderRevisionLine>)> {
    if !matches!(
        order.stable.status,
        PurchaseOrderStatus::Effective
            | PurchaseOrderStatus::PartiallyExecuted
            | PurchaseOrderStatus::Completed
    ) {
        return Err(Error::BusinessLogicError("采购单生效后才能确认费用发生".into()));
    }
    let revision_id = order
        .stable
        .current_revision_id
        .clone()
        .map(PurchaseOrderRevisionId::new)
        .ok_or_else(|| Error::BusinessLogicError("采购单缺少当前生效版本".into()))?;
    let lines = db.purchase_order().list_revision_lines(&revision_id, executor).await?;
    Ok((revision_id, lines))
}

/// 同一费用行只能确认一次；并发确认由唯一索引兜底。
async fn ensure_not_confirmed(
    db: &Database,
    command: &ConfirmCommand,
    executor: &mut dyn Executor,
) -> Result<()> {
    let existing =
        find_occurrence(db, &command.purchase_order_id, &command.expense_line_id, executor).await?;
    if existing.is_some() {
        return Err(Error::ConflictError("该费用行已确认发生，请刷新后查看".into()));
    }
    Ok(())
}

fn build_occurrence(
    order: &PurchaseOrder,
    revision_id: &PurchaseOrderRevisionId,
    line: &PurchaseOrderRevisionLine,
    command: &ConfirmCommand,
) -> Result<PurchaseExpenseOccurrence> {
    let expense = line.expense.clone().ok_or_else(|| Error::NotFound("费用行不存在".into()))?;
    let rate = line.input_tax_rate.ok_or_else(|| Error::BusinessLogicError("费用行缺少进项税率".into()))?;
    Ok(PurchaseExpenseOccurrence::new(
        next_id(),
        PurchaseExpenseOccurrenceData {
            purchase_order_id: PurchaseOrderId::new(order.base.id.clone()),
            purchase_order_revision_id: revision_id.clone(),
            sales_order_id: order.sales_order_id.clone(),
            supplier_id: order.supplier_id.clone(),
            expense,
            gross_amount: line.gross_amount,
            net_amount: line.net_amount,
            tax_amount: line.tax_amount,
            input_tax_rate: rate,
            occurred_on: command.occurred_on,
            evidence_attachment_id: command.evidence.clone(),
            confirmed_by: command.actor.id().to_string(),
        },
        BusinessDate::today(),
    )?)
}

fn cost_fact(occurrence: &PurchaseExpenseOccurrence, entry_id: CostEntryId) -> PurchaseExpenseCostFact {
    PurchaseExpenseCostFact {
        entry_id,
        purchase_order_id: occurrence.purchase_order_id.to_string(),
        expense_line_id: occurrence.expense_line_id.clone(),
        category: extra_cost_category(occurrence.expense.category),
        description: occurrence.expense.description.clone(),
        supplier_id: occurrence.supplier_id.clone(),
        gross_amount: occurrence.gross_amount,
        net_amount: occurrence.net_amount,
        tax_amount: occurrence.tax_amount,
        input_tax_rate: occurrence.input_tax_rate,
        occurred_on: occurrence.occurred_on,
        sales_order_id: occurrence.sales_order_id.clone(),
        sales_order_line_id: occurrence.expense.sales_order_line_id.clone(),
        evidence_attachment_id: occurrence.evidence_attachment_id.clone(),
    }
}

/// 费用采购单按已确认发生的费用行推进履约进度；商品采购单的进度由入库与交付决定。
async fn refresh_expense_progress(
    db: &Database,
    order: &mut PurchaseOrder,
    lines: &[PurchaseOrderRevisionLine],
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    if order.fulfillment_responsibility
        != erp_procurement::entity::purchase_order::FulfillmentResponsibility::NotApplicable
    {
        return Ok(());
    }
    let occurred = occurrences_by_orders(db, std::slice::from_ref(&order.base.id), executor)
        .await?
        .into_iter()
        .map(|occurrence| occurrence.expense_line_id)
        .collect::<HashSet<_>>();
    order.set_fulfillment_progress(expense_progress(lines, &occurred), actor_id.to_string());
    db.purchase_orders().update(order, executor).await?;
    Ok(())
}

/// 金额为正的费用行全部确认发生为已完成，部分确认为部分，否则未开始。
pub(crate) fn expense_progress(
    lines: &[PurchaseOrderRevisionLine],
    occurred: &HashSet<String>,
) -> ProgressStatus {
    let open = lines
        .iter()
        .filter(|line| line.gross_amount.to_decimal() > rust_decimal::Decimal::ZERO)
        .filter_map(|line| line.expense.as_ref())
        .collect::<Vec<_>>();
    let done = open.iter().filter(|expense| occurred.contains(&expense.expense_line_id)).count();
    match done {
        0 => ProgressStatus::None,
        count if count == open.len() => ProgressStatus::Completed,
        _ => ProgressStatus::Partial,
    }
}

/// 把凭证临时引用替换为正式资产；确认发生的凭证必须随命令上传。
fn resolve_evidence(id: &mut FileAssetId, pending: &dyn PendingAttachmentBatch) -> Result<()> {
    let mut used = HashSet::new();
    if !pending.resolve_id(id, &mut used)? {
        return Err(Error::ValidationError("请上传费用发生凭证".into()));
    }
    pending.ensure_all_used(&used)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use entity_core::BaseModel;
    use erp_core::money::{Amount, Rate};
    use erp_procurement::entity::purchase_order::{
        PurchaseExpenseCategory, PurchaseExpenseDetail, PurchaseLineType,
    };

    use super::*;

    fn fee_line(id: &str, gross: &str) -> PurchaseOrderRevisionLine {
        PurchaseOrderRevisionLine {
            base: BaseModel::fake(),
            purchase_order_revision_id: PurchaseOrderRevisionId::new("por-1"),
            line_no: 1,
            line_type: PurchaseLineType::LogisticsFee,
            procurement_confirmation_line_id: None,
            sku_id: None,
            sku_revision_id: None,
            supplier_offering_source: None,
            delivery_allocations: Vec::new(),
            product_name_snapshot: None,
            specification_snapshot: None,
            quantity: None,
            base_unit_code: None,
            unit_cost_gross: None,
            gross_amount: Amount::from_str(gross).unwrap(),
            net_amount: Amount::from_str(gross).unwrap(),
            tax_amount: Amount::zero(),
            input_tax_rate: Some(Rate::from_str("0").unwrap()),
            expected_delivery_date: None,
            sales_order_line_id: None,
            sales_order_revision_line_id: None,
            allocated_quantity: None,
            expense: Some(
                PurchaseExpenseDetail::new(id.into(), PurchaseExpenseCategory::Other, "费用".into(), None)
                    .unwrap(),
            ),
        }
    }

    #[test]
    fn progress_counts_only_positive_fee_lines() {
        let lines = [fee_line("a", "10.00"), fee_line("b", "5.00"), fee_line("c", "0.00")];
        assert_eq!(expense_progress(&lines, &HashSet::new()), ProgressStatus::None);
        assert_eq!(expense_progress(&lines, &HashSet::from(["a".to_string()])), ProgressStatus::Partial);
        let both = HashSet::from(["a".to_string(), "b".to_string()]);
        assert_eq!(expense_progress(&lines, &both), ProgressStatus::Completed);
    }
}
