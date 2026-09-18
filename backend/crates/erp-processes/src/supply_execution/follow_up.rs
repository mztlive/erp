//! 从固定供应关系解析内部跟进人；缺映射失败关闭。

use erp_core::common::time::Instant;
use erp_core::ids::SupplierAccountId;
use erp_identity::AccessControlExt;
use erp_identity::entity::organization_change::OrganizationState;
use erp_identity::repository::OrganizationRepository;
use erp_supplier::repository::prelude::*;
use erp_supplier::{CapabilityCode, SupplierExt};
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 固定供应关系解析出的内部跟进人及其主属组织。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FollowUpAssignment {
    /// 合格内部人员。
    pub user_id: String,
    /// 该人员有效主属组织，禁止公司根。
    pub org_unit_id: String,
}

/// 从供应商 API 能力负责人或整体维护人解析唯一内部跟进人。
///
/// # 参数
/// * `db` - 供应商与身份集合
/// * `supplier_id` - 固定供应商
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回跟进人及其主属组织。
///
/// # 错误
/// 缺映射、账号无效或组织为公司根时拒绝。
pub async fn resolve_follow_up(
    db: &Database,
    supplier_id: &SupplierAccountId,
    executor: &mut dyn Executor,
) -> Result<FollowUpAssignment> {
    let supplier = db
        .supplier_accounts()
        .find_by_id(supplier_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("供应商不存在".into()))?;
    let capability = db
        .supplier_capabilities()
        .find_by_supplier_and_code(supplier_id, CapabilityCode::Api, executor)
        .await?;
    let user_id = pick_follow_up_user(
        capability.as_ref().map(|item| item.owner_user_id.as_str()),
        Some(supplier.maintainer_user_id.as_str()),
    )?;
    ensure_internal_person(db, &user_id, executor).await?;
    let org = own_org(db, &user_id, executor).await?;
    Ok(FollowUpAssignment { user_id, org_unit_id: org })
}

/// 在能力负责人与供应商维护人中取唯一内部跟进人。
pub fn pick_follow_up_user(capability_owner: Option<&str>, maintainer: Option<&str>) -> Result<String> {
    let capability = capability_owner.map(str::trim).filter(|value| !value.is_empty());
    let maintainer = maintainer.map(str::trim).filter(|value| !value.is_empty());
    match (capability, maintainer) {
        (Some(owner), _) => Ok(owner.to_string()),
        (None, Some(owner)) => Ok(owner.to_string()),
        (None, None) => {
            Err(Error::ValidationError("无法解析供应商订单内部跟进人：固定供应关系缺少合格映射".into()))
        },
    }
}

/// 拒绝公司根作为内部组织。
pub fn reject_company_org(org: &str) -> Result<()> {
    if org.trim().is_empty() || org.eq_ignore_ascii_case("company") {
        return Err(Error::ValidationError("跟进人缺少有效内部组织，禁止写入公司根".into()));
    }
    Ok(())
}

async fn ensure_internal_person(db: &Database, user_id: &str, executor: &mut dyn Executor) -> Result<()> {
    let Some(account) = db.accounts().find_by_id(user_id, executor).await? else {
        return Err(Error::ValidationError("跟进人不是合格内部人员".into()));
    };
    if !account.is_active_backoffice() {
        return Err(Error::ValidationError("禁止用系统账号、停用账号或供应商联系人作为跟进人".into()));
    }
    Ok(())
}

async fn own_org(db: &Database, user_id: &str, executor: &mut dyn Executor) -> Result<String> {
    let state: OrganizationState = OrganizationRepository::new(db).state(executor).await?;
    let org = state
        .own_org(user_id, Instant::now())?
        .map(str::to_string)
        .ok_or_else(|| Error::ValidationError("跟进人缺少有效主属组织".into()))?;
    reject_company_org(&org)?;
    Ok(org)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_mapping_fail_closed() {
        match pick_follow_up_user(None, Some("  ")) {
            Err(Error::ValidationError(message)) => assert!(message.contains("缺少合格映射")),
            other => panic!("expected mapping error, got {other:?}"),
        }
        assert_eq!(pick_follow_up_user(Some("cap-1"), Some("maint-1")).unwrap(), "cap-1");
        assert_eq!(pick_follow_up_user(None, Some("maint-1")).unwrap(), "maint-1");
    }

    #[test]
    fn company_org_is_rejected() {
        assert!(reject_company_org("company").is_err());
        assert!(reject_company_org("Company").is_err());
        assert!(reject_company_org("").is_err());
        assert!(reject_company_org("org-procurement").is_ok());
    }
}
