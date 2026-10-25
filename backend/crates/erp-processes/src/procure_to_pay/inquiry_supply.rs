//! 询价专项报价经现有正式选源入口采用；原始采购价与销售供给价分别追溯。
use erp_core::common::time::BusinessDate;
use erp_core::ids::{SalesOrderLineId, SkuId, SupplierAccountId};
use erp_core::money::{Quantity, Rate, UnitPrice};
use erp_procurement::entity::purchase_order::{
    FulfillmentResponsibility, PurchaseChangeSubmissionLine, PurchaseDeliveryAllocation,
    PurchaseInquiryAddress, PurchaseInquiryQuoteSource, PurchaseOfferingSource, PurchaseOrder,
    PurchaseOrderRevisionLine, PurchaseOrderSubmissionLine,
};
use erp_read_models::purchase_center::repository::inquiry_quote_source;
use erp_sales::repository::sales_order::inquiry_source::inquiry_sources;
use erp_supply::inquiry::{
    InquiryQuote, InquiryQuoteUse, InquirySupplyExt, InquirySupplyService, QuoteAddressLimit,
    QuoteReservationInput,
};
use mongodb::Database;
use persistence_core::Executor;
use rust_decimal::Decimal;

use crate::supply_governance::offering::MongoOfferingQualification;
use crate::{Error, Result};

mod change;
pub(super) use change::{
    resize_change_quotes, resize_draft_quotes, resize_revision_quotes, restore_current_quotes,
};

trait InquiryPurchaseLine {
    fn stable_line(&self) -> Option<&SalesOrderLineId>;
    fn sku(&self) -> Option<&SkuId>;
    fn quantity(&self) -> Option<Quantity>;
    fn cost(&self) -> Option<UnitPrice>;
    fn tax(&self) -> Option<Rate>;
    fn delivery_date(&self) -> Option<BusinessDate>;
    fn deliveries(&self) -> &[PurchaseDeliveryAllocation];
    fn source(&self) -> Option<&PurchaseOfferingSource>;
}
macro_rules! quote_line {
    ($($line:ty),* $(,)?)=>{$(impl InquiryPurchaseLine for $line {
        fn stable_line(&self)->Option<&SalesOrderLineId>{self.sales_order_line_id.as_ref()}
        fn sku(&self)->Option<&SkuId>{self.sku_id.as_ref()}
        fn quantity(&self)->Option<Quantity>{self.quantity}
        fn cost(&self)->Option<UnitPrice>{self.unit_cost_gross}
        fn tax(&self)->Option<Rate>{self.input_tax_rate}
        fn delivery_date(&self)->Option<BusinessDate>{self.expected_delivery_date}
        fn deliveries(&self)->&[PurchaseDeliveryAllocation]{&self.delivery_allocations}
        fn source(&self)->Option<&PurchaseOfferingSource>{self.supplier_offering_source.as_ref()}
    })*};
}
quote_line!(PurchaseOrderSubmissionLine, PurchaseOrderRevisionLine, PurchaseChangeSubmissionLine);

/// 新采购采用和专项供应余额写入必须与订单、审批、命令回执共用原事务。
pub(super) async fn reserve_selected_quotes(
    db: &Database,
    order: &PurchaseOrder,
    lines: &[PurchaseOrderSubmissionLine],
    executor: &mut dyn Executor,
) -> Result<()> {
    let domain = InquirySupplyService::new(db.clone());
    let qualification = MongoOfferingQualification::new(db.clone());
    for line in lines {
        let Some(source) =
            line.supplier_offering_source.as_ref().and_then(|source| source.inquiry_quote.as_ref())
        else {
            continue;
        };
        ensure_line_terms(line, source)?;
        let addresses = adoption_addresses(db, order, line, source, executor).await?;
        let usage = quote_usage(line, &order.supplier_id, source, addresses)?;
        let quote = domain
            .reserve_quote(
                QuoteReservationInput {
                    quote_revision_id: &source.quote_revision_id,
                    quote_version: source.quote_version,
                    reservation_id: &reservation_id(&order.base.id, line)?,
                    purchase_order_id: &order.base.id,
                    usage: &usage,
                },
                &qualification,
                executor,
            )
            .await?;
        ensure_exact_quote(source, &quote)?;
    }
    Ok(())
}

/// 提交及重提只核验原采购对应的准确采用记录，不重新消费数量或切换报价。
pub(super) async fn revalidate_quote(
    db: &Database,
    order_id: &str,
    supplier_id: &SupplierAccountId,
    line: &PurchaseOrderSubmissionLine,
    source: &PurchaseInquiryQuoteSource,
    executor: &mut dyn Executor,
) -> Result<()> {
    ensure_line_terms(line, source)?;
    let id = reservation_id(order_id, line)?;
    let reservation = db
        .inquiry_quote_reservations()
        .find_by_id(&id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("专项采购采用记录缺失，禁止重提".into()))?;
    if reservation.released
        || reservation.quote_revision_id != source.quote_revision_id
        || reservation.quantity != line.quantity.ok_or_else(changed)?
    {
        return Err(changed());
    }
    if !line.delivery_allocations.is_empty() {
        let current =
            PurchaseInquiryAddress::from_delivery(&line.delivery_allocations, &source.address_limits)?;
        let current = current
            .into_iter()
            .map(|row| QuoteAddressLimit {
                allocation_id: row.allocation_id,
                address_fingerprint: row.address_fingerprint,
                quantity: row.quantity,
            })
            .collect::<Vec<_>>();
        ensure_same_addresses(&current, &reservation.addresses)?;
    }
    let usage = quote_usage(line, supplier_id, source, reservation.addresses)?;
    let quote = InquirySupplyService::new(db.clone())
        .ensure_quote(
            &source.quote_revision_id,
            source.quote_version,
            &usage,
            &MongoOfferingQualification::new(db.clone()),
            executor,
        )
        .await?;
    ensure_exact_quote(source, &quote)
}

fn ensure_exact_quote(source: &PurchaseInquiryQuoteSource, quote: &InquiryQuote) -> Result<()> {
    if inquiry_quote_source(quote) != *source {
        return Err(changed());
    }
    Ok(())
}
fn ensure_line_terms<L: InquiryPurchaseLine>(line: &L, source: &PurchaseInquiryQuoteSource) -> Result<()> {
    if line.cost() != Some(source.unit_cost_gross) || line.tax() != Some(source.input_tax_rate) {
        return Err(Error::ConflictError("专项采购单价或税率与原始报价不符，禁止任意覆盖".into()));
    }
    Ok(())
}
fn reservation_id<L: InquiryPurchaseLine>(order_id: &str, line: &L) -> Result<String> {
    Ok(format!("{order_id}:{}", line.stable_line().ok_or_else(changed)?))
}
fn quote_usage<L: InquiryPurchaseLine>(
    line: &L,
    supplier_id: &SupplierAccountId,
    source: &PurchaseInquiryQuoteSource,
    addresses: Vec<QuoteAddressLimit>,
) -> Result<InquiryQuoteUse> {
    let quantity = line.quantity().ok_or_else(changed)?;
    Ok(InquiryQuoteUse {
        inquiry_id: source.inquiry_id.clone(),
        requirement_version: source.requirement_version,
        line_id: source.inquiry_line_id.clone(),
        sku_id: line.sku().cloned().ok_or_else(changed)?,
        supplier_id: supplier_id.clone(),
        quantity,
        addresses,
        expected_delivery_date: line.delivery_date().ok_or_else(changed)?,
        on_date: BusinessDate::today(),
        consumed_quantity: Quantity::try_from(Decimal::ZERO)?,
    })
}

async fn adoption_addresses<L: InquiryPurchaseLine>(
    db: &Database,
    order: &PurchaseOrder,
    line: &L,
    source: &PurchaseInquiryQuoteSource,
    executor: &mut dyn Executor,
) -> Result<Vec<QuoteAddressLimit>> {
    ensure_source_binding(db, order, line, source, executor).await?;
    let rows = if order.fulfillment_responsibility == FulfillmentResponsibility::Warehouse {
        warehouse_source_addresses(db, order, line, source, executor).await?
    } else {
        PurchaseInquiryAddress::from_delivery(line.deliveries(), &source.address_limits)?
    };
    Ok(rows
        .into_iter()
        .map(|row| QuoteAddressLimit {
            allocation_id: row.allocation_id,
            address_fingerprint: row.address_fingerprint,
            quantity: row.quantity,
        })
        .collect())
}

/// 入仓单仅保存目标仓；报价内部按销售稳定地址及报价剩余范围验证，不泄露给供应商。
async fn warehouse_source_addresses<L: InquiryPurchaseLine>(
    db: &Database,
    order: &PurchaseOrder,
    line: &L,
    quote: &PurchaseInquiryQuoteSource,
    executor: &mut dyn Executor,
) -> Result<Vec<PurchaseInquiryAddress>> {
    if quote.address_limits.is_empty() {
        return Ok(vec![]);
    }
    let source =
        inquiry_sources(db).find_by_id(order.sales_order_id.as_ref(), executor).await?.ok_or_else(changed)?;
    if source.inquiry_id != quote.inquiry_id
        || source.requirement_version != quote.requirement_version
        || source.feedback_round != quote.feedback_round
    {
        return Err(changed());
    }
    let binding = source
        .lines
        .iter()
        .find(|binding| {
            Some(&binding.sales_order_line_id) == line.stable_line()
                && binding.inquiry_line_id == quote.inquiry_line_id
        })
        .ok_or_else(changed)?;
    let balance =
        db.inquiry_quote_balances().find_by_id(&quote.quote_id, executor).await?.ok_or_else(changed)?;
    let own =
        db.inquiry_quote_reservations().find_by_id(&reservation_id(&order.base.id, line)?, executor).await?;
    let bindings = binding
        .allocations
        .iter()
        .map(|row| PurchaseInquiryAddress {
            allocation_id: row.allocation_id.clone(),
            address_fingerprint: PurchaseInquiryAddress::fingerprint(
                &row.delivery.recipient,
                &row.delivery.phone,
                row.delivery.region_code.as_str(),
                &row.delivery.address,
            ),
            quantity: row.delivery.quantity,
        })
        .collect::<Vec<_>>();
    let held = balance.address_quantities.iter().map(purchase_address).collect::<Vec<_>>();
    let owned = own
        .as_ref()
        .filter(|row| !row.released)
        .map(|reservation| reservation.addresses.iter().map(purchase_address).collect::<Vec<_>>())
        .unwrap_or_default();
    quote
        .warehouse_allocations(&bindings, &held, &owned, line.quantity().ok_or_else(changed)?)
        .map_err(Into::into)
}
fn purchase_address(row: &QuoteAddressLimit) -> PurchaseInquiryAddress {
    PurchaseInquiryAddress {
        allocation_id: row.allocation_id.clone(),
        address_fingerprint: row.address_fingerprint.clone(),
        quantity: row.quantity,
    }
}
fn ensure_same_addresses(left: &[QuoteAddressLimit], right: &[QuoteAddressLimit]) -> Result<()> {
    let mut left = left.iter().collect::<Vec<_>>();
    let mut right = right.iter().collect::<Vec<_>>();
    left.sort_by(|a, b| a.allocation_id.cmp(&b.allocation_id));
    right.sort_by(|a, b| a.allocation_id.cmp(&b.allocation_id));
    if left != right {
        return Err(changed());
    }
    Ok(())
}
async fn ensure_source_binding<L: InquiryPurchaseLine>(
    db: &Database,
    order: &PurchaseOrder,
    line: &L,
    quote: &PurchaseInquiryQuoteSource,
    executor: &mut dyn Executor,
) -> Result<()> {
    let source =
        inquiry_sources(db).find_by_id(order.sales_order_id.as_ref(), executor).await?.ok_or_else(changed)?;
    if source.inquiry_id != quote.inquiry_id
        || source.requirement_version != quote.requirement_version
        || source.feedback_round != quote.feedback_round
        || source.released
        || !source.consumed
        || !source.lines.iter().any(|binding| {
            Some(&binding.sales_order_line_id) == line.stable_line()
                && binding.inquiry_line_id == quote.inquiry_line_id
                && Some(&binding.sku_id) == line.sku()
        })
    {
        return Err(changed());
    }
    Ok(())
}
fn changed() -> Error {
    Error::ConflictError("专项报价采用范围、原价或数量已变化，请重新确认选源".into())
}

#[cfg(test)]
mod tests {
    use std::slice;

    use super::*;

    fn address(id: &str, fingerprint: &str, quantity: i64) -> QuoteAddressLimit {
        QuoteAddressLimit {
            allocation_id: id.into(),
            address_fingerprint: fingerprint.into(),
            quantity: Quantity::try_from(Decimal::from(quantity)).unwrap(),
        }
    }

    #[test]
    fn address_comparison_accepts_reordered_allocations_and_empty_sets() {
        let first = address("a", "address-a", 1);
        let second = address("b", "address-b", 2);
        assert!(ensure_same_addresses(&[first.clone(), second.clone()], &[second, first]).is_ok());
        assert!(ensure_same_addresses(&[], &[]).is_ok());
    }

    #[test]
    fn address_comparison_rejects_missing_or_changed_allocation_facts() {
        let original = address("a", "address-a", 1);
        for replacement in [
            vec![],
            vec![address("b", "address-a", 1)],
            vec![address("a", "address-b", 1)],
            vec![address("a", "address-a", 2)],
            vec![original.clone(), original.clone()],
        ] {
            assert!(matches!(
                ensure_same_addresses(slice::from_ref(&original), &replacement),
                Err(Error::ConflictError(message))
                    if message == "专项报价采用范围、原价或数量已变化，请重新确认选源"
            ));
        }
    }

    #[test]
    fn address_comparison_preserves_duplicate_allocation_order() {
        let first = address("a", "address-a", 1);
        let second = address("a", "address-b", 2);
        let original = [first.clone(), second.clone()];
        assert!(ensure_same_addresses(&original, &original).is_ok());
        assert!(ensure_same_addresses(&original, &[second, first]).is_err());
    }
}
