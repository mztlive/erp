//! 集成本域查询、实体准备与调用方事务内写入。
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction};
use validator::Validate;

use super::IntegrationOpsService;
use crate::dto::*;
use crate::entity::integration_ops::*;
use crate::repository::IntegrationOpsExt;
use crate::{Error, Result};
/// 对账差异列表筛选条件类型。
type DifferenceFilter = <Database as IntegrationOpsExt>::ReconciliationDifferenceFilter;
impl IntegrationOpsService {
    /// 分页查询对账差异，并按最新决定派生状态与版本。
    ///
    /// # 错误
    /// 查询参数非法或仓储查询失败时返回错误。
    pub async fn difference_list(&self, params: &DifferenceListParams) -> Result<PageView<DifferenceView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = DifferenceFilter {
            q: query.q,
            business_object_type: query.business_object_type,
            business_object_id: query.business_object_id,
            difference_type: query.difference_type,
            created_at_from: query.created_at_from,
            created_at_to: query.created_at_to,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page =
            self.db.reconciliation_differences().search_differences(&filter, &mut NoTransaction).await?;
        let difference_ids = page.items.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
        let latest_by_difference = self
            .db
            .reconciliation_difference_resolutions()
            .find_latest_by_differences(&difference_ids, &mut NoTransaction)
            .await?;
        let mut items = Vec::with_capacity(page.items.len());
        for row in page.items {
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
            });
        }
        Ok(PageView { items, total: page.total, page: filter.page, page_size: filter.page_size })
    }
}
/// 构造差异并保留原字段不变量的 ValidationError 映射。
/// # Errors
/// 至少一侧证据与身份字段不满足原实体规则时返回原校验错误。
pub fn prepare_difference(req: &CreateDifferenceRequest) -> Result<ReconciliationDifference> {
    let difference = ReconciliationDifference::new(
        ReconciliationDifferenceId::new(next_id()),
        ReconciliationDifferenceData {
            business_object_type: req.business_object_type.clone(),
            business_object_id: req.business_object_id.clone(),
            difference_type: req.difference_type.clone(),
            left_fact_reference: req.left_fact_reference.clone(),
            right_fact_reference: req.right_fact_reference.clone(),
        },
    )
    .map_err(|error| Error::ValidationError(error.to_string()))?;

    Ok(difference)
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

#[cfg(test)]
mod tests {
    /// 生产代码（测试模块之前部分），供分层守卫断言，避免字面量自匹配。
    ///
    /// # 返回
    /// 返回去掉测试模块后的生产代码全文。
    fn production_source() -> &'static str {
        include_str!("reconciliation_difference.rs").split("mod tests {").next().expect("必须存在生产代码")
    }

    /// 分层守卫（INT-R26）：差异列表经单次批量装载最新决定，无逐行查询。
    ///
    /// 锁定 `find_latest_by_differences` 单次批量入口与缺失映射为无状态版本零；
    /// 逐行 `difference_state` 与单条 `find_latest_by_difference` 不得回潮。
    #[test]
    fn difference_list_resolves_latest_via_single_batch() {
        let source = production_source();
        assert!(source.contains("find_latest_by_differences(&difference_ids"));
        assert!(source.contains("map_or((None, 0)"));
        assert!(!source.contains("fn difference_state"));
        assert!(!source.contains("find_latest_by_difference("));
    }

    /// 分层守卫（INT-E17）：至少一侧证据引用不变量由实体独占，服务只映射错误类别。
    ///
    /// 锁定服务不再保留重复业务判断，实体错误统一映射为 `ValidationError`；
    /// 四格矩阵（均无/仅左/仅右/两者）由实体单测覆盖，此处只锁定归属。
    #[test]
    fn create_difference_defers_reference_invariant_to_entity() {
        let source = production_source();
        assert!(!source.contains("left_fact_reference.is_none() && req.right_fact_reference.is_none()"));
        assert!(source.contains("ReconciliationDifference::new("));
        assert!(source.contains("Error::ValidationError(error.to_string())"));
    }
}
