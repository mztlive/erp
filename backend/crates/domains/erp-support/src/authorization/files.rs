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

/// 转授文件材料前，在原业务快照内通过共享授权逐一检查当前有效账号和完整文件集合的固定转授权限。
/// 空集合仍校验账号；非快照、缺文件、无权或身份/事实读取失败时拒绝。
pub async fn require_transfer(
    authorization: &AuthorizationService,
    actor: &AuditActor,
    ids: &[&str],
    executor: &mut dyn Executor,
) -> Result<()> {
    require_files(authorization, actor, ids, &file_asset::TRANSFER, executor).await
}

/// 仅允许使用本人上传材料的写入用例，用原事务向共享授权核对完整文件集合的真实上传人。
/// 空集合仍检查账号有效性；非快照、缺文件、非本人上传或设施读取失败时拒绝。
pub async fn require_owned(
    authorization: &AuthorizationService,
    actor: &AuditActor,
    ids: &[&str],
    executor: &mut dyn Executor,
) -> Result<()> {
    require_files(authorization, actor, ids, &file_asset::OWNED_TRANSFER, executor).await
}

// 先拒绝非快照执行器；空集合验证账号有效性，其余按每批 1000 个 ID 沿原事务检查固定对象权限，不在本域拼接授权结论。
async fn require_files(
    authorization: &AuthorizationService,
    actor: &AuditActor,
    ids: &[&str],
    permission: &Permission,
    executor: &mut dyn Executor,
) -> Result<()> {
    if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
        return Err(IdentityError::ValidationError("写入授权必须使用快照事务".into()).into());
    }

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
/// 装配读取文件真实上传者的对象事实端口，绑定文件库但不执行 I/O；供 identity 作所有权检查。
pub fn facts(db: Database) -> Arc<dyn AccessFactsPort> {
    Arc::new(FileFacts(db))
}

struct FileFacts(Database);

#[async_trait]
impl AccessFactsPort for FileFacts {
    // 在授权原执行器上从文件仓储读取实际资产 ID、当前版本和上传者，以仓储事实而非请求声明证明归属。
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
    use erp_identity::entity::access_control::ResolvedScope;
    use erp_identity::entity::authorization::{FactValue, ObjectFacts};
    use erp_identity::testing::{fixed_authorization, unavailable_authorization};
    use erp_permissions::file_asset;
    use persistence_core::NoTransaction;
    use persistence_core::testing::SnapshotExecutor;

    use super::{require_owned, require_transfer};
    use crate::Error;

    #[tokio::test]
    async fn file_association_checks_require_a_snapshot_before_identity_or_file_reads() {
        let authorization = unavailable_authorization();
        let actor = AuditActor::new("user".into(), "user".into(), AccountKind::Admin);

        for ids in [&[][..], &["asset"][..]] {
            let transfer = require_transfer(&authorization, &actor, ids, &mut NoTransaction).await;
            let owned = require_owned(&authorization, &actor, ids, &mut NoTransaction).await;
            for result in [transfer, owned] {
                assert!(matches!(
                    result,
                    Err(Error::ValidationError(message)) if message == "写入授权必须使用快照事务"
                ));
            }
        }
    }

    #[tokio::test]
    async fn an_empty_file_set_still_requires_an_active_identity() {
        let authorization = unavailable_authorization();
        let actor = AuditActor::new("user".into(), "user".into(), AccountKind::Admin);
        let transfer = require_transfer(&authorization, &actor, &[], &mut SnapshotExecutor::default()).await;
        let owned = require_owned(&authorization, &actor, &[], &mut SnapshotExecutor::default()).await;
        for result in [transfer, owned] {
            assert!(matches!(
                result,
                Err(Error::Internal(message)) if message == "测试授权设施不可用"
            ));
        }
    }

    #[tokio::test]
    async fn active_identity_requires_a_snapshot_for_empty_and_owned_file_sets() {
        let authorization = fixed_authorization(
            &[file_asset::TRANSFER, file_asset::OWNED_TRANSFER],
            ResolvedScope { role_clauses: Vec::new(), user_limit: None },
            vec![ObjectFacts {
                resource: "file_asset".into(),
                id: "asset".into(),
                version: "1".into(),
                owner_id: FactValue::Known(Some("user".into())),
                ..Default::default()
            }],
            Vec::new(),
        )
        .unwrap();
        let actor = AuditActor::new("user".into(), "user".into(), AccountKind::Admin);

        for ids in [&[][..], &["asset"][..]] {
            let transfer = require_transfer(&authorization, &actor, ids, &mut NoTransaction).await;
            let owned = require_owned(&authorization, &actor, ids, &mut NoTransaction).await;
            for result in [transfer, owned] {
                assert!(matches!(
                    result,
                    Err(Error::ValidationError(message)) if message == "写入授权必须使用快照事务"
                ));
            }

            require_transfer(&authorization, &actor, ids, &mut SnapshotExecutor::default()).await.unwrap();
            require_owned(&authorization, &actor, ids, &mut SnapshotExecutor::default()).await.unwrap();
        }
    }
}
