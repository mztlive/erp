use erp_core::AccountKind;
use persistence_core::NoTransaction;
use tracing::info;

use super::AdminService;
use super::dto::InitializeSuperAdminParams;
use crate::AccessControlExt;
use crate::entity::LoginAccount;
use crate::error::{Error, Result};
use crate::repository::prelude::AccountCoreRepositoryExt;
use crate::service::iam::ensure_root_role;

/// 启动配置创建超级管理员的结果。
///
/// 已有账号时不改密码、状态或角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapSuperAdminResult {
    /// 本次启动新建了超级管理员。
    Created,
    /// 账号已存在，初始密码未使用。
    Unchanged,
}

/// 根据是否已有管理员账号决定启动时要不要创建。
#[derive(Debug)]
enum SuperAdminBootstrapPlan {
    Create,
    LeaveUnchanged,
}

impl AdminService {
    /// 仅在超级管理员账号不存在时，按启动配置创建并绑定 `role-root`。
    ///
    /// 账号已存在时直接返回，不修改密码、状态或角色。并发启动抢到唯一索引时
    /// 也按已存在处理，避免另一个副本把已写入的密码覆盖掉。
    ///
    /// # 参数
    /// * `params` - 账号、初始密码与名称
    ///
    /// # 返回
    /// 返回本次是新建还是未改动已有账号。
    ///
    /// # 错误
    /// 参数不合法、同名账号不是系统管理员，或创建过程中的非重复冲突失败时返回错误。
    pub async fn bootstrap_super_admin_if_absent(
        &self,
        params: InitializeSuperAdminParams,
    ) -> Result<BootstrapSuperAdminResult> {
        let (account, password, name) = params.into_validated_parts()?;
        ensure_root_role(&self.rbac).await?;
        let existing = self.load_account(&account).await?;
        match plan_super_admin_bootstrap(existing)? {
            SuperAdminBootstrapPlan::LeaveUnchanged => {
                info!(account = account.as_str(), "超级管理员账号已存在，启动配置中的初始密码未使用");
                return Ok(BootstrapSuperAdminResult::Unchanged);
            },
            SuperAdminBootstrapPlan::Create => {},
        }

        match self.create_super_admin(account.clone(), password, name).await {
            Ok(admin) => {
                info!(account = account.as_str(), admin_id = %admin.base.id, "已按启动配置创建超级管理员");
                Ok(BootstrapSuperAdminResult::Created)
            },
            Err(error @ Error::ConflictError(_)) => {
                let existing = self.load_account(&account).await?;
                finish_bootstrap_create_conflict(existing, error, account.as_str())
            },
            Err(error) => Err(error),
        }
    }

    /// 查询包含已删除记录在内的同名账号，并标出它是不是系统管理员。
    async fn load_account(&self, account: &LoginAccount) -> Result<Option<bool>> {
        Ok(self
            .db
            .accounts()
            .find_by_account_including_deleted(account.as_str(), &mut NoTransaction)
            .await?
            .map(|account| account.is_kind(AccountKind::Admin)))
    }
}

/// 没有账号时创建；已是系统管理员时保持原样。
fn plan_super_admin_bootstrap(existing_is_admin: Option<bool>) -> Result<SuperAdminBootstrapPlan> {
    match existing_is_admin {
        None => Ok(SuperAdminBootstrapPlan::Create),
        Some(true) => Ok(SuperAdminBootstrapPlan::LeaveUnchanged),
        Some(false) => Err(Error::BusinessLogicError(
            "账号存在但不属于系统管理员，无法按启动配置创建超级管理员".to_string(),
        )),
    }
}

/// 创建撞上唯一索引后，再看账号是否已经由其他实例写好。
fn finish_bootstrap_create_conflict(
    existing_is_admin: Option<bool>,
    conflict: Error,
    account: &str,
) -> Result<BootstrapSuperAdminResult> {
    match plan_super_admin_bootstrap(existing_is_admin)? {
        SuperAdminBootstrapPlan::LeaveUnchanged => {
            info!(account, "超级管理员账号已由其他实例创建，启动配置中的初始密码未使用");
            Ok(BootstrapSuperAdminResult::Unchanged)
        },
        SuperAdminBootstrapPlan::Create => Err(conflict),
    }
}

#[cfg(test)]
mod tests {
    use super::{BootstrapSuperAdminResult, finish_bootstrap_create_conflict, plan_super_admin_bootstrap};
    use crate::error::Error;

    #[test]
    fn absent_account_is_created_and_existing_admin_is_left_unchanged() {
        assert!(matches!(plan_super_admin_bootstrap(None), Ok(super::SuperAdminBootstrapPlan::Create)));
        assert!(matches!(
            plan_super_admin_bootstrap(Some(true)),
            Ok(super::SuperAdminBootstrapPlan::LeaveUnchanged)
        ));
    }

    #[test]
    fn non_admin_account_blocks_bootstrap() {
        let error = plan_super_admin_bootstrap(Some(false)).unwrap_err();

        assert!(matches!(error, Error::BusinessLogicError(_)));
    }

    #[test]
    fn create_conflict_keeps_existing_admin_and_preserves_other_conflicts() {
        let conflict = Error::ConflictError("唯一索引冲突".to_string());

        assert_eq!(
            finish_bootstrap_create_conflict(Some(true), conflict, "admin").unwrap(),
            BootstrapSuperAdminResult::Unchanged
        );

        let conflict = Error::ConflictError("唯一索引冲突".to_string());
        let error = finish_bootstrap_create_conflict(None, conflict, "admin").unwrap_err();
        assert!(matches!(error, Error::ConflictError(message) if message == "唯一索引冲突"));

        let conflict = Error::ConflictError("唯一索引冲突".to_string());
        let error = finish_bootstrap_create_conflict(Some(false), conflict, "admin").unwrap_err();
        assert!(matches!(error, Error::BusinessLogicError(_)));
    }
}
