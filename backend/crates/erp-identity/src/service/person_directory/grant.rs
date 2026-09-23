//! 查询资格的首次授予。角色撤销和初始化都不会删除或恢复记录。

use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::person_directory::{
    ExistingGrant, PersonDirectoryCategory, PersonQueryQualification, PersonQueryStatus, should_insert_grant,
};
use crate::repository::prelude::*;
use crate::{AccessControlExt, Error, Result};

/// 为本次角色分配中新出现的目录来源角色补记查询资格。
///
/// # 参数
/// * `db` - 身份数据库
/// * `account_id` - 被分配角色的账号
/// * `role_ids` - 分配后的完整角色集合
/// * `executor` - 调用方事务执行器
///
/// # 错误
/// 资格写入失败时返回错误，以便与角色分配一起回滚。
///
/// # 关键业务约束
/// 只在集合中出现 `role-sales` 或 `role-procurement` 时插入缺失记录。
/// 集合里没有这些角色时不做任何删除。
pub async fn grant_assigned_roles(
    db: &Database,
    account_id: &str,
    role_ids: &[String],
    executor: &mut dyn Executor,
) -> Result<()> {
    for role_id in role_ids {
        let Some(category) = PersonDirectoryCategory::from_grant_role(role_id) else {
            continue;
        };
        ensure_grant(db, account_id, category, executor).await?;
    }
    Ok(())
}

/// 在没有资格记录时插入一条有效资格。
///
/// # 参数
/// * `db` - 身份数据库
/// * `account_id` - 账号 ID
/// * `category` - 查询类别
/// * `executor` - 调用方执行器
///
/// # 错误
/// 非唯一冲突的写入失败时返回错误。
///
/// # 关键业务约束
/// 已终止记录保持终止。并发插入冲突视为另一事务已经写过，不再覆盖。
pub async fn ensure_grant(
    db: &Database,
    account_id: &str,
    category: PersonDirectoryCategory,
    executor: &mut dyn Executor,
) -> Result<()> {
    let existing = db.person_query_qualifications().find_for_account(account_id, category, executor).await?;
    let state = match existing.as_ref().map(|row| row.status) {
        None => ExistingGrant::Absent,
        Some(PersonQueryStatus::Active) => ExistingGrant::Active,
        Some(PersonQueryStatus::Terminated) => ExistingGrant::Terminated,
    };
    if !should_insert_grant(state) {
        return Ok(());
    }
    let qualification = PersonQueryQualification::grant(next_id(), account_id, category)?;
    match db.person_query_qualifications().create(&qualification, executor).await {
        Ok(()) => Ok(()),
        Err(persistence_core::Error::DuplicateKey(_)) => Ok(()),
        Err(error) => Err(Error::from(error)),
    }
}
