//! 集成本域查询、实体准备与调用方事务内写入。
use application_core::AuditActor;
use erp_identity::CompiledAccess;
use erp_permissions::integration_ops;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::IntegrationOpsService;
use super::scope::{ScopedIntegrationList, ensure_page, resolve_list_scope};
use crate::dto::{self, *};
use crate::entity::integration_ops::*;
use crate::repository::IntegrationOpsExt;
use crate::repository::integration_ops::ReconciliationDifferenceFilter;
use crate::repository::prelude::*;
use crate::{Error, Result};

impl IntegrationOpsService {
    /// 分页查询对账差异，并按最新决定派生状态与版本。
    ///
    /// 返回带范围元数据的分页视图；无决定的差异状态为 `None`、版本为 0。完整授权条件无匹配记录时返回空页。查询参数非法、范围版本变化、授权或组织展开失败、
    /// 仓储查询或事务失败时返回对应错误。
    pub async fn difference_list(
        &self,
        params: &DifferenceListParams,
        actor: &AuditActor,
    ) -> Result<DifferenceListView> {
        let params = params.clone();
        let this = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let (view, access) = this.difference_list_read(&params, &actor, executor).await?;
                    this.data_scope.validate(&access, executor).await?;
                    Ok(view)
                })
            })
            .await
    }

    /// 在调用方读取阶段取得完整授权分页及待交付的原查询凭证。
    ///
    /// 分页视图及查询凭证；调用方必须补齐展示后通过 `IntegrationAccess::validate` 才能交付。参数非法、资格不足、
    /// 范围变化或分页读取失败时拒绝。
    ///
    /// `params` 为原列表筛选，`actor` 为当前账号，`executor` 为用例原执行器。
    pub async fn difference_list_read(
        &self,
        params: &DifferenceListParams,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(DifferenceListView, CompiledAccess)> {
        params.validate()?;
        let query = params.normalized()?;
        ensure_page(query.paging.page, query.scope_version.as_deref())?;

        let scope = resolve_list_scope(
            &self.data_scope,
            actor,
            &integration_ops::DIFFERENCE_LIST,
            &query.org_unit_ids,
            query.include_descendants,
            query.scope_version.as_deref(),
            executor,
        )
        .await?;

        let operator_difference_ids =
            self.operator_difference_ids(&query.operator_user_ids, executor).await?;
        let filter = difference_filter(query, scope.owner_org_unit_ids, operator_difference_ids);
        let page = self
            .db
            .reconciliation_differences()
            .search_differences(&filter, &scope.read_scope, executor)
            .await?;

        let total = page.total;
        let items = project_difference_rows(self, page.items, executor).await?;
        Ok((difference_list_view(total, items, &filter, scope.meta), scope.read_scope))
    }

    // 将历史处理人筛选转换为曾由这些用户处理的差异 ID，复用列表读取的执行器。
    // 未指定处理人时返回 `None`，与指定了处理人但没有匹配记录区分。
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
/// 构造差异并把实体不变量失败映射为 `ValidationError`。
///
/// 返回新建的对账差异。对象身份、证据引用或处理人不满足实体规则时返回 `ValidationError`，文案来自领域错误。
///
/// `owner_org_unit_id` 为处理人当前内部组织。
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

// 把已规范化查询与授权条件装配为差异仓储筛选。
fn difference_filter(
    query: dto::DifferenceListQuery,
    owner_org_unit_ids: Vec<String>,
    operator_difference_ids: Option<Vec<String>>,
) -> ReconciliationDifferenceFilter {
    ReconciliationDifferenceFilter {
        q: query.q,
        business_object_type: query.business_object_type,
        business_object_id: query.business_object_id,
        difference_type: query.difference_type,
        created_at_from: query.created_at_from,
        created_at_to: query.created_at_to,
        handler_user_ids: query.handler_user_ids,
        operator_difference_ids,
        owner_org_unit_ids,
        page: query.paging.page,
        page_size: query.paging.page_size,
        sort_by: Some(query.paging.sort_by.to_string()),
        sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
    }
}

// 组装对账差异分页响应，供调用方返回当前筛选及授权范围的摘要。
fn difference_list_view(
    total: i64,
    items: Vec<DifferenceView>,
    filter: &ReconciliationDifferenceFilter,
    meta: ScopedIntegrationList,
) -> DifferenceListView {
    DifferenceListView {
        data: PageView { items, total, page: filter.page, page_size: filter.page_size },
        scope_version: meta.scope_version,
        as_of: meta.as_of,
        empty_reason: None,
        scope_summary: meta.scope_summary,
        ownership_basis: meta.ownership_basis,
    }
}

// 用每条差异的最新决定派生列表状态和版本；无决定的差异保留空状态和版本 0。
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
            business_object_label: None,
            difference_type: row.difference_type,
            left_fact_reference: row.left_fact_reference,
            left_fact_label: None,
            right_fact_reference: row.right_fact_reference,
            right_fact_label: None,
            status,
            version,
            created_at: row.created_at,
            owner_user_id: row.owner_user_id.unwrap_or_default(),
            owner_user_name: None,
        });
    }
    Ok(items)
}

/// 在调用方事务内保存不可变差异事实。成功时差异已写入。
pub async fn persist_difference(
    db: &Database,
    difference: &ReconciliationDifference,
    executor: &mut dyn Executor,
) -> Result<()> {
    db.reconciliation_differences().create(difference, executor).await?;
    Ok(())
}
