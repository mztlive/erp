//! 文件材料的上传者与转授授权；完整对象规则统一由 identity 执行。
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::ids::FileAssetId;
use erp_identity::entity::authorization::{AccessContext, FactValue, ObjectFacts};
use erp_identity::{
    AccessFactsPort, AccessTarget, AuthorizationService, Error as IdentityError, ResourceTargets,
    Result as IdentityResult,
};
use erp_permissions::{Permission, file_asset};
use mongodb::Database;
use persistence_core::Executor;

use crate::repository::prelude::*;
use crate::{FileAssetExt, Result};

/// 沿当前写入执行器核验全部文件的转授资格。
/// # 参数
/// authorization 为共享服务，actor 为真实发起人，ids 为完整文件集合，executor 为原事务执行器。
/// # 返回
/// 全部文件均可转授时成功；空集合仍核验账号有效性。
/// # 错误
/// 账号失效、任一文件缺失或无权、身份或事实读取失败时返回错误。
pub async fn require_transfer(
    authorization: &AuthorizationService,
    actor: &AuditActor,
    ids: &[&str],
    executor: &mut dyn Executor,
) -> Result<()> {
    require_files(authorization, actor, ids, &file_asset::TRANSFER, executor).await
}

/// 核验全部文件由当前有效账号上传。
/// # 参数
/// authorization 为共享服务，actor、ids 为真实账号和完整文件集合，executor 为原业务事务。
/// # 返回
/// 所有文件均为本人上传时成功。
/// # 错误
/// 账号失效、缺失文件、非本人文件或基础设施失败时拒绝。
pub async fn require_owned(
    authorization: &AuthorizationService,
    actor: &AuditActor,
    ids: &[&str],
    executor: &mut dyn Executor,
) -> Result<()> {
    require_files(authorization, actor, ids, &file_asset::OWNED_TRANSFER, executor).await
}

/// 沿原事务分批核验完整文件集合，不在调用方组合授权政策。
async fn require_files(
    authorization: &AuthorizationService,
    actor: &AuditActor,
    ids: &[&str],
    permission: &Permission,
    executor: &mut dyn Executor,
) -> Result<()> {
    if ids.is_empty() {
        authorization
            .permissions(actor, &[], executor)
            .await?
            .ok_or_else(|| IdentityError::Forbidden("文件材料提交账号已失效".into()))?;
    }
    for chunk in ids.chunks(1000) {
        authorization
            .require(actor, permission, AccessTarget::Records(ResourceTargets::new(chunk)?), executor)
            .await?;
    }
    Ok(())
}
/// 提供文件真实上传者事实。
/// # 参数
/// db 为文件数据库。
/// # 返回
/// 文件根 Facts。
/// # 错误
/// 构造不执行 I/O。
pub fn facts(db: Database) -> Arc<dyn AccessFactsPort> {
    Arc::new(FileFacts(db))
}

struct FileFacts(Database);

#[async_trait]
impl AccessFactsPort for FileFacts {
    /// 从拥有领域仓储读取当前文件身份、版本与上传者。
    async fn load(
        &self,
        _: &AccessContext,
        ids: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> IdentityResult<Vec<ObjectFacts>> {
        let ids = ids.ids().iter().cloned().map(FileAssetId::new).collect::<Vec<_>>();
        Ok(self
            .0
            .file_assets()
            .find_by_ids(&ids, executor)
            .await?
            .into_iter()
            .map(|file| ObjectFacts {
                resource: "file_asset".into(),
                id: file.base.id,
                version: file.base.version.to_string(),
                owner_id: FactValue::Known(Some(file.created_by)),
                ..Default::default()
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use application_core::AuditActor;
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use persistence_core::NoTransaction;

    use super::{require_owned, require_transfer};
    use crate::Error;

    #[tokio::test]
    async fn file_association_checks_require_a_snapshot_before_identity_or_file_reads() {
        let authorization = unavailable_authorization();
        let actor = AuditActor::new("user".into(), "user".into(), AccountKind::Admin);
        let transfer = require_transfer(&authorization, &actor, &["asset"], &mut NoTransaction).await;
        let owned = require_owned(&authorization, &actor, &["asset"], &mut NoTransaction).await;
        for result in [transfer, owned] {
            assert!(matches!(
                result,
                Err(Error::ValidationError(message)) if message == "写入授权必须使用快照事务"
            ));
        }
    }

    #[tokio::test]
    async fn an_empty_file_set_still_requires_an_active_identity() {
        let authorization = unavailable_authorization();
        let actor = AuditActor::new("user".into(), "user".into(), AccountKind::Admin);
        let transfer = require_transfer(&authorization, &actor, &[], &mut NoTransaction).await;
        let owned = require_owned(&authorization, &actor, &[], &mut NoTransaction).await;
        for result in [transfer, owned] {
            assert!(matches!(
                result,
                Err(Error::Internal(message)) if message == "测试授权设施不可用"
            ));
        }
    }
}
