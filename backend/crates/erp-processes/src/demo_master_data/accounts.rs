//! 按规格补齐岗位账号。已有账号不改密码，缺角色时补上。

use std::collections::HashMap;

use application_core::AuditActor;
use erp_core::AccountKind;
use erp_identity::{
    AccessControlExt, AccountCoreRepositoryExt, AdminService, CreateAdminParams, UpdateAdminRoleParams,
};
use persistence_core::NoTransaction;

use super::spec::{self, AccountSpec};
use super::{DemoFoundationReport, DemoMasterDataService};
use crate::{Error, Result};

/// 规格键和登录名到账号 ID。
pub(super) struct PreparedAccounts {
    /// 审批链使用的规格键。
    pub by_key: HashMap<String, String>,
    /// 部门成员使用的登录名。
    pub by_login: HashMap<String, String>,
}

impl DemoMasterDataService {
    pub(super) async fn ensure_accounts(
        &self,
        actor: &AuditActor,
        report: &mut DemoFoundationReport,
    ) -> Result<PreparedAccounts> {
        let mut prepared = PreparedAccounts { by_key: HashMap::new(), by_login: HashMap::new() };
        for spec in &spec::foundation_spec().accounts {
            if self.ensure_account(actor, spec).await? {
                report.accounts_created += 1;
            } else {
                report.accounts_existing += 1;
            }
            let id = self.account_id(&spec.account).await?;
            prepared.by_key.insert(spec.key.clone(), id.clone());
            prepared.by_login.insert(spec.account.clone(), id);
        }
        if report.accounts_created > 0 {
            let password = &spec::foundation_spec().password;
            report
                .notices
                .push(format!("已新建 {} 个岗位账号，初始密码为 {password}", report.accounts_created));
        }
        Ok(prepared)
    }

    async fn ensure_account(&self, actor: &AuditActor, spec: &AccountSpec) -> Result<bool> {
        let admin = AdminService::new(self.db.clone(), self.rbac.clone());
        match admin
            .create_admin(
                CreateAdminParams {
                    account: spec.account.clone(),
                    password: spec::foundation_spec().password.clone(),
                    name: spec.name.clone(),
                    role_ids: vec![spec.role_id.clone()],
                },
                actor.clone(),
            )
            .await
        {
            Ok(()) => Ok(true),
            Err(erp_identity::Error::ConflictError(_)) => {
                self.ensure_account_role(actor, spec).await?;
                Ok(false)
            },
            Err(error) => Err(error.into()),
        }
    }

    async fn ensure_account_role(&self, actor: &AuditActor, spec: &AccountSpec) -> Result<()> {
        let id = self.account_id(&spec.account).await?;
        let mut role_ids = self.rbac.role_ids(AccountKind::Admin, &id).await?;
        if role_ids.iter().any(|role| role == &spec.role_id) {
            return Ok(());
        }
        role_ids.push(spec.role_id.clone());
        AdminService::new(self.db.clone(), self.rbac.clone())
            .update_admin_role(UpdateAdminRoleParams { id, role_ids }, actor.clone())
            .await?;
        Ok(())
    }

    pub(super) async fn role_actor(&self, login: &str) -> Result<Option<AuditActor>> {
        let found = self.db.accounts().find_by_account(login, &mut NoTransaction).await?;
        Ok(found.map(|account| AuditActor::new(account.base.id, login.to_string(), AccountKind::Admin)))
    }

    pub(super) async fn account_id(&self, account: &str) -> Result<String> {
        self.db
            .accounts()
            .find_by_account(account, &mut NoTransaction)
            .await?
            .map(|found| found.base.id)
            .ok_or_else(|| Error::NotFound(format!("岗位账号 {account} 不存在")))
    }
}
