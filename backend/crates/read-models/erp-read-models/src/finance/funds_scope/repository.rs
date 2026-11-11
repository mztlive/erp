//! 资金列表在最终来源范围形成后执行数据库分页，完整轻量版本索引用于重验。

pub(super) mod accounts;
pub(super) mod allocation;
pub(super) mod flow;
pub(super) mod invoice;
pub(super) mod invoice_sources;
pub(super) mod invoice_summary;
mod management;
mod management_facts;
mod page;
pub(super) mod payment;
pub(super) mod receipt;
mod request;
mod source;

pub(super) mod payable_source;
pub(super) mod settlement_mapping;
pub(super) mod source_access;
pub(super) mod whole_access;
pub(super) mod whole_facts;
pub(super) mod whole_read;

pub(super) use page::{aggregate, page_facet, page_only_facet, sort_document};
pub(super) use source::{linked_condition_document, source_stages};

#[cfg(test)]
pub(super) mod tests {
    use erp_core::common::time::Instant;

    use super::super::{FundsAuthorization, FundsReadContext};
    pub(super) fn authorization() -> FundsAuthorization {
        FundsAuthorization {
            whole: Default::default(),
            sales: None,
            ledger_read: false,
            settlement: None,
            purchase_scope: None,

            context: FundsReadContext {
                resource: "invoice".into(),
                scope_version: "scope".into(),
                as_of: Instant::from_unix_secs(1),
            },
        }
    }

    /// 未登记销售来源时保留真实来源事实，但不得交付销售授权份额。
    #[test]
    fn missing_sales_source_preserves_facts_and_denies_shares() {
        use mongodb::bson::{Bson, doc};

        let authorization = authorization();
        assert!(matches!(authorization.sales_scope(), Err(crate::Error::Forbidden(_))));
        let stages = super::source_stages(
            "sales_orders",
            Bson::String("$sales_order_id".into()),
            "source",
            doc! {},
            "sales_owner_user_id",
            "order_no",
            &authorization.sales_stages(),
        );
        let lookup = stages[0].get_document("$lookup").unwrap();
        let pipeline = lookup.get_array("pipeline").unwrap();
        let facet = pipeline[1].as_document().unwrap().get_document("$facet").unwrap();
        let allowed = facet.get_array("allowed").unwrap();
        assert_eq!(allowed[0], Bson::Document(doc! { "$match": { "$expr": false } }));
        let facts = facet.get_array("facts").unwrap();
        assert_eq!(
            facts[0].as_document().unwrap().get_document("$project").unwrap().get_i32("id").unwrap(),
            1
        );
    }
}
