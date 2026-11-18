//! 连接鉴权异常只接受失败后、当前技术配置的成功鉴权检查作为恢复依据。
use erp_integration::dto::{ControlledEvidenceKind, ControlledEvidenceRef};
use erp_integration::entity::integration_ops::{
    CanonicalEvidenceReference, ErrorClass, IntegrationErrorTask,
};
use erp_integration::ports::evidence::EvidenceSubject;
use erp_integration::repository::IntegrationOpsExt;
use erp_integration::{Error, Result};
use erp_supply::entity::supplier_api::{
    SupplierApiConnection, SupplierApiConnectionId, SupplierHealthCheckRun, SupplierHealthCheckType,
};
use erp_supply::repository::SupplierApiExt;
use persistence_core::Executor;

use super::evidence_adapter::{
    MongoIntegrationEvidenceAuthority, canonical_verified, controlled_ref, kind_mismatch,
};

impl MongoIntegrationEvidenceAuthority {
    pub(super) async fn discover_connection_recovery(
        &self,
        subject: &EvidenceSubject,
        executor: &mut dyn Executor,
        evidence: &mut Vec<ControlledEvidenceRef>,
    ) -> Result<()> {
        let Some((task, connection)) = self.connection_failure_context(subject, executor).await? else {
            return Ok(());
        };

        let runs = self
            .db
            .supplier_api()
            .recent_health_runs(&SupplierApiConnectionId::new(&connection.base.id), 50, executor)
            .await?;
        if let Some(run) = latest_authentication(&runs)
            && run.verifies_authentication_after(&connection, task.base.created_at)
        {
            evidence.push(controlled_ref(
                ControlledEvidenceKind::BusinessObjectVerification,
                "supplier_api_health_check_run",
                &run.base.id,
                "当前配置鉴权检查通过",
            )?);
        }
        Ok(())
    }

    pub(super) async fn verify_connection_recovery(
        &self,
        subject: &EvidenceSubject,
        record_id: &str,
        kind: ControlledEvidenceKind,
        executor: &mut dyn Executor,
    ) -> Result<CanonicalEvidenceReference> {
        if kind != ControlledEvidenceKind::BusinessObjectVerification {
            return kind_mismatch();
        }

        let (task, connection) = self
            .connection_failure_context(subject, executor)
            .await?
            .ok_or_else(|| Error::ConflictError("健康检查不能用于当前异常的处理完成依据".into()))?;
        let run = self
            .db
            .supplier_api_health_check_runs()
            .find_by_id(record_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("核验引用的健康检查不存在".into()))?;
        if !run.verifies_authentication_after(&connection, task.base.created_at) {
            return Err(Error::ConflictError(
                "请先完成失败后、当前配置下的鉴权检查；连接仍异常或配置已变更时不能完成处理".into(),
            ));
        }

        let latest = self
            .db
            .supplier_api()
            .recent_health_runs(&SupplierApiConnectionId::new(&connection.base.id), 50, executor)
            .await?;
        if latest_authentication(&latest).is_none_or(|latest| latest.base.id != run.base.id) {
            return Err(Error::ConflictError("鉴权检查已有更新记录，请刷新后按最新结果处理".into()));
        }
        canonical_verified("supplier_api_health_check_run", &run.base.id, Some(run.base.version), "succeeded")
    }

    async fn connection_failure_context(
        &self,
        subject: &EvidenceSubject,
        executor: &mut dyn Executor,
    ) -> Result<Option<(IntegrationErrorTask, SupplierApiConnection)>> {
        let Some(suffix) = subject.item_id.strip_prefix("w20-error-") else { return Ok(None) };
        if suffix.len() != 64
            || !suffix.chars().all(|ch| ch.is_ascii_hexdigit())
            || subject.message_id.is_some()
        {
            return Ok(None);
        }

        let Some(task) = self.db.integration_error_tasks().find_by_id(&subject.item_id, executor).await?
        else {
            return Ok(None);
        };
        if !accepts_connection_recovery(&task, subject) {
            return Ok(None);
        }

        let Some(id) = task.business_object_id.as_deref() else { return Ok(None) };
        let connection =
            self.db.supplier_api().connection(&SupplierApiConnectionId::new(id), executor).await?;
        Ok(connection.map(|connection| (task, connection)))
    }
}

/// 候选由仓储按创建时间倒序提供；新失败或执行中检查必须阻断旧成功记录。
fn latest_authentication(runs: &[SupplierHealthCheckRun]) -> Option<&SupplierHealthCheckRun> {
    runs.iter().find(|run| run.check_type == SupplierHealthCheckType::Authentication)
}

fn accepts_connection_recovery(task: &IntegrationErrorTask, subject: &EvidenceSubject) -> bool {
    task.error_class == ErrorClass::AuthSignature
        && task.message_id.is_none()
        && task.business_object_id.is_some()
        && task.business_object_id == subject.business_object_id
        && subject.business_object_type.is_none()
}

#[cfg(test)]
mod tests {
    use erp_core::ids::IntegrationErrorTaskId;
    use erp_supply::entity::supplier_api::{SupplierHealthCheckRunData, SupplierHealthCheckStatus};

    use super::*;

    #[test]
    fn newer_failed_authentication_blocks_older_success() {
        let run = |id: &str, status| {
            let mut run = SupplierHealthCheckRun::new(
                id,
                SupplierHealthCheckRunData {
                    connection_id: SupplierApiConnectionId::new("conn-1"),
                    background_job_id: id.into(),
                    check_type: SupplierHealthCheckType::Authentication,
                    technical_config_version: 1,
                    capability_versions: vec![],
                    requested_by: "admin".into(),
                    idempotency_key_hash: "hash".into(),
                    request_fingerprint: "fingerprint".into(),
                },
            )
            .unwrap();
            run.status = status;
            run
        };

        let runs = vec![
            run("newer", SupplierHealthCheckStatus::Failed),
            run("older", SupplierHealthCheckStatus::Succeeded),
        ];

        assert_eq!(latest_authentication(&runs).unwrap().base.id, "newer");
        assert_eq!(latest_authentication(&runs).unwrap().status, SupplierHealthCheckStatus::Failed);
        assert!(latest_authentication(&[]).is_none());
    }

    #[test]
    fn connection_recovery_only_handles_matching_authentication_error() {
        let mut task = IntegrationErrorTask::with_derived_owner_role(
            IntegrationErrorTaskId::new("error-1"),
            None,
            Some("connection-1".into()),
            ErrorClass::AuthSignature,
            "admin".into(),
            "org-1".into(),
        )
        .unwrap();
        let mut subject = EvidenceSubject::error(&task);
        assert!(accepts_connection_recovery(&task, &subject));

        subject.business_object_id = Some("connection-2".into());
        assert!(!accepts_connection_recovery(&task, &subject));

        subject = EvidenceSubject::error(&task);
        task.error_class = ErrorClass::MappingError;
        assert!(!accepts_connection_recovery(&task, &subject));
    }
}
