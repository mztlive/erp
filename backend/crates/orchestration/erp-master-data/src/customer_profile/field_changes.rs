//! 从属事实写入使用其各自既有操作资格。
use application_core::AuditActor;
use erp_customer::{CustomerProfileOperation, SaveCustomerProfileRequest};
use erp_identity::AccessTarget;
use erp_permissions::{Permission, party};
use persistence_core::Executor;

use super::CustomerProfileProcess;
use crate::Result;

/// 从请求中的实际字段变更确定既有从属资源操作。
/// # 参数
/// req 为已校验请求，operation 为当前根命令类型。
/// # 返回
/// 缺省集合不产生写入资格；空集合仍保留该集合的修改资格。
/// # 错误
/// 不执行 I/O。
pub(super) fn permissions(
    req: &SaveCustomerProfileRequest,
    operation: CustomerProfileOperation,
) -> Vec<Permission> {
    let mut permissions = Vec::new();
    for (present, create, detail, update) in [
        (req.contacts.is_some(), party::CONTACT_CREATE, party::CONTACT_DETAIL, party::CONTACT_UPDATE),
        (req.addresses.is_some(), party::ADDRESS_CREATE, party::ADDRESS_DETAIL, party::ADDRESS_UPDATE),
        (
            req.bank_accounts.is_some(),
            party::BANK_ACCOUNT_CREATE,
            party::BANK_ACCOUNT_DETAIL,
            party::BANK_ACCOUNT_UPDATE,
        ),
    ] {
        if present {
            match operation {
                CustomerProfileOperation::Create => permissions.push(create),
                CustomerProfileOperation::Update => permissions.extend([detail, update]),
            }
        }
    }
    permissions
}
impl CustomerProfileProcess {
    /// 每个实际被修改的从属资源在原事务单独完成授权。
    /// # 参数
    /// actor 为当前账号，permissions 为服务端按字段变更确定的权限，executor 为原事务。
    /// # 返回
    /// 全部独立操作资格成立时成功。
    /// # 错误
    /// 任一资格缺失或身份设施失败时拒绝。
    pub(super) async fn require_fields(
        &self,
        actor: &AuditActor,
        permissions: &[Permission],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        for permission in permissions {
            self.authorization.require(actor, permission, AccessTarget::Operation, executor).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    #[test]
    fn empty_update_collection_requires_both_detail_and_update_while_omitted_collection_requires_neither() {
        let mut req: SaveCustomerProfileRequest = serde_json::from_value(json!({
            "idempotency_key":"key", "legal_name":"客户", "effective_from":"2026-11-15", "change_reason":"修订"
        })).unwrap();
        assert!(permissions(&req, CustomerProfileOperation::Update).is_empty());
        req.contacts = Some(vec![]);
        assert_eq!(
            permissions(&req, CustomerProfileOperation::Update),
            [party::CONTACT_DETAIL, party::CONTACT_UPDATE]
        );
        assert_eq!(permissions(&req, CustomerProfileOperation::Create), [party::CONTACT_CREATE]);
    }
}
