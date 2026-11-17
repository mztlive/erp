//! 新建供应连接的类型目录，在原快照内验证创建功能资格。
use application_core::AuditActor;
use erp_identity::AccessTarget;
use erp_permissions::supplier_api;
use erp_supply::dto::supplier_api::SupplierConnectorTypeOption;
use erp_supply::entity::supplier_api::SupplierConnectorType;
use persistence_core::{Executor, Transactional};

use super::SupplierApiReadService;
use crate::Result;

impl SupplierApiReadService {
    /// 返回当前程序已实现的连接类型。
    ///
    /// # 参数
    /// `actor` 为当前已认证的内部账号。
    ///
    /// # 返回
    /// 稳定类型编码及中文名称；不表示部署已配置对应凭据。
    ///
    /// # 错误
    /// 缺少创建连接资格、身份无效或授权读取失败时拒绝。
    pub async fn connector_types(&self, actor: &AuditActor) -> Result<Vec<SupplierConnectorTypeOption>> {
        let service = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { service.connector_types_snapshot(&actor, executor).await })
            })
            .await
    }

    /// 验证创建资格后返回本程序固定支持的协议，不读取外部服务。
    async fn connector_types_snapshot(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierConnectorTypeOption>> {
        self.authorization
            .require(actor, &supplier_api::CONNECTION_CREATE, AccessTarget::Operation, executor)
            .await?;
        Ok(SupplierConnectorType::all().into_iter().map(SupplierConnectorTypeOption::from).collect())
    }
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_identity::entity::access_control::ResolvedScope;
    use erp_identity::testing::fixed_authorization;
    use erp_permissions::Permission;
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;
    use serde_json::to_value;

    use super::*;
    use crate::Error;

    async fn service(permissions: &[Permission]) -> SupplierApiReadService {
        let client = mongodb::Client::with_options(Default::default()).unwrap();
        let authorization = fixed_authorization(
            permissions,
            ResolvedScope { role_clauses: vec![], user_limit: None },
            vec![],
            vec![],
        )
        .unwrap();
        SupplierApiReadService::new(client.database("connector_type_authorization"), authorization)
    }

    fn actor() -> AuditActor {
        AuditActor::new("buyer".into(), "采购".into(), AccountKind::Admin)
    }

    #[tokio::test]
    async fn catalog_returns_supported_codes_only_after_creation_qualification() {
        let service = service(&[supplier_api::CONNECTION_CREATE]).await;
        let options =
            service.connector_types_snapshot(&actor(), &mut SnapshotExecutor::default()).await.unwrap();
        let value = to_value(options).unwrap();
        assert_eq!(value[0]["code"], "dangaoshushu");
        assert_eq!(value[0]["label"], "蛋糕叔叔 API");
        assert!(value[0].get("capabilities").is_none());
    }

    #[tokio::test]
    async fn catalog_rejects_missing_creation_qualification_or_snapshot() {
        let denied = service(&[supplier_api::CONNECTION_LIST]).await;
        assert!(matches!(
            denied.connector_types_snapshot(&actor(), &mut SnapshotExecutor::default()).await,
            Err(Error::Forbidden(_))
        ));
        let allowed = service(&[supplier_api::CONNECTION_CREATE]).await;
        assert!(matches!(
            allowed.connector_types_snapshot(&actor(), &mut ReadExecutor::default()).await,
            Err(Error::ValidationError(_))
        ));
    }
}
