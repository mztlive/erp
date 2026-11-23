//! 身份领域声明全库重置必须保留的最小账号与授权记录。

use application_core::AuditActor;
use mongodb::Database;
use mongodb::bson::doc;
use persistence_core::{Executor, ResetRetention};

use crate::entity::database_reset::ensure_reset_admin;
use crate::{AccessControlExt, AccountCoreRepositoryExt, Error, PolicyStore, ROOT_ROLE_ID, Result};

/// 全库重置前在调用方事务内确认操作者是启用的 `admin` 超管，并登记其账号、内建角色、直接授权规则及策略版本的物理文档 ID。账号或角色缺失、资格不符或登记失败时拒绝重置。
pub(crate) async fn retention(
    db: &Database,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<ResetRetention> {
    let account = db
        .accounts()
        .find_by_account("admin", executor)
        .await?
        .ok_or_else(|| Error::Forbidden("admin 账号不存在，拒绝清空数据库".into()))?;
    let role = db
        .roles()
        .find_by_id(ROOT_ROLE_ID, executor)
        .await?
        .ok_or_else(|| Error::Forbidden("admin 超管角色不存在，拒绝清空数据库".into()))?;
    ensure_reset_admin(actor, &account, &role)?;

    let mut kept = ResetRetention::default();
    kept.require(db, db.accounts().collection().name(), doc! { "id": &account.base.id }, executor).await?;
    kept.require(db, db.roles().collection().name(), doc! { "id": ROOT_ROLE_ID }, executor).await?;
    PolicyStore::new(db.clone()).retain_root_authorization(&account.base.id, &mut kept, executor).await?;
    Ok(kept)
}
