//! 采购首次正式化逐行确认成本的财务单域构造与写入。
use std::str::FromStr;

use erp_core::common::time::Instant;
use erp_core::ids::{CostEntryId, SupplierAccountId};
use erp_core::money::{Amount, Rate};
use id_generator::next_id;
use persistence_core::Executor;

use crate::Result;
use crate::entity::cost::CostEntry;
use crate::repository::CostExt;
/// 财务从采购冻结提交逐行消费的成本事实。
pub struct PurchaseCostLine {
    /// 原采购提交行身份。
    pub id: String,
    /// 该行是否物流费用。
    pub is_logistics: bool,
    /// 冻结含税金额。
    pub gross_amount: Amount,
    /// 冻结不含税金额。
    pub net_amount: Amount,
    /// 冻结税额。
    pub tax_amount: Amount,
    /// 冻结进项税率；缺省使用原零税率。
    pub input_tax_rate: Option<Rate>,
}
/// 按采购提交原行序分配成本身份与发生时间，任一成本实体构造失败立即返回。
pub fn prepare(
    order_id: &str,
    supplier_id: &SupplierAccountId,
    lines: &[PurchaseCostLine],
    revision_no: u32,
) -> Result<Vec<CostEntry>> {
    let mut entries = Vec::new();
    for line in lines {
        let tax_rate = line.input_tax_rate.unwrap_or_else(zero_rate);
        entries.push(crate::entity::cost::CostEntry::new(
            CostEntryId::new(next_id()),
            crate::entity::cost::CostEntryData {
                cost_type: if line.is_logistics {
                    crate::entity::cost::CostType::Logistics
                } else {
                    crate::entity::cost::CostType::Product
                },
                cost_stage: crate::entity::cost::CostStage::Confirmed,
                cost_scope: crate::entity::cost::CostScope::NonVoucherFulfillment,
                cost_basis: None,
                supplier_id: Some(supplier_id.clone()),
                gross_amount: line.gross_amount,
                net_amount: line.net_amount,
                tax_amount: line.tax_amount,
                tax_inclusion: true,
                input_tax_rate: tax_rate,
                occurred_at: Instant::now(),
                source_fact_type: "purchase_order".to_string(),
                source_document_id: order_id.to_string(),
                source_line_id: line.id.clone(),
                source_version: revision_no.to_string(),
                adjusts_cost_entry_id: None,
                evidence_attachment_id: None,
            },
        )?);
    }
    Ok(entries)
}
fn zero_rate() -> Rate {
    Rate::from_str("0").expect("零税率合法")
}
/// 逐行写入原确认成本，复用调用方事务并保留首个失败。
pub async fn persist(
    db: &mongodb::Database,
    entries: &[CostEntry],
    executor: &mut dyn Executor,
) -> Result<()> {
    for entry in entries {
        db.cost().create_cost_entry_with_allocations(entry, Vec::new(), executor).await?;
    }
    Ok(())
}
