//! 文件材料的上传者与转授授权；完整对象规则统一由 identity 执行。
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::ids::FileAssetId;
use erp_identity::entity::authorization::{AccessContext, FactKind, FactValue, ObjectFacts};
use erp_identity::{
    AccessFactsPort, AccessMode, AccessPolicy, AccessRegistration, AccessSubject, AccessTarget,
    AuthorizationRegistry, AuthorizationService, DataAccess, Error as IdentityError, Permission,
    ResourceTargets, Result as IdentityResult, SharedRbacService,
};
use mongodb::Database;
use persistence_core::Executor;

use crate::repository::prelude::*;
use crate::{Error, FileAssetExt, Result};

/// 沿当前写入执行器核验全部文件的转授资格。
/// # 参数
/// db、identity 为共享身份设施，actor 为真实发起人，ids 为完整文件集合，executor 为原执行器。
/// # 返回
/// 全部文件均可转授时成功；空集合仍核验账号有效性。
/// # 错误
/// 账号失效、任一文件缺失或无权、身份或事实读取失败时返回错误。
pub async fn require_transfer(
    db: &Database,
    identity: &SharedRbacService,
    actor: &AuditActor,
    ids: &[&str],
    executor: &mut dyn Executor,
) -> Result<()> {
    require_files(
        db,
        identity,
        actor,
        ids,
        AccessPolicy::Any(vec![
            AccessPolicy::Owned,
            AccessPolicy::Permission(Permission::parse("file_asset:preview")?),
        ]),
        executor,
    )
    .await
}

/// 核验全部文件确由当前有效账号上传。
/// # 参数
/// db、identity 为共享身份设施，actor 为真实提交人，ids 为待关联的完整文件集合，executor 为原执行器。
/// # 返回
/// 全部文件的原上传者为当前账号时成功；空集合仍核验账号有效性。
/// # 错误
/// 账号失效、任一文件缺失或非本人上传、身份或事实读取失败时返回错误。
pub async fn require_owned(
    db: &Database,
    identity: &SharedRbacService,
    actor: &AuditActor,
    ids: &[&str],
    executor: &mut dyn Executor,
) -> Result<()> {
    require_files(db, identity, actor, ids, AccessPolicy::Owned, executor).await
}

async fn require_files(
    db: &Database,
    identity: &SharedRbacService,
    actor: &AuditActor,
    ids: &[&str],
    policy: AccessPolicy,
    executor: &mut dyn Executor,
) -> Result<()> {
    if ids.len() > 1000 && executor.session().is_none() {
        return Err(Error::ValidationError("分批文件授权必须使用业务事务".into()));
    }
    let permission = Permission::parse("file_asset:transfer")?;
    let registry = AuthorizationRegistry::new(vec![AccessRegistration {
        permission: permission.clone(),
        required_permissions: vec![],
        access: AccessMode::Object {
            data: DataAccess {
                subject: AccessSubject::Direct,
                required_facts: [FactKind::Owner].into(),
                policy: AccessPolicy::Account { policy: Box::new(policy) },
            },
            facts: Arc::new(FileFacts(db.clone())),
        },
    }])?;
    let service = AuthorizationService::new(db.clone(), identity.clone(), registry);
    if ids.is_empty() {
        service
            .permissions(actor, &[], executor)
            .await?
            .ok_or_else(|| IdentityError::Forbidden("文件材料提交账号已失效".into()))?;
        return Ok(());
    }
    for chunk in ids.chunks(1000) {
        service
            .require(
                actor,
                &permission,
                AccessTarget::Records { subject: AccessSubject::Direct, ids: ResourceTargets::new(chunk)? },
                executor,
            )
            .await?;
    }
    Ok(())
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
