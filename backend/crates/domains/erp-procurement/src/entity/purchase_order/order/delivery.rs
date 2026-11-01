//! 采购单冻结的价格依据、配送判定与"供应商不收运费"标记。

use erp_core::common::time::Instant;
use erp_core::ids::SalesOrderLineId;
use erp_core::{Error, Result};

use super::PurchaseOrder;
use crate::entity::purchase_order::delivery_judgement::{PurchaseDeliveryJudgement, PurchaseFreightWaiver};
use crate::entity::purchase_order::types::SupplyPriceBasis;

impl PurchaseOrder {
    /// 冻结选源确定的价格依据与逐行配送判定。
    ///
    /// # 参数
    /// * `price_basis` - 价格依据
    /// * `judgements` - 逐行配送判定
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 无。
    pub fn record_sourcing_delivery(
        &mut self,
        price_basis: SupplyPriceBasis,
        judgements: Vec<PurchaseDeliveryJudgement>,
    ) {
        self.sourcing_delivery.supply_price_basis = Some(price_basis);
        self.sourcing_delivery.delivery_judgements = judgements;
    }

    /// 标记某销售明细的运费另计由供应商免收。
    ///
    /// # 参数
    /// * `sales_order_line_id` - 运费另计的稳定销售行
    /// * `reason` - 原因
    /// * `actor` - 操作人
    /// * `at` - 标记时间
    ///
    /// # 返回
    /// 标记成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 该行不是运费另计、已标记过或原因不合法时返回错误。
    pub fn waive_freight(
        &mut self,
        sales_order_line_id: &SalesOrderLineId,
        reason: String,
        actor: &str,
        at: Instant,
    ) -> Result<()> {
        let freight_extra = self.sourcing_delivery.delivery_judgements.iter().any(|judgement| {
            judgement.sales_order_line_id == *sales_order_line_id && judgement.freight_extra
        });
        if !freight_extra {
            return Err(Error::from("该商品不是运费另计，无需标记"));
        }
        if self
            .sourcing_delivery
            .freight_waivers
            .iter()
            .any(|waiver| waiver.sales_order_line_id == *sales_order_line_id)
        {
            return Err(Error::from("该商品已标记供应商不收运费"));
        }
        let waiver = PurchaseFreightWaiver::new(sales_order_line_id.clone(), reason, actor, at)?;
        self.sourcing_delivery.freight_waivers.push(waiver);
        self.stable.touch(actor);
        Ok(())
    }

    /// 返回运费另计且尚未标记供应商不收运费的销售行。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 按冻结顺序返回待跟进的销售行；是否已有物流费用采购由读取模型另行判定。
    ///
    /// # 错误
    /// 无。
    pub fn unwaived_freight_lines(&self) -> Vec<&SalesOrderLineId> {
        self.sourcing_delivery
            .delivery_judgements
            .iter()
            .filter(|judgement| judgement.freight_extra)
            .map(|judgement| &judgement.sales_order_line_id)
            .filter(|line| {
                !self
                    .sourcing_delivery
                    .freight_waivers
                    .iter()
                    .any(|waiver| waiver.sales_order_line_id == **line)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;
    use erp_core::ids::SalesOrderLineId;

    use super::super::tests::new_order;
    use crate::entity::purchase_order::delivery_judgement::PurchaseDeliveryJudgement;
    use crate::entity::purchase_order::types::SupplyPriceBasis;

    fn judgement(line: &str, freight_extra: bool) -> PurchaseDeliveryJudgement {
        PurchaseDeliveryJudgement {
            sales_order_line_id: SalesOrderLineId::new(line),
            template_name: Some("常温".into()),
            template_revision_no: Some(2),
            freight_extra,
        }
    }

    #[test]
    fn waiver_clears_pending_freight_once() {
        let mut order = new_order();
        order.record_sourcing_delivery(
            SupplyPriceBasis::Bulk,
            vec![judgement("sol-1", true), judgement("sol-2", false)],
        );
        assert_eq!(order.unwaived_freight_lines(), [&SalesOrderLineId::new("sol-1")]);
        let line = SalesOrderLineId::new("sol-1");
        order.waive_freight(&line, "供应商承担".into(), "buyer", Instant::now()).unwrap();
        assert!(order.unwaived_freight_lines().is_empty());
        assert!(order.waive_freight(&line, "再次".into(), "buyer", Instant::now()).is_err());
    }

    #[test]
    fn waiver_rejects_lines_without_freight_extra() {
        let mut order = new_order();
        order.record_sourcing_delivery(SupplyPriceBasis::Dropship, vec![judgement("sol-2", false)]);
        let error = order
            .waive_freight(&SalesOrderLineId::new("sol-2"), "原因".into(), "buyer", Instant::now())
            .unwrap_err();
        assert!(error.to_string().contains("不是运费另计"));
        assert!(
            order
                .waive_freight(&SalesOrderLineId::new("sol-1"), "原因".into(), "buyer", Instant::now())
                .is_err()
        );
    }

    #[test]
    fn sourcing_delivery_stays_top_level_and_old_documents_default_to_empty() {
        let mut order = new_order();
        order.record_sourcing_delivery(SupplyPriceBasis::Bulk, vec![judgement("sol-1", true)]);
        let json = serde_json::to_value(&order).unwrap();
        assert_eq!(json["supply_price_basis"], "BULK");
        assert_eq!(json["delivery_judgements"][0]["freight_extra"], true);
        let restored: super::PurchaseOrder = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(restored.sourcing_delivery, order.sourcing_delivery);
        let mut legacy = json;
        let object = legacy.as_object_mut().unwrap();
        object.remove("supply_price_basis");
        object.remove("delivery_judgements");
        object.remove("freight_waivers");
        let restored: super::PurchaseOrder = serde_json::from_value(legacy).unwrap();
        assert!(restored.sourcing_delivery.delivery_judgements.is_empty());
        assert!(restored.sourcing_delivery.supply_price_basis.is_none());
    }
}
