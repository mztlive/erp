//! 文件治理在同一事务检查功能资格、实际版本及状态迁移。
use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_permissions::{Permission, file_asset};
use persistence_core::Transactional;
use validator::Validate;

use super::{DestroyFileAssetRequest, FileAssetService, FileAssetView, MarkScanResultRequest};
use crate::Result;
use crate::entity::file_asset::{FileAsset, SecurityScanStatus};
use crate::repository::FileAssetExt;
use crate::service::check_expected_version;

#[derive(Clone, Copy)]
enum Mutation {
    Scan(SecurityScanStatus),
    Destroy,
}
impl Mutation {
    fn permission(self) -> Permission {
        match self {
            Self::Scan(_) => file_asset::SCAN_RESULT,
            Self::Destroy => file_asset::DESTROY,
        }
    }
    fn action(self) -> &'static str {
        match self {
            Self::Scan(_) => "file_asset.scan",
            Self::Destroy => "file_asset.destroy",
        }
    }
    fn apply(self, asset: &mut FileAsset) -> Result<()> {
        match self {
            Self::Scan(status) => asset.mark_scan_result(status)?,
            Self::Destroy => asset.destroy(Instant::now())?,
        };
        Ok(())
    }
}
impl FileAssetService {
    /// 实际安全扫描完成后，用请求的资产 ID 和所见版本登记扫描结论。在同一事务检查扫描权限、版本和状态并写审计。
    /// 权限不足、资产缺失、版本或状态冲突、仓储或审计失败时不返回更新视图。
    pub async fn mark_scan_result(
        &self,
        id: &str,
        req: MarkScanResultRequest,
        actor: &AuditActor,
    ) -> Result<FileAssetView> {
        req.validate()?;
        self.mutate_asset(id, req.version, Mutation::Scan(req.security_scan_status), actor).await
    }
    /// 文件治理决定销毁时，在原写事务检查销毁权限和实际资产版本，执行销毁状态迁移并保存审计。
    /// 操作不删除对象字节；资产缺失、无权、版本或状态冲突及写入失败时拒绝。
    pub async fn destroy_file_asset(
        &self,
        id: &str,
        req: DestroyFileAssetRequest,
        actor: &AuditActor,
    ) -> Result<FileAssetView> {
        req.validate()?;
        self.mutate_asset(id, req.version, Mutation::Destroy, actor).await
    }
    // 扫描与销毁共用写入阶段：在原事务先检查对应权限，再重读资产、核对请求版本并执行状态迁移，最后更新资产及审计。
    async fn mutate_asset(
        &self,
        id: &str,
        version: u64,
        mutation: Mutation,
        actor: &AuditActor,
    ) -> Result<FileAssetView> {
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    this.require_operation(&actor, &mutation.permission(), executor).await?;

                    let mut asset = this.require_asset(&id, executor).await?;
                    check_expected_version(asset.base.version, version)?;
                    mutation.apply(&mut asset)?;

                    let audit = this.audit.resource_log(
                        actor,
                        mutation.action(),
                        "file_asset",
                        asset.base.id.clone(),
                    )?;
                    this.db.file_assets().update(&mut asset, executor).await?;
                    this.audit.persist(&audit, executor).await?;
                    Ok(asset.into())
                })
            })
            .await
    }
}
