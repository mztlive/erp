//! 当前页最新决定补齐后，才结束授权读取阶段并交付响应。
use std::collections::HashMap;

use async_trait::async_trait;
use erp_identity::CompiledAccess;
use persistence_core::{Executor, PageResult};

use super::{DifferenceFilter, difference_list_view, project_difference_rows};
use crate::Result;
use crate::dto::DifferenceListView;
use crate::entity::integration_ops::ReconciliationDifferenceResolution;
use crate::repository::IntegrationOpsExt;
use crate::repository::integration_ops::ReconciliationDifferenceRow;
use crate::repository::prelude::ReconciliationDifferenceResolutionBatchExt;
use crate::service::{IntegrationOpsService, ScopedIntegrationList};

struct AuthorizedPage<'a> {
    service: &'a IntegrationOpsService,
    access: &'a CompiledAccess,
}

// 仅隔离交付中的两个异步步骤，授权资格和范围仍由真实 AuthorizationService 决定。
#[async_trait]
trait Delivery: Sync {
    async fn latest(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, ReconciliationDifferenceResolution>>;

    async fn validate(&self, executor: &mut dyn Executor) -> Result<()>;
}

#[async_trait]
impl Delivery for AuthorizedPage<'_> {
    async fn latest(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, ReconciliationDifferenceResolution>> {
        Ok(self
            .service
            .db
            .reconciliation_difference_resolutions()
            .find_latest_by_differences(ids, executor)
            .await?)
    }

    async fn validate(&self, executor: &mut dyn Executor) -> Result<()> {
        self.service.data_scope.validate(self.access, executor).await
    }
}

/// 补齐已授权页并在交付前复验原凭证。
/// # 参数
/// service、page、access、filter、meta 为当前页及原授权上下文，executor 为原事务执行器。
/// # 返回
/// 返回原分页口径、最新决定状态与范围元数据。
/// # 错误
/// 补齐或最终复验失败时丢弃整个响应。
pub(super) async fn deliver_difference_page(
    service: &IntegrationOpsService,
    page: PageResult<ReconciliationDifferenceRow>,
    access: &CompiledAccess,
    filter: &DifferenceFilter,
    meta: ScopedIntegrationList,
    executor: &mut dyn Executor,
) -> Result<DifferenceListView> {
    deliver(&AuthorizedPage { service, access }, page, filter, meta, executor).await
}

async fn deliver(
    delivery: &impl Delivery,
    page: PageResult<ReconciliationDifferenceRow>,
    filter: &DifferenceFilter,
    meta: ScopedIntegrationList,
    executor: &mut dyn Executor,
) -> Result<DifferenceListView> {
    let ids = page.items.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
    let latest = delivery.latest(&ids, executor).await?;
    let items = project_difference_rows(page.items, &latest);
    delivery.validate(executor).await?;
    Ok(difference_list_view(page.total, items, filter, meta))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use entity_core::BaseModel;
    use erp_core::common::time::Instant;
    use erp_identity::testing::compiled_access;
    use persistence_core::ExecutionBoundary;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::Error;
    use crate::dto::DifferenceListParams;
    use crate::entity::integration_ops::{ReconciliationDifferenceId, ResolutionAction, ResultingStatus};
    use crate::repository::authorization::mapping;
    use crate::service::reconciliation_difference::difference_filter;

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Outcome {
        Success,
        LatestFailure,
        ValidationFailure,
        AdvanceStage,
        Expired,
    }

    struct DeliveryStub {
        calls: Mutex<Vec<&'static str>>,
        ids: Mutex<Vec<String>>,
        boundary: ExecutionBoundary,
        access: CompiledAccess,
        outcome: Outcome,
    }

    impl DeliveryStub {
        fn new(executor: &dyn Executor, outcome: Outcome) -> Self {
            let model = mapping("reconciliation_difference").unwrap().model().clone();
            let expiry = (outcome == Outcome::Expired).then_some(Instant::from_unix_secs(1));
            Self {
                calls: Mutex::new(Vec::new()),
                ids: Mutex::new(Vec::new()),
                boundary: executor.boundary().unwrap(),
                access: compiled_access(model, executor, expiry).unwrap(),
                outcome,
            }
        }
    }

    #[async_trait]
    impl Delivery for DeliveryStub {
        async fn latest(
            &self,
            ids: &[String],
            executor: &mut dyn Executor,
        ) -> Result<HashMap<String, ReconciliationDifferenceResolution>> {
            assert!(self.boundary.is_current(executor), "最新决定必须在根分页的原阶段读取");
            self.calls.lock().unwrap().push("latest");
            *self.ids.lock().unwrap() = ids.to_vec();
            if self.outcome == Outcome::LatestFailure {
                return Err(Error::Internal("最新决定读取失败".into()));
            }
            if self.outcome == Outcome::AdvanceStage {
                executor.next_read();
            }
            Ok([("decided".into(), resolution())].into())
        }

        async fn validate(&self, executor: &mut dyn Executor) -> Result<()> {
            self.calls.lock().unwrap().push("validate");
            if self.outcome == Outcome::ValidationFailure {
                return Err(Error::Forbidden("交付复验拒绝".into()));
            }
            self.access.ensure_current(executor)?;
            executor.next_read();
            Ok(())
        }
    }

    fn resolution() -> ReconciliationDifferenceResolution {
        ReconciliationDifferenceResolution {
            base: BaseModel::fake(),
            reconciliation_difference_id: ReconciliationDifferenceId::new("decided"),
            resolution_no: 4,
            resolution_action: ResolutionAction::ConfirmNoError,
            resulting_status: ResultingStatus::ConfirmedNoError,
            evidence_reference: Some("evidence".into()),
            handled_by: "operator".into(),
            handled_at: Instant::from_unix_secs(8),
        }
    }

    fn page(empty: bool) -> PageResult<ReconciliationDifferenceRow> {
        let items = if empty {
            Vec::new()
        } else {
            ["decided", "undecided"]
                .into_iter()
                .map(|id| ReconciliationDifferenceRow {
                    id: id.into(),
                    business_object_type: "sales_order".into(),
                    business_object_id: "business".into(),
                    difference_type: "amount".into(),
                    left_fact_reference: Some("left".into()),
                    right_fact_reference: None,
                    owner_user_id: Some("handler".into()),
                    owner_org_unit_id: Some("org".into()),
                    version: 87,
                    created_at: 9,
                })
                .collect()
        };
        PageResult { items, total: 17 }
    }

    fn filter() -> DifferenceFilter {
        let mut filter =
            difference_filter(DifferenceListParams::default().normalized().unwrap(), vec![], None);
        filter.page = 3;
        filter.page_size = 5;
        filter
    }

    fn meta() -> ScopedIntegrationList {
        ScopedIntegrationList {
            scope_version: "scope-version".into(),
            as_of: "123".into(),
            scope_summary: "summary",
            ownership_basis: "current_handler",
        }
    }

    #[tokio::test]
    async fn latest_decisions_precede_validation_and_preserve_page_projection() {
        let mut executor = SnapshotExecutor::default();
        let delivery = DeliveryStub::new(&executor, Outcome::Success);
        let view = deliver(&delivery, page(false), &filter(), meta(), &mut executor).await.unwrap();
        assert_eq!(*delivery.calls.lock().unwrap(), ["latest", "validate"]);
        assert_eq!(*delivery.ids.lock().unwrap(), ["decided", "undecided"]);
        assert!(!delivery.boundary.is_current(&executor), "交付成功才结束原读取阶段");
        assert_eq!((view.data.total, view.data.page, view.data.page_size), (17, 3, 5));
        assert_eq!((view.scope_version.as_str(), view.as_of.as_str()), ("scope-version", "123"));
        assert_eq!((view.scope_summary, view.ownership_basis), ("summary", "current_handler"));
        assert_eq!(view.data.items.len(), 2);
        let decided = &view.data.items[0];
        assert_eq!(decided.id, "decided");
        assert_eq!(decided.status, Some(ResultingStatus::ConfirmedNoError));
        assert_eq!(decided.version, 4, "显示版本来自最新决定序号，不是根版本");
        assert_eq!(decided.business_object_id, "business");
        assert_eq!(decided.left_fact_reference.as_deref(), Some("left"));
        assert_eq!(decided.owner_user_id, "handler");
        assert_eq!(view.data.items[1].status, None);
        assert_eq!(view.data.items[1].version, 0);
    }

    #[tokio::test]
    async fn empty_page_still_requires_final_validation_and_preserves_total() {
        let mut executor = SnapshotExecutor::default();
        let delivery = DeliveryStub::new(&executor, Outcome::Success);
        let view = deliver(&delivery, page(true), &filter(), meta(), &mut executor).await.unwrap();
        assert!(delivery.ids.lock().unwrap().is_empty());
        assert_eq!(*delivery.calls.lock().unwrap(), ["latest", "validate"]);
        assert!(view.data.items.is_empty());
        assert_eq!(view.data.total, 17);
    }

    #[tokio::test]
    async fn latest_decision_failure_stops_before_validation_and_delivery() {
        let mut executor = SnapshotExecutor::default();
        let delivery = DeliveryStub::new(&executor, Outcome::LatestFailure);
        assert!(matches!(
            deliver(&delivery, page(false), &filter(), meta(), &mut executor).await,
            Err(Error::Internal(reason)) if reason == "最新决定读取失败"
        ));
        assert_eq!(*delivery.calls.lock().unwrap(), ["latest"]);
        assert!(delivery.boundary.is_current(&executor));
    }

    #[tokio::test]
    async fn final_validation_failure_discards_the_enriched_page() {
        let mut executor = SnapshotExecutor::default();
        let delivery = DeliveryStub::new(&executor, Outcome::ValidationFailure);
        assert!(matches!(
            deliver(&delivery, page(false), &filter(), meta(), &mut executor).await,
            Err(Error::Forbidden(reason)) if reason == "交付复验拒绝"
        ));
        assert_eq!(*delivery.calls.lock().unwrap(), ["latest", "validate"]);
    }

    #[tokio::test]
    async fn ended_stage_and_expired_credentials_discard_the_enriched_page() {
        for outcome in [Outcome::AdvanceStage, Outcome::Expired] {
            let mut executor = SnapshotExecutor::default();
            let delivery = DeliveryStub::new(&executor, outcome);
            let error = deliver(&delivery, page(false), &filter(), meta(), &mut executor)
                .await
                .expect_err("最终凭证守卫失败不得交付页");
            if outcome == Outcome::AdvanceStage {
                assert!(
                    matches!(error, Error::ValidationError(reason) if reason == "授权结果不能跨事务或读取阶段使用")
                );
            } else {
                assert!(
                    matches!(error, Error::ConflictError(reason) if reason == "DATA_SCOPE_CHANGED：授权关系已到期")
                );
            }
            assert_eq!(*delivery.calls.lock().unwrap(), ["latest", "validate"]);
        }
    }
}
