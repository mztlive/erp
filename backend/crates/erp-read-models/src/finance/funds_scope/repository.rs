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

#[cfg(test)]
pub(super) mod tests {
    use erp_core::common::time::Instant;
    use erp_identity::entity::access_control::ResolvedScope;
    use erp_identity::service::access_control::resolve::AuthorizedDataScope;

    use super::super::FundsAuthorization;

    /// 构造已完成解析的空范围，查询合同测试只验证最终数据库条件，不执行资格 I/O。
    pub(super) fn authorization() -> FundsAuthorization {
        FundsAuthorization {
            sales: Default::default(),
            ledger_read: false,
            settlement: None,
            purchase_scope: None,
            no_scope: true,
            context: AuthorizedDataScope {
                user_id: "actor".into(),
                resource: "invoice".into(),
                action: "list".into(),
                scope: ResolvedScope { role_clauses: Vec::new(), user_limit: None },
                role_scopes: Default::default(),
                organizations: Default::default(),
                policy_version: 1,
                scope_version: "scope".into(),
                as_of: Instant::from_unix_secs(1),
            },
        }
    }
}
