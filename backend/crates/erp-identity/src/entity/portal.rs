//! 外部供应商账号绑定与门户操作资格；不采用内部角色或组织范围。

use application_core::AuditActor;
use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::AccountKind;
use erp_core::validation::normalize_required_text;
use serde::{Deserialize, Serialize};

use crate::entity::AccountCore;
use crate::{Error, Result};

/// 门户固定岗位，不授予内部 RBAC 权限。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PortalRole {
    /// 允许维护可供事实及提交申请。
    Maintainer,
    /// 仅查看本人供应商授权范围内的资料。
    ReadOnly,
}

impl PortalRole {
    /// 校验门户写入资格。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 供给维护员返回成功。
    /// # 错误
    /// 只读人员返回稳定权限错误。
    pub fn require_write(self) -> Result<()> {
        match self {
            Self::Maintainer => Ok(()),
            Self::ReadOnly => Err(Error::Forbidden("当前供应商账号只允许查看".into())),
        }
    }
}

/// 每个供应商账号唯一的实名供应商绑定，撤销后保留原身份。
#[derive(Debug, Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct PortalBinding {
    #[serde(flatten)]
    pub base: BaseModel,
    pub account_id: String,
    pub supplier_id: String,
    pub role: PortalRole,
    pub active: bool,
    pub created_by: String,
    pub updated_by: String,
}

impl PortalBinding {
    /// 创建仅绑定一家供应商的外部账号关系。
    ///
    /// # 参数
    /// `id`、`account_id`、`supplier_id` 是稳定身份；`role` 是门户岗位；`actor_id` 是开通人。
    /// # 返回
    /// 已规范化的有效绑定。
    /// # 错误
    /// 任一身份为空或超长时拒绝。
    pub fn new(
        id: String,
        account_id: String,
        supplier_id: String,
        role: PortalRole,
        actor_id: String,
    ) -> Result<Self> {
        let account_id = required_id(account_id, "账号标识")?;
        let supplier_id = required_id(supplier_id, "供应商标识")?;
        let actor_id = required_id(actor_id, "开通人标识")?;
        Ok(Self {
            base: BaseModel::new(required_id(id, "绑定标识")?),
            account_id,
            supplier_id,
            role,
            active: true,
            created_by: actor_id.clone(),
            updated_by: actor_id,
        })
    }

    /// 在已核对的绑定版本上调整岗位或撤销绑定。
    ///
    /// # 参数
    /// `expected_version` 为读取版本；`role`、`active` 为目标状态；`actor_id` 为内部操作人。
    /// # 返回
    /// 更新已应用的绑定；身份和原始开通人保持稳定。
    /// # 错误
    /// 版本过期、绑定删除或操作人无效时拒绝。
    pub fn update(
        &mut self,
        expected_version: u64,
        role: PortalRole,
        active: bool,
        actor_id: String,
    ) -> Result<()> {
        if self.base.is_deleted() || self.base.version != expected_version {
            return Err(Error::ConflictError("供应商账号绑定已变化，请刷新后重试".into()));
        }
        self.updated_by = required_id(actor_id, "操作人标识")?;
        self.role = role;
        self.active = active;
        Ok(())
    }

    /// 将当前账号与绑定收窄为可信门户身份。
    ///
    /// # 参数
    /// `account` 为当前账号；`login` 和两个版本为已签发会话身份。
    /// # 返回
    /// 当前岗位及供应商归属，均取自持久化事实。
    /// # 错误
    /// 账号、类型、状态、绑定或任一版本失效时返回统一认证错误。
    pub fn identity(
        &self,
        account: &AccountCore,
        login: &str,
        account_version: u64,
        binding_version: u64,
    ) -> Result<PortalActor> {
        if !self.active
            || self.base.is_deleted()
            || self.base.version != binding_version
            || self.account_id != account.base.id
            || account.base.is_deleted()
            || !account.matches_session_identity(login, AccountKind::Supplier, account_version)
        {
            return Err(Error::Unauthenticated("认证已失效".into()));
        }
        Ok(PortalActor {
            account_id: account.base.id.clone(),
            account: account.secret.account().to_string(),
            name: account.name.clone(),
            supplier_id: self.supplier_id.clone(),
            role: self.role,
            account_version: account.base.version,
            binding_version: self.base.version,
        })
    }
}

/// 从当前账号及有效绑定解析的供应商操作人，不携带内部组织或角色。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PortalActor {
    pub account_id: String,
    pub account: String,
    pub name: String,
    pub supplier_id: String,
    pub role: PortalRole,
    pub account_version: u64,
    pub binding_version: u64,
}

impl PortalActor {
    /// 校验当前门户岗位是否允许写入。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 供给维护员返回成功。
    /// # 错误
    /// 只读人员返回权限错误。
    pub fn require_write(&self) -> Result<()> {
        self.role.require_write()
    }

    /// 生成保留供应商身份类别的审计操作人。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 真实外部操作人；不授予后台资格。
    /// # 错误
    /// 无。
    pub fn audit_actor(&self) -> AuditActor {
        AuditActor::new(self.account_id.clone(), self.account.clone(), AccountKind::Supplier)
    }
}

/// 规范化稳定身份，避免无效绑定进入持久化。
fn required_id(value: String, field: &str) -> Result<String> {
    Ok(normalize_required_text(value, &format!("{field}不能为空"), 128, &format!("{field}过长"))?)
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;

    use super::{PortalBinding, PortalRole};
    use crate::{AccountCore, AccountCoreData, AccountStatus, LoginAccount, Secret};

    fn account() -> AccountCore {
        AccountCore::new(
            "external-1".into(),
            AccountCoreData {
                secret: Secret::new(LoginAccount::new("supplier01").unwrap(), "password123").unwrap(),
                name: "供应商联系人".into(),
                kind: AccountKind::Supplier,
                status: AccountStatus::Active,
                email: None,
                phone: None,
                avatar: None,
            },
        )
        .unwrap()
    }

    fn binding() -> PortalBinding {
        PortalBinding::new(
            "binding-1".into(),
            "external-1".into(),
            "supplier-1".into(),
            PortalRole::Maintainer,
            "buyer-1".into(),
        )
        .unwrap()
    }

    #[test]
    fn roles_separate_write_qualification_and_external_audit_identity() {
        let actor = binding().identity(&account(), "supplier01", 1, 1).unwrap();
        assert!(actor.require_write().is_ok());
        assert_eq!(actor.supplier_id, "supplier-1");
        assert_eq!(actor.audit_actor().kind(), AccountKind::Supplier);
        assert!(PortalRole::ReadOnly.require_write().is_err());
        assert!(!account().is_active_backoffice());
    }

    #[test]
    fn session_rejects_changed_identity_status_binding_and_versions() {
        let account = account();
        let binding = binding();
        assert!(binding.identity(&account, "supplier01", 1, 1).is_ok());
        assert!(binding.identity(&account, "supplier01", 2, 1).is_err());
        assert!(binding.identity(&account, "supplier01", 1, 2).is_err());
        assert!(binding.identity(&account, "other", 1, 1).is_err());
        let mut disabled = account.clone();
        disabled.status = AccountStatus::Suspended;
        assert!(binding.identity(&disabled, "supplier01", 1, 1).is_err());
        disabled = account.clone();
        disabled.kind = AccountKind::Admin;
        assert!(binding.identity(&disabled, "supplier01", 1, 1).is_err());
        let mut revoked = binding.clone();
        revoked.active = false;
        assert!(revoked.identity(&account, "supplier01", 1, 1).is_err());
        revoked = binding;
        revoked.account_id = "another-account".into();
        assert!(revoked.identity(&account, "supplier01", 1, 1).is_err());
    }

    #[test]
    fn management_requires_exact_version_and_preserves_supplier_binding() {
        let mut binding = binding();
        assert!(binding.update(2, PortalRole::ReadOnly, false, "manager".into()).is_err());
        assert!(binding.active);
        binding.update(1, PortalRole::ReadOnly, false, "manager".into()).unwrap();
        assert_eq!(binding.supplier_id, "supplier-1");
        assert_eq!(binding.created_by, "buyer-1");
        assert_eq!(binding.updated_by, "manager");
        assert!(!binding.active);
    }

    #[test]
    fn binding_rejects_empty_or_oversized_identity() {
        assert!(
            PortalBinding::new(
                "id".into(),
                " ".into(),
                "supplier".into(),
                PortalRole::ReadOnly,
                "buyer".into()
            )
            .is_err()
        );
        assert!(
            PortalBinding::new(
                "id".into(),
                "user".into(),
                "x".repeat(129),
                PortalRole::ReadOnly,
                "buyer".into()
            )
            .is_err()
        );
    }
}
