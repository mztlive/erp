//! 草稿、待审变更及生效变更共用专项条款复验与采用余额调整。
use erp_procurement::entity::purchase_order::{
    PurchaseChangeSubmissionLine, PurchaseInquiryAddress, PurchaseOrder, PurchaseOrderRevisionLine,
    PurchaseOrderSubmissionLine,
};
use erp_procurement::repository::PurchaseOrderExt;
use erp_supply::inquiry::{
    InquiryQuoteReservation, InquirySupplyExt, InquirySupplyService, QuoteAddressLimit,
};
use mongodb::Database;
use persistence_core::Executor;

use super::{
    InquiryPurchaseLine, adoption_addresses, changed, ensure_exact_quote, ensure_line_terms, quote_usage,
    reservation_id,
};
use crate::supply_governance::offering::MongoOfferingQualification;
use crate::{Error, Result};

/// 草稿保存调整采用，当前仍在审批或可重提时继续占用新目标。
pub(in crate::procure_to_pay) async fn resize_draft_quotes(
    db: &Database,
    order: &PurchaseOrder,
    lines: &[PurchaseOrderSubmissionLine],
    executor: &mut dyn Executor,
) -> Result<()> {
    resize(db, order, lines, false, executor).await
}
/// 变更提交先占增加部分，保留原正式数量与地址；减少不提前释放。
pub(in crate::procure_to_pay) async fn resize_change_quotes(
    db: &Database,
    order: &PurchaseOrder,
    lines: &[PurchaseChangeSubmissionLine],
    executor: &mut dyn Executor,
) -> Result<()> {
    resize(db, order, lines, true, executor).await
}
/// 正式变更生效时才将采用切换至新完整数量及地址，原历史保留。
pub(in crate::procure_to_pay) async fn resize_revision_quotes(
    db: &Database,
    order: &PurchaseOrder,
    lines: &[PurchaseOrderRevisionLine],
    executor: &mut dyn Executor,
) -> Result<()> {
    resize(db, order, lines, false, executor).await
}
async fn resize<L: InquiryPurchaseLine>(
    db: &Database,
    order: &PurchaseOrder,
    lines: &[L],
    keep_existing: bool,
    executor: &mut dyn Executor,
) -> Result<()> {
    let service = InquirySupplyService::new(db.clone());
    let qualification = MongoOfferingQualification::new(db.clone());
    let mut kept = Vec::new();
    for line in lines {
        let Some(source) = line.source().and_then(|source| source.inquiry_quote.as_ref()) else { continue };
        ensure_line_terms(line, source)?;
        let id = reservation_id(&order.base.id, line)?;
        kept.push(id.clone());
        let addresses = adoption_addresses(db, order, line, source, executor).await?;
        let usage = quote_usage(line, &order.supplier_id, source, addresses)?;
        let quote = service.resize_quote(&id, &usage, keep_existing, &qualification, executor).await?;
        ensure_exact_quote(source, &quote)?;
    }
    if !keep_existing {
        let reservations = service.reservations(&order.base.id, executor).await?;
        for reservation in reservations {
            if !kept.contains(&reservation.base.id) {
                service.release_reservation(&reservation.base.id, executor).await?;
            }
        }
    }
    Ok(())
}
/// 变更正式作废时恢复原生效数量的采用余额，不使撤回/驳回提前释放。
pub(in crate::procure_to_pay) async fn restore_current_quotes(
    db: &Database,
    order_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let order = db
        .purchase_orders()
        .find_by_id(order_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("原采购单不存在".into()))?;
    let revision = order
        .stable
        .current_revision_id
        .as_deref()
        .ok_or_else(|| Error::ConflictError("原采购生效版本缺失".into()))?;
    let lines = db.purchase_order().list_revision_lines(&revision.to_string().into(), executor).await?;
    let service = InquirySupplyService::new(db.clone());
    let mut kept = Vec::new();
    for line in &lines {
        if let Some(id) = restore_current_line(db, &order, line, &service, executor).await? {
            kept.push(id);
        }
    }
    for reservation in service.reservations(&order.base.id, executor).await? {
        if !kept.contains(&reservation.base.id) {
            service.release_reservation(&reservation.base.id, executor).await?;
        }
    }
    Ok(())
}

async fn restore_current_line(
    db: &Database,
    order: &PurchaseOrder,
    line: &PurchaseOrderRevisionLine,
    service: &InquirySupplyService,
    executor: &mut dyn Executor,
) -> Result<Option<String>> {
    let Some(source) = line.source().and_then(|source| source.inquiry_quote.as_ref()) else {
        return Ok(None);
    };
    ensure_line_terms(line, source)?;
    let id = reservation_id(&order.base.id, line)?;
    let reservation = db.inquiry_quote_reservations().find_by_id(&id, executor).await?.ok_or_else(changed)?;
    let addresses = original_formal_addresses(line, &reservation)?;
    let quote = service
        .restore_existing_usage(
            &id,
            &source.quote_revision_id,
            line.quantity().ok_or_else(changed)?,
            &addresses,
            executor,
        )
        .await?;
    ensure_exact_quote(source, &quote)?;
    Ok(Some(id))
}

fn original_formal_addresses(
    line: &PurchaseOrderRevisionLine,
    reservation: &InquiryQuoteReservation,
) -> Result<Vec<QuoteAddressLimit>> {
    if !line.delivery_allocations.is_empty() {
        let quote = line
            .supplier_offering_source
            .as_ref()
            .and_then(|source| source.inquiry_quote.as_ref())
            .ok_or_else(changed)?;
        return Ok(PurchaseInquiryAddress::from_delivery(&line.delivery_allocations, &quote.address_limits)?
            .into_iter()
            .map(|row| QuoteAddressLimit {
                allocation_id: row.allocation_id,
                address_fingerprint: row.address_fingerprint,
                quantity: row.quantity,
            })
            .collect());
    }
    let quantity = line.quantity.ok_or_else(changed)?;
    if reservation.quantity == quantity {
        return Ok(reservation.addresses.clone());
    }
    reservation
        .history
        .iter()
        .rev()
        .find(|history| history.quantity == quantity)
        .map(|history| history.addresses.clone())
        .ok_or_else(|| Error::ConflictError("原正式专项采用范围历史缺失，禁止猜测恢复".into()))
}
