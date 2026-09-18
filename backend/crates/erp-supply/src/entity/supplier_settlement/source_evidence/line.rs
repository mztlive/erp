//! 结算来源证据行。

use erp_core::ids::{SupplierFulfillmentItemId, SupplierFulfillmentOrderId};
use erp_core::money::{Amount, Quantity};
use erp_core::{Error, Result};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::{
    MAX_REFERENCES_PER_LINE, SettlementAmountComponents, SettlementSourceFactType, ensure_non_negative,
    ensure_triple, normalize_references,
};

/// 供应商结算来源证据行构造数据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupplierSettlementSourceEvidenceLineData {
    /// 供应商子订单。
    pub supplier_fulfillment_order_id: SupplierFulfillmentOrderId,
    /// 供应商履约明细。
    pub supplier_fulfillment_item_id: SupplierFulfillmentItemId,
    /// 冻结数量。
    pub quantity: Quantity,
    /// 本行覆盖的正式事实类别。
    pub source_fact_types: Vec<SettlementSourceFactType>,
    /// 本行全部正式证据引用。
    pub evidence_reference_ids: Vec<String>,
    /// 订单成本金额。
    pub order: SettlementAmountComponents,
    /// 运费金额。
    pub freight: SettlementAmountComponents,
    /// 服务费金额。
    pub service_fee: SettlementAmountComponents,
    /// 退款金额。
    pub refund: SettlementAmountComponents,
    /// 供应商账单金额。
    pub supplier_billed: SettlementAmountComponents,
}

/// 一条来源证据的冻结金额和逐行身份。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierSettlementSourceEvidenceLine {
    /// 供应商子订单。
    pub supplier_fulfillment_order_id: SupplierFulfillmentOrderId,
    /// 供应商履约明细。
    pub supplier_fulfillment_item_id: SupplierFulfillmentItemId,
    /// 冻结数量。
    pub quantity: Quantity,
    /// 本行覆盖的正式事实类别。
    pub source_fact_types: Vec<SettlementSourceFactType>,
    /// 订单完成、取消、退款分配、费用及账单行的正式证据引用。
    pub evidence_reference_ids: Vec<String>,
    /// 订单成本含税金额。
    pub order_gross: Amount,
    /// 订单成本不含税金额。
    pub order_net: Amount,
    /// 订单成本税额。
    pub order_tax: Amount,
    /// 运费含税金额。
    pub freight_gross: Amount,
    /// 运费不含税金额。
    pub freight_net: Amount,
    /// 运费税额。
    pub freight_tax: Amount,
    /// 服务费含税金额。
    pub service_fee_gross: Amount,
    /// 服务费不含税金额。
    pub service_fee_net: Amount,
    /// 服务费税额。
    pub service_fee_tax: Amount,
    /// 退款含税金额。
    pub refund_gross: Amount,
    /// 退款不含税金额。
    pub refund_net: Amount,
    /// 退款税额。
    pub refund_tax: Amount,
    /// ERP 试算含税金额。
    pub erp_gross: Amount,
    /// ERP 试算不含税金额。
    pub erp_net: Amount,
    /// ERP 试算税额。
    pub erp_tax: Amount,
    /// 供应商账单行含税金额。
    pub supplier_billed_gross: Amount,
    /// 供应商账单行不含税金额。
    pub supplier_billed_net: Amount,
    /// 供应商账单行税额。
    pub supplier_billed_tax: Amount,
}

impl SupplierSettlementSourceEvidenceLine {
    /// 由正式事实和金额三元组构造来源证据行。
    ///
    /// ERP 金额统一由订单、运费、服务费和退款三元组逐项派生，调用方不得自行
    /// 拼装 `erp_gross/net/tax`。
    ///
    /// # 参数
    /// * `data` - 已完成跨域事实查询的行构造数据
    ///
    /// # 返回
    /// 返回已规范化事实类别、证据引用和 ERP 金额的来源证据行。
    ///
    /// # 错误
    /// 退款超过订单加费用、数量/证据非法或金额恒等不成立时返回领域错误。
    pub fn from_components(data: SupplierSettlementSourceEvidenceLineData) -> Result<Self> {
        let erp = data
            .order
            .checked_add(data.freight, "ERP 金额")?
            .checked_add(data.service_fee, "ERP 金额")?
            .checked_sub(data.refund, "ERP 金额")?;
        let mut line = Self {
            supplier_fulfillment_order_id: data.supplier_fulfillment_order_id,
            supplier_fulfillment_item_id: data.supplier_fulfillment_item_id,
            quantity: data.quantity,
            source_fact_types: data.source_fact_types,
            evidence_reference_ids: data.evidence_reference_ids,
            order_gross: data.order.gross,
            order_net: data.order.net,
            order_tax: data.order.tax,
            freight_gross: data.freight.gross,
            freight_net: data.freight.net,
            freight_tax: data.freight.tax,
            service_fee_gross: data.service_fee.gross,
            service_fee_net: data.service_fee.net,
            service_fee_tax: data.service_fee.tax,
            refund_gross: data.refund.gross,
            refund_net: data.refund.net,
            refund_tax: data.refund.tax,
            erp_gross: erp.gross,
            erp_net: erp.net,
            erp_tax: erp.tax,
            supplier_billed_gross: data.supplier_billed.gross,
            supplier_billed_net: data.supplier_billed.net,
            supplier_billed_tax: data.supplier_billed.tax,
        };
        line.validate()?;
        Ok(line)
    }

    /// 校验逐行身份、证据与金额恒等式。
    ///
    /// # 错误
    /// 缺少事实/证据、金额为负或任一 `gross = net + tax` 与 ERP 构成恒等不成立时返回错误。
    pub fn validate(&mut self) -> Result<()> {
        if self.quantity.to_decimal() <= Decimal::ZERO {
            return Err(Error::from("来源证据行数量必须大于零"));
        }
        self.source_fact_types.sort_by_key(|value| value.as_str());
        self.source_fact_types.dedup();
        if self.source_fact_types.is_empty() {
            return Err(Error::from("来源证据行至少包含一种正式事实"));
        }
        normalize_references(&mut self.evidence_reference_ids, MAX_REFERENCES_PER_LINE)?;
        if self.evidence_reference_ids.is_empty() {
            return Err(Error::from("来源证据行至少包含一个正式证据引用"));
        }
        for (value, field) in [
            (self.order_gross, "订单含税金额"),
            (self.order_net, "订单不含税金额"),
            (self.order_tax, "订单税额"),
            (self.freight_gross, "运费含税金额"),
            (self.freight_net, "运费不含税金额"),
            (self.freight_tax, "运费税额"),
            (self.service_fee_gross, "服务费含税金额"),
            (self.service_fee_net, "服务费不含税金额"),
            (self.service_fee_tax, "服务费税额"),
            (self.refund_gross, "退款含税金额"),
            (self.refund_net, "退款不含税金额"),
            (self.refund_tax, "退款税额"),
            (self.erp_gross, "ERP 含税金额"),
            (self.erp_net, "ERP 不含税金额"),
            (self.erp_tax, "ERP 税额"),
            (self.supplier_billed_gross, "供应商账单含税金额"),
            (self.supplier_billed_net, "供应商账单不含税金额"),
            (self.supplier_billed_tax, "供应商账单税额"),
        ] {
            ensure_non_negative(value, field)?;
        }
        ensure_triple(self.order_gross, self.order_net, self.order_tax, "订单金额")?;
        ensure_triple(self.freight_gross, self.freight_net, self.freight_tax, "运费金额")?;
        ensure_triple(self.service_fee_gross, self.service_fee_net, self.service_fee_tax, "服务费金额")?;
        ensure_triple(self.refund_gross, self.refund_net, self.refund_tax, "退款金额")?;
        ensure_triple(self.erp_gross, self.erp_net, self.erp_tax, "ERP 金额")?;
        ensure_triple(
            self.supplier_billed_gross,
            self.supplier_billed_net,
            self.supplier_billed_tax,
            "供应商账单金额",
        )?;
        let expected_gross = self
            .order_gross
            .checked_add(self.freight_gross)
            .checked_add(self.service_fee_gross)
            .checked_sub(self.refund_gross);
        let expected_net = self
            .order_net
            .checked_add(self.freight_net)
            .checked_add(self.service_fee_net)
            .checked_sub(self.refund_net);
        let expected_tax = self
            .order_tax
            .checked_add(self.freight_tax)
            .checked_add(self.service_fee_tax)
            .checked_sub(self.refund_tax);
        if (expected_gross, expected_net, expected_tax) != (self.erp_gross, self.erp_net, self.erp_tax) {
            return Err(Error::from("ERP 金额必须逐项等于订单加费用减退款"));
        }
        Ok(())
    }
}
