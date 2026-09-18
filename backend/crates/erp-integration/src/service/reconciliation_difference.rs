//! 集成本域查询、实体准备与调用方事务内写入。
use application_core::AuditActor;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::IntegrationOpsService;
use super::scope::{ScopedIntegrationList, ensure_page, resolve_list_scope};
use crate::dto::{self, *};
use crate::entity::integration_ops::*;
use crate::repository::IntegrationOpsExt;
use crate::repository::prelude::*;
use crate::{Error, Result};
/// 对账差异列表筛选条件类型。
type DifferenceFilter = <Database as IntegrationOpsExt>::ReconciliationDifferenceFilter;
impl IntegrationOpsService {
    /// 分页查询对账差异，并按最新决定派生状态与版本。
    ///
    /// # 错误
    /// 查询参数非法、范围变化或仓储查询失败时返回错误。
    pub async fn difference_list(
        &self,
        params: &DifferenceListParams,
        actor: &AuditActor,
    ) -> Result<DifferenceListView> {
        params.validate()?;
        let query = params.normalized()?;
        ensure_page(query.paging.page, query.scope_version.as_deref())?;
        let this = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.difference_page(query, &actor, executor).await })
            })
            .await
    }

    async fn difference_page(
        &self,
        query: dto::DifferenceListQuery,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<DifferenceListView> {
        let access = self.access();
        let scope = resolve_list_scope(
            &access,
            actor,
            "reconciliation_difference",
            &query.org_unit_ids,
            query.include_descendants,
            query.scope_version.as_deref(),
            executor,
        )
        .await?;
        let operator_difference_ids =
            self.operator_difference_ids(&query.operator_user_ids, executor).await?;
        let filter =
            difference_filter(&query, &scope.read_scope, scope.owner_org_unit_ids, operator_difference_ids);
        if scope.meta.empty_reason == Some("no_scope") {
            return Ok(empty_difference_page(&filter, scope.meta));
        }
        let page = self.db.reconciliation_differences().search_differences(&filter, executor).await?;
        let total = page.total;
        let items = project_difference_rows(self, page.items, executor).await?;
        Ok(difference_list_view(total, items, &filter, scope.meta))
    }

    /// 把历史处理人筛选解析为差异 ID 集合。
    ///
    /// # 参数
    /// * `operator_user_ids` - 已规范化的历史处理人 ID
    /// * `executor` - 与列表查询相同的执行器
    ///
    /// # 返回
    /// 未筛选时返回 `None`；否则返回处理过的差异 ID。
    ///
    /// # 错误
    /// 仓储读取失败时返回错误。
    async fn operator_difference_ids(
        &self,
        operator_user_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Option<Vec<String>>> {
        if operator_user_ids.is_empty() {
            return Ok(None);
        }
        Ok(Some(
            self.db
                .reconciliation_difference_resolutions()
                .find_difference_ids_handled_by(operator_user_ids, executor)
                .await?,
        ))
    }
}
/// 构造差异并保留原字段不变量的 ValidationError 映射。
/// # Errors
/// 至少一侧证据与身份字段不满足原实体规则时返回原校验错误。
pub fn prepare_difference(
    req: &CreateDifferenceRequest,
    owner_org_unit_id: String,
) -> Result<ReconciliationDifference> {
    let difference = ReconciliationDifference::new(
        ReconciliationDifferenceId::new(next_id()),
        ReconciliationDifferenceData {
            business_object_type: req.business_object_type.clone(),
            business_object_id: req.business_object_id.clone(),
            difference_type: req.difference_type.clone(),
            left_fact_reference: req.left_fact_reference.clone(),
            right_fact_reference: req.right_fact_reference.clone(),
            owner_user_id: req.owner_user_id.clone(),
            owner_org_unit_id,
        },
    )
    .map_err(|error| Error::ValidationError(error.to_string()))?;

    Ok(difference)
}

/// 把已规范化查询与授权条件装配为差异仓储筛选。
fn difference_filter(
    query: &dto::DifferenceListQuery,
    read_scope: &crate::repository::IntegrationReadScope,
    owner_org_unit_ids: Vec<String>,
    operator_difference_ids: Option<Vec<String>>,
) -> DifferenceFilter {
    DifferenceFilter {
        q: query.q.clone(),
        business_object_type: query.business_object_type.clone(),
        business_object_id: query.business_object_id.clone(),
        difference_type: query.difference_type.clone(),
        created_at_from: query.created_at_from,
        created_at_to: query.created_at_to,
        handler_user_ids: query.handler_user_ids.clone(),
        operator_difference_ids,
        owner_org_unit_ids,
        scope_document: Some(read_scope.document()),
        page: query.paging.page,
        page_size: query.paging.page_size,
        sort_by: Some(query.paging.sort_by.to_string()),
        sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
    }
}

/// 装配带范围元数据的差异列表响应。
fn difference_list_view(
    total: i64,
    items: Vec<DifferenceView>,
    filter: &DifferenceFilter,
    meta: ScopedIntegrationList,
) -> DifferenceListView {
    DifferenceListView {
        data: PageView { items, total, page: filter.page, page_size: filter.page_size },
        scope_version: meta.scope_version,
        policy_version: meta.policy_version,
        organization_version: meta.organization_version,
        as_of: meta.as_of,
        empty_reason: None,
        scope_summary: meta.scope_summary,
        ownership_basis: meta.ownership_basis,
    }
}

async fn project_difference_rows(
    service: &IntegrationOpsService,
    rows: Vec<crate::repository::integration_ops::ReconciliationDifferenceRow>,
    executor: &mut dyn Executor,
) -> Result<Vec<DifferenceView>> {
    let difference_ids = rows.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
    let latest_by_difference = service
        .db
        .reconciliation_difference_resolutions()
        .find_latest_by_differences(&difference_ids, executor)
        .await?;
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        let (status, version) = latest_by_difference
            .get(&row.id)
            .map_or((None, 0), |record| (Some(record.resulting_status), u64::from(record.resolution_no)));
        items.push(DifferenceView {
            id: row.id,
            business_object_type: row.business_object_type,
            business_object_id: row.business_object_id,
            difference_type: row.difference_type,
            left_fact_reference: row.left_fact_reference,
            right_fact_reference: row.right_fact_reference,
            status,
            version,
            created_at: row.created_at,
            owner_user_id: row.owner_user_id.unwrap_or_default(),
        });
    }
    Ok(items)
}

fn empty_difference_page(filter: &DifferenceFilter, meta: ScopedIntegrationList) -> DifferenceListView {
    DifferenceListView {
        data: PageView { items: Vec::new(), total: 0, page: filter.page, page_size: filter.page_size },
        scope_version: meta.scope_version,
        policy_version: meta.policy_version,
        organization_version: meta.organization_version,
        as_of: meta.as_of,
        empty_reason: meta.empty_reason,
        scope_summary: meta.scope_summary,
        ownership_basis: meta.ownership_basis,
    }
}

/// 在调用方事务内保存不可变差异事实。
/// # Errors
/// 返回原仓储错误。
pub async fn persist_difference(
    db: &Database,
    difference: &ReconciliationDifference,
    executor: &mut dyn Executor,
) -> Result<()> {
    db.reconciliation_differences().create(difference, executor).await?;
    Ok(())
}
