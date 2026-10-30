//! 按连接声明的接口类型分派调用；没有注册的类型不得借用其它供应商适配器。
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use erp_supply::entity::failure::SupplierFailureClass;
use erp_supply::entity::supplier_api::{
    SupplierApiCapabilityCode, SupplierApiConnection, SupplierConnectorType, SupplierHealthCheckType,
};
use erp_supply::ports::catalog_sync::CatalogSyncPolicy;
use erp_supply::ports::connector::SupplierConnector;
use erp_supply::ports::connector::common::{ConnectorError, ConnectorResult, Lookup, Scan, Snapshot};
use erp_supply::ports::connector::order::{OrderKey, OrderLine, OrderSnapshot};
use erp_supply::ports::supplier_api_gateway::{
    CatalogRead, CatalogScopeRead, ClassifiedError, SupplierApiGateway,
};
use erp_supply::ports::supplier_reference_registry::{
    ResolvedSupplierReference, SupplierDeploymentConfig, SupplierReferenceKind, SupplierReferenceOption,
    SupplierReferenceRegistry, SupplierReferenceTarget,
};

/// 按服务器连接身份绑定履约能力；新增写操作与既有订单调查分别核验资格。
pub trait SupplierFulfillmentFactory: Send + Sync {
    /// 绑定允许新增创建及未发送确认的连接。
    /// # 参数
    /// `connection` 为服务端读取的当前连接记录。
    /// # 返回
    /// 已绑定供应商、连接及技术账号的窄能力集合。
    /// # 错误
    /// 停用、健康检查失效、技术绑定无效或协议未实现时拒绝。
    fn creation_connector(
        &self,
        connection: &SupplierApiConnection,
    ) -> Result<Arc<dyn SupplierConnector>, ClassifiedError>;

    /// 绑定既有订单调查、跟踪及取消的连接。
    /// # 参数
    /// `connection` 为服务端读取的原连接记录。
    /// # 返回
    /// 不受新增下单开关限制的已绑定能力集合。
    /// # 错误
    /// 删除、账号归属或技术绑定失效时拒绝。
    fn fulfillment_connector(
        &self,
        connection: &SupplierApiConnection,
    ) -> Result<Arc<dyn SupplierConnector>, ClassifiedError>;
}

struct Adapter {
    gateway: Arc<dyn SupplierApiGateway>,
    references: Arc<dyn SupplierReferenceRegistry>,
    fulfillment: Arc<dyn SupplierFulfillmentFactory>,
}

/// 组合根按协议注册适配器；业务身份和部署配置不参与类型推断。
#[derive(Default)]
pub struct SupplierConnectorRegistry {
    adapters: BTreeMap<SupplierConnectorType, Adapter>,
}

impl SupplierConnectorRegistry {
    /// 将同一协议运行时登记为技术网关及配置目录。
    /// # 参数
    /// `kind` 为协议类型，`runtime` 为当前部署构造的共享运行时。
    /// # 返回
    /// 登记成功返回 `Ok(())`。
    /// # 错误
    /// 相同协议重复登记时拒绝覆盖。
    pub fn register<T>(&mut self, kind: SupplierConnectorType, runtime: Arc<T>) -> Result<(), ClassifiedError>
    where
        T: SupplierApiGateway + SupplierReferenceRegistry + SupplierFulfillmentFactory + 'static,
    {
        if self.adapters.contains_key(&kind) {
            return Err(ClassifiedError {
                class: SupplierFailureClass::MappingError,
                code: "SUPPLIER_CONNECTOR_DUPLICATE".into(),
                summary: "供应商接口类型重复登记".into(),
            });
        }
        self.adapters.insert(
            kind,
            Adapter { gateway: runtime.clone(), references: runtime.clone(), fulfillment: runtime },
        );
        Ok(())
    }

    /// 为新建或未发送的确认动作解析当前连接协议。
    /// # 参数
    /// `connection` 为服务端读取的连接，不接受前端指定协议归属。
    /// # 返回
    /// 实际实现的已绑定窄接口。
    /// # 错误
    /// 缺少类型、运行时或写操作资格时返回明确分类错误。
    pub fn creation_connector(
        &self,
        connection: &SupplierApiConnection,
    ) -> Result<Arc<dyn SupplierConnector>, ClassifiedError> {
        self.adapter(connection.connector_type)?.fulfillment.creation_connector(connection)
    }

    /// 为原请求调查、跟踪及取消解析当前连接协议。
    /// # 参数
    /// `connection` 为原订单所属的服务端连接。
    /// # 返回
    /// 已绑定能力集合；新增下单关闭不影响此入口。
    /// # 错误
    /// 缺少类型、运行时或账号绑定失效时返回分类错误。
    pub fn fulfillment_connector(
        &self,
        connection: &SupplierApiConnection,
    ) -> Result<Arc<dyn SupplierConnector>, ClassifiedError> {
        self.adapter(connection.connector_type)?.fulfillment.fulfillment_connector(connection)
    }

    /// 按原订单身份只读调查，不受新增下单开关影响。
    /// # 参数
    /// `connection` 为原连接，`key` 为已持久化商户或供应商单号。
    /// # 返回
    /// 供应商原始订单观察或明确暂不可见。
    /// # 错误
    /// 协议、绑定、读取能力缺失或供应商查询失败时拒绝。
    pub async fn observe_order(
        &self,
        connection: &SupplierApiConnection,
        key: &OrderKey,
    ) -> ConnectorResult<Lookup<Snapshot<OrderSnapshot>>> {
        self.observe_order_with_lines(connection, key, &[]).await
    }

    /// 按原冻结行查询订单并解析可逆的子单数量关联。
    /// # 参数
    /// `connection` 为原连接，`key` 为原单号，`frozen` 为服务端冻结的本执行组行。
    /// # 返回
    /// 与原明细身份精确对应的订单观察；无映射证据不得补造行归属。
    /// # 错误
    /// 绑定、能力或供应商读取失败时返回分类错误。
    pub async fn observe_order_with_lines(
        &self,
        connection: &SupplierApiConnection,
        key: &OrderKey,
        frozen: &[OrderLine],
    ) -> ConnectorResult<Lookup<Snapshot<OrderSnapshot>>> {
        let connector = self.fulfillment_connector(connection).map_err(|error| ConnectorError {
            class: error.class,
            code: error.code,
            summary: error.summary,
            retry_after: None,
        })?;
        connector
            .orders()
            .ok_or_else(|| ConnectorError {
                class: SupplierFailureClass::CapabilityGap,
                code: "ORDER_READ_UNAVAILABLE".into(),
                summary: "当前接口尚未实现订单结果查询，请联系管理员核实原请求".into(),
                retry_after: None,
            })?
            .order_with_lines(key, frozen)
            .await
    }

    fn adapter(&self, kind: Option<SupplierConnectorType>) -> Result<&Adapter, ClassifiedError> {
        let kind = kind.ok_or_else(|| ClassifiedError {
            class: SupplierFailureClass::CapabilityGap,
            code: "CONNECTOR_TYPE_REQUIRED".into(),
            summary: "请联系管理员补充接口类型".into(),
        })?;
        self.adapters.get(&kind).ok_or_else(|| ClassifiedError {
            class: SupplierFailureClass::CapabilityGap,
            code: "SUPPLIER_CONNECTOR_UNAVAILABLE".into(),
            summary: format!("{} 尚未登记接口配置，请联系管理员", kind.label()),
        })
    }
}

impl SupplierApiGateway for SupplierConnectorRegistry {
    fn catalog_scope<'a>(&'a self, connection: &'a SupplierApiConnection) -> CatalogScopeRead<'a> {
        Box::pin(
            async move { self.adapter(connection.connector_type)?.gateway.catalog_scope(connection).await },
        )
    }

    fn catalog_availability_validity(&self, connection: &SupplierApiConnection) -> Option<u64> {
        self.adapter(connection.connector_type)
            .ok()
            .and_then(|adapter| adapter.gateway.catalog_availability_validity(connection))
    }
    fn catalog_connector_types(&self) -> Vec<SupplierConnectorType> {
        self.adapters.keys().copied().collect()
    }

    fn catalog_policy(
        &self,
        connection: &SupplierApiConnection,
    ) -> Result<Option<CatalogSyncPolicy>, ClassifiedError> {
        let Some(adapter) = connection.connector_type.and_then(|kind| self.adapters.get(&kind)) else {
            return Ok(None);
        };
        adapter.gateway.catalog_policy(connection)
    }

    fn catalog_page<'a>(&'a self, connection: &'a SupplierApiConnection, scan: &'a Scan) -> CatalogRead<'a> {
        Box::pin(async move {
            self.adapter(connection.connector_type)?.gateway.catalog_page(connection, scan).await
        })
    }

    fn health_check<'a>(
        &'a self,
        connection: &'a SupplierApiConnection,
        check_type: SupplierHealthCheckType,
    ) -> Pin<Box<dyn Future<Output = Result<(), ClassifiedError>> + Send + 'a>> {
        Box::pin(async move {
            self.adapter(connection.connector_type)?.gateway.health_check(connection, check_type).await
        })
    }
}

impl SupplierReferenceRegistry for SupplierConnectorRegistry {
    fn capabilities(
        &self,
        target: &SupplierReferenceTarget,
    ) -> Result<Vec<SupplierApiCapabilityCode>, ClassifiedError> {
        self.adapter(target.connector_type)?.references.capabilities(target)
    }

    fn deployment_config(
        &self,
        target: &SupplierReferenceTarget,
    ) -> Result<SupplierDeploymentConfig, ClassifiedError> {
        self.adapter(target.connector_type)?.references.deployment_config(target)
    }

    fn is_available(&self) -> bool {
        !self.adapters.is_empty()
    }

    fn options<'a>(
        &'a self,
        kind: SupplierReferenceKind,
        target: &'a SupplierReferenceTarget,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<SupplierReferenceOption>, ClassifiedError>> + Send + 'a>>
    {
        Box::pin(async move { self.adapter(target.connector_type)?.references.options(kind, target).await })
    }

    fn resolve<'a>(
        &'a self,
        kind: SupplierReferenceKind,
        payload_reference: &'a str,
        target: &'a SupplierReferenceTarget,
    ) -> Pin<Box<dyn Future<Output = Result<ResolvedSupplierReference, ClassifiedError>> + Send + 'a>> {
        Box::pin(async move {
            self.adapter(target.connector_type)?.references.resolve(kind, payload_reference, target).await
        })
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;
    use std::sync::Mutex;

    use erp_core::ids::{SupplierAccountId, SupplierApiConnectionId};
    use erp_supply::entity::supplier_api::{SupplierApiConnectionData, SupplierApiConnectionStatus};
    use erp_supply::ports::connector::common::ScanWindow;
    use erp_supply::ports::connector::offer::IncrementalSupport;
    use erp_supply::ports::supplier_api_gateway::CatalogPage;

    use super::*;

    #[derive(Default)]
    struct RecordingAdapter(Mutex<Vec<&'static str>>);

    impl SupplierFulfillmentFactory for RecordingAdapter {
        fn creation_connector(
            &self,
            _: &SupplierApiConnection,
        ) -> Result<Arc<dyn SupplierConnector>, ClassifiedError> {
            self.0.lock().unwrap().push("create-binding");
            Err(ClassifiedError {
                class: SupplierFailureClass::CapabilityGap,
                code: "TEST_NO_ORDER".into(),
                summary: "未实现订单".into(),
            })
        }
        fn fulfillment_connector(
            &self,
            _: &SupplierApiConnection,
        ) -> Result<Arc<dyn SupplierConnector>, ClassifiedError> {
            self.0.lock().unwrap().push("tracking-binding");
            Err(ClassifiedError {
                class: SupplierFailureClass::CapabilityGap,
                code: "TEST_NO_ORDER".into(),
                summary: "未实现订单".into(),
            })
        }
    }

    impl SupplierApiGateway for RecordingAdapter {
        fn catalog_policy(
            &self,
            _: &SupplierApiConnection,
        ) -> Result<Option<CatalogSyncPolicy>, ClassifiedError> {
            self.0.lock().unwrap().push("policy");
            Ok(Some(CatalogSyncPolicy {
                mapping_version: 99,
                sync_interval_seconds: 300,
                full_sync_interval_seconds: 1000,
                overlap_seconds: 120,
                page_size: NonZeroU32::new(20).unwrap(),
                incremental: IncrementalSupport::Complete,
            }))
        }

        fn health_check<'a>(
            &'a self,
            _: &'a SupplierApiConnection,
            _: SupplierHealthCheckType,
        ) -> Pin<Box<dyn Future<Output = Result<(), ClassifiedError>> + Send + 'a>> {
            Box::pin(async move {
                self.0.lock().unwrap().push("health");
                Ok(())
            })
        }
        fn catalog_page<'a>(&'a self, _: &'a SupplierApiConnection, _: &'a Scan) -> CatalogRead<'a> {
            Box::pin(async move {
                self.0.lock().unwrap().push("catalog");
                Ok(CatalogPage { items: vec![], next: None, skipped: vec![], membership: None })
            })
        }
    }

    impl SupplierReferenceRegistry for RecordingAdapter {
        fn is_available(&self) -> bool {
            true
        }
        fn options<'a>(
            &'a self,
            _: SupplierReferenceKind,
            _: &'a SupplierReferenceTarget,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<SupplierReferenceOption>, ClassifiedError>> + Send + 'a>>
        {
            Box::pin(async move {
                self.0.lock().unwrap().push("options");
                Ok(vec![SupplierReferenceOption {
                    reference_id: "ticket".into(),
                    alias: "接口配置".into(),
                    version: "v1".into(),
                    expires_at: 1,
                }])
            })
        }
        fn resolve<'a>(
            &'a self,
            _: SupplierReferenceKind,
            _: &'a str,
            _: &'a SupplierReferenceTarget,
        ) -> Pin<Box<dyn Future<Output = Result<ResolvedSupplierReference, ClassifiedError>> + Send + 'a>>
        {
            Box::pin(async move {
                self.0.lock().unwrap().push("resolve");
                Ok(ResolvedSupplierReference { internal_reference: "resolved".into() })
            })
        }
    }

    fn connection() -> SupplierApiConnection {
        SupplierApiConnection::new(
            SupplierApiConnectionId::new("connection"),
            SupplierApiConnectionData {
                supplier_id: SupplierAccountId::new("supplier"),
                connection_code: "unrelated-code".into(),
                connector_type: SupplierConnectorType::Dangaoshushu,
                endpoint_reference: String::new(),
                credential_reference: None,
                rate_limit_policy: None,
                status: SupplierApiConnectionStatus::Disabled,
            },
            "actor",
        )
        .unwrap()
    }

    #[test]
    fn scheduling_and_capability_queries_use_registered_provider_policy() {
        let mut registry = SupplierConnectorRegistry::default();
        let mut connection = connection();
        assert!(registry.catalog_connector_types().is_empty());
        assert!(registry.catalog_policy(&connection).unwrap().is_none());
        let adapter = Arc::new(RecordingAdapter::default());
        registry.register(SupplierConnectorType::Dangaoshushu, adapter.clone()).unwrap();
        assert_eq!(registry.catalog_connector_types(), vec![SupplierConnectorType::Dangaoshushu]);
        let policy = registry.catalog_policy(&connection).unwrap().unwrap();
        assert_eq!((policy.mapping_version, policy.overlap_seconds, policy.page_size.get()), (99, 120, 20));
        assert_eq!(policy.sync_interval_seconds, 300);
        assert_eq!(policy.incremental, IncrementalSupport::Complete);
        assert_eq!(*adapter.0.lock().unwrap(), vec!["policy"]);
        connection.connector_type = None;
        assert!(registry.catalog_policy(&connection).unwrap().is_none());
        assert_eq!(*adapter.0.lock().unwrap(), vec!["policy"]);
    }

    #[tokio::test]
    async fn registered_type_routes_all_operations_and_missing_type_never_falls_back() {
        let mut registry = SupplierConnectorRegistry::default();
        let adapter = Arc::new(RecordingAdapter::default());
        registry.register(SupplierConnectorType::Dangaoshushu, adapter.clone()).unwrap();
        let mut connection = connection();
        let target = SupplierReferenceTarget::from(&connection);
        let scan = Scan { window: ScanWindow::Full, cursor: None, limit: NonZeroU32::new(50).unwrap() };
        registry.health_check(&connection, SupplierHealthCheckType::Authentication).await.unwrap();
        registry.catalog_page(&connection, &scan).await.unwrap();
        let options = registry.options(SupplierReferenceKind::Endpoint, &target).await.unwrap();
        assert_eq!(options[0].reference_id, "ticket");
        let resolved = registry.resolve(SupplierReferenceKind::Endpoint, "ticket", &target).await.unwrap();
        assert_eq!(resolved.internal_reference, "resolved");
        assert_eq!(*adapter.0.lock().unwrap(), ["health", "catalog", "options", "resolve"]);

        adapter.0.lock().unwrap().clear();
        connection.connector_type = None;
        // Neither a suggestive code nor supplier identity can select an adapter.
        connection.connection_code = "DGSS".into();
        connection.supplier_id = SupplierAccountId::new("dangaoshushu");
        let target = SupplierReferenceTarget::from(&connection);
        assert_eq!(
            registry.health_check(&connection, SupplierHealthCheckType::Connectivity).await.unwrap_err().code,
            "CONNECTOR_TYPE_REQUIRED"
        );
        assert_eq!(
            registry.catalog_page(&connection, &scan).await.unwrap_err().code,
            "CONNECTOR_TYPE_REQUIRED"
        );
        assert_eq!(
            registry.options(SupplierReferenceKind::Endpoint, &target).await.err().unwrap().code,
            "CONNECTOR_TYPE_REQUIRED"
        );
        assert_eq!(
            registry.resolve(SupplierReferenceKind::Endpoint, "ticket", &target).await.unwrap_err().code,
            "CONNECTOR_TYPE_REQUIRED"
        );
        assert!(adapter.0.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn absent_runtime_and_duplicate_registration_fail_explicitly() {
        let mut registry = SupplierConnectorRegistry::default();
        let connection = connection();
        assert!(!registry.is_available());
        assert_eq!(
            registry.health_check(&connection, SupplierHealthCheckType::Connectivity).await.unwrap_err().code,
            "SUPPLIER_CONNECTOR_UNAVAILABLE"
        );
        let first = Arc::new(RecordingAdapter::default());
        let replacement = Arc::new(RecordingAdapter::default());
        registry.register(SupplierConnectorType::Dangaoshushu, first.clone()).unwrap();
        assert_eq!(
            registry.register(SupplierConnectorType::Dangaoshushu, replacement.clone()).unwrap_err().code,
            "SUPPLIER_CONNECTOR_DUPLICATE"
        );
        registry.health_check(&connection, SupplierHealthCheckType::Connectivity).await.unwrap();
        assert_eq!(*first.0.lock().unwrap(), ["health"]);
        assert!(replacement.0.lock().unwrap().is_empty());
    }

    #[test]
    fn fulfillment_and_creation_bindings_route_by_explicit_type() {
        let mut registry = SupplierConnectorRegistry::default();
        let provider = Arc::new(RecordingAdapter::default());
        registry.register(SupplierConnectorType::Dangaoshushu, provider.clone()).unwrap();
        let mut connection = connection();
        assert_eq!(registry.creation_connector(&connection).err().unwrap().code, "TEST_NO_ORDER");
        assert_eq!(registry.fulfillment_connector(&connection).err().unwrap().code, "TEST_NO_ORDER");
        assert_eq!(*provider.0.lock().unwrap(), ["create-binding", "tracking-binding"]);
        provider.0.lock().unwrap().clear();
        connection.connector_type = Some(SupplierConnectorType::JdVop);
        assert_eq!(
            registry.fulfillment_connector(&connection).err().unwrap().code,
            "SUPPLIER_CONNECTOR_UNAVAILABLE"
        );
        connection.connector_type = None;
        assert_eq!(registry.creation_connector(&connection).err().unwrap().code, "CONNECTOR_TYPE_REQUIRED");
        assert!(provider.0.lock().unwrap().is_empty());
    }
}
