//! 合同列表查询编排，所有筛选与排序先于分页。

use super::ContractService;
use crate::dto::contract::{
    ContractListParams, ContractListQuery, ContractListView, ContractRevisionView, ContractView, PageView,
    SortDir,
};
use crate::error::Result;
use crate::repository::list_search::{ContractCustomer, ContractSearch};
use crate::repository::{ContractExt, ContractFilter, ContractRow};
use erp_core::common::time::BusinessDate;
use erp_core::ids::CustomerAccountId;
use persistence_core::NoTransaction;
use std::collections::HashMap;
use validator::Validate;

impl ContractService {
    /// 分页查询合同并返回当前可见范围的指标和筛选候选项。
    ///
    /// # 参数
    /// * `params` - 关键词、结构化筛选与分页排序
    /// * `actor_user_id` - 当前用户，用于 assigned 客户范围
    ///
    /// # 错误
    /// 参数非法、外域事实读取或数据库查询失败。
    ///
    /// # 约束
    /// 无可见客户时保持空集合，禁止回退到全量合同。
    pub async fn contract_list(
        &self,
        params: &ContractListParams,
        actor_user_id: &str,
    ) -> Result<ContractListView> {
        params.validate()?;
        let query = params.normalized()?;
        let customer_ids = self
            .visible_customer_ids(query.scope, query.customer_id.clone(), actor_user_id)
            .await?;
        let filter = list_filter(&query, customer_ids);
        let search = self.list_search(&query, &filter).await?;
        let result = self
            .db
            .contract()
            .search_list(&filter, &search, &mut NoTransaction)
            .await?;
        let total = result.total();
        let items = self.contract_rows(result.items, &search.customers).await?;
        Ok(ContractListView {
            ownership_basis: "current_customer_owner",
            page: PageView {
                items,
                total,
                page: filter.page,
                page_size: filter.page_size,
            },
            metrics: result.metrics.into_iter().next().unwrap_or_default(),
            settlement_options: result.settlement_options,
            owner_options: self
                .accounts
                .filter_options(
                    &search
                        .customers
                        .iter()
                        .filter_map(|c| c.owner_id.clone())
                        .collect::<Vec<_>>(),
                )
                .await?
                .into_iter()
                .map(|o| crate::dto::contract::ContractFilterOption {
                    value: o.value,
                    label: o.label,
                })
                .collect(),
        })
    }

    /// 只解析可见合同引用的客户事实，跨域读取继续通过既有窄端口。
    async fn list_search(
        &self,
        query: &ContractListQuery,
        filter: &ContractFilter,
    ) -> Result<ContractSearch> {
        let ids = self
            .db
            .contract()
            .list_customer_ids(filter, &mut NoTransaction)
            .await?;
        Ok(ContractSearch {
            q: query.q.clone(),
            metric: query.metric,
            settlement_party_id: query.settlement_party_id.clone(),
            owner_user_ids: query.owner_user_ids.clone(),
            customers: self.list_customer_facts(&ids).await?,
        })
    }

    /// 按客户批量取得编号与当前负责人，同一批事实供搜索、排序与显示使用。
    pub(super) async fn list_customer_facts(&self, ids: &[String]) -> Result<Vec<ContractCustomer>> {
        let customer_ids = ids
            .iter()
            .cloned()
            .map(CustomerAccountId::new)
            .collect::<Vec<_>>();
        let customers = self.customers.find_by_ids(&customer_ids).await?;
        let owners = self
            .assignments
            .owner_user_ids_by_customer(ids, BusinessDate::today())
            .await?;
        let names = self
            .accounts
            .names_by_ids(&owners.values().cloned().collect::<Vec<_>>())
            .await?;
        let numbers = customers
            .into_iter()
            .map(|c| (c.id, c.customer_no))
            .collect::<HashMap<_, _>>();
        Ok(ids
            .iter()
            .map(|id| ContractCustomer {
                id: id.clone(),
                number: numbers.get(id).cloned().unwrap_or_default(),
                owner_id: owners.get(id).cloned(),
                owner: owner_label(owners.get(id), &names),
            })
            .collect())
    }

    /// 按结果页批量读取不可变当前修订，避免重复读取客户与负责人。
    async fn contract_rows(
        &self,
        rows: Vec<ContractRow>,
        customers: &[ContractCustomer],
    ) -> Result<Vec<ContractView>> {
        let revision_ids = rows
            .iter()
            .filter_map(|row| row.current_revision_id.clone())
            .collect::<Vec<_>>();
        let revisions = self
            .db
            .contract_revisions()
            .find_by_ids(&revision_ids, &mut NoTransaction)
            .await?;
        let mut revisions = revisions
            .into_iter()
            .map(|r| (r.base.id.clone(), ContractRevisionView::from(r)))
            .collect::<HashMap<_, _>>();
        let customers = customers
            .iter()
            .map(|c| (c.id.as_str(), c))
            .collect::<HashMap<_, _>>();
        Ok(rows
            .into_iter()
            .map(|row| {
                let customer = customers.get(row.customer_id.as_str()).copied();
                let revision = row
                    .current_revision_id
                    .as_ref()
                    .and_then(|id| revisions.remove(id));
                contract_view(row, revision, customer)
            })
            .collect())
    }
}

/// 负责人空显示名回退稳定用户 ID；未分配显示破折号。
fn owner_label(owner: Option<&String>, names: &HashMap<String, String>) -> String {
    owner
        .map(|id| {
            names
                .get(id)
                .map(|name| name.trim())
                .filter(|name| !name.is_empty())
                .unwrap_or(id)
                .to_string()
        })
        .unwrap_or_else(|| "—".to_string())
}

/// 同一客户事实供接口显示与搜索使用，防止搜索命中后展示另一套名称。
fn contract_view(
    row: ContractRow,
    current_revision: Option<ContractRevisionView>,
    customer: Option<&ContractCustomer>,
) -> ContractView {
    ContractView {
        id: row.id,
        contract_no: row.contract_no,
        customer_id: row.customer_id,
        settlement_party_id: row.settlement_party_id,
        status: row.status,
        current_revision_id: row.current_revision_id,
        current_revision,
        customer_no: customer
            .map(|c| c.number.clone())
            .filter(|number| !number.is_empty()),
        owner_user_id: customer.and_then(|c| c.owner_id.clone()),
        owner_user_name: customer.filter(|c| c.owner_id.is_some()).map(|c| c.owner.clone()),
        created_at: row.created_at,
        version: row.version,
    }
}

/// 归一化查询与权限客户集合求交后的仓储条件；空集合必须保留。
fn list_filter(query: &ContractListQuery, customer_ids: Option<Vec<String>>) -> ContractFilter {
    ContractFilter {
        contract_no: query.contract_no.clone(),
        customer_id: None,
        customer_ids,
        status: query.status,
        page: query.paging.page,
        page_size: query.paging.page_size,
        sort_by: Some(query.paging.sort_by.to_string()),
        sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
    }
}
