//! 按当前有效角色绑定补齐尚未建立的查询资格。

use persistence_core::{NoTransaction, Transactional};

use super::grant::ensure_grant;
use crate::entity::person_directory::PersonDirectoryCategory;
use crate::repository::prelude::*;
use crate::service::iam::SharedRbacService;
use crate::{AccessControlExt, Result};

/// 为当前仍绑定授予来源角色的后台账号补齐缺失资格。
///
/// # 参数
/// * `rbac` - 共享 RBAC 服务
///
/// # 错误
/// 角色、策略或资格写入失败时返回错误。
///
/// # 关键业务约束
/// 角色停用时不再新增资格。已有终止记录不会被恢复，已撤销角色的历史资格也不会被删除。
pub async fn sync_role_grants(rbac: &SharedRbacService) -> Result<()> {
    for category in [PersonDirectoryCategory::Sales, PersonDirectoryCategory::Procurement] {
        sync_category(rbac, category).await?;
    }
    Ok(())
}

async fn sync_category(rbac: &SharedRbacService, category: PersonDirectoryCategory) -> Result<()> {
    let Some(role_id) = category.grant_role_id() else {
        return Ok(());
    };
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
            let account_ids = account_ids.clone();
            let db = db.clone();
            Box::pin(async move {
                for account_id in &account_ids {
                    ensure_grant(&db, account_id, category, executor).await?;
                }
                Ok(())
            })
        })
        .await
}
