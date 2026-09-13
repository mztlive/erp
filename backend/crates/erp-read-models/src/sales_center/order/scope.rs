//! 销售列表与候选的一致授权快照；范围与业务版本跨页携带。
use super::{dto::SalesOrderView, SalesOrderReadService};
use crate::{sales_center::access::SalesAccess, Error, Result};
use application_core::{AuditActor, FilterOption, FilteredPage, PageView};
use erp_identity::{service::access_control::resolve::AuthorizedDataScope, AccessControlExt};
use erp_sales::{
    dto::sales_order::SalesOrderListParams,
    repository::{
        sales_order::{SalesOrderRow, SalesOrderSearch},
        SalesOrderExt,
    },
};
use persistence_core::Transactional;
use serde::Serialize;
use std::hash::{Hash, Hasher};

/// 查询直接复用领域 DTO，scope_version 与原有数值参数均在 URL 边界解码。
pub type SalesListParams = SalesOrderListParams;
/// 列表响应保持现有字段并声明独立的授权时点及版本。
#[derive(Serialize)]
pub struct SalesListView {
    #[serde(flatten)]
    pub data: FilteredPage<SalesOrderView>,
    pub scope_version: String,
    pub policy_version: u64,
    pub organization_version: u64,
    pub as_of: String,
    pub empty_reason: Option<&'static str>,
    pub scope_summary: &'static str,
}

pub(super) struct SalesSnapshot {
    pub page: PageView<SalesOrderRow>,
    pub owner_options: Vec<FilterOption>,
    pub context: AuthorizedDataScope,
    pub no_scope: bool,
}
impl SalesOrderReadService {
    /// 授权、总数、候选与业务身份版本全部在同一个事务读取。
    pub(super) async fn list_snapshot(
        &self,
        params: &SalesOrderListParams,
        search: SalesOrderSearch,
        actor: &AuditActor,
    ) -> Result<SalesSnapshot> {
        let db = self.db.clone();
        let rbac = self.require_rbac()?.clone();
        let params = params.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let (mut context, scope) = SalesAccess::new(db.clone(), rbac)
                        .resolve(&actor, "list", &[], executor)
                        .await?;
                    let (page, versions) =
                        erp_sales::service::sales_order::SalesOrderService::new(db.clone())
                            .list_rows(&params, search, &scope, executor)
                            .await?;
                    let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
                    versions.hash(&mut fingerprint);
                    context.scope_version = format!("{}:{:x}", context.scope_version, fingerprint.finish());
                    let ids = db.sales_orders().current_owner_ids(&scope, executor).await?;
                    if ids.len() > 10_000 {
                        return Err(Error::ValidationError("负责人候选超过查询上限".into()));
                    }
                    let owner_options = db.accounts().filter_options(&ids, executor).await?;
                    let no_scope = scope.is_empty();
                    Ok(SalesSnapshot {
                        no_scope,
                        page,
                        owner_options,
                        context,
                    })
                })
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scope_version_is_consumed_and_unsupported_owner_alias_is_rejected() {
        let query: SalesListParams = serde_json::from_value(
            serde_json::json!({"page": 2, "scope_version": "v1", "owner_user_ids": "a,b"}),
        )
        .unwrap();
        assert_eq!(query.scope_version.as_deref(), Some("v1"));
        assert!(serde_json::from_value::<SalesListParams>(serde_json::json!({"owner": "张三"})).is_err());
    }
}
