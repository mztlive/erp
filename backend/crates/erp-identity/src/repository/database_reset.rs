//! 身份领域声明全库重置必须保留的最小账号与授权记录。

use application_core::AuditActor;
use mongodb::Database;
use mongodb::bson::doc;
use persistence_core::{Executor, ResetRetention};

use crate::entity::database_reset::ensure_reset_admin;
use crate::{AccessControlExt, AccountCoreRepositoryExt, Error, MongoCasbinAdapter, ROOT_ROLE_ID, Result};

/// 在调用方事务内核验 admin 身份并冻结必须保留的物理文档 ID。
///
/// # 参数
/// * `db` - 身份领域数据库
/// * `actor` - 当前操作人
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 返回只含 admin 账号、内建超管角色及其直接授权规则和策略版本的保留集。
///
/// # 错误
/// admin 账号或超管角色不存在、操作人不是启用的 admin 超级管理员，或保留登记失败时返回错误。
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
    MongoCasbinAdapter::new(db.clone())
        .retain_root_authorization(&account.base.id, &mut kept, executor)
        .await?;
    Ok(kept)
}
