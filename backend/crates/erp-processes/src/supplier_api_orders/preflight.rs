use erp_catalog::entity::catalog::UnitOfMeasure;
use erp_catalog::repository::CatalogExt;
use erp_core::common::time::Instant;
use erp_core::money::round_to_cent;
use erp_procurement::entity::purchase_order::PurchaseOrderRevisionLine;
use erp_procurement::repository::PurchaseOrderExt;
use erp_supply::api_order::ApiOrderExecution;
use erp_supply::entity::supplier_offering::OfferingStatus;
use erp_supply::ports::connector::SupplierConnector;
use erp_supply::ports::connector::common::Lookup;
use erp_supply::ports::connector::offer::{AvailabilityQuery, AvailabilityState, AvailableQuantity};
use erp_supply::ports::connector::order::{CreateOrder, OrderLine};
use erp_supply::repository::SupplierOfferingExt;
use persistence_core::NoTransaction;

use super::{SupplierApiOrderProcess, missing_capability};
use crate::{Error, Result};

impl SupplierApiOrderProcess {
    pub(super) async fn preflight(
        &self,
        execution: &ApiOrderExecution,
        connector: &dyn SupplierConnector,
        request: &CreateOrder,
    ) -> Result<()> {
        if let Some(preflight) = connector.preflight() {
            let proof = preflight
                .inspect(request)
                .await
                .map_err(|failure| Error::BusinessLogicError(failure.summary))?;
            proof.value.validate(request, Instant::now())?;
            for line in &proof.value.lines {
                self.ensure_frozen_unit(execution, &line.line_id, line.source_unit.as_deref()).await?;
            }
            return ensure_total(request);
        }
        self.legacy_preflight(execution, connector, request).await
    }

    async fn legacy_preflight(
        &self,
        execution: &ApiOrderExecution,
        connector: &dyn SupplierConnector,
        request: &CreateOrder,
    ) -> Result<()> {
        let query = AvailabilityQuery {
            skus: request.lines.iter().map(|line| line.sku.clone()).collect(),
            region: Some(request.recipient.region.clone()),
        };
        let available = connector
            .availability()
            .ok_or_else(|| missing_capability("可供核验"))?
            .availability(&query)
            .await
            .map_err(|failure| Error::BusinessLogicError(failure.summary))?;
        for line in &request.lines {
            let state = available
                .iter()
                .find(|fact| fact.value.sku == line.sku)
                .ok_or_else(|| Error::BusinessLogicError("供应商未返回完整可供核验结果".into()))?;
            if state.value.state != AvailabilityState::Available
                || matches!(state.value.quantity,AvailableQuantity::Exact(quantity) if quantity<line.quantity)
            {
                return Err(Error::BusinessLogicError("供应商当前不可供或数量不足，尚未发单".into()));
            }
            self.verify_offer(execution, connector, line).await?;
        }
        ensure_total(request)
    }

    async fn verify_offer(
        &self,
        execution: &ApiOrderExecution,
        connector: &dyn SupplierConnector,
        line: &OrderLine,
    ) -> Result<()> {
        let Lookup::Found(offer) = connector
            .offers()
            .ok_or_else(|| missing_capability("采购报价核验"))?
            .offer(&line.sku)
            .await
            .map_err(|failure| Error::BusinessLogicError(failure.summary))?
        else {
            return Err(Error::BusinessLogicError("供应商当前未返回原订货规格，尚未发单".into()));
        };
        if let Some(terms) = &offer.value.details.source_purchase_terms {
            terms.ensure_quantity(line.quantity)?;
            terms.ensure_automatic_order()?;
        }
        let quote = offer
            .value
            .quote
            .and_then(|quote| quote.dropship)
            .ok_or_else(|| Error::BusinessLogicError("供应商当前采购报价未核实，尚未发单".into()))?;
        if quote.unit_price != line.approved_unit_price
            || quote.minimum_quantity.is_some_and(|minimum| line.quantity < minimum)
        {
            return Err(Error::BusinessLogicError("供应商当前报价或起订量与批准条件不符，尚未发单".into()));
        }
        self.ensure_frozen_unit(execution, &line.line_id, offer.value.unit.as_deref()).await
    }

    async fn ensure_frozen_unit(
        &self,
        execution: &ApiOrderExecution,
        item: &str,
        source_unit: Option<&str>,
    ) -> Result<()> {
        let source = execution
            .source_lines
            .iter()
            .find(|source| source.fulfillment_item_id == item)
            .ok_or_else(|| Error::BusinessLogicError("发单行缺少正式采购来源".into()))?;
        let line = self
            .db
            .purchase_order_revision_lines()
            .find_by_id(&source.purchase_order_revision_line_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("冻结采购行不存在".into()))?;
        let unit = self.current_unit(&line).await?;
        if line.base_unit_code.as_deref() != Some(unit.unit_code.as_str())
            || source_unit.is_some_and(|source| {
                ![unit.name.as_str(), unit.unit_code.as_str(), unit.symbol.as_str()].contains(&source.trim())
            })
        {
            return Err(Error::BusinessLogicError("供应商规格单位与采购已确认映射不一致，尚未发单".into()));
        }
        let frozen = line
            .supplier_offering_source
            .as_ref()
            .ok_or_else(|| Error::BusinessLogicError("冻结采购选源缺失".into()))?;
        let offering = self
            .db
            .supplier_offerings()
            .find_by_id(frozen.supplier_offering_id.as_ref(), &mut NoTransaction)
            .await?
            .filter(|offering| offering.stable.status == OfferingStatus::Active)
            .ok_or_else(|| Error::BusinessLogicError("原采购供给已停用，尚未发单".into()))?;
        if offering.source_connection_id.as_ref().map(ToString::to_string).as_deref()
            != Some(execution.connection_id.as_str())
        {
            return Err(Error::BusinessLogicError("原采购供给连接已变化，尚未发单".into()));
        }
        Ok(())
    }
    async fn current_unit(&self, line: &PurchaseOrderRevisionLine) -> Result<UnitOfMeasure> {
        let sku = self
            .db
            .skus()
            .find_by_id(
                line.sku_id
                    .as_ref()
                    .ok_or_else(|| Error::BusinessLogicError("冻结采购SKU缺失".into()))?
                    .as_ref(),
                &mut NoTransaction,
            )
            .await?
            .filter(|sku| sku.is_active())
            .ok_or_else(|| Error::BusinessLogicError("公司采购SKU已停用，尚未发单".into()))?;
        let unit = self
            .db
            .unit_of_measures()
            .find_by_id(sku.base_unit_id.as_ref(), &mut NoTransaction)
            .await?
            .filter(|unit| unit.is_active())
            .ok_or_else(|| Error::BusinessLogicError("采购基础单位无效，尚未发单".into()))?;
        Ok(unit)
    }
}

fn ensure_total(request: &CreateOrder) -> Result<()> {
    let total = request.lines.iter().try_fold(request.delivery.shipping_fee.to_decimal(), |sum, line| {
        sum.checked_add(round_to_cent(
            line.quantity
                .to_decimal()
                .checked_mul(line.approved_unit_price.to_decimal())
                .ok_or_else(|| Error::ValidationError("接口采购金额超限".into()))?,
        ))
        .ok_or_else(|| Error::ValidationError("接口采购金额超限".into()))
    })?;
    if total > request.approved_total.to_decimal() {
        return Err(Error::BusinessLogicError("商品及配送费用合计超出采购已批准金额，尚未发单".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use erp_supply::ports::connector::common::{ActionKey, SupplierSku};
    use erp_supply::ports::connector::offer::DeliveryContext;
    use erp_supply::ports::connector::order::{DeliveryChoice, DeliveryMethod, Recipient};

    use super::*;

    fn request(price: &str, count: usize, total: &str) -> CreateOrder {
        CreateOrder {
            action: ActionKey { id: "create".into(), payload_hash: "hash".into() },
            merchant_order_no: "group".into(),
            lines: (0..count)
                .map(|index| OrderLine {
                    source_purchase_terms: None,
                    line_id: format!("line-{index}"),
                    sku: SupplierSku { product_id: None, spec_id: format!("sku-{index}") },
                    quantity: "1".parse().unwrap(),
                    approved_unit_price: price.parse().unwrap(),
                    attributes: vec![],
                })
                .collect(),
            recipient: Recipient {
                name: "收件人".into(),
                phone: "13800000000".into(),
                region: "110105".into(),
                address: "某路".into(),
                geocoded_address: None,
                supplier_address: None,
            },
            delivery_context: DeliveryContext { reference: "confirmed".into(), supplier_address: None },
            delivery: DeliveryChoice {
                method: DeliveryMethod::Courier,
                shipping_fee: "0".parse().unwrap(),
                reference: "option".into(),
            },
            approved_total: total.parse().unwrap(),
        }
    }

    #[test]
    fn frozen_total_uses_the_same_per_line_cent_rounding_as_supplier_proof() {
        ensure_total(&request("0.3333", 1, "0.33")).unwrap();
        ensure_total(&request("0.335", 2, "0.68")).unwrap();
        assert!(ensure_total(&request("0.335", 2, "0.67")).is_err());
        let mut shipping = request("0.3333", 1, "0.34");
        shipping.delivery.shipping_fee = "0.01".parse().unwrap();
        ensure_total(&shipping).unwrap();
        shipping.delivery.shipping_fee = "0.02".parse().unwrap();
        assert!(ensure_total(&shipping).is_err());
    }
}
