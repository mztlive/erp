//! 往来列表跨域关键词解析，财务关联与 Mongo 表达式归财务仓储。
use application_core::normalized_text;
use erp_core::ids::{PartyId, SalesOrderId, SupplierAccountId};
use erp_finance::repository::keyword::{FinanceKeyword, FinanceKeywordRepository, FinanceSearchTarget};
use erp_party::PartyExt;
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::repository::prelude::*;
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use erp_supplier::SupplierExt;
use erp_supplier::repository::prelude::*;
use erp_supply::repository::SupplierSettlementExt;
use erp_supply::repository::prelude::*;
use mongodb::Database;
use persistence_core::NoTransaction;

use crate::{Error, Result};

/// 空关键词不读取任何关联；非空关键词解析完整身份，不截断结果。
///
/// 名称、来源单号、财务关系任何一段失败时整次查询失败。
///
/// # 参数
/// 数据库、可选关键词及需要解析的财务对象类型。
/// # 返回
/// 返回完整关键词命中身份；空关键词不限制列表。
/// # 错误
/// 关联读取按原名称、供应商、来源单号、财务关系顺序返回首个错误。
pub(super) async fn keyword_ids(
    db: &Database,
    q: Option<&str>,
    target: FinanceSearchTarget,
) -> Result<Option<Vec<String>>> {
    let Some(q) = normalized_text(q) else {
        return Ok(None);
    };
    let facts = if matches!(
        target,
        FinanceSearchTarget::Receivable | FinanceSearchTarget::Receipt | FinanceSearchTarget::SalesInvoice
    ) {
        sales_facts(db, q).await?
    } else {
        purchase_facts(db, q).await?
    };
    Ok(Some(FinanceKeywordRepository::new(db).matching_ids(target, &facts, &mut NoTransaction).await?))
}

/// 销售单号和主体名称使用各自非事务执行器并行读取，不共享可变执行器。
async fn sales_facts(db: &Database, q: String) -> Result<FinanceKeyword> {
    let (parties, orders) = tokio::join!(
        async { db.party().matching_current_party_ids_by_name(&q, &mut NoTransaction).await },
        async { db.sales_orders().matching_ids_by_number(&q, &mut NoTransaction).await },
    );
    sales_keyword(q, parties.map_err(Error::from), orders.map_err(Error::from))
}

/// 名称到供应商仍按依赖顺序读取，与采购及结算单号两个独立读取并行。
async fn purchase_facts(db: &Database, q: String) -> Result<FinanceKeyword> {
    let (parties, orders, statements) = tokio::join!(
        async {
            let parties = db.party().matching_current_party_ids_by_name(&q, &mut NoTransaction).await?;
            let suppliers =
                db.supplier_accounts().matching_ids_by_parties(&parties, &mut NoTransaction).await?;
            Ok::<_, Error>((parties, suppliers))
        },
        async { db.purchase_orders().matching_ids_by_number(&q, &mut NoTransaction).await },
        async { db.supplier_settlement_statements().matching_ids_by_number(&q, &mut NoTransaction).await },
    );
    purchase_keyword(q, parties, orders.map_err(Error::from), statements.map_err(Error::from))
}

/// 销售侧先解包主体名称错误，再解包单号错误，保留所有命中身份及原顺序。
fn sales_keyword(
    q: String,
    parties: Result<Vec<PartyId>>,
    orders: Result<Vec<SalesOrderId>>,
) -> Result<FinanceKeyword> {
    let party_ids = parties?.iter().map(ToString::to_string).collect();
    let sales_order_ids = orders?.iter().map(ToString::to_string).collect();
    Ok(FinanceKeyword { q, party_ids, sales_order_ids, ..Default::default() })
}

/// 采购侧先解包名称和供应商错误，再按原采购、结算顺序传播单号错误。
fn purchase_keyword(
    q: String,
    parties: Result<(Vec<PartyId>, Vec<SupplierAccountId>)>,
    orders: Result<Vec<String>>,
    statements: Result<Vec<String>>,
) -> Result<FinanceKeyword> {
    let (parties, suppliers) = parties?;
    let purchase_order_ids = orders?;
    let statement_ids = statements?;
    Ok(FinanceKeyword {
        q,
        party_ids: parties.iter().map(ToString::to_string).collect(),
        supplier_ids: suppliers.iter().map(ToString::to_string).collect(),
        purchase_order_ids,
        statement_ids,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 并行读取结果仍保留销售侧所有命中身份，不把空集合或大量结果解释为无筛选。
    #[test]
    fn sales_facts_keep_complete_ids_and_empty_matches() {
        let orders = (0..10_001).map(|index| SalesOrderId::new(index.to_string())).collect();
        let facts = sales_keyword("q".into(), Ok(vec![PartyId::new("party")]), Ok(orders)).unwrap();
        assert_eq!(facts.party_ids, ["party"]);
        assert_eq!(facts.sales_order_ids.len(), 10_001);
        assert_eq!(facts.sales_order_ids.last().map(String::as_str), Some("10000"));
        assert!(facts.purchase_order_ids.is_empty());
        let empty = sales_keyword("q".into(), Ok(vec![]), Ok(vec![])).unwrap();
        assert!(empty.party_ids.is_empty());
        assert!(empty.sales_order_ids.is_empty());
        assert_eq!(empty.q, "q");
    }

    /// 名称与单号同时失败时，错误优先级沿用原串行查询合同。
    #[test]
    fn sales_errors_keep_party_before_order_priority() {
        let error = sales_keyword(
            "q".into(),
            Err(Error::Internal("party".into())),
            Err(Error::Internal("order".into())),
        );
        assert!(matches!(error, Err(Error::Internal(message)) if message == "party"));
        let error = sales_keyword("q".into(), Ok(vec![]), Err(Error::Internal("order".into())));
        assert!(matches!(error, Err(Error::Internal(message)) if message == "order"));
    }

    /// 采购与结算单号始终保持独立来源身份，即使原始主键相同。
    #[test]
    fn purchase_facts_keep_supplier_and_typed_source_ids() {
        let facts = purchase_keyword(
            "q".into(),
            Ok((vec![PartyId::new("party")], vec![SupplierAccountId::new("supplier")])),
            Ok(vec!["same".into(), "second".into()]),
            Ok(vec!["same".into()]),
        )
        .unwrap();
        assert_eq!(facts.party_ids, ["party"]);
        assert_eq!(facts.supplier_ids, ["supplier"]);
        assert_eq!(facts.purchase_order_ids, ["same", "second"]);
        assert_eq!(facts.statement_ids, ["same"]);
        assert!(facts.sales_order_ids.is_empty());
    }

    /// 名称或供应商失败优先于采购；采购错误优先于结算，成功空集继续生成查询事实。
    #[test]
    fn purchase_errors_keep_original_priority() {
        for first_error in ["party", "supplier"] {
            let error = purchase_keyword(
                "q".into(),
                Err(Error::Internal(first_error.into())),
                Err(Error::Internal("purchase".into())),
                Err(Error::Internal("statement".into())),
            );
            assert!(matches!(error, Err(Error::Internal(message)) if message == first_error));
        }
        let error = purchase_keyword(
            "q".into(),
            Ok((vec![], vec![])),
            Err(Error::Internal("purchase".into())),
            Err(Error::Internal("statement".into())),
        );
        assert!(matches!(error, Err(Error::Internal(message)) if message == "purchase"));
        let error = purchase_keyword(
            "q".into(),
            Ok((vec![], vec![])),
            Ok(vec![]),
            Err(Error::Internal("statement".into())),
        );
        assert!(matches!(error, Err(Error::Internal(message)) if message == "statement"));
        let empty = purchase_keyword("q".into(), Ok((vec![], vec![])), Ok(vec![]), Ok(vec![])).unwrap();
        assert!(empty.party_ids.is_empty());
        assert!(empty.supplier_ids.is_empty());
        assert!(empty.purchase_order_ids.is_empty());
        assert!(empty.statement_ids.is_empty());
    }
}
