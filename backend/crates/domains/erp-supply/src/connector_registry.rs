//! 按连接声明的接口类型分派调用；没有注册的类型不得借用其它供应商适配器。
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::entity::failure::SupplierFailureClass;
use crate::entity::supplier_api::{
    SupplierApiCapabilityCode, SupplierApiConnection, SupplierConnectorType, SupplierHealthCheckType,
};
use crate::ports::catalog_sync::CatalogSyncPolicy;
use crate::ports::connector::SupplierConnector;
use crate::ports::connector::common::{ConnectorError, ConnectorResult, Lookup, Scan, Snapshot};
use crate::ports::connector::description::ConnectorDescription;
use crate::ports::connector::order::{OrderKey, OrderLine, OrderSnapshot};
use crate::ports::connector::runtime::{ConnectorPurpose, SupplierRuntime, bind_checked};
use crate::ports::supplier_api_gateway::{
    CatalogRead, CatalogScopeRead, ClassifiedError, SupplierApiGateway,
};
use crate::ports::supplier_reference_registry::{
    ResolvedSupplierReference, SupplierDeploymentConfig, SupplierReferenceKind, SupplierReferenceOption,
    SupplierReferenceRegistry, SupplierReferenceTarget,
};

/// 组合根按协议注册适配器；业务身份和部署配置不参与类型推断。
#[derive(Default)]
pub struct SupplierConnectorRegistry {
    adapters: BTreeMap<SupplierConnectorType, Arc<dyn SupplierRuntime>>,
}

impl SupplierConnectorRegistry {
    /// 解析通知消费连接，不把订单资金准入当作商品通知准入。
    /// # 参数
    /// connection 为服务端加载的原连接。
    /// # 返回
    /// 技术身份已核验的通知适配器。
    /// # 错误
    /// 协议或技术绑定无效时拒绝。
    pub fn notification_connector(
        &self,
        connection: &SupplierApiConnection,
    ) -> Result<Arc<dyn SupplierConnector>, ClassifiedError> {
        bind_checked(
            self.adapter(connection.connector_type)?.as_ref(),
            connection,
            ConnectorPurpose::Notification,
        )
    }

    /// 返回所有已登记协议，不依赖目录能力筛选通知消费者。
    /// # 参数
    /// 无。
    /// # 返回
    /// 有序协议清单。
    /// # 错误
    /// 不返回错误。
    pub fn connector_types(&self) -> Vec<SupplierConnectorType> {
        self.adapters.keys().copied().collect()
    }

    /// 以服务端连接及路由协议取得回调能力。
    /// # 参数
    /// connection 为持久化连接，kind 为路由协议。
    /// # 返回
    /// 绑定同一连接且提供回调能力的适配器。
    /// # 错误
    /// 协议、连接身份或回调能力不符时拒绝。
    pub fn callback_connector(
        &self,
        connection: &SupplierApiConnection,
        kind: SupplierConnectorType,
    ) -> Result<Arc<dyn SupplierConnector>, ClassifiedError> {
        let invalid = || ClassifiedError {
            class: SupplierFailureClass::CapabilityGap,
            code: "SUPPLIER_CALLBACK_UNAVAILABLE".into(),
            summary: "供应商回调协议或能力不匹配".into(),
        };
        if connection.connector_type != Some(kind) {
            return Err(invalid());
        }
        let connector = self.notification_connector(connection)?;
        if connector.connection_id().as_ref() != connection.base.id || connector.callbacks().is_none() {
            return Err(invalid());
        }
        Ok(connector)
    }

    /// 将同一协议运行时登记为技术网关及配置目录。
    /// # 参数
    /// `kind` 为协议类型，`runtime` 为当前部署构造的共享运行时。
    /// # 返回
    /// 登记成功返回 `Ok(())`。
    /// # 错误
    /// 相同协议重复登记时拒绝覆盖。
    pub fn register<T>(&mut self, kind: SupplierConnectorType, runtime: Arc<T>) -> Result<(), ClassifiedError>
    where
        T: SupplierRuntime + 'static,
    {
        if self.adapters.contains_key(&kind) {
            return Err(ClassifiedError {
                class: SupplierFailureClass::MappingError,
                code: "SUPPLIER_CONNECTOR_DUPLICATE".into(),
                summary: "供应商接口类型重复登记".into(),
            });
        }
        if kind != runtime.connector_type() {
            return Err(ClassifiedError {
                class: SupplierFailureClass::MappingError,
                code: "SUPPLIER_CONNECTOR_TYPE_MISMATCH".into(),
                summary: "登记协议与适配器实现不一致".into(),
            });
        }
        self.adapters.insert(kind, runtime);
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
        bind_checked(
            self.adapter(connection.connector_type)?.as_ref(),
            connection,
            ConnectorPurpose::Creation,
        )
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
        bind_checked(
            self.adapter(connection.connector_type)?.as_ref(),
            connection,
            ConnectorPurpose::Fulfillment,
        )
    }

    /// 为退款及结算绑定原技术连接，不借用新增下单或付款准入。
    /// # 参数
    /// `connection` 为服务端原连接。
    /// # 返回
    /// 已验证身份的可选能力集合。
    /// # 错误
    /// 技术身份或协议不一致时拒绝。
    pub fn after_sales_connector(
        &self,
        connection: &SupplierApiConnection,
    ) -> Result<Arc<dyn SupplierConnector>, ClassifiedError> {
        bind_checked(
            self.adapter(connection.connector_type)?.as_ref(),
            connection,
            ConnectorPurpose::AfterSales,
        )
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

    fn adapter(
        &self,
        kind: Option<SupplierConnectorType>,
    ) -> Result<&Arc<dyn SupplierRuntime>, ClassifiedError> {
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
        Box::pin(async move { self.adapter(connection.connector_type)?.catalog_scope(connection).await })
    }

    fn catalog_availability_validity(&self, connection: &SupplierApiConnection) -> Option<u64> {
        self.adapter(connection.connector_type)
            .ok()
            .and_then(|adapter| adapter.catalog_availability_validity(connection))
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
        adapter.catalog_policy(connection)
    }

    fn catalog_page<'a>(&'a self, connection: &'a SupplierApiConnection, scan: &'a Scan) -> CatalogRead<'a> {
        Box::pin(async move { self.adapter(connection.connector_type)?.catalog_page(connection, scan).await })
    }

    fn health_check<'a>(
        &'a self,
        connection: &'a SupplierApiConnection,
        check_type: SupplierHealthCheckType,
    ) -> Pin<Box<dyn Future<Output = Result<(), ClassifiedError>> + Send + 'a>> {
        Box::pin(async move {
            self.adapter(connection.connector_type)?.health_check(connection, check_type).await
        })
    }
}

impl SupplierReferenceRegistry for SupplierConnectorRegistry {
    fn description(&self, target: &SupplierReferenceTarget) -> Result<ConnectorDescription, ClassifiedError> {
        self.adapter(target.connector_type)?.description(target)
    }

    fn capabilities(
        &self,
        target: &SupplierReferenceTarget,
    ) -> Result<Vec<SupplierApiCapabilityCode>, ClassifiedError> {
        self.adapter(target.connector_type)?.capabilities(target)
    }

    fn deployment_config(
        &self,
        target: &SupplierReferenceTarget,
    ) -> Result<SupplierDeploymentConfig, ClassifiedError> {
        self.adapter(target.connector_type)?.deployment_config(target)
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
        Box::pin(async move { self.adapter(target.connector_type)?.options(kind, target).await })
    }

    fn resolve<'a>(
        &'a self,
        kind: SupplierReferenceKind,
        payload_reference: &'a str,
        target: &'a SupplierReferenceTarget,
    ) -> Pin<Box<dyn Future<Output = Result<ResolvedSupplierReference, ClassifiedError>> + Send + 'a>> {
        Box::pin(async move {
            self.adapter(target.connector_type)?.resolve(kind, payload_reference, target).await
        })
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;
    use std::sync::Mutex;

    use erp_core::ids::{SupplierAccountId, SupplierApiConnectionId};

    use super::*;
    use crate::entity::supplier_api::{SupplierApiConnectionData, SupplierApiConnectionStatus};
    use crate::ports::connector::common::{Page, ScanWindow, SupplierSku};
    use crate::ports::connector::offer::{
        IncrementalSupport, Offer, OfferChange, OfferSource, ScopedOfferPage,
    };

    #[derive(Default)]
    struct RecordingAdapter(Mutex<Vec<&'static str>>, Option<&'static str>, bool);

    impl SupplierRuntime for RecordingAdapter {
        fn connector_type(&self) -> SupplierConnectorType {
            SupplierConnectorType::Dangaoshushu
        }
        fn bind(
            &self,
            connection: &SupplierApiConnection,
            purpose: ConnectorPurpose,
        ) -> Result<Arc<dyn SupplierConnector>, ClassifiedError> {
            if purpose == ConnectorPurpose::Catalog {
                self.0.lock().unwrap().push("catalog");
                return Ok(Arc::new(RecordingConnector(
                    SupplierApiConnectionId::new(self.1.unwrap_or(&connection.base.id)),
                    !self.2,
                )));
            }
            self.0.lock().unwrap().push(match purpose {
                ConnectorPurpose::Creation => "create-binding",
                _ => "tracking-binding",
            });
            Err(ClassifiedError {
                class: SupplierFailureClass::CapabilityGap,
                code: "TEST_NO_ORDER".into(),
                summary: "未实现订单".into(),
            })
        }
        fn catalog_schedule(
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
        fn check_health<'a>(
            &'a self,
            _: &'a SupplierApiConnection,
            _: SupplierHealthCheckType,
        ) -> Pin<Box<dyn Future<Output = Result<(), ClassifiedError>> + Send + 'a>> {
            Box::pin(async move {
                self.0.lock().unwrap().push("health");
                Ok(())
            })
        }
    }

    struct RecordingConnector(SupplierApiConnectionId, bool);
    impl SupplierConnector for RecordingConnector {
        fn connection_id(&self) -> &SupplierApiConnectionId {
            &self.0
        }
        fn offers(&self) -> Option<&dyn OfferSource> {
            self.1.then_some(self as &dyn OfferSource)
        }
    }
    #[async_trait::async_trait]
    impl OfferSource for RecordingConnector {
        fn incremental_support(&self) -> IncrementalSupport {
            IncrementalSupport::Complete
        }
        async fn scan(&self, _: &Scan) -> ConnectorResult<Page<Snapshot<OfferChange>>> {
            Ok(Page { items: vec![], next: Some("next".into()) })
        }
        async fn scan_scoped(&self, scan: &Scan) -> ConnectorResult<ScopedOfferPage> {
            Ok(ScopedOfferPage {
                page: self.scan(scan).await?,
                skipped: vec!["来源缺少规格".into()],
                membership: None,
            })
        }
        async fn offer(&self, _: &SupplierSku) -> ConnectorResult<Lookup<Snapshot<Offer>>> {
            Ok(Lookup::NotVisible)
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
        let page = registry.catalog_page(&connection, &scan).await.unwrap();
        assert_eq!(page.next.as_deref(), Some("next"));
        assert_eq!(page.skipped, ["来源缺少规格"]);
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

    #[tokio::test]
    async fn mismatched_connection_and_missing_catalog_never_produce_successful_pages() {
        let scan = Scan { window: ScanWindow::Full, cursor: None, limit: NonZeroU32::new(1).unwrap() };
        for (bound_id, missing, code) in [
            (Some("another-connection"), false, "SUPPLIER_CONNECTOR_BINDING_MISMATCH"),
            (None, true, "CATALOG_SYNC_ADAPTER_UNAVAILABLE"),
        ] {
            let mut registry = SupplierConnectorRegistry::default();
            let runtime = Arc::new(RecordingAdapter(Mutex::new(vec![]), bound_id, missing));
            registry.register(SupplierConnectorType::Dangaoshushu, runtime).unwrap();
            assert_eq!(registry.catalog_page(&connection(), &scan).await.unwrap_err().code, code);
            assert_eq!(registry.catalog_scope(&connection()).await.unwrap_err().code, code);
        }
    }

    #[test]
    fn registration_cannot_relabel_another_supplier_protocol() {
        let mut registry = SupplierConnectorRegistry::default();
        assert_eq!(
            registry
                .register(SupplierConnectorType::JdVop, Arc::new(RecordingAdapter::default()))
                .unwrap_err()
                .code,
            "SUPPLIER_CONNECTOR_TYPE_MISMATCH"
        );
        assert!(registry.connector_types().is_empty());
    }
}
