use database::{NoTransaction, ProjectionExt};
use entities::ids::SalesOrderProjectionId;
use mongodb::Database;
use validator::Validate;

use crate::errors::{Error, Result};
use crate::projection::dto::SortDir;
use crate::projection::service::ProjectionService;
use crate::projection::{
    PageView, ProjectionActionBlockerView, SalesOrderProjectionDeliveryListParams,
    SalesOrderProjectionDeliveryView, SalesOrderProjectionListParams, SalesOrderProjectionRevisionView,
    SalesOrderProjectionView,
};

/// 投影列表筛选条件类型（经 `ProjectionExt` 关联类型跨 crate 可达）。
type SalesOrderProjectionFilter = <Database as ProjectionExt>::SalesOrderProjectionFilter;
/// 投影下发列表筛选条件类型。
type SalesOrderProjectionDeliveryFilter = <Database as ProjectionExt>::SalesOrderProjectionDeliveryFilter;

impl ProjectionService {
    /// 分页查询执行投影列表。
    ///
    /// 排序字段白名单在 Service 层校验（api-contract §4）。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn projection_list(
        &self,
        params: &SalesOrderProjectionListParams,
    ) -> Result<PageView<SalesOrderProjectionView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = SalesOrderProjectionFilter {
            sales_order_id: query.sales_order_id,
            target_mall_id: query.target_mall_id,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .sales_order_projections()
            .search_sales_order_projections(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| SalesOrderProjectionView {
                id: row.id,
                sales_order_id: row.sales_order_id,
                target_mall_id: row.target_mall_id,
                current_acked_revision_id: row.current_acked_revision_id,
                version: row.version,
                created_at: row.created_at,
            })
            .collect();

        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询执行投影详情。
    ///
    /// # 参数
    /// * `id` - 投影 ID
    ///
    /// # 返回
    /// 返回投影详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 投影不存在
    pub async fn projection_detail(&self, id: &str) -> Result<SalesOrderProjectionView> {
        let projection = self
            .db
            .sales_order_projections()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("执行投影不存在".to_string()))?;
        Ok(projection.into())
    }

    /// 列出投影版本（修订号降序）。
    ///
    /// # 参数
    /// * `projection_id` - 所属投影稳定身份
    ///
    /// # 返回
    /// 返回投影版本视图列表。
    pub async fn revision_list(&self, projection_id: &str) -> Result<Vec<SalesOrderProjectionRevisionView>> {
        let rows = self
            .db
            .sales_order_projection_revisions()
            .list_revisions_by_projection(
                &SalesOrderProjectionId::new(projection_id.to_string()),
                &mut NoTransaction,
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| SalesOrderProjectionRevisionView {
                id: row.id,
                projection_id: row.projection_id,
                revision_no: row.revision_no,
                projection_source: row.projection_source,
                sales_order_revision_id: row.sales_order_revision_id,
                customer_external_identity: row.customer_external_identity,
                face_value: row.face_value,
                card_count: row.card_count,
                card_form: row.card_form,
                effective_at: row.effective_at,
                version: row.version,
                created_at: row.created_at,
            })
            .collect())
    }

    /// 分页查询投影下发记录。
    ///
    /// # 参数
    /// * `params` - 查询参数（`target_mall_id`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn delivery_list(
        &self,
        params: &SalesOrderProjectionDeliveryListParams,
    ) -> Result<PageView<SalesOrderProjectionDeliveryView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = SalesOrderProjectionDeliveryFilter {
            target_mall_id: query.target_mall_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .sales_order_projection_deliveries()
            .search_sales_order_projection_deliveries(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| {
                let (allowed_actions, action_blockers) =
                    delivery_action_projection(row.status, row.error_class);
                SalesOrderProjectionDeliveryView {
                    id: row.id,
                    projection_revision_id: row.projection_revision_id,
                    target_mall_id: row.target_mall_id,
                    status: row.status,
                    attempt_count: row.attempt_count,
                    last_attempt_at: row.last_attempt_at,
                    next_attempt_at: row.next_attempt_at,
                    mall_ack_at: row.mall_ack_at,
                    mall_execution_baseline: row.mall_execution_baseline,
                    error_class: row.error_class,
                    error_code: row.error_code,
                    error_summary: row.error_summary,
                    error_task_id: row.error_task_id,
                    work_item_id: row.work_item_id,
                    allowed_actions,
                    action_blockers,
                    version: row.version,
                    created_at: row.created_at,
                }
            })
            .collect();

        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }
}

fn delivery_action_projection(
    status: entities::projection::ProjectionDeliveryStatus,
    error_class: Option<entities::integration_ops::ErrorClass>,
) -> (Vec<String>, Vec<ProjectionActionBlockerView>) {
    use entities::integration_ops::ErrorClass;
    use entities::projection::ProjectionDeliveryStatus;

    if matches!(
        status,
        ProjectionDeliveryStatus::Confirmed | ProjectionDeliveryStatus::Manual
    ) {
        return (Vec::new(), Vec::new());
    }
    if matches!(
        status,
        ProjectionDeliveryStatus::PendingSend | ProjectionDeliveryStatus::Retrying
    ) {
        return (
            Vec::new(),
            vec![ProjectionActionBlockerView {
                action: "RETRY".to_string(),
                code: "CONTROLLED_PROCESSING_PENDING".to_string(),
                message: "投递已进入受控处理队列，禁止重复提交。".to_string(),
            }],
        );
    }

    let mut actions = Vec::new();
    let mut blockers = Vec::new();
    if error_class == Some(ErrorClass::MappingError) {
        blockers.push(ProjectionActionBlockerView {
            action: "RETRY".to_string(),
            code: "MAPPING_DIFFERENCE_REQUIRES_ESCALATION".to_string(),
            message: "映射差异禁止对象级重试，必须升级到 W29。".to_string(),
        });
        blockers.push(ProjectionActionBlockerView {
            action: "QUERY_RESULT".to_string(),
            code: "MAPPING_DIFFERENCE_REQUIRES_ESCALATION".to_string(),
            message: "映射差异必须由 W29 核对，不得在 W23 覆盖结果。".to_string(),
        });
    } else {
        actions.push("QUERY_RESULT".to_string());
    }
    if status == ProjectionDeliveryStatus::Failed && error_class.is_some_and(|class| class.can_auto_retry()) {
        actions.push("RETRY".to_string());
    } else if status == ProjectionDeliveryStatus::Failed && error_class != Some(ErrorClass::MappingError) {
        blockers.push(ProjectionActionBlockerView {
            action: "RETRY".to_string(),
            code: "ERROR_NOT_RETRYABLE".to_string(),
            message: "当前错误不允许对象级重试。".to_string(),
        });
    }
    actions.push("ESCALATE".to_string());
    (actions, blockers)
}

#[cfg(test)]
mod action_projection_tests {
    use entities::integration_ops::ErrorClass;
    use entities::projection::ProjectionDeliveryStatus;

    use super::delivery_action_projection;

    #[test]
    fn mapping_error_only_allows_escalation() {
        let (actions, blockers) =
            delivery_action_projection(ProjectionDeliveryStatus::Failed, Some(ErrorClass::MappingError));
        assert_eq!(actions, vec!["ESCALATE"]);
        assert!(blockers.iter().any(|item| item.action == "RETRY"));
    }

    #[test]
    fn result_unknown_never_allows_retry() {
        let (actions, _) = delivery_action_projection(
            ProjectionDeliveryStatus::ResultUnknown,
            Some(ErrorClass::ResultUnknown),
        );
        assert_eq!(actions, vec!["QUERY_RESULT", "ESCALATE"]);
    }
}
