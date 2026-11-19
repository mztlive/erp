//! 按当前有效角色绑定补齐尚未建立的查询资格。

use persistence_core::{NoTransaction, Transactional};

use super::grant::ensure_grant;
use crate::entity::person_directory::PersonDirectoryCategory;
use crate::repository::prelude::*;
use crate::service::iam::SharedRbacService;
use crate::{AccessControlExt, Result};

/// 在初始化时为仍绑定启用来源角色的后台账号补齐销售、采购目录资格。保留终止记录与已撤销角色的历史资格；读取或写入失败时返回错误。
pub async fn sync_role_grants(rbac: &SharedRbacService) -> Result<()> {
    for (category, role_id) in PersonDirectoryCategory::GRANT_ROLES {
        sync_category(rbac, category, role_id).await?;
    }
    Ok(())
}

// 读取某一启用来源角色的直接后台成员，再为整批成员开启事务补齐缺失资格。角色停用或没有成员时跳过。
async fn sync_category(
    rbac: &SharedRbacService,
    category: PersonDirectoryCategory,
    role_id: &str,
) -> Result<()> {
    let enabled = rbac.database().roles().enabled_roles(&[role_id.to_string()], &mut NoTransaction).await?;
    if enabled.is_empty() {
        return Ok(());
    }

    let account_ids = rbac.direct_admin_ids_for_role(role_id).await?;
    if account_ids.is_empty() {
        return Ok(());
    }

    let db = rbac.database().clone();
    db.client()
        .clone()
        .with_transaction(move |executor| {
            Box::pin(async move {
                for account_id in &account_ids {
                    ensure_grant(&db, account_id, category, executor).await?;
                }
                Ok(())
            })
        })
        .await
}
