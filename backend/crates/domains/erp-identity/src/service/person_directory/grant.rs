//! 查询资格的首次授予。角色撤销和初始化都不会删除或恢复记录。

use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::person_directory::{
    ExistingGrant, PersonDirectoryCategory, PersonQueryQualification, PersonQueryStatus, should_insert_grant,
};
use crate::repository::prelude::*;
use crate::{AccessControlExt, Error, Result, RoleId};

/// 分配角色时，为最终角色集合里包含销售或采购来源角色的账号补记对应查询资格。无对应角色时保留历史资格；写入失败须与角色分配一起回滚。
pub async fn grant_assigned_roles(
    db: &Database,
    account_id: &str,
    role_ids: &[RoleId],
    executor: &mut dyn Executor,
) -> Result<()> {
    for role_id in role_ids {
        let Some(category) = PersonDirectoryCategory::from_grant_role(role_id.as_str()) else {
            continue;
        };
        ensure_grant(db, account_id, category, executor).await?;
    }
    Ok(())
}

/// 首次授予时插入有效资格，保留已有有效或终止记录。并发插入的唯一键冲突视为记录已存在，其余持久化错误原样返回。
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
