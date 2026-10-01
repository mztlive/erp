//! 资金列表在最终来源范围形成后执行数据库分页，完整轻量版本索引用于重验。

pub(super) mod accounts;
pub(super) mod allocation;
pub(super) mod flow;
pub(super) mod invoice;
pub(super) mod invoice_sources;
pub(super) mod invoice_summary;
mod page;
pub(super) mod payment;
pub(super) mod receipt;
mod source;

pub(super) use page::{aggregate, page_facet, page_only_facet, sort_document};
pub(super) use source::{linked_condition_document, source_scope_document, source_stages};
