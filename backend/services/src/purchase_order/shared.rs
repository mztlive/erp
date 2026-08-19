//! 跨采购单编排模块共享的最小 helper。

use std::str::FromStr;

use database::{NoTransaction, PartyExt, SupplierExt};
use entities::ids::{PurchaseChangeSubmissionLineId, PurchaseOrderSubmissionLineId};
use entities::money::{line_amounts, Amount, Quantity, Rate, UnitPrice};
use entities::purchase_order::{
    PaymentTermSnapshot, PurchaseChangeOrder, PurchaseChangeSubmissionLine, PurchaseChangeSubmissionLineData,
    PurchaseLineType, PurchaseOrder, PurchaseOrderSubmissionLine, PurchaseOrderSubmissionLineData,
};
use id_generator::next_id;

use super::dto::SavePurchaseOrderLine;
use super::PurchaseOrderService;
use crate::errors::{Error, Result};

impl PurchaseOrderService {
    /// 校验乐观锁版本一致。
    pub(super) fn ensure_version(&self, entity: &impl Versioned, expected: u64) -> Result<()> {
        if entity.version() != expected {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        Ok(())
    }

    /// 解析供应商名称（D09 供应商角色 → D07 主体 → 当前主体修订法定名称）。
    pub(super) async fn resolve_supplier_name(
        &self,
        supplier_id: &entities::ids::SupplierAccountId,
    ) -> Result<Option<String>> {
        let supplier = self
            .db
            .supplier_accounts()
            .find_by_id(supplier_id, &mut NoTransaction)
            .await?;
        let Some(supplier) = supplier else { return Ok(None) };
        let party = self
            .db
            .parties()
            .find_by_id(&supplier.party_id, &mut NoTransaction)
            .await?;
        let Some(party) = party else { return Ok(None) };
        let Some(revision_id) = party.stable.current_revision_id else {
            return Ok(None);
        };
        let revision = self
            .db
            .party_revisions()
            .find_by_id(&revision_id, &mut NoTransaction)
            .await?;
        Ok(revision.map(|revision| revision.legal_name))
    }

    /// 解析付款条件门禁快照（PREPAY 前缀判定先款后货，金额/比例门槛暂空）。
    pub(super) async fn payment_term_snapshot(&self, payment_term_code: &str) -> Result<PaymentTermSnapshot> {
        let prepay_gate = payment_term_code.trim().to_uppercase().starts_with("PREPAY");
        PaymentTermSnapshot::new(payment_term_code.to_string(), prepay_gate, None, None).map_err(Into::into)
    }

    /// 构建变更提交行。
    pub(super) async fn build_change_submission_lines(
        &self,
        submission_id: &str,
        lines: &[SavePurchaseOrderLine],
    ) -> Result<Vec<PurchaseChangeSubmissionLine>> {
        self.build_change_lines_inner(submission_id, lines).await
    }

    /// 从请求行构建提交行（逐行计算金额）。
    pub(super) async fn build_lines_from_request(
        &self,
        submission_id: &entities::ids::PurchaseOrderSubmissionId,
        lines: &[SavePurchaseOrderLine],
    ) -> Result<Vec<PurchaseOrderSubmissionLine>> {
        let mut result = Vec::with_capacity(lines.len());
        for (index, line) in lines.iter().enumerate() {
            let (gross, net, tax) = self.compute_line_amounts(line).await?;
            result.push(PurchaseOrderSubmissionLine::new(
                PurchaseOrderSubmissionLineId::new(next_id()),
                PurchaseOrderSubmissionLineData {
                    purchase_order_submission_id: submission_id.clone(),
                    line_no: (index + 1) as u32,
                    line_type: line.line_type,
                    procurement_confirmation_line_id: line
                        .procurement_confirmation_line_id
                        .as_ref()
                        .map(|value| entities::ids::ProcurementConfirmationLineId::new(value.clone())),
                    sku_id: line
                        .sku_id
                        .as_ref()
                        .map(|value| entities::ids::SkuId::new(value.clone())),
                    sku_revision_id: line
                        .sku_revision_id
                        .as_ref()
                        .map(|value| entities::ids::SkuRevisionId::new(value.clone())),
                    product_name_snapshot: line.product_name.clone(),
                    specification_snapshot: line.specification.clone(),
                    quantity: self.parse_quantity(line.quantity.as_deref())?,
                    base_unit_code: line.base_unit_code.clone(),
                    unit_cost_gross: self.parse_unit_price(line.unit_cost_gross.as_deref())?,
                    gross_amount: gross,
                    net_amount: net,
                    tax_amount: tax,
                    input_tax_rate: self.parse_rate(line.input_tax_rate.as_deref())?,
                    expected_delivery_date: line
                        .expected_delivery_date
                        .as_deref()
                        .map(parse_business_date)
                        .transpose()?,
                    sales_order_submission_line_id: line
                        .sales_order_submission_line_id
                        .as_ref()
                        .map(|value| entities::ids::SalesOrderSubmissionLineId::new(value.clone())),
                    allocated_quantity: self.parse_quantity(line.allocated_quantity.as_deref())?,
                },
            )?);
        }
        Ok(result)
    }

    /// 从请求行构建变更提交行（复用同构字段组）。
    async fn build_change_lines_inner(
        &self,
        submission_id: &str,
        lines: &[SavePurchaseOrderLine],
    ) -> Result<Vec<PurchaseChangeSubmissionLine>> {
        let mut result = Vec::with_capacity(lines.len());
        for (index, line) in lines.iter().enumerate() {
            let (gross, net, tax) = self.compute_line_amounts(line).await?;
            result.push(PurchaseChangeSubmissionLine::new(
                PurchaseChangeSubmissionLineId::new(next_id()),
                PurchaseChangeSubmissionLineData {
                    purchase_change_submission_id: entities::ids::PurchaseChangeSubmissionId::new(
                        submission_id.to_string(),
                    ),
                    line_no: (index + 1) as u32,
                    line_type: line.line_type,
                    procurement_confirmation_line_id: line
                        .procurement_confirmation_line_id
                        .as_ref()
                        .map(|value| entities::ids::ProcurementConfirmationLineId::new(value.clone())),
                    sku_id: line
                        .sku_id
                        .as_ref()
                        .map(|value| entities::ids::SkuId::new(value.clone())),
                    sku_revision_id: line
                        .sku_revision_id
                        .as_ref()
                        .map(|value| entities::ids::SkuRevisionId::new(value.clone())),
                    product_name_snapshot: line.product_name.clone(),
                    specification_snapshot: line.specification.clone(),
                    quantity: self.parse_quantity(line.quantity.as_deref())?,
                    base_unit_code: line.base_unit_code.clone(),
                    unit_cost_gross: self.parse_unit_price(line.unit_cost_gross.as_deref())?,
                    gross_amount: gross,
                    net_amount: net,
                    tax_amount: tax,
                    input_tax_rate: self.parse_rate(line.input_tax_rate.as_deref())?,
                    expected_delivery_date: line
                        .expected_delivery_date
                        .as_deref()
                        .map(parse_business_date)
                        .transpose()?,
                    sales_order_submission_line_id: line
                        .sales_order_submission_line_id
                        .as_ref()
                        .map(|value| entities::ids::SalesOrderSubmissionLineId::new(value.clone())),
                    allocated_quantity: self.parse_quantity(line.allocated_quantity.as_deref())?,
                },
            )?);
        }
        Ok(result)
    }

    /// 计算单行金额（商品行 `line_amounts`，物流行 `gross − round(gross×税率)`）。
    async fn compute_line_amounts(&self, line: &SavePurchaseOrderLine) -> Result<(Amount, Amount, Amount)> {
        let tax_rate = self
            .parse_rate(line.input_tax_rate.as_deref())?
            .unwrap_or_else(zero_rate);
        match line.line_type {
            PurchaseLineType::ItemService => {
                let quantity = self
                    .parse_quantity(line.quantity.as_deref())?
                    .ok_or_else(|| Error::ValidationError("商品行数量不能为空".to_string()))?;
                let unit_cost = self
                    .parse_unit_price(line.unit_cost_gross.as_deref())?
                    .ok_or_else(|| Error::ValidationError("商品行含税单价不能为空".to_string()))?;
                Ok(line_amounts(unit_cost, quantity, tax_rate))
            }
            PurchaseLineType::LogisticsFee => {
                let gross = self
                    .parse_amount(line.gross_amount.as_deref())?
                    .ok_or_else(|| Error::ValidationError("物流费用行含税金额不能为空".to_string()))?;
                let tax = entities::money::Amount::try_from(entities::money::round_to_cent(
                    gross.to_decimal() * tax_rate.to_decimal(),
                ))
                .expect("舍入后小数位不超过 2 位");
                let net = Amount::try_from(gross.to_decimal() - tax.to_decimal())
                    .expect("物流行净额小数位不超过 2 位");
                Ok((gross, net, tax))
            }
        }
    }

    /// 汇总请求行的表头金额。
    pub(super) async fn compute_request_totals(
        &self,
        lines: &[SavePurchaseOrderLine],
    ) -> Result<(Amount, Amount, Amount)> {
        let mut gross = zero_amount();
        let mut net = zero_amount();
        let mut tax = zero_amount();
        for line in lines {
            let (gross_line, net_line, tax_line) = self.compute_line_amounts(line).await?;
            gross = gross.checked_add(gross_line);
            net = net.checked_add(net_line);
            tax = tax.checked_add(tax_line);
        }
        Ok((gross, net, tax))
    }

    /// 解析数量。
    fn parse_quantity(&self, value: Option<&str>) -> Result<Option<Quantity>> {
        match value {
            Some(value) if !value.trim().is_empty() => Quantity::from_str(value.trim())
                .map(Some)
                .map_err(|_| Error::ValidationError(format!("非法数量: {value}"))),
            _ => Ok(None),
        }
    }

    /// 解析含税单价。
    fn parse_unit_price(&self, value: Option<&str>) -> Result<Option<UnitPrice>> {
        match value {
            Some(value) if !value.trim().is_empty() => UnitPrice::from_str(value.trim())
                .map(Some)
                .map_err(|_| Error::ValidationError(format!("非法含税单价: {value}"))),
            _ => Ok(None),
        }
    }

    /// 解析税率。
    fn parse_rate(&self, value: Option<&str>) -> Result<Option<Rate>> {
        match value {
            Some(value) if !value.trim().is_empty() => Rate::from_str(value.trim())
                .map(Some)
                .map_err(|_| Error::ValidationError(format!("非法税率: {value}"))),
            _ => Ok(None),
        }
    }

    /// 解析金额。
    fn parse_amount(&self, value: Option<&str>) -> Result<Option<Amount>> {
        match value {
            Some(value) if !value.trim().is_empty() => Amount::from_str(value.trim())
                .map(Some)
                .map_err(|_| Error::ValidationError(format!("非法金额: {value}"))),
            _ => Ok(None),
        }
    }
}

/// 版本化访问（乐观锁校验统一入口）。
pub(super) trait Versioned {
    /// 返回实体乐观锁版本。
    fn version(&self) -> u64;
}

impl Versioned for PurchaseOrder {
    fn version(&self) -> u64 {
        self.base.version
    }
}

impl Versioned for PurchaseChangeOrder {
    fn version(&self) -> u64 {
        self.base.version
    }
}

/// 零金额。
pub(super) fn zero_amount() -> Amount {
    Amount::from_str("0").expect("零金额合法")
}

/// 零税率。
pub(super) fn zero_rate() -> Rate {
    Rate::from_str("0").expect("零税率合法")
}

/// 解析业务日期字符串。
fn parse_business_date(value: &str) -> Result<entities::common::time::BusinessDate> {
    entities::common::time::BusinessDate::from_str(value.trim())
        .map_err(|_| Error::ValidationError(format!("非法业务日期: {value}")))
}
