use erp_catalog::entity::catalog::UnitOfMeasure;
use erp_catalog::repository::CatalogExt;
use erp_procurement::entity::purchase_order::PurchaseOrderRevisionLine;
use erp_procurement::repository::PurchaseOrderExt;
use erp_supply::api_order::ApiOrderExecution;
use erp_supply::entity::supplier_offering::OfferingStatus;
use erp_supply::ports::connector::common::Lookup;
use erp_supply::ports::connector::offer::{
    AvailabilityQuery, AvailabilitySource, AvailabilityState, AvailableQuantity, OfferSource,
};
use erp_supply::ports::connector::order::CreateOrder;
use erp_supply::repository::SupplierOfferingExt;
use persistence_core::NoTransaction;

use super::SupplierApiOrderProcess;
use crate::connectors::dangaoshushu::DangaoshushuConnector;
use crate::{Error, Result};

impl SupplierApiOrderProcess {
    pub(super) async fn preflight(
        &self,
        execution: &ApiOrderExecution,
        connector: &DangaoshushuConnector,
        request: &CreateOrder,
    ) -> Result<()> {
        let query = AvailabilityQuery {
            skus: request.lines.iter().map(|line| line.sku.clone()).collect(),
            region: Some(request.recipient.region.clone()),
        };
        let available = connector
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
            let Lookup::Found(offer) = connector
                .offer(&line.sku)
                .await
                .map_err(|failure| Error::BusinessLogicError(failure.summary))?
            else {
                return Err(Error::BusinessLogicError("供应商当前未返回原订货规格，尚未发单".into()));
            };
            let quote = offer
                .value
                .quote
                .and_then(|quote| quote.dropship)
                .ok_or_else(|| Error::BusinessLogicError("供应商当前采购报价未核实，尚未发单".into()))?;
            if quote.unit_price != line.approved_unit_price || line.quantity < quote.minimum_quantity {
                return Err(Error::BusinessLogicError(
                    "供应商当前报价或起订量与批准条件不符，尚未发单".into(),
                ));
            }
            self.ensure_frozen_unit(execution, &line.line_id, offer.value.unit.as_deref()).await?;
        }
        ensure_total(request)
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
        sum.checked_add(
            line.quantity
                .to_decimal()
                .checked_mul(line.approved_unit_price.to_decimal())
                .ok_or_else(|| Error::ValidationError("接口采购金额超限".into()))?,
        )
        .ok_or_else(|| Error::ValidationError("接口采购金额超限".into()))
    })?;
    if total > request.approved_total.to_decimal() {
        return Err(Error::BusinessLogicError("商品及配送费用合计超出采购已批准金额，尚未发单".into()));
    }
    Ok(())
}
