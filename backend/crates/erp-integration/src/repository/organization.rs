//! 组织停用使用的集成异常和对账差异未结事实。

use mongodb::Database;
use mongodb::bson::doc;
use persistence_core::{Executor, Result};

use super::{IntegrationOpsExt, ReconciliationDifferenceResolutionBatchExt};
use crate::entity::integration_ops::ResultingStatus;

/// 检查组织是否仍持有开放异常或没有终态处理结论的差异。
///
/// # 错误
/// 数据库读取失败时拒绝完成停用检查。
pub async fn has_unsettled_business_org(
    db: &Database,
    org: &str,
    executor: &mut dyn Executor,
) -> Result<bool> {
    if db
        .integration_error_tasks()
        .exists(
            doc! {
                "owner_org_unit_id": org, "status": { "$nin": ["resolved", "closed"] }
            },
            executor,
        )
        .await?
    {
        return Ok(true);
    }
    let differences =
        db.reconciliation_differences().find_many(doc! { "owner_org_unit_id": org }, executor).await?;
    let ids = differences.into_iter().map(|item| item.base.id).collect::<Vec<_>>();
    let latest =
        db.reconciliation_difference_resolutions().find_latest_by_differences(&ids, executor).await?;
    Ok(ids.iter().any(|id| unsettled_difference(latest.get(id).map(|item| item.resulting_status))))
}

fn unsettled_difference(status: Option<ResultingStatus>) -> bool {
    status.is_none_or(|status| !status.is_terminal())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_terminal_difference_conclusions_allow_org_disable() {
        for status in [None, Some(ResultingStatus::Open), Some(ResultingStatus::EvidencePending)] {
            assert!(unsettled_difference(status));
        }
        for status in [
            ResultingStatus::ConfirmedNoError,
            ResultingStatus::ConfirmedValidDifference,
            ResultingStatus::Closed,
        ] {
            assert!(!unsettled_difference(Some(status)));
        }
    }
}
