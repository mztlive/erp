//! 按连接声明的接口类型分派调用；没有注册的类型不得借用其它供应商适配器。
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use erp_supply::entity::failure::SupplierFailureClass;
use erp_supply::entity::supplier_api::{
    SupplierApiCapabilityCode, SupplierApiConnection, SupplierConnectorType, SupplierHealthCheckType,
};
use erp_supply::ports::connector::common::Scan;
use erp_supply::ports::connector::offer::IncrementalSupport;
use erp_supply::ports::supplier_api_gateway::{CatalogRead, ClassifiedError, SupplierApiGateway};
use erp_supply::ports::supplier_reference_registry::{
    ResolvedSupplierReference, SupplierDeploymentConfig, SupplierReferenceKind, SupplierReferenceOption,
    SupplierReferenceRegistry, SupplierReferenceTarget,
};

struct Adapter {
    gateway: Arc<dyn SupplierApiGateway>,
    references: Arc<dyn SupplierReferenceRegistry>,
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
        T: SupplierApiGateway + SupplierReferenceRegistry + 'static,
    {
        if self.adapters.contains_key(&kind) {
            return Err(ClassifiedError {
                class: SupplierFailureClass::MappingError,
                code: "SUPPLIER_CONNECTOR_DUPLICATE".into(),
                summary: "供应商接口类型重复登记".into(),
            });
        }
        self.adapters.insert(kind, Adapter { gateway: runtime.clone(), references: runtime });
        Ok(())
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
    fn catalog_availability_validity(&self, connection: &SupplierApiConnection) -> Option<u64> {
        self.adapter(connection.connector_type)
            .ok()
            .and_then(|adapter| adapter.gateway.catalog_availability_validity(connection))
    }
    fn catalog_full_sync_interval(&self, connection: &SupplierApiConnection) -> u64 {
        self.adapter(connection.connector_type)
            .map_or(86_400, |adapter| adapter.gateway.catalog_full_sync_interval(connection))
    }

    fn catalog_incremental_support(
        &self,
        connection: &SupplierApiConnection,
    ) -> Result<IncrementalSupport, ClassifiedError> {
        self.adapter(connection.connector_type)?.gateway.catalog_incremental_support(connection)
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
    use erp_supply::ports::supplier_api_gateway::CatalogPage;

    use super::*;

    #[derive(Default)]
    struct RecordingAdapter(Mutex<Vec<&'static str>>);

    impl SupplierApiGateway for RecordingAdapter {
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
                Ok(CatalogPage { items: vec![], next: None, skipped: vec![] })
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
}
