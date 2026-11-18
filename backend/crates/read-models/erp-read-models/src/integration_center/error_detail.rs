//! 异常详情的跨领域展示合同；不从展示记录推导业务身份或开放动作。
use erp_audit::entity::AuditLog;
use erp_audit::repository::{AuditExt, AuditTimelineRepositoryExt, TimelineTarget};
use erp_integration::dto::{ErrorTaskDetailView, ErrorTaskView};
use erp_supply::service::supplier_api::context::digest;
use erp_support::repository::prelude::*;
use erp_support::{
    BackgroundJob, BulkJobExt, SUPPLIER_CATALOG_SYNC_JOB_TYPE, SUPPLIER_HEALTH_CHECK_JOB_TYPE,
};
use persistence_core::Executor;
use serde::Serialize;

use super::IntegrationCenterReadService;
use super::error_labels::connection_task_identity;
use crate::Result;

/// 已授权错误详情及其关联责任、来源与处理历史。
#[derive(Debug, Clone, Serialize)]
pub struct IntegrationErrorDetailView {
    #[serde(flatten)]
    pub detail: ErrorTaskDetailView,
    pub work_item_id: Option<String>,
    pub connection_failure: Option<ConnectionFailureView>,
    pub audit_history: Vec<IntegrationAuditView>,
}

/// 原失败的脱敏展示；只接受与异常稳定身份精确匹配的后台任务。
#[derive(Debug, Clone, Serialize)]
pub struct ConnectionFailureView {
    pub connection_id: String,
    pub operation_label: &'static str,
    pub job_number: Option<String>,
    pub failed_at: Option<i64>,
    pub error_summary: Option<String>,
}

/// 已授权目标上的结构化审计事件；不返回任意消息或技术字段。
#[derive(Debug, Clone, Serialize)]
pub struct IntegrationAuditView {
    pub id: String,
    pub occurred_at: u64,
    pub actor: String,
    pub action: String,
}

impl IntegrationCenterReadService {
    pub(super) async fn connection_failure(
        &self,
        task: &ErrorTaskView,
        executor: &mut dyn Executor,
    ) -> Result<Option<ConnectionFailureView>> {
        let Some(connection_id) = connection_task_identity(task) else { return Ok(None) };
        for kind in [SUPPLIER_HEALTH_CHECK_JOB_TYPE, SUPPLIER_CATALOG_SYNC_JOB_TYPE] {
            let candidates = self
                .db
                .background_jobs()
                .connection_failure_candidates(connection_id, kind, task.created_at, executor)
                .await?;
            if let Some(job) = candidates.iter().find(|job| matches_failure(task, job)) {
                return Ok(Some(ConnectionFailureView::from_job(connection_id, job)));
            }
        }
        Ok(Some(ConnectionFailureView {
            connection_id: connection_id.to_string(),
            operation_label: "接口检查或商品同步",
            job_number: None,
            failed_at: None,
            error_summary: None,
        }))
    }

    pub(super) async fn error_audit_history(
        &self,
        task_id: &str,
        work_item_id: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<IntegrationAuditView>> {
        let mut targets = vec![TimelineTarget {
            resource_type: "integration_error_task".into(),
            resource_id: task_id.to_string(),
        }];
        if let Some(id) = work_item_id {
            targets.push(TimelineTarget { resource_type: "work_item".into(), resource_id: id.to_string() });
        }

        let events = self.db.audit_logs().timeline_events(&targets, None, 50, executor).await?;
        Ok(events.into_iter().filter_map(IntegrationAuditView::from_log).collect())
    }
}

fn matches_failure(task: &ErrorTaskView, job: &BackgroundJob) -> bool {
    task.id == format!("w20-error-{}", digest(&[&job.base.id]))
        && task.business_object_id.as_deref() == job.domain_job_id.as_deref()
}

impl ConnectionFailureView {
    fn from_job(connection_id: &str, job: &BackgroundJob) -> Self {
        Self {
            connection_id: connection_id.to_string(),
            operation_label: if job.domain_job_type.as_deref() == Some(SUPPLIER_HEALTH_CHECK_JOB_TYPE) {
                "接口健康检查"
            } else {
                "商品目录同步"
            },
            job_number: Some(job.job_no.clone()),
            failed_at: job.finished_at.map(|at| at.unix_secs()),
            error_summary: job.error_summary.clone(),
        }
    }
}

impl IntegrationAuditView {
    fn from_log(log: AuditLog) -> Option<Self> {
        let event = log.structured_event?;
        Some(Self {
            id: log.base.id,
            occurred_at: event.occurred_at,
            actor: event
                .actor_name_snapshot
                .filter(|name| !name.trim().is_empty())
                .unwrap_or(event.actor_account),
            action: event.action_label,
        })
    }
}

#[cfg(test)]
mod tests {
    use application_core::AuditActor;
    use entity_core::BaseModel;
    use erp_audit::AuditActorLogs;
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;
    use erp_core::ids::BackgroundJobId;
    use erp_integration::entity::integration_ops::{ErrorClass, IntegrationErrorTask};
    use erp_support::{SupplierGovernanceJobKind, SupplierGovernanceJobSpec};

    use super::*;

    fn job() -> BackgroundJob {
        BackgroundJob::for_supplier_governance(SupplierGovernanceJobSpec {
            job_id: BackgroundJobId::new("job-1"),
            connection_id: "conn-1".into(),
            kind: SupplierGovernanceJobKind::HealthCheck,
            requested_by: "admin".into(),
            idempotency_hash: "a".repeat(64),
        })
        .unwrap()
    }

    #[test]
    fn original_failure_requires_both_job_identity_and_connection() {
        let job = job();
        let mut task: ErrorTaskView = IntegrationErrorTask::with_derived_owner_role(
            erp_core::ids::IntegrationErrorTaskId::new(format!("w20-error-{}", digest(&[&job.base.id]))),
            None,
            Some("conn-1".into()),
            ErrorClass::AuthSignature,
            "admin".into(),
            "org-1".into(),
        )
        .unwrap()
        .into();
        assert!(matches_failure(&task, &job));

        task.business_object_id = Some("another-connection".into());
        assert!(!matches_failure(&task, &job));

        task.business_object_id = Some("conn-1".into());
        task.id = "another-task".into();
        assert!(!matches_failure(&task, &job));
    }

    #[test]
    fn failure_view_keeps_original_summary_and_human_job_number() {
        let mut job = job();
        job.base = BaseModel::fake();
        job.start(Instant::from_unix_secs(10)).unwrap();
        job.mark_failed(Some("JD_VOP_TOKEN_UNAVAILABLE: 授权令牌已过期".into()), Instant::from_unix_secs(12))
            .unwrap();

        let view = ConnectionFailureView::from_job("conn-1", &job);
        assert_eq!(view.operation_label, "接口健康检查");
        assert_eq!(view.job_number.as_deref(), Some(job.job_no.as_str()));
        assert_eq!(view.failed_at, Some(12));
        assert_eq!(view.error_summary, job.error_summary);
    }

    #[test]
    fn audit_view_only_exposes_registered_action_and_actor() {
        let actor = AuditActor::new("admin".into(), "admin".into(), AccountKind::Admin);
        let log = actor
            .resource_log_with_id(
                "log-1".into(),
                "integration_error_task.work_item.create",
                "work_item",
                "wi-1".into(),
                Some("job_id=private-id".into()),
            )
            .unwrap();

        let view = IntegrationAuditView::from_log(log.clone()).unwrap();
        assert_eq!(view.action, "创建集成异常待办");
        let json = serde_json::to_string(&view).unwrap();
        assert!(!json.contains("private-id"));

        let mut legacy = log;
        legacy.structured_event = None;
        assert!(IntegrationAuditView::from_log(legacy).is_none());
    }

    #[test]
    fn detail_preserves_flat_task_contract_and_adds_optional_associations() {
        let task = IntegrationErrorTask::with_derived_owner_role(
            erp_core::ids::IntegrationErrorTaskId::new("error-1"),
            None,
            Some("connection-1".into()),
            ErrorClass::AuthSignature,
            "admin".into(),
            "org-1".into(),
        )
        .unwrap();
        let view = IntegrationErrorDetailView {
            detail: ErrorTaskDetailView {
                task: task.into(),
                resolution: None,
                allowed_actions: vec![],
                action_blockers: vec![],
                linked_evidence: vec![],
                resolution_evidence_policy: None,
            },
            work_item_id: Some("wi-1".into()),
            connection_failure: None,
            audit_history: vec![],
        };

        let json = serde_json::to_value(view).unwrap();
        assert_eq!(json["id"], "error-1");
        assert_eq!(json["error_class"], "auth_signature");
        assert_eq!(json["work_item_id"], "wi-1");
        assert!(json["connection_failure"].is_null());
        assert!(json["audit_history"].as_array().unwrap().is_empty());
        assert!(json.get("detail").is_none());
    }
}
