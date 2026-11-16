//! 供应连接详情、动作权限和后台任务只读投影。
use std::sync::Arc;

use erp_identity::AuthorizationService;
use erp_supply::ports::supplier_api_gateway::SupplierApiGateway;
use erp_supply::ports::supplier_reference_registry::SupplierReferenceRegistry;
use erp_supply::service::supplier_api::SupplierApiService;
use mongodb::Database;
mod catalog;
mod context;
pub use catalog::{CatalogQuery, CatalogView};
pub mod dto;
mod jobs;
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
    use erp_identity::testing::unbound_authorization;
    use erp_supply::dto::supplier_api::SupplierApiConnectionListParams;
    use erp_supply::ports::supplier_reference_registry::SupplierReferenceKind;
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
        let detail = service.connection_detail_for_actor("connection", &actor()).await;
        assert!(matches!(detail, Err(Error::ValidationError(reason)) if reason == "缺少查询 Mapping 绑定"));
    }

    #[tokio::test]
    async fn catalog_rejects_missing_authorization_before_loading_items_or_jobs() {
        let query = serde_json::from_value::<CatalogQuery>(serde_json::json!({})).unwrap();
        let result = service().await.catalog("connection", query, &actor()).await;
        assert!(matches!(result, Err(Error::ValidationError(reason)) if reason == "缺少查询 Mapping 绑定"));
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
