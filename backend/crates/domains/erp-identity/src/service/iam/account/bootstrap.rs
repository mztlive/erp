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

// 启动引导按同名账号的存在性和类型选择创建或保留。
#[derive(Debug)]
enum SuperAdminBootstrapPlan {
    Create,
    LeaveUnchanged,
}

impl AdminService {
    /// 服务启动配置了初始密码时调用：确保根角色存在，仅在同名账号缺失时创建管理员并绑定 `role-root`。
    ///
    /// 已有管理员（含软删除）保留原密码、状态和角色；并发创建冲突后查到同名管理员也按成功处理。
    /// 参数非法、同名账号类型不符、复查仍无同名账号或其他创建失败时返回错误。
    pub async fn bootstrap_super_admin_if_absent(&self, params: InitializeSuperAdminParams) -> Result<()> {
        let (account, password, name) = params.into_validated_parts()?;
        ensure_root_role(&self.rbac).await?;

        let existing = self.load_account(&account).await?;
        match plan_super_admin_bootstrap(existing)? {
            SuperAdminBootstrapPlan::LeaveUnchanged => {
                info!(account = account.as_str(), "超级管理员账号已存在，启动配置中的初始密码未使用");
                return Ok(());
            },
            SuperAdminBootstrapPlan::Create => {},
        }

        match self.create_super_admin(account.clone(), password, name).await {
            Ok(admin) => {
                info!(account = account.as_str(), admin_id = %admin.base.id, "已按启动配置创建超级管理员");
                Ok(())
            },
            Err(error @ Error::ConflictError(_)) => {
                let existing = self.load_account(&account).await?;
                finish_bootstrap_create_conflict(existing, error, account.as_str())
            },
            Err(error) => Err(error),
        }
    }

    // 启动首次查询或创建冲突复查时读取同名账号（含软删除）；区分缺失、系统管理员和其他账号类型。
    async fn load_account(&self, account: &LoginAccount) -> Result<Option<bool>> {
        Ok(self
            .db
            .accounts()
            .find_by_account_including_deleted(account.as_str(), &mut NoTransaction)
            .await?
            .map(|account| account.is_kind(AccountKind::Admin)))
    }
}

// 启动读库后按账号分类决策：缺失则创建，已有管理员则原样保留，其他类型返回业务错误。
fn plan_super_admin_bootstrap(existing_is_admin: Option<bool>) -> Result<SuperAdminBootstrapPlan> {
    match existing_is_admin {
        None => Ok(SuperAdminBootstrapPlan::Create),
        Some(true) => Ok(SuperAdminBootstrapPlan::LeaveUnchanged),
        Some(false) => Err(Error::BusinessLogicError(
            "账号存在但不属于系统管理员，无法按启动配置创建超级管理员".to_string(),
        )),
    }
}

// 并发创建冲突复查后，确认同名管理员已存在时返回成功；仍缺失时保留原冲突，同名其他类型沿用业务错误。
fn finish_bootstrap_create_conflict(
    existing_is_admin: Option<bool>,
    conflict: Error,
    account: &str,
) -> Result<()> {
    match plan_super_admin_bootstrap(existing_is_admin)? {
        SuperAdminBootstrapPlan::LeaveUnchanged => {
            info!(account, "超级管理员账号已由其他实例创建，启动配置中的初始密码未使用");
            Ok(())
        },
        SuperAdminBootstrapPlan::Create => Err(conflict),
    }
}

#[cfg(test)]
mod tests {
    use super::{finish_bootstrap_create_conflict, plan_super_admin_bootstrap};
    use crate::error::Error;

    // 验证启动时缺失账号才安排创建，已有管理员原样保留。
    #[test]
    fn absent_account_is_created_and_existing_admin_is_left_unchanged() {
        assert!(matches!(plan_super_admin_bootstrap(None), Ok(super::SuperAdminBootstrapPlan::Create)));
        assert!(matches!(
            plan_super_admin_bootstrap(Some(true)),
            Ok(super::SuperAdminBootstrapPlan::LeaveUnchanged)
        ));
    }

    // 验证同名非管理员通过业务错误阻止启动引导创建账号。
    #[test]
    fn non_admin_account_blocks_bootstrap() {
        let error = plan_super_admin_bootstrap(Some(false)).unwrap_err();
        assert!(matches!(error, Error::BusinessLogicError(_)));
    }

    // 验证冲突复查识别已创建的同名管理员，缺失时保留原冲突，类型不符时报告业务错误。
    #[test]
    fn create_conflict_keeps_existing_admin_and_preserves_other_conflicts() {
        let conflict = Error::ConflictError("唯一索引冲突".to_string());
        assert!(finish_bootstrap_create_conflict(Some(true), conflict, "admin").is_ok());

        let conflict = Error::ConflictError("唯一索引冲突".to_string());
        let error = finish_bootstrap_create_conflict(None, conflict, "admin").unwrap_err();
        assert!(matches!(error, Error::ConflictError(message) if message == "唯一索引冲突"));

        let conflict = Error::ConflictError("唯一索引冲突".to_string());
        let error = finish_bootstrap_create_conflict(Some(false), conflict, "admin").unwrap_err();
        assert!(matches!(error, Error::BusinessLogicError(_)));
    }
}
