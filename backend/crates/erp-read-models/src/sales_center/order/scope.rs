//! 销售列表与候选的一致授权快照；范围与业务版本跨页携带。
use std::hash::{Hash, Hasher};

use application_core::{AuditActor, FilterOption, FilteredPage, PageView};
use erp_identity::AccessControlExt;
use erp_identity::repository::prelude::*;
use erp_identity::service::access_control::resolve::AuthorizedDataScope;
use erp_sales::dto::sales_order::SalesOrderListParams;
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use erp_sales::repository::sales_order::{SalesOrderRow, SalesOrderSearch};
use persistence_core::Transactional;
use serde::Serialize;

use super::SalesOrderReadService;
use super::dto::SalesOrderView;
use crate::sales_center::access::SalesAccess;
use crate::{Error, Result};

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
    ///
    /// # 参数
    /// * `params` - 原始列表查询，含组织筛选
    /// * `search` - 跨域关键词解析结果
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回授权快照、负责人候选和范围版本。
    ///
    /// # 错误
    /// 无动作权限、组织筛选非法、未知组织或查询超限时拒绝。
    ///
    /// # 关键业务约束
    /// 组织筛选按单据 `business_org_unit_id` 收窄；缺范围保持空集。
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
                    let access = SalesAccess::new(db.clone(), rbac);
                    let (mut context, scope) = access.resolve(&actor, "list", &[], executor).await?;
                    let org_ids = requested_business_org_units(&access, &params, executor).await?;
                    let (page, versions) =
                        erp_sales::service::sales_order::SalesOrderService::new(db.clone())
                            .list_rows(&params, search, &scope, org_ids, executor)
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
                    Ok(SalesSnapshot { no_scope, page, owner_options, context })
                })
            })
            .await
    }
}

/// 将请求中的组织筛选展开为单据业务组织条件。
///
/// # 参数
/// * `access` - 销售范围访问器
/// * `params` - 原始列表查询
/// * `executor` - 调用方执行器
///
/// # 返回
/// 无组织筛选时返回 `None`；否则返回已展开的组织 ID。
///
/// # 错误
/// 包含下级但未提供组织、未知组织或展开失败时拒绝。
///
/// # 关键业务约束
/// 组织筛选匹配当前 `business_org_unit_id`，不得改写历史归属快照。
async fn requested_business_org_units(
    access: &SalesAccess,
    params: &SalesOrderListParams,
    executor: &mut dyn persistence_core::Executor,
) -> Result<Option<Vec<String>>> {
    let Some(org_ids) = &params.org_unit_ids else {
        if params.include_descendants == Some(true) {
            return Err(Error::ValidationError("包含下级时必须提供组织筛选".into()));
        }
        return Ok(None);
    };
    if params.include_descendants == Some(true) && org_ids.as_slice().is_empty() {
        return Err(Error::ValidationError("包含下级时必须提供组织筛选".into()));
    }
    let expanded = access
        .expand_org_units(org_ids.as_slice(), params.include_descendants.unwrap_or(false), executor)
        .await?;
    Ok(Some(expanded.into_iter().collect()))
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
        let org: SalesListParams = serde_json::from_value(serde_json::json!({
            "org_unit_ids": "org-1,org-2",
            "include_descendants": true
        }))
        .unwrap();
        assert_eq!(org.org_unit_ids.unwrap().as_slice(), &["org-1".to_string(), "org-2".to_string()]);
        assert_eq!(org.include_descendants, Some(true));
    }
}
