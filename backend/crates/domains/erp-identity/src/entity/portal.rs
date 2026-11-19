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
    /// 仅允许维护岗位执行门户写入，只读岗位返回固定的权限错误。
    pub fn require_write(self) -> Result<()> {
        match self {
            Self::Maintainer => Ok(()),
            Self::ReadOnly => Err(Error::Forbidden("当前供应商账号只允许查看".into())),
        }
    }
}

/// 供应商账号与单家供应商的唯一实名绑定，撤销后保留原账号及供应商归属。
#[derive(Debug, Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct PortalBinding {
    /// 持久化元数据，含绑定 ID、版本及软删除标记。
    #[serde(flatten)]
    pub base: BaseModel,
    /// 绑定的外部账号稳定 ID。
    pub account_id: String,
    /// 该账号在门户中代表的唯一供应商 ID。
    pub supplier_id: String,
    /// 绑定当前授予的门户岗位。
    pub role: PortalRole,
    /// 绑定是否启用；撤销后无法解析门户身份。
    pub active: bool,
    /// 首次开通绑定的内部操作人 ID。
    pub created_by: String,
    /// 最近修改绑定的内部操作人 ID，开通时与创建人相同。
    pub updated_by: String,
}

impl PortalBinding {
    /// 开通时按给定岗位创建启用绑定，并记录内部开通人。
    /// 绑定、账号、供应商和开通人 ID 均裁剪首尾空白，空值或超过 128 字符时拒绝。
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

    /// 按读取版本调整岗位或启停绑定，更新操作人并保留账号、供应商和原开通人。
    /// 版本不符或绑定已删除时报冲突，内部操作人 ID 为空或超长时报校验错误。
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

    /// 登录或会话复验时，核对登录名和两个版本是否匹配启用的供应商账号及其绑定。
    /// 通过后按持久化事实返回岗位和供应商归属；身份、状态、删除标记或版本不符时统一报认证失效。
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

/// 由当前账号及有效绑定解析的门户操作人，身份范围限于所属供应商及门户岗位。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PortalActor {
    /// 外部账号稳定 ID，用于关联操作人及审计记录。
    pub account_id: String,
    /// 当前账号的登录名。
    pub account: String,
    /// 当前账号的显示名称。
    pub name: String,
    /// 门户操作限定的供应商 ID。
    pub supplier_id: String,
    /// 用于判断门户读写资格的当前岗位。
    pub role: PortalRole,
    /// 解析身份时的账号版本，用于会话复验。
    pub account_version: u64,
    /// 解析身份时的绑定版本，用于会话复验。
    pub binding_version: u64,
}

impl PortalActor {
    /// 在门户写入前检查操作人岗位，只读岗位返回权限错误。
    pub fn require_write(&self) -> Result<()> {
        self.role.require_write()
    }

    /// 用账号 ID 和登录名生成审计操作人，身份类别固定为外部供应商。
    pub fn audit_actor(&self) -> AuditActor {
        AuditActor::new(self.account_id.clone(), self.account.clone(), AccountKind::Supplier)
    }
}

// 写入门户绑定前裁剪标识首尾空白，按字段名报告空值或超过 128 字符的错误。
fn required_id(value: String, field: &str) -> Result<String> {
    Ok(normalize_required_text(value, &format!("{field}不能为空"), 128, &format!("{field}过长"))?)
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;

    use super::{PortalBinding, PortalRole};
    use crate::{AccountCore, AccountCoreData, AccountStatus, LoginAccount, Secret};

    // 构造启用的供应商账号，供门户身份和会话复验测试使用。
    fn account() -> AccountCore {
        AccountCore::new(
            "external-1".into(),
            AccountCoreData {
                secret: Secret::new(LoginAccount::new("supplier01").unwrap(), "password123").unwrap(),
                name: "供应商联系人".into(),
                kind: AccountKind::Supplier,
                status: AccountStatus::Active,
            },
        )
        .unwrap()
    }

    // 为测试账号开通指定供应商的维护岗位绑定。
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

    // 验证门户维护与只读的写入资格有别，且归属和审计保留外部供应商身份。
    #[test]
    fn roles_separate_write_qualification_and_external_audit_identity() {
        let actor = binding().identity(&account(), "supplier01", 1, 1).unwrap();
        assert!(actor.require_write().is_ok());
        assert_eq!(actor.supplier_id, "supplier-1");
        assert_eq!(actor.audit_actor().kind(), AccountKind::Supplier);
        assert!(PortalRole::ReadOnly.require_write().is_err());
        assert!(!account().is_active_backoffice());
    }

    // 验证登录名、任一版本、账号类型或状态、绑定状态或归属变化时会话失效。
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

    // 验证过期版本无法修改绑定，成功撤销时保留供应商和开通人并记录修改人。
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

    // 验证开通绑定时拒绝空白账号 ID 和超长供应商 ID。
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
