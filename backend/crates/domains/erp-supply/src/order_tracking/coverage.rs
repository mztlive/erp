use erp_core::money::Quantity;
use rust_decimal::Decimal;

use super::TrackingSourceLine;
use crate::{Error, Result};

/// 消费方已经按真实采购分配解析的发货行事实。
pub struct DeliveryCoverageLine {
    pub purchase_order_revision_line_id: String,
    pub sales_order_line_id: String,
    pub quantity: Quantity,
}

/// 核验该子订单是否完整覆盖一张既有发货单，禁止部分数量冒充整单发货。
/// # 参数
/// 冻结发单来源行与当前发货单的真实采购分配、销售明细及数量。
/// # 返回
/// 无交集返回空集合；完整覆盖返回所用的外部订货行身份。
/// # 错误
/// 存在交集但不能证明全部发货数量、身份或算术守恒时拒绝。
pub fn complete_delivery_coverage(
    sources: &[TrackingSourceLine],
    delivery: &[DeliveryCoverageLine],
) -> Result<Vec<String>> {
    let mut covered = Vec::new();
    let mut unmatched = false;
    for line in delivery {
        let matching = sources
            .iter()
            .filter(|source| {
                source.purchase_order_revision_line_id == line.purchase_order_revision_line_id
                    && source.sales_order_line_id.as_deref() == Some(line.sales_order_line_id.as_str())
            })
            .collect::<Vec<_>>();
        if matching.is_empty() {
            unmatched = true;
            continue;
        }
        let quantity = matching.iter().try_fold(Decimal::ZERO, |total, source| {
            total
                .checked_add(source.quantity.to_decimal())
                .ok_or_else(|| Error::ValidationError("来源发货数量超限".into()))
        })?;
        if quantity != line.quantity.to_decimal() {
            return Err(Error::BusinessLogicError(
                "供应商子单数量不能证明整张发货单已发货，请核对分包裹数量".into(),
            ));
        }
        covered.extend(matching.into_iter().map(|source| source.fulfillment_item_id.clone()));
    }
    if !covered.is_empty() && unmatched {
        return Err(Error::BusinessLogicError("供应商子单只覆盖部分发货行，请拆分核对后处理".into()));
    }
    Ok(covered)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source(id: &str, purchase: &str, sales: &str, quantity: &str) -> TrackingSourceLine {
        TrackingSourceLine {
            fulfillment_item_id: id.into(),
            purchase_order_revision_line_id: purchase.into(),
            sales_order_line_id: Some(sales.into()),
            delivery_id: Some("delivery".into()),
            quantity: quantity.parse().unwrap(),
        }
    }
    fn delivery(purchase: &str, sales: &str, quantity: &str) -> DeliveryCoverageLine {
        DeliveryCoverageLine {
            purchase_order_revision_line_id: purchase.into(),
            sales_order_line_id: sales.into(),
            quantity: quantity.parse().unwrap(),
        }
    }
    #[test]
    fn exact_coverage_preserves_distinct_external_lines() {
        let sources = [source("one", "p", "s", "1"), source("two", "p", "s", "2")];
        assert_eq!(
            complete_delivery_coverage(&sources, &[delivery("p", "s", "3")]).unwrap(),
            vec!["one", "two"]
        );
    }
    #[test]
    fn partial_quantity_or_partial_rows_never_expand_to_whole_delivery() {
        let sources = [source("one", "p", "s", "1")];
        assert!(complete_delivery_coverage(&sources, &[delivery("p", "s", "2")]).is_err());
        assert!(
            complete_delivery_coverage(
                &sources,
                &[delivery("p", "s", "1"), delivery("another", "other", "1")]
            )
            .is_err()
        );
        assert!(
            complete_delivery_coverage(&sources, &[delivery("p", "other-sale", "1")]).unwrap().is_empty()
        );
    }
}
