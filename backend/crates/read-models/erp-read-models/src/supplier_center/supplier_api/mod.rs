//! 供应连接详情、动作权限和后台任务只读投影。
use std::sync::Arc;

use erp_identity::AuthorizationService;
use erp_supply::ports::supplier_api_gateway::SupplierApiGateway;
use erp_supply::ports::supplier_reference_registry::SupplierReferenceRegistry;
use erp_supply::service::supplier_api::SupplierApiService;
use mongodb::Database;
mod catalog;
mod connector_types;
mod context;
pub use catalog::{CatalogQuery, CatalogView};
pub mod dto;
mod jobs;
mod protocol;
mod query;
mod references;
/// 供应连接跨域只读入口。
#[derive(Clone)]
pub struct SupplierApiReadService {
    db: Database,
    authorization: Arc<AuthorizationService>,
    reference_registry: Option<Arc<dyn SupplierReferenceRegistry>>,
    gateway: Option<Arc<dyn SupplierApiGateway>>,
}

impl SupplierApiReadService {
    /// 复用应用数据库与权威身份服务。
    ///
    /// # 参数
    /// * `db` - 应用数据库。
    /// * `authorization` - 当前共享授权服务。
    ///
    /// # 返回
    /// 返回已绑定身份服务的读取服务。构造不执行查询。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database, authorization: Arc<AuthorizationService>) -> Self {
        Self { db, authorization, reference_registry: None, gateway: None }
    }

    /// 复用组合根的引用注册表；不创建新的外部连接器。
    ///
    /// # 参数
    /// * `registry` - 组合根的引用注册表。
    ///
    /// # 返回
    /// 消耗当前服务并返回已写入注册表的同一服务。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn with_reference_registry(mut self, registry: Arc<dyn SupplierReferenceRegistry>) -> Self {
        self.reference_registry = Some(registry);
        self
    }
    /// 注入与执行器相同的协议能力读取网关。
    /// # 参数
    /// `gateway` 为组合根注册表，不执行外部网络请求。
    /// # 返回
    /// 能力展示使用适配器策略的读取服务。
    /// # 错误
    /// 不返回错误。
    pub fn with_gateway(mut self, gateway: Arc<dyn SupplierApiGateway>) -> Self {
        self.gateway = Some(gateway);
        self
    }

    fn domain(&self) -> SupplierApiService {
        SupplierApiService::new(self.db.clone())
    }
}

#[cfg(test)]
mod tests {
    use application_core::AuditActor;
    use erp_core::AccountKind;
    use erp_identity::entity::access_control::{ResolvedScope, ScopeClause};
    use erp_identity::testing::{fixed_authorization, unbound_authorization};
    use erp_identity::{Error as IdentityError, Permission};
    use erp_permissions::{internal, supplier, supplier_api};
    use erp_supply::dto::supplier_api::SupplierApiConnectionListParams;
    use erp_supply::ports::supplier_reference_registry::SupplierReferenceKind;
    use erp_supply::repository::supplier_api::connection_mapping;
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::{CatalogQuery, SupplierApiReadService};
    use crate::Error;

    async fn service() -> SupplierApiReadService {
        let client =
            mongodb::Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1").await.unwrap();
        SupplierApiReadService::new(client.database("connection_authorization"), unbound_authorization())
    }

    fn actor() -> AuditActor {
        AuditActor::new("buyer".into(), "采购".into(), AccountKind::Admin)
    }

    async fn service_with_permissions(permissions: &[Permission]) -> SupplierApiReadService {
        let service = service().await;
        let (_, supplier) = erp_supplier::authorization::bindings(service.db.clone()).unwrap();
        let connection = connection_mapping(supplier).unwrap();
        let authorization = fixed_authorization(
            permissions,
            ResolvedScope {
                role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
                user_limit: None,
            },
            Vec::new(),
            vec![
                (supplier_api::CONNECTION_DETAIL, connection.clone()),
                (internal::CONNECTION_CATALOG_READ, connection),
            ],
        )
        .unwrap();
        SupplierApiReadService::new(service.db, authorization)
    }

    #[tokio::test]
    async fn list_and_detail_reject_missing_authorization_before_loading_connections() {
        let service = service().await;
        let list = service
            .connection_list_snapshot(
                &SupplierApiConnectionListParams::default(),
                &actor(),
                &mut SnapshotExecutor::default(),
            )
            .await;
        assert!(matches!(list, Err(Error::Internal(message)) if message == "测试授权设施不可用"));

        let detail = service
            .connection_detail_snapshot("connection", &actor(), &mut SnapshotExecutor::default())
            .await;
        assert!(matches!(detail, Err(Error::Internal(reason)) if reason == "测试授权设施不可用"));
    }

    #[tokio::test]
    async fn catalog_rejects_missing_authorization_before_loading_items_or_jobs() {
        let query = serde_json::from_value::<CatalogQuery>(serde_json::json!({})).unwrap();
        let result = service()
            .await
            .catalog_snapshot("connection", &query, &actor(), &mut SnapshotExecutor::default())
            .await;
        assert!(matches!(result, Err(Error::Internal(reason)) if reason == "测试授权设施不可用"));
    }

    #[tokio::test]
    async fn display_qualifications_precede_the_final_query_in_the_same_snapshot() {
        for visible in [false, true] {
            let mut permissions = vec![supplier_api::CONNECTION_DETAIL, supplier::DETAIL];
            if visible {
                permissions.extend([
                    supplier_api::REFERENCES_OPTIONS,
                    internal::CONNECTION_UPDATE_BUSINESS_PROFILE,
                    internal::SUPPLIER_OFFERING_COST_DETAIL,
                ]);
            }
            let service = service_with_permissions(&permissions).await;
            let executor = &mut SnapshotExecutor::default();
            let (access, display) = service.connection_detail_access(&actor(), executor).await.unwrap();
            assert_eq!(display.covers_one(&supplier_api::REFERENCES_OPTIONS), visible);
            assert_eq!(display.covers_one(&internal::CONNECTION_UPDATE_BUSINESS_PROFILE), visible);
            service.authorization.validate(&access, executor).await.unwrap();

            let (access, can_read_cost) = service.catalog_access(&actor(), executor).await.unwrap();
            assert_eq!(can_read_cost, visible);
            service.authorization.validate(&access, executor).await.unwrap();
        }
    }

    #[tokio::test]
    async fn display_permissions_do_not_bypass_final_connection_query_qualification() {
        let service = service_with_permissions(&[
            supplier_api::REFERENCES_OPTIONS,
            internal::CONNECTION_UPDATE_BUSINESS_PROFILE,
            internal::SUPPLIER_OFFERING_COST_DETAIL,
        ])
        .await;
        let detail = service.connection_detail_access(&actor(), &mut SnapshotExecutor::default()).await;
        assert!(matches!(detail, Err(Error::Forbidden(_))));

        let catalog = service.catalog_access(&actor(), &mut SnapshotExecutor::default()).await;
        assert!(matches!(catalog, Err(Error::Forbidden(_))));
    }

    #[tokio::test]
    async fn detail_and_catalog_refuse_non_snapshot_execution_before_business_reads() {
        let service = service().await;
        let detail =
            service.connection_detail_snapshot("connection", &actor(), &mut ReadExecutor::default()).await;
        assert!(
            matches!(detail, Err(Error::ValidationError(reason)) if reason == "供应连接详情读取必须使用快照事务")
        );

        let query = serde_json::from_value::<CatalogQuery>(serde_json::json!({})).unwrap();
        let catalog =
            service.catalog_snapshot("connection", &query, &actor(), &mut ReadExecutor::default()).await;
        assert!(
            matches!(catalog, Err(Error::ValidationError(reason)) if reason == "供应连接目录读取必须使用快照事务")
        );
    }

    #[tokio::test]
    async fn final_validation_rejects_detail_and_catalog_credentials_after_a_new_query() {
        let service = service_with_permissions(&[supplier_api::CONNECTION_DETAIL, supplier::DETAIL]).await;
        let executor = &mut SnapshotExecutor::default();
        let (detail, _) = service.connection_detail_access(&actor(), executor).await.unwrap();
        let (catalog, _) = service.catalog_access(&actor(), executor).await.unwrap();
        let result = service.authorization.validate(&detail, executor).await;
        assert!(
            matches!(result, Err(IdentityError::ValidationError(reason)) if reason == "授权结果不能跨事务或读取阶段使用")
        );

        service.connection_detail_access(&actor(), executor).await.unwrap();
        let result = service.authorization.validate(&catalog, executor).await;
        assert!(
            matches!(result, Err(IdentityError::ValidationError(reason)) if reason == "授权结果不能跨事务或读取阶段使用")
        );
    }

    #[tokio::test]
    async fn reference_options_require_binding_qualification_before_resolving_registry_options() {
        let service = service().await;
        for kind in [SupplierReferenceKind::Endpoint, SupplierReferenceKind::Credential] {
            let result = service
                .reference_options_snapshot("connection", kind, &actor(), &mut SnapshotExecutor::default())
                .await;
            assert!(matches!(result, Err(Error::Internal(reason)) if reason == "测试授权设施不可用"));
        }
    }
}
