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
    /// 在原事务核验安全检查管理资格及业务版本后记录扫描结果。
    /// # 参数
    /// id为实际资产，req含所见版本及扫描结果，actor为当前账号。
    /// # 返回
    /// 原字段口径的更新后文件视图。
    /// # 错误
    /// 无资格、不存在、版本或状态冲突、审计失败时拒绝。
    pub async fn mark_scan_result(
        &self,
        id: &str,
        req: MarkScanResultRequest,
        actor: &AuditActor,
    ) -> Result<FileAssetView> {
        req.validate()?;
        self.mutate_asset(id, req.version, Mutation::Scan(req.security_scan_status), actor).await
    }
    /// 在原事务核验销毁管理资格及业务版本后销毁资产。
    /// # 参数
    /// id为实际资产，req含所见版本，actor为当前账号。
    /// # 返回
    /// 更新后文件视图。
    /// # 错误
    /// 无资格、不存在、版本或状态冲突、审计失败时拒绝。
    pub async fn destroy_file_asset(
        &self,
        id: &str,
        req: DestroyFileAssetRequest,
        actor: &AuditActor,
    ) -> Result<FileAssetView> {
        req.validate()?;
        self.mutate_asset(id, req.version, Mutation::Destroy, actor).await
    }
    /// 原写事务内重读资产并执行状态迁移及审计。
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
