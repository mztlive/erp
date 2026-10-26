use erp_core::common::time::BusinessDate;
use erp_core::money::{Amount, round_to_cent};
use erp_procurement::entity::purchase_order::PurchaseOrderRevisionLine;
use erp_procurement::repository::PurchaseOrderExt;
use erp_supply::api_order::{ApiOrderExecution, ApiOrderSourceLine};
use erp_supply::ports::connector::common::SupplierSku;
use erp_supply::ports::connector::offer::DeliveryContext;
use erp_supply::ports::connector::order::{DeliveryChoice, OrderLine, Recipient};
use erp_supply::repository::SupplierOfferingExt;
use persistence_core::NoTransaction;
use serde::{Deserialize, Serialize};

use super::SupplierApiOrderProcess;
use crate::{Error, Result};

/// 协议请求及配送令牌只在加密边界内序列化，禁止 Debug。
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct ExecutionSnapshot {
    pub recipient: Recipient,
    pub lines: Vec<OrderLine>,
    pub approved_total: Amount,
    pub expected_delivery_date: Option<BusinessDate>,
    pub context: Option<DeliveryContext>,
    pub choices: Vec<DeliveryChoice>,
    pub selected_choice: Option<usize>,
    pub actual_payable: Option<Amount>,
    pub requires_payment: bool,
}

impl SupplierApiOrderProcess {
    pub(super) async fn build_snapshot(&self, execution: &ApiOrderExecution) -> Result<ExecutionSnapshot> {
        let mut lines = Vec::new();
        let mut recipient = None;
        let mut expected_delivery_date = None;
        for source in &execution.source_lines {
            let line = self
                .db
                .purchase_order_revision_lines()
                .find_by_id(&source.purchase_order_revision_line_id, &mut NoTransaction)
                .await?
                .filter(|line| {
                    line.purchase_order_revision_id.as_ref() == execution.purchase_order_revision_id
                })
                .ok_or_else(|| Error::NotFound("冻结采购行不存在".into()))?;
            let value = if source.delivery_allocation_index == u32::MAX {
                self.warehouse_recipient(execution).await?
            } else {
                recipient_for(&line, source.delivery_allocation_index)?
            };
            recipient.get_or_insert(value);
            expected_delivery_date = line.expected_delivery_date;
            lines.push(self.order_line(execution, source, &line).await?);
        }
        let total = lines.iter().try_fold(rust_decimal::Decimal::ZERO, |sum, line| {
            sum.checked_add(
                line.quantity
                    .to_decimal()
                    .checked_mul(line.approved_unit_price.to_decimal())
                    .ok_or_else(|| Error::ValidationError("采购金额超出范围".into()))?,
            )
            .ok_or_else(|| Error::ValidationError("采购金额超出范围".into()))
        })?;
        Ok(ExecutionSnapshot {
            recipient: recipient.ok_or_else(|| Error::BusinessLogicError("采购执行无明细".into()))?,
            lines,
            approved_total: Amount::try_from(round_to_cent(total))
                .map_err(|_| Error::ValidationError("采购金额无效".into()))?,
            expected_delivery_date,
            context: None,
            choices: Vec::new(),
            selected_choice: None,
            actual_payable: None,
            requires_payment: false,
        })
    }

    async fn order_line(
        &self,
        execution: &ApiOrderExecution,
        source: &ApiOrderSourceLine,
        line: &PurchaseOrderRevisionLine,
    ) -> Result<OrderLine> {
        let frozen = line
            .supplier_offering_source
            .as_ref()
            .ok_or_else(|| Error::BusinessLogicError("采购行未冻结供给".into()))?;
        let offering = self
            .db
            .supplier_offerings()
            .find_by_id(frozen.supplier_offering_id.as_ref(), &mut NoTransaction)
            .await?
            .filter(|offering| {
                offering.supplier_id.as_ref() == execution.supplier_id
                    && offering
                        .source_connection_id
                        .as_ref()
                        .is_some_and(|id| id.as_ref() == execution.connection_id)
            })
            .ok_or_else(|| Error::BusinessLogicError("采购供给与接口连接不一致".into()))?;
        Ok(OrderLine {
            line_id: source.fulfillment_item_id.clone(),
            sku: SupplierSku {
                product_id: offering.supplier_product_code,
                spec_id: offering.supplier_sku_code,
            },
            quantity: source.quantity,
            approved_unit_price: line
                .unit_cost_gross
                .ok_or_else(|| Error::BusinessLogicError("采购单价缺失".into()))?,
            attributes: Vec::new(),
        })
    }

    pub(super) fn decode(&self, execution: &ApiOrderExecution) -> Result<ExecutionSnapshot> {
        let encrypted = execution
            .request_encrypted
            .as_ref()
            .ok_or_else(|| Error::BusinessLogicError("接口配送资料尚未建立".into()))?;
        serde_json::from_str(&self.codec.decrypt(encrypted)?)
            .map_err(|_| Error::Internal("接口请求快照无法读取".into()))
    }

    pub(super) fn encode(
        &self,
        execution: &mut ApiOrderExecution,
        snapshot: &ExecutionSnapshot,
    ) -> Result<()> {
        let text =
            serde_json::to_string(snapshot).map_err(|_| Error::Internal("接口请求快照无法保存".into()))?;
        execution.request_encrypted = Some(self.codec.encrypt(&text)?);
        Ok(())
    }

    async fn warehouse_recipient(&self, execution: &ApiOrderExecution) -> Result<Recipient> {
        use erp_warehouse::WarehouseExt;
        let id = execution.warehouse_revision_id.as_ref().ok_or_else(|| {
            Error::BusinessLogicError("采购未冻结有效收货仓资料，请通过正式采购变更补齐交付安排".into())
        })?;
        let revision = self
            .db
            .warehouse_revisions()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("收货仓冻结修订不存在".into()))?;
        let region = revision
            .region_code
            .ok_or_else(|| Error::BusinessLogicError("收货仓未维护地区代码，请先完善仓库资料".into()))?;
        Ok(Recipient {
            name: self.codec.decrypt(revision.contact.encrypted())?,
            phone: String::new(),
            region: region.as_str().into(),
            address: format!("{}{}", region.full_name(), self.codec.decrypt(revision.address.encrypted())?),
            geocoded_address: None,
        })
    }
}

fn recipient_for(line: &PurchaseOrderRevisionLine, index: u32) -> Result<Recipient> {
    let index = usize::try_from(index).map_err(|_| Error::ValidationError("采购地址序号无效".into()))?;
    let row = line.delivery_allocations.get(index).ok_or_else(|| {
        Error::BusinessLogicError("采购未冻结接口收货地址，请通过正式采购变更补齐交付安排".into())
    })?;
    Ok(Recipient {
        name: row.recipient.clone(),
        phone: row.phone.clone(),
        region: row.region_code.as_str().into(),
        address: row.full_address(),
        geocoded_address: None,
    })
}
