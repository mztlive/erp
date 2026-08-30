//! 合同列表查询编排。

use std::collections::HashMap;

use database::{AccessControlExt, ContractExt, CustomerExt, NoTransaction};
use entities::common::time::BusinessDate;
use entities::customer::AssignmentRole;
use entities::ids::CustomerAccountId;
use validator::Validate;

use crate::errors::Result;

use super::dto::{ContractListParams, ContractRevisionView, ContractView, PageView};
use super::ContractService;

/// 合同列表筛选条件类型（经 `ContractExt` 关联类型跨 crate 可达）。
type ContractFilter = <mongodb::Database as ContractExt>::ContractFilter;

impl ContractService {
    /// 分页查询合同列表。
    ///
    /// 排序字段白名单在 Service 层校验（api-contract §4），禁止任意字段透传。
    /// `scope=assigned` 时按当前用户有效归属客户收窄，避免选择器先露出再在提交时 403。
    ///
    /// # 参数
    /// * `params` - 查询参数
    /// * `actor_user_id` - 当前登录用户 ID，用于 `assigned` 范围
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    ///
    /// # 约束
    /// `scope=assigned` 且当前用户无有效归属时返回空页，不回退为全量合同。
    pub async fn contract_list(
        &self,
        params: &ContractListParams,
        actor_user_id: &str,
    ) -> Result<PageView<ContractView>> {
        params.validate()?;
        let query = params.normalized()?;
        let customer_ids = self
            .visible_customer_ids(query.scope, query.customer_id.clone(), actor_user_id)
            .await?;
        let filter = list_filter(&query, customer_ids);
        let page = self
            .db
            .contracts()
            .search_contracts(&filter, &mut NoTransaction)
            .await?;
        let rows = page.items;
        let revision_ids = rows
            .iter()
            .filter_map(|row| row.current_revision_id.clone())
            .collect::<Vec<_>>();
        let revisions = self
            .db
            .contract_revisions()
            .find_by_ids(&revision_ids, &mut NoTransaction)
            .await?;
        let mut revisions_by_id = revisions
            .into_iter()
            .map(|revision| (revision.base.id.clone(), ContractRevisionView::from(revision)))
            .collect::<HashMap<_, _>>();
        let customer_ids = rows
            .iter()
            .map(|row| CustomerAccountId::new(row.customer_id.clone()))
            .collect::<Vec<_>>();
        let customers = self
            .db
            .customer_accounts()
            .find_accounts_by_ids(&customer_ids, &mut NoTransaction)
            .await?;
        let customers_by_id = customers
            .into_iter()
            .map(|customer| (customer.base.id.clone(), customer))
            .collect::<HashMap<_, _>>();
        let customer_id_strings = customer_ids.iter().map(ToString::to_string).collect::<Vec<_>>();
        let assignments = self
            .db
            .customer_assignments()
            .list_active_for_customers(&customer_id_strings, BusinessDate::today(), &mut NoTransaction)
            .await?;
        let owners_by_customer = assignments
            .into_iter()
            .filter(|assignment| assignment.assignment_role == AssignmentRole::Owner)
            .map(|assignment| (assignment.customer_id.to_string(), assignment.user_id))
            .collect::<HashMap<_, _>>();
        let owner_ids = owners_by_customer.values().cloned().collect::<Vec<_>>();
        let owner_names = self
            .db
            .accounts()
            .names_by_ids(&owner_ids, &mut NoTransaction)
            .await?;

        Ok(PageView {
            items: rows
                .into_iter()
                .map(|row| {
                    let current_revision = row
                        .current_revision_id
                        .as_ref()
                        .and_then(|id| revisions_by_id.remove(id));
                    let customer_no = customers_by_id
                        .get(&row.customer_id)
                        .map(|customer| customer.customer_no.clone());
                    let owner_user_id = owners_by_customer.get(&row.customer_id).cloned();
                    let owner_user_name = owner_user_id.as_ref().and_then(|id| owner_names.get(id)).cloned();
                    ContractView {
                        id: row.id,
                        contract_no: row.contract_no,
                        customer_id: row.customer_id,
                        settlement_party_id: row.settlement_party_id,
                        status: row.status,
                        current_revision_id: row.current_revision_id,
                        current_revision,
                        customer_no,
                        owner_user_id,
                        owner_user_name,
                        created_at: row.created_at,
                        version: row.version,
                    }
                })
                .collect(),
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }
}

/// 把归一化查询与可见客户集合转成仓储筛选条件。
///
/// # 参数
/// * `query` - 已校验的列表查询
/// * `customer_ids` - 可见客户；`None` 表示不按客户过滤
///
/// # 返回
/// 返回仓储筛选条件。
///
/// # 错误
/// 无。
///
/// # 约束
/// 客户范围已在 `visible_customer_ids` 求交，这里不再读取 `query.customer_id`。
fn list_filter(query: &super::dto::ContractListQuery, customer_ids: Option<Vec<String>>) -> ContractFilter {
    ContractFilter {
        contract_no: query.contract_no.clone(),
        customer_id: None,
        customer_ids,
        status: query.status,
        page: query.paging.page,
        page_size: query.paging.page_size,
        sort_by: Some(query.paging.sort_by.to_string()),
        sort_ascending: matches!(query.paging.sort_dir, super::dto::SortDir::Asc),
    }
}
