//! 合同列表查询编排，所有筛选与排序先于分页。

use std::collections::HashMap;

use application_core::AuditActor;
use erp_core::common::time::BusinessDate;
use erp_core::ids::CustomerAccountId;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction};
use validator::Validate;

use super::ContractService;
use super::scope::{ensure_page, ensure_scope_version, ensure_stable_snapshot, to_list_view};
use crate::dto::contract::{ContractListParams, ContractListView, ContractRevisionView, ContractView};
use crate::error::Result;
use crate::ports::{AccountNamePort, CustomerAssignmentFactsPort, CustomerFactsPort};
use crate::repository::list_search::ContractCustomer;
use crate::repository::{ContractExt, ContractRow};

impl ContractService {
    /// 分页查询合同并返回当前可见范围的指标和筛选候选项。
    ///
    /// # 参数
    /// * `params` - 关键词、结构化筛选、组织与范围版本
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回分页视图及 `scope_summary`、`as_of` 与权限／组织／范围版本。
    ///
    /// # 错误
    /// 参数非法、跨页缺版本、范围变化、无 list 动作或仓储失败。
    ///
    /// # 关键业务约束
    /// 角色无有效范围返回空集并标记 `no_scope`；不得用公司范围兜底。
    pub async fn contract_list(
        &self,
        params: &ContractListParams,
        actor: &AuditActor,
    ) -> Result<ContractListView> {
        params.validate()?;
        let query = params.normalized()?;
        ensure_page(query.paging.page, params.scope_version.as_deref())?;
        let snapshot = self.list_snapshot(params, query.clone(), actor).await?;
        ensure_scope_version(params.scope_version.as_deref(), &snapshot.context.scope_version)?;
        let current = self.list_snapshot(params, query, actor).await?;
        ensure_stable_snapshot(&snapshot.context.scope_version, &current.context.scope_version)?;
        Ok(to_list_view(snapshot))
    }

    /// 按客户批量取得编号与当前负责人，同一批事实供搜索、排序与显示使用。
    ///
    /// # 参数
    /// * `ids` - 可见合同引用的客户
    ///
    /// # 返回
    /// 返回客户编号与当前主负责人。
    ///
    /// # 错误
    /// 外域事实读取失败。
    ///
    /// # 关键业务约束
    /// 当前负责人只取客户当前主负责人，不得用签约经办兜底。
    pub(super) async fn list_customer_facts(&self, ids: &[String]) -> Result<Vec<ContractCustomer>> {
        list_customer_facts_with(
            self.customers.as_ref(),
            self.assignments.as_ref(),
            self.accounts.as_ref(),
            ids,
            BusinessDate::today(),
            &mut NoTransaction,
        )
        .await
    }
}

/// 在指定执行器上读取客户编号与当前主负责人。
///
/// # 参数
/// * `customers` - 客户编号事实
/// * `assignments` - 客户归属 Port
/// * `accounts` - 账号显示名 Port
/// * `ids` - 客户 ID
/// * `as_of` - 归属自然日
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回列表搜索所需客户事实。
///
/// # 错误
/// 外域读取失败。
///
/// # 关键业务约束
/// 未分配主责时显示破折号，不得回填创建人或签约人。
pub(super) async fn list_customer_facts_with(
    customers: &dyn CustomerFactsPort,
    assignments: &dyn CustomerAssignmentFactsPort,
    accounts: &dyn AccountNamePort,
    ids: &[String],
    as_of: BusinessDate,
    executor: &mut dyn Executor,
) -> Result<Vec<ContractCustomer>> {
    let customer_ids = ids.iter().cloned().map(CustomerAccountId::new).collect::<Vec<_>>();
    let facts = customers.find_by_ids(&customer_ids).await?;
    let owners = assignments.owner_user_ids_by_customer(ids, as_of, executor).await?;
    let names = accounts.names_by_ids(&owners.values().cloned().collect::<Vec<_>>()).await?;
    let numbers = facts.into_iter().map(|c| (c.id, c.customer_no)).collect::<HashMap<_, _>>();
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
///
/// # 参数
/// * `db` - 合同数据库
/// * `rows` - 当前页合同行
/// * `customers` - 同一批客户事实
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回装配后的列表视图。
///
/// # 错误
/// 修订读取失败。
///
/// # 关键业务约束
/// 当前负责人来自客户事实，不读取合同创建人或签订日期作为负责人。
pub(super) async fn contract_rows(
    db: &Database,
    rows: Vec<ContractRow>,
    customers: &[ContractCustomer],
    executor: &mut dyn Executor,
) -> Result<Vec<ContractView>> {
    let revision_ids = rows.iter().filter_map(|row| row.current_revision_id.clone()).collect::<Vec<_>>();
    let revisions = db.contract_revisions().find_by_ids(&revision_ids, executor).await?;
    let mut revisions = revisions
        .into_iter()
        .map(|r| (r.base.id.clone(), ContractRevisionView::from(r)))
        .collect::<HashMap<_, _>>();
    let customers = customers.iter().map(|c| (c.id.as_str(), c)).collect::<HashMap<_, _>>();
    Ok(rows
        .into_iter()
        .map(|row| {
            let customer = customers.get(row.customer_id.as_str()).copied();
            let revision = row.current_revision_id.as_ref().and_then(|id| revisions.remove(id));
            contract_view(row, revision, customer)
        })
        .collect())
}

/// 负责人空显示名回退稳定用户 ID；未分配显示破折号。
///
/// # 参数
/// * `owner` - 当前主负责人 ID
/// * `names` - 账号显示名
///
/// # 返回
/// 返回展示标签。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 不得用签约经办姓名填充未分配主责。
fn owner_label(owner: Option<&String>, names: &HashMap<String, String>) -> String {
    owner
        .map(|id| {
            names.get(id).map(|name| name.trim()).filter(|name| !name.is_empty()).unwrap_or(id).to_string()
        })
        .unwrap_or_else(|| "—".to_string())
}

/// 同一客户事实供接口显示与搜索使用，防止搜索命中后展示另一套名称。
///
/// # 参数
/// * `row` - 合同列表行
/// * `current_revision` - 当前修订摘要
/// * `customer` - 客户编号与当前主负责人
///
/// # 返回
/// 返回列表视图。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// `owner_user_id` 只来自客户当前主负责人。
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
        customer_no: customer.map(|c| c.number.clone()).filter(|number| !number.is_empty()),
        owner_user_id: customer.and_then(|c| c.owner_id.clone()),
        owner_user_name: customer.filter(|c| c.owner_id.is_some()).map(|c| c.owner.clone()),
        created_at: row.created_at,
        version: row.version,
    }
}

#[cfg(test)]
mod tests {
    use super::super::scope::{ensure_page, ensure_scope_version, ensure_stable_snapshot};
    use crate::error::Error;

    #[test]
    fn later_page_and_version_drift_are_data_scope_changed() {
        match ensure_page(3, None) {
            Err(Error::ConflictError(message)) => {
                assert!(message.starts_with("DATA_SCOPE_CHANGED："));
            },
            other => panic!("expected DATA_SCOPE_CHANGED, got {other:?}"),
        }
        match ensure_scope_version(Some("scope-a"), "scope-b") {
            Err(Error::ConflictError(message)) => {
                assert!(message.starts_with("DATA_SCOPE_CHANGED："));
            },
            other => panic!("expected DATA_SCOPE_CHANGED, got {other:?}"),
        }
        match ensure_stable_snapshot("scope-a", "scope-b") {
            Err(Error::ConflictError(message)) => {
                assert!(message.starts_with("DATA_SCOPE_CHANGED："));
            },
            other => panic!("expected DATA_SCOPE_CHANGED, got {other:?}"),
        }
        assert!(ensure_scope_version(Some("scope-a"), "scope-a").is_ok());
        assert!(ensure_stable_snapshot("scope-a", "scope-a").is_ok());
    }
}
