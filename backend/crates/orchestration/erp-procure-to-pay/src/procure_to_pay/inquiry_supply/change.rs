//! 草稿、待审变更及生效变更共用专项条款复验与采用余额调整。
use erp_procurement::entity::purchase_order::{
    PurchaseInquiryAddress, PurchaseOrder, PurchaseOrderRevisionLine,
};
use erp_procurement::repository::PurchaseOrderExt;
use erp_supplier_catalog::supply_governance::offering::MongoOfferingQualification;
use erp_supply::inquiry::{
    InquiryQuoteReservation, InquirySupplyExt, InquirySupplyService, QuoteAddressLimit,
};
use mongodb::Database;
use persistence_core::Executor;

use super::{
    InquiryPurchaseLine, adoption_addresses, changed, ensure_exact_quote, ensure_line_terms, quote_usage,
    reservation_id,
};
use crate::{Error, Result};

pub(in crate::procure_to_pay) enum QuoteResize {
    /// 草稿或生效版本使用新的完整数量和地址，释放不再引用的采用。
    Replace,
    /// 待审变更只占增加部分，不提前释放现有采用。
    KeepExisting,
}

pub(in crate::procure_to_pay) async fn resize_quotes<L: InquiryPurchaseLine>(
    db: &Database,
    order: &PurchaseOrder,
    lines: &[L],
    mode: QuoteResize,
    executor: &mut dyn Executor,
) -> Result<()> {
    let keep_existing = matches!(mode, QuoteResize::KeepExisting);
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
        release_unused_quotes(&service, order, &kept, executor).await?;
    }
    Ok(())
}
/// 变更终态驳回或作废时恢复原生效数量的采用余额，释放本次变更增加占用。
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

    release_unused_quotes(&service, &order, &kept, executor).await
}

async fn release_unused_quotes(
    service: &InquirySupplyService,
    order: &PurchaseOrder,
    kept: &[String],
    executor: &mut dyn Executor,
) -> Result<()> {
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
        .ok_or_else(|| {
            Error::ConflictError("找不到原采购采用专项报价时的有效范围，请联系采购经办人核对".into())
        })
}
