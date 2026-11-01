use application_core::AuditActor;
use erp_supply::entity::supplier_api::SupplierConnectionAction;
use erp_supply::ports::supplier_reference_registry::{
    SupplierReferenceKind, SupplierReferenceOption, SupplierReferenceTarget,
};
use persistence_core::NoTransaction;

use super::SupplierApiReadService;
use crate::{Error, Result};

impl SupplierApiReadService {
    /// 按当前连接记录签发配置选择项；只返回安全别名和短时票据。
    /// # 参数
    /// `id` 为后台连接 ID，`kind` 为配置种类，`actor` 为已认证操作人。
    /// # 返回
    /// 适用于连接环境的配置选项，未登记配置时为空。
    /// # 错误
    /// 无绑定权限、连接不存在、种类不支持或注册表失败时返回错误。
    pub async fn reference_options_for_actor(
        &self,
        id: &str,
        kind: SupplierReferenceKind,
        actor: &AuditActor,
    ) -> Result<Vec<SupplierReferenceOption>> {
        let action = reference_action(kind)?;
        if !self.has_action_permission(actor, action).await? {
            return Err(Error::Forbidden("当前账号没有绑定该配置的权限".into()));
        }
        let connection = self.domain().load_connection(id, &mut NoTransaction).await?;
        let Some(registry) = self.reference_registry.as_ref() else {
            return Ok(Vec::new());
        };
        registry
            .options(kind, &SupplierReferenceTarget::from(&connection))
            .await
            .map_err(|failure| Error::BusinessLogicError(format!("{}: {}", failure.code, failure.summary)))
    }
}

fn reference_action(kind: SupplierReferenceKind) -> Result<SupplierConnectionAction> {
    match kind {
        SupplierReferenceKind::Endpoint => Ok(SupplierConnectionAction::BindEndpointReference),
        SupplierReferenceKind::Credential => Ok(SupplierConnectionAction::BindCredentialReference),
        SupplierReferenceKind::BusinessProfile => {
            Err(Error::ValidationError("此入口仅提供地址和密钥配置".into()))
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_require_the_corresponding_binding_permission() {
        assert_eq!(
            reference_action(SupplierReferenceKind::Endpoint).unwrap(),
            SupplierConnectionAction::BindEndpointReference
        );
        assert_eq!(
            reference_action(SupplierReferenceKind::Credential).unwrap(),
            SupplierConnectionAction::BindCredentialReference
        );
        assert!(reference_action(SupplierReferenceKind::BusinessProfile).is_err());
    }
}
