//! 仅沿正式销售冻结询价来源加载内部专项候选；不加入通用推荐。
use std::collections::HashMap;

use erp_core::money::Quantity;
use erp_procurement::entity::purchase_order::{PurchaseInquiryAddress, PurchaseInquiryQuoteSource};
use erp_procurement::service::purchase_order::creation_basis::InquiryBasisCandidate;
use erp_sales::entity::sales_order::SalesOrder;
use erp_sales::entity::sales_order::inquiry_source::InquirySalesLineBinding;
use erp_sales::repository::sales_order::inquiry_source::inquiry_sources;
use erp_sales::service::sales_inquiry::SalesInquiryService;
use erp_supply::inquiry::{
    InquiryQuote, InquiryQuoteBalance, InquiryQuoteBalanceRepositoryExt, InquiryQuoteRepositoryExt,
    InquirySupplyExt,
};
use mongodb::Database;
use persistence_core::Executor;
use rust_decimal::Decimal;

use crate::Result;

/// 读取准确已接受结果关联的内部报价版本，不读取采购未发布工作。
/// # 参数
/// `db`、`order` 为已授权正式销售及数据库；`executor` 为当前读取或写事务。
/// # 返回
/// 返回仅可用于该销售稳定行的报价候选，余额跨销售和报价修订核算。
/// # 错误
/// 来源损坏、读取失败或报价数量溢出时拒绝。
pub(super) async fn inquiry_candidates(
    db: &Database,
    order: &SalesOrder,
    executor: &mut dyn Executor,
) -> Result<Vec<InquiryBasisCandidate>> {
    let Some(source) = inquiry_sources(db).find_by_id(&order.base.id, executor).await? else {
        return Ok(vec![]);
    };
    if source.released || !source.consumed {
        return Ok(vec![]);
    }
    let inquiry = SalesInquiryService::new(db.clone()).require(&source.inquiry_id, executor).await?;
    if !inquiry.acceptances.iter().any(|accepted| {
        accepted.requirement_version == source.requirement_version
            && accepted.feedback_round == source.feedback_round
    }) {
        return Ok(vec![]);
    }
    let (quotes, balances) =
        quote_balances(db, &source.inquiry_id, source.requirement_version, source.feedback_round, executor)
            .await?;
    candidates_for_bindings(&source.lines, &quotes, &balances)
}

fn candidates_for_bindings(
    bindings: &[InquirySalesLineBinding],
    quotes: &[InquiryQuote],
    balances: &HashMap<String, InquiryQuoteBalance>,
) -> Result<Vec<InquiryBasisCandidate>> {
    let mut candidates = Vec::new();
    for quote in quotes {
        if quote.data.terms.unavailable {
            continue;
        }
        let Some(balance) = balances.get(&quote.data.quote_id) else { continue };
        for binding in bindings.iter().filter(|line| {
            line.inquiry_line_id == quote.data.scope.line_id && line.sku_id == quote.data.sku_id
        }) {
            candidates.push(quote_candidate(quote, balance, binding)?);
        }
    }
    candidates.sort_by(|left, right| {
        (&left.sales_order_line_id, &left.source.quote_revision_id)
            .cmp(&(&right.sales_order_line_id, &right.source.quote_revision_id))
    });
    Ok(candidates)
}

fn quote_candidate(
    quote: &InquiryQuote,
    balance: &InquiryQuoteBalance,
    binding: &InquirySalesLineBinding,
) -> Result<InquiryBasisCandidate> {
    let remaining = quote
        .data
        .terms
        .available_quantity
        .to_decimal()
        .checked_sub(balance.reserved_quantity.to_decimal())
        .unwrap_or(Decimal::ZERO)
        .max(Decimal::ZERO);
    Ok(InquiryBasisCandidate {
        sales_order_line_id: binding.sales_order_line_id.to_string(),
        supplier_offering_id: quote.data.supplier_offering_id.clone(),
        remaining_quote_quantity: Quantity::try_from(remaining)?,
        remaining_address_limits: balance
            .remaining_addresses(quote)?
            .into_iter()
            .map(|row| PurchaseInquiryAddress {
                allocation_id: row.allocation_id,
                address_fingerprint: row.address_fingerprint,
                quantity: row.quantity,
            })
            .collect(),
        source: inquiry_quote_source(quote),
    })
}

async fn quote_balances(
    db: &Database,
    inquiry_id: &str,
    requirement_version: u32,
    feedback_round: u32,
    executor: &mut dyn Executor,
) -> Result<(Vec<InquiryQuote>, HashMap<String, InquiryQuoteBalance>)> {
    let quotes =
        db.inquiry_quotes().for_feedback(inquiry_id, requirement_version, feedback_round, executor).await?;
    let mut latest = HashMap::new();
    for quote in quotes {
        latest.insert(quote.data.quote_id.clone(), quote);
    }
    let ids = latest.keys().cloned().collect::<Vec<_>>();
    let balances = db.inquiry_quote_balances().for_quotes(&ids, executor).await?;
    let balances =
        balances.into_iter().map(|balance| (balance.base.id.clone(), balance)).collect::<HashMap<_, _>>();
    Ok((latest.into_values().collect(), balances))
}

/// 从供应领域原始报价确定性冻结实际采购采用链，不接受销售反馈价格替代。
/// # 参数
/// `quote` 为已核验范围的不可变采购内部报价。
/// # 返回
/// 返回不含备注附件的采购专项快照。
/// # 错误
/// 无。
pub fn inquiry_quote_source(quote: &InquiryQuote) -> PurchaseInquiryQuoteSource {
    let data = &quote.data;
    let terms = &data.terms;
    PurchaseInquiryQuoteSource {
        inquiry_id: data.scope.inquiry_id.clone(),
        requirement_version: data.scope.requirement_version,
        feedback_round: data.feedback_round,
        inquiry_line_id: data.scope.line_id.clone(),
        quote_id: data.quote_id.clone(),
        quote_revision_id: quote.base.id.clone(),
        quote_revision_no: data.revision_no,
        quote_version: quote.base.version,
        unit_cost_gross: terms.unit_cost_gross,
        input_tax_rate: terms.input_tax_rate,
        available_quantity: terms.available_quantity,
        address_limits: terms
            .address_limits
            .iter()
            .map(|row| PurchaseInquiryAddress {
                allocation_id: row.allocation_id.clone(),
                address_fingerprint: row.address_fingerprint.clone(),
                quantity: row.quantity,
            })
            .collect(),
        expected_delivery_date: terms.expected_delivery_date,
        valid_to: terms.valid_to,
    }
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_core::common::time::Instant;
    use erp_sales::entity::sales_inquiry::InquirySupplyPrice;
    use erp_supply::inquiry::{InquiryQuoteData, InquiryQuoteScope, QuoteTerms};

    use super::*;

    #[test]
    fn split_sales_lines_each_receive_matching_quote_without_resetting_balance() {
        let quote = InquiryQuote::new(
            "revision".into(),
            InquiryQuoteData {
                quote_id: "quote".into(),
                revision_no: 1,
                feedback_round: 1,
                scope: InquiryQuoteScope {
                    inquiry_id: "inquiry".into(),
                    requirement_version: 1,
                    line_id: "demand".into(),
                },
                supplier_id: "supplier".to_string().into(),
                supplier_offering_id: "offering".to_string().into(),
                sku_id: "sku".to_string().into(),
                terms: QuoteTerms {
                    unavailable: false,
                    freight_gross: None,
                    unit_cost_gross: "10".parse().unwrap(),
                    input_tax_rate: "0.13".parse().unwrap(),
                    available_quantity: "10".parse().unwrap(),
                    address_limits: vec![],
                    expected_delivery_date: "2026-10-25".parse().unwrap(),
                    valid_to: Some("2026-11-01".parse().unwrap()),
                },
                internal_note: String::new(),
                recorded_by: "buyer".into(),
            },
        )
        .unwrap();
        let balance = InquiryQuoteBalance {
            base: BaseModel { id: "quote".into(), ..BaseModel::fake() },
            reserved_quantity: "3".parse().unwrap(),
            address_quantities: vec![],
        };
        let first = InquirySalesLineBinding {
            sales_order_line_id: "sales-line-1".to_string().into(),
            inquiry_line_id: "demand".into(),
            sku_id: "sku".to_string().into(),
            sku_revision_id: "sku-revision".to_string().into(),
            fulfillment_due_at: Instant::from_unix_secs(1000),
            quantity: "5".parse().unwrap(),
            allocations: vec![],
            price: InquirySupplyPrice {
                price_id: "price-1".into(),
                unit_price_gross: Some("12".parse().unwrap()),
                tax_rate: Some("0.13".parse().unwrap()),
                valid_until: Instant::from_unix_secs(1000),
            },
        };
        let mut second = first.clone();
        second.sales_order_line_id = "sales-line-2".to_string().into();
        second.price.price_id = "price-2".into();
        let mut unrelated = first.clone();
        unrelated.inquiry_line_id = "other-demand".into();
        let mut wrong_sku = first.clone();
        wrong_sku.sku_id = "other-sku".to_string().into();
        let balances = HashMap::from([("quote".into(), balance)]);
        let bindings = [first, second, unrelated, wrong_sku];
        let candidates = candidates_for_bindings(&bindings, std::slice::from_ref(&quote), &balances).unwrap();
        assert_eq!(
            candidates.iter().map(|row| row.sales_order_line_id.as_str()).collect::<Vec<_>>(),
            vec!["sales-line-1", "sales-line-2"]
        );
        assert!(candidates.iter().all(|row| row.remaining_quote_quantity == "7".parse().unwrap()));
        assert!(
            candidates_for_bindings(&bindings, std::slice::from_ref(&quote), &HashMap::new())
                .unwrap()
                .is_empty()
        );
        let mut unavailable = quote;
        unavailable.data.terms.unavailable = true;
        unavailable.data.terms.unit_cost_gross = "0".parse().unwrap();
        unavailable.data.terms.available_quantity = "0".parse().unwrap();
        unavailable.data.internal_note = "停产".into();
        assert!(candidates_for_bindings(&bindings, &[unavailable], &balances).unwrap().is_empty());
    }
}
