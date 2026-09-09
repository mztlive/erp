//! 往来列表跨域关键词解析，财务关联与 Mongo 表达式归财务仓储。
use crate::Result;
use erp_finance::repository::keyword::{FinanceKeyword, FinanceKeywordRepository, FinanceSearchTarget};
use erp_party::PartyExt;
use erp_procurement::repository::PurchaseOrderExt;
use erp_sales::repository::SalesOrderExt;
use erp_supplier::SupplierExt;
use erp_supply::repository::SupplierSettlementExt;
use mongodb::Database;
use persistence_core::NoTransaction;

/// 空关键词不读取任何关联；非空关键词解析完整身份，不截断结果。
///
/// 名称、来源单号、财务关系任何一段失败时整次查询失败。
pub(super) async fn keyword_ids(
    db: &Database,
    q: Option<&str>,
    target: FinanceSearchTarget,
) -> Result<Option<Vec<String>>> {
    let Some(q) = application_core::normalized_text(q) else {
        return Ok(None);
    };
    let parties = db
        .party()
        .matching_current_party_ids_by_name(&q, &mut NoTransaction)
        .await?;
    let mut facts = FinanceKeyword {
        q,
        party_ids: parties.iter().map(ToString::to_string).collect(),
        ..Default::default()
    };
    if matches!(
        target,
        FinanceSearchTarget::Receivable | FinanceSearchTarget::Receipt | FinanceSearchTarget::SalesInvoice
    ) {
        facts.sales_order_ids = db
            .sales_orders()
            .matching_ids_by_number(&facts.q, &mut NoTransaction)
            .await?
            .iter()
            .map(ToString::to_string)
            .collect();
    } else {
        facts.supplier_ids = db
            .supplier_accounts()
            .matching_ids_by_parties(&parties, &mut NoTransaction)
            .await?
            .iter()
            .map(ToString::to_string)
            .collect();
        facts.purchase_order_ids = db
            .purchase_orders()
            .matching_ids_by_number(&facts.q, &mut NoTransaction)
            .await?;
        facts.statement_ids = db
            .supplier_settlement_statements()
            .matching_ids_by_number(&facts.q, &mut NoTransaction)
            .await?;
    }
    Ok(Some(
        FinanceKeywordRepository::new(db)
            .matching_ids(target, &facts, &mut NoTransaction)
            .await?,
    ))
}
