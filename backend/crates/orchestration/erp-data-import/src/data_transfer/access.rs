//! 内部文件任务由 identity 执行本人政策；门户任务保留原入口，等待门户重做。
use application_core::AuditActor;
use erp_core::AccountKind;
use erp_support::BackgroundJob;
use persistence_core::NoTransaction;

use super::{DataTransferProcess, TransferKind};
use crate::{Error, Result};

impl DataTransferProcess {
    /// 内部导入要求本人，导出由原业务授权继续证明当前访问范围。
    pub(super) async fn require_reader(
        &self,
        job: &BackgroundJob,
        kind: TransferKind,
        actor: &AuditActor,
    ) -> Result<bool> {
        if kind == TransferKind::PortalBatch {
            return portal_owner(job, actor).map(|()| true);
        }
        if !kind.is_export() {
            return self.job_access.require(&job.base.id, actor, &mut NoTransaction).await.map(|()| true);
        }
        self.job_access
            .service
            .permissions(actor, &[], &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Forbidden("当前账号不能读取导出任务".into()))?;
        match self.job_access.require(&job.base.id, actor, &mut NoTransaction).await {
            Ok(()) => Ok(true),
            Err(Error::Forbidden(_)) => Ok(false),
            Err(error) => Err(error),
        }
    }

    /// 原入口查询进度或恢复结果时重验本人，不新增后台任务管理权限。
    pub(super) async fn require_owner(&self, job: &BackgroundJob, actor: &AuditActor) -> Result<()> {
        if actor.kind() == AccountKind::Supplier {
            return portal_owner(job, actor);
        }
        self.job_access.require(&job.base.id, actor, &mut NoTransaction).await
    }
}

/// 门户本人任务规则维持现状，重做范围统一标记在 supplier_portal 入口。
fn portal_owner(job: &BackgroundJob, actor: &AuditActor) -> Result<()> {
    if job.requested_by != actor.id() {
        return Err(Error::Forbidden("导入预览和批量处理结果仅供原提交人使用".into()));
    }
    Ok(())
}
