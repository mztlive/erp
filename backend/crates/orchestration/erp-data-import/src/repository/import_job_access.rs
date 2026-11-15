//! 导入任务复用共享授权；任务类型由用例在原执行器核对。
use std::sync::Arc;

use application_core::AuditActor;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::Permission;
use erp_support::{BackgroundJob, BulkJobExt};
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use crate::{Error, Result};

#[derive(Clone)]
pub(crate) struct ImportJobAccess {
    pub(crate) service: Arc<AuthorizationService>,
    db: Database,
    domain: &'static str,
    permission: Permission,
}

impl ImportJobAccess {
    /// 绑定共享授权和本用例任务类型。
    /// # 参数
    /// db 为数据库，service 为进程共享授权服务，domain 为固定任务类型，permission 为该类型的固定内部读取操作。
    /// # 返回
    /// 不缓存权限结果的任务访问器。
    /// # 错误
    /// 无。
    pub(crate) fn new(
        db: Database,
        service: Arc<AuthorizationService>,
        domain: &'static str,
        permission: Permission,
    ) -> Self {
        Self { service, db, domain, permission }
    }

    /// 在原执行器核验任务归属、入口资格和业务类型。
    /// # 参数
    /// id 为真实任务，actor 为操作人，executor 为原执行器。
    /// # 返回
    /// 共享授权与任务类型均符合时成功。
    /// # 错误
    /// 缺失、其他类型或无权任务拒绝。
    pub(crate) async fn require(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.load(id, actor, executor).await.map(|_| ())
    }

    /// 在原授权快照读取任务，调用方在该快照内读取关联数据。
    /// # 参数
    /// id、actor 为真实任务与当前账号；executor 沿用调用方读取事务。
    /// # 返回
    /// 满足固定入口资格、归属和类型的真实任务。
    /// # 错误
    /// 身份、资格、归属、类型或数据库读取失败时拒绝。
    pub(crate) async fn load(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<BackgroundJob> {
        self.service.require(actor, &self.permission, AccessTarget::record(id)?, executor).await?;
        let job = self
            .db
            .background_jobs()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("任务不存在".into()))?;
        if job.domain_job_type.as_deref() != Some(self.domain) {
            return Err(Error::NotFound("任务不存在".into()));
        }
        Ok(job)
    }

    /// 在短读取事务内授权任务，事务结束后才允许读取对象存储。
    /// # 参数
    /// id、actor 为真实任务和当前账号。
    /// # 返回
    /// 同一快照授权并读取的任务。
    /// # 错误
    /// 资格、归属、类型或事务失败时拒绝。
    pub(crate) async fn snapshot(&self, id: &str, actor: &AuditActor) -> Result<BackgroundJob> {
        let access = self.clone();
        let id = id.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { access.load(&id, &actor, executor).await })
            })
            .await
    }

    /// 外部源读取完成后重新授权，并拒绝交付其他任务版本的数据。
    /// # 参数
    /// original 为读取外部源前的任务；actor 为当前账号；executor 为交付读取事务。
    /// # 返回
    /// 当前授权仍成立且版本未变的真实任务。
    /// # 错误
    /// 当前授权失败、任务变化或数据库读取失败时拒绝。
    pub(crate) async fn reload(
        &self,
        original: &BackgroundJob,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<BackgroundJob> {
        let current = self.load(&original.base.id, actor, executor).await?;
        ensure_same_job_version(original, &current)?;
        Ok(current)
    }
}

/// 任务版本覆盖源身份、归属和状态；变化后必须重新读取源内容。
fn ensure_same_job_version(original: &BackgroundJob, current: &BackgroundJob) -> Result<()> {
    if original.base.id != current.base.id || original.base.version != current.base.version {
        return Err(Error::ConflictError("导入任务已变化，请重新下载".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use erp_permissions::internal;
    use erp_support::{BackgroundJobData, BackgroundJobId, JobType};
    use mongodb::Client;
    use persistence_core::ReadExecutor;

    use super::*;

    fn job() -> BackgroundJob {
        BackgroundJob::new(
            BackgroundJobId::new("job"),
            BackgroundJobData {
                job_no: "JOB-1".into(),
                job_type: JobType::Import,
                domain_job_type: Some("BRAND_IMPORT".into()),
                domain_job_id: Some("source".into()),
                selection_snapshot_id: None,
                requested_by: "owner".into(),
                request_id: "request".into(),
                input_file_asset_id: None,
                result_file_asset_id: None,
                total_count: 1,
            },
        )
        .unwrap()
    }

    /// 只有当前任务及原版本可以消费已经读取的外部源。
    #[test]
    fn download_rejects_changed_task_version_or_identity() {
        let original = job();
        assert!(ensure_same_job_version(&original, &original).is_ok());
        let mut changed = original.clone();
        changed.base.version += 1;
        assert!(
            matches!(ensure_same_job_version(&original, &changed), Err(Error::ConflictError(reason)) if reason == "导入任务已变化，请重新下载")
        );
        changed = original.clone();
        changed.base.id = "other-job".into();
        assert!(matches!(ensure_same_job_version(&original, &changed), Err(Error::ConflictError(_))));
    }

    /// 外部源已读取后也必须重新取证；身份设施失效时不能读取行或交付源数据。
    #[tokio::test]
    async fn reload_reauthorizes_before_reading_current_job() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("job_authorization_unit");
        let access = ImportJobAccess::new(
            db,
            unavailable_authorization(),
            "BRAND_IMPORT",
            internal::BRAND_IMPORT_JOB_READ,
        );
        let actor = AuditActor::new("owner".into(), "operator".into(), AccountKind::Admin);
        let error =
            access.reload(&job(), &actor, &mut ReadExecutor::default()).await.expect_err("必须重新授权");
        assert!(matches!(error, Error::Internal(reason) if reason == "测试授权设施不可用"));
    }
}
