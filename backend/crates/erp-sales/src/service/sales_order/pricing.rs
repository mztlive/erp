//! 在构造草稿金额前解析 AUTO 单价；手工价与历史快照保持独立。

use erp_core::common::time::BusinessDate;
use persistence_core::Executor;

use super::SalesOrderService;
use crate::dto::sales_order::SalesOrderDraftLineRequest;
use crate::entity::sales_order::{GoodsLineFields, SalesPricingMode};
use crate::ports::sales_order::{
    SalesReferencePriceFact, SalesReferencePricePort, SalesReferencePriceRequest,
};
use crate::{Error, Result};

impl SalesOrderService {
    /// 用可信 SKU 修订参考价填充自动报价行，供统一行金额工厂计算税额与合计。
    ///
    /// # 参数
    /// * `lines` - 尚未构造持久化快照的可编辑明细
    /// * `port` - 公司 SKU 参考价提供方
    /// * `executor` - 调用方数据执行器
    ///
    /// # 返回
    /// 自动报价行完成参考价填充；手工行不产生价格读取且保持成交价。
    ///
    /// # 错误
    /// 自动报价引用失效或没有参考价时拒绝整个命令，仓储错误原样传播。
    pub async fn resolve_draft_reference_prices(
        &self,
        lines: &mut [SalesOrderDraftLineRequest],
        port: &dyn SalesReferencePricePort,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        resolve_prices(lines, port, executor).await
    }
}

/// 按同一执行器读取自动行参考价；历史与手工行不触发提供方读取。
async fn resolve_prices(
    lines: &mut [SalesOrderDraftLineRequest],
    port: &dyn SalesReferencePricePort,
    executor: &mut dyn Executor,
) -> Result<()> {
    let requests = lines
        .iter()
        .filter_map(|line| line.goods.as_ref())
        .filter(|goods| goods.pricing_mode == SalesPricingMode::Auto)
        .map(reference_price_request)
        .collect::<Vec<_>>();
    if requests.is_empty() {
        return Ok(());
    }
    let facts = port.reference_prices(&requests, BusinessDate::today(), executor).await?;
    apply_reference_prices(lines, &facts)
}

/// 提取包含数量的精确报价身份，允许同一 SKU 在不同行使用不同数量。
fn reference_price_request(goods: &GoodsLineFields) -> SalesReferencePriceRequest {
    SalesReferencePriceRequest {
        sku_id: goods.sku_id.to_string(),
        sku_revision_id: goods.sku_revision_id.to_string(),
        quantity: goods.quantity,
    }
}

/// 先验证全部自动行再填价，任何缺失均不产生部分修改。
fn apply_reference_prices(
    lines: &mut [SalesOrderDraftLineRequest],
    facts: &[SalesReferencePriceFact],
) -> Result<()> {
    let replacements = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| {
            line.goods
                .as_ref()
                .filter(|goods| goods.pricing_mode == SalesPricingMode::Auto)
                .map(|goods| (index, reference_price_request(goods)))
        })
        .map(|(index, request)| {
            facts
                .iter()
                .find(|fact| fact.request == request)
                .map(|fact| (index, fact.unit_price_gross))
                .ok_or_else(|| {
                    Error::BusinessLogicError(format!(
                        "第 {} 行商品参考价已失效，请刷新公司商品池后重新选择",
                        lines[index].line_no
                    ))
                })
        })
        .collect::<Result<Vec<_>>>()?;
    for (index, unit_price) in replacements {
        if let Some(goods) = lines[index].goods.as_mut() {
            goods.unit_price_gross = unit_price;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;
    use erp_core::money::{Amount, UnitPrice};
    use persistence_core::NoTransaction;

    use super::*;
    use crate::entity::sales_order::types::build_line_groups;

    struct Prices {
        facts: Vec<SalesReferencePriceFact>,
        calls: AtomicUsize,
        fail: bool,
    }
    #[async_trait]
    impl SalesReferencePricePort for Prices {
        /// 记录实际报价编排读取，验证手工模式零次价格读取。
        async fn reference_prices(
            &self,
            _: &[SalesReferencePriceRequest],
            _: BusinessDate,
            _: &mut dyn Executor,
        ) -> Result<Vec<SalesReferencePriceFact>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                return Err(Error::ConflictError("报价读取失败".into()));
            }
            Ok(self.facts.clone())
        }
    }

    /// 使用真实明细工厂入参构造自动或手工报价请求。
    fn line(mode: SalesPricingMode, quantity: &str) -> SalesOrderDraftLineRequest {
        serde_json::from_value(serde_json::json!({
            "line_no": 1, "line_type": "GOODS_SERVICE", "sales_tax_rate": "0.13",
            "item_name_snapshot": "茶礼", "spec_snapshot": "礼盒", "unit_snapshot": "件",
            "goods": {
                "sku_id": "sku-1", "sku_revision_id": "rev-1", "welfare_scenario": null,
                "service_region": "上海", "fulfillment_due_at": 1800000000,
                "quantity": quantity, "base_unit_code": "件", "unit_price_gross": "99.90",
                "pricing_mode": mode,
            }, "voucher": null,
        }))
        .unwrap()
    }

    /// 以精确 SKU、修订与数量构造替身价格事实。
    fn fact(line: &SalesOrderDraftLineRequest, price: &str) -> SalesReferencePriceFact {
        SalesReferencePriceFact {
            request: reference_price_request(line.goods.as_ref().unwrap()),
            unit_price_gross: UnitPrice::from_str(price).unwrap(),
        }
    }

    /// 自动报价覆写客户端单价，实际金额工厂统一重新计算总额。
    #[tokio::test]
    async fn auto_price_uses_trusted_fact_then_recomputes_amounts() {
        let mut lines = vec![line(SalesPricingMode::Auto, "10")];
        let prices = Prices { facts: vec![fact(&lines[0], "80")], calls: AtomicUsize::new(0), fail: false };
        resolve_prices(&mut lines, &prices, &mut NoTransaction).await.unwrap();
        assert_eq!(lines[0].goods.as_ref().unwrap().unit_price_gross, UnitPrice::from_str("80").unwrap());
        let built =
            build_line_groups(lines[0].line_type, lines[0].goods.clone(), None, lines[0].sales_tax_rate)
                .unwrap();
        assert_eq!(built.gross_amount, Amount::from_str("800").unwrap());
        assert_eq!(built.net_amount.checked_add(built.tax_amount), built.gross_amount);
        assert_eq!(prices.calls.load(Ordering::SeqCst), 1);
    }

    /// 手工价数量变化保持原成交价，提供方不可用也不触发价格读取。
    #[tokio::test]
    async fn manual_quantity_change_keeps_price_without_catalog_reads() {
        let mut lines = vec![line(SalesPricingMode::Manual, "100")];
        let original = lines[0].goods.as_ref().unwrap().unit_price_gross;
        let prices = Prices { facts: Vec::new(), calls: AtomicUsize::new(0), fail: true };
        resolve_prices(&mut lines, &prices, &mut NoTransaction).await.unwrap();
        assert_eq!(lines[0].goods.as_ref().unwrap().unit_price_gross, original);
        assert_eq!(prices.calls.load(Ordering::SeqCst), 0);
    }

    /// 同 SKU 不同数量逐行匹配；缺失修订或数量不能借用其他行报价。
    #[test]
    fn exact_revision_and_quantity_match_before_any_price_changes() {
        let mut lines = vec![line(SalesPricingMode::Auto, "1"), line(SalesPricingMode::Auto, "10")];
        let original = lines[0].goods.as_ref().unwrap().unit_price_gross;
        let partial = [fact(&lines[0], "100")];
        assert!(apply_reference_prices(&mut lines, &partial).is_err());
        assert_eq!(lines[0].goods.as_ref().unwrap().unit_price_gross, original);
        let facts = vec![fact(&lines[0], "100"), fact(&lines[1], "80")];
        apply_reference_prices(&mut lines, &facts).unwrap();
        assert_eq!(lines[0].goods.as_ref().unwrap().unit_price_gross, UnitPrice::from_str("100").unwrap());
        assert_eq!(lines[1].goods.as_ref().unwrap().unit_price_gross, UnitPrice::from_str("80").unwrap());
        let mut stale = facts;
        stale[0].request.sku_revision_id = "other-revision".into();
        assert!(apply_reference_prices(&mut lines, &stale).is_err());
    }

    /// 提供方错误保持原错误且不替换成交单价。
    #[tokio::test]
    async fn reference_price_failure_stops_before_mutating_lines() {
        let mut lines = vec![line(SalesPricingMode::Auto, "10")];
        let original = lines[0].goods.as_ref().unwrap().unit_price_gross;
        let prices = Prices { facts: Vec::new(), calls: AtomicUsize::new(0), fail: true };
        assert!(matches!(
            resolve_prices(&mut lines, &prices, &mut NoTransaction).await,
            Err(Error::ConflictError(_))
        ));
        assert_eq!(lines[0].goods.as_ref().unwrap().unit_price_gross, original);
    }
}
