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
            sales: Default::default(),
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
}
