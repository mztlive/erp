use application_core::AuditActor;
use erp_identity::AccessTarget;
use erp_supply::entity::supplier_api::SupplierConnectionAction;
use erp_supply::ports::supplier_api_gateway::ClassifiedError;
use erp_supply::ports::supplier_reference_registry::{
    SupplierReferenceKind, SupplierReferenceOption, SupplierReferenceTarget,
};
use erp_supply::repository::SupplierApiExt;
use erp_supply::service::supplier_api::context::action_permission;
use persistence_core::{Executor, Transactional};

use super::SupplierApiReadService;
use crate::{Error, Result};

impl SupplierApiReadService {
    /// 依据当前连接记录签发配置选择项；只返回安全别名和短时票据。
    ///
    /// # 参数
    /// `id` 为后台连接 ID，`kind` 为配置种类，`actor` 为已认证操作人。
    ///
    /// # 返回
    /// 适用于连接环境的配置选项，未登记配置时为空。
    ///
    /// # 错误
    /// 无绑定权限、连接不存在、种类不支持或注册表失败时返回错误。
    pub async fn reference_options_for_actor(
        &self,
        id: &str,
        kind: SupplierReferenceKind,
        actor: &AuditActor,
    ) -> Result<Vec<SupplierReferenceOption>> {
        let service = self.clone();
        let id = id.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.reference_options_snapshot(&id, kind, &actor, executor).await })
            })
            .await
    }

    // 读取供应连接时，在同一资格快照中读取连接并生成进程内配置选项。
    //
    // 调用参数：
    // id、kind 为真实连接与配置种类，actor 为当前账号，executor 为原事务。
    //
    // 返回相应绑定资格允许的安全配置别名和短时票据。
    //
    // 资格不足、连接缺失或进程内注册表失败时拒绝。
    pub(super) async fn reference_options_snapshot(
        &self,
        id: &str,
        kind: SupplierReferenceKind,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierReferenceOption>> {
        let action = reference_action(kind)?;
        self.authorization
            .require(actor, &action_permission(action), AccessTarget::Operation, executor)
            .await?;

        let connection = self
            .db
            .supplier_api_connections()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("连接不存在".into()))?;

        if let Some(registry) = self.reference_registry.as_ref() {
            registry
                .options(kind, &SupplierReferenceTarget::from(&connection))
                .await
                .map_err(|failure| reference_options_error(kind, &failure))
        } else {
            Ok(Vec::new())
        }
    }
}

fn reference_options_error(kind: SupplierReferenceKind, failure: &ClassifiedError) -> Error {
    tracing::warn!(code = %failure.code, class = ?failure.class, "供应商连接配置选项不可读取");
    let message = match kind {
        SupplierReferenceKind::Endpoint => "暂时无法获取连接地址选项，请联系管理员核对供应商连接设置",
        SupplierReferenceKind::Credential => "暂时无法获取授权凭据选项，请联系管理员核对供应商授权设置",
        SupplierReferenceKind::BusinessProfile => "暂时无法获取商务配置选项，请联系管理员核对供应商设置",
    };
    Error::BusinessLogicError(message.into())
}

fn reference_action(kind: SupplierReferenceKind) -> Result<SupplierConnectionAction> {
    match kind {
        SupplierReferenceKind::Endpoint => Ok(SupplierConnectionAction::BindEndpointReference),
        SupplierReferenceKind::Credential => Ok(SupplierConnectionAction::BindCredentialReference),
        SupplierReferenceKind::BusinessProfile => Err(Error::ValidationError(
            "此处只能选择连接地址或授权凭据，请返回供应商连接设置选择配置类型".into(),
        )),
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
