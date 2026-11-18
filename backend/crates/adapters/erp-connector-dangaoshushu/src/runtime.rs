use std::future::Future;
use std::num::NonZeroU32;
use std::pin::Pin;
use std::sync::Arc;

use config::DangaoshushuConfig;
use erp_core::common::time::Instant;
use erp_supply::entity::failure::SupplierFailureClass;
use erp_supply::entity::supplier_api::{
    SupplierApiConnection, SupplierApiConnectionStatus, SupplierConnectorType, SupplierHealthCheckType,
};
use erp_supply::ports::catalog_sync::CatalogSyncPolicy;
use erp_supply::ports::connector::SupplierConnector;
use erp_supply::ports::connector::common::{ConnectorError, ConnectorResult, Lookup, Snapshot};
use erp_supply::ports::connector::description::ConnectorDescription;
use erp_supply::ports::connector::offer::IncrementalSupport;
use erp_supply::ports::connector::order::{OrderKey, OrderSnapshot, Orders};
use erp_supply::ports::connector::runtime::{ConnectorPurpose, SupplierRuntime};
use erp_supply::ports::supplier_api_gateway::ClassifiedError;
use erp_supply::ports::supplier_reference_registry::{
    ResolvedSupplierReference, SupplierDeploymentConfig, SupplierReferenceKind, SupplierReferenceOption,
    SupplierReferenceRegistry, SupplierReferenceTarget,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::parsing::mapping;
use super::transport::{HttpTransport, Transport};
use super::{DangaoshushuConnector, DangaoshushuReadQuery, error, proof};

/// 技术配置注册表；每次按后台连接绑定客户端，共享 HTTP 连接池与渠道限流。
pub struct DangaoshushuRuntime {
    settings: DangaoshushuConfig,
    transport: Arc<dyn Transport>,
}

#[derive(Serialize)]
pub struct SupplierReferenceTickets {
    pub endpoint_ticket: String,
    pub credential_ticket: String,
    pub expires_at: i64,
}
#[derive(Serialize, Deserialize)]
struct ReferenceTicket {
    connection_id: String,
    supplier_id: String,
    connector_type: SupplierConnectorType,
    kind: String,
    fingerprint: String,
    expires_at: i64,
}

impl DangaoshushuRuntime {
    /// 应用启动时用 SafeConfig 快照装配蛋糕叔叔运行时，供后续连接共享 HTTP 连接池和渠道限流。
    /// 此时不选择 ERP 供应商或连接，也不发请求；配置未启用、配置无效或 HTTP 客户端构建失败时返回分类错误。
    pub fn new(settings: DangaoshushuConfig) -> ConnectorResult<Self> {
        validate_settings(&settings)?;
        let transport = Arc::new(HttpTransport::new(&settings)?);
        Ok(Self { settings, transport })
    }

    /// 需要由调用方提供通信实现时，用注入的传输替代默认 HTTP 客户端，仍按真实协议绑定连接、分派能力和解析响应。
    /// 传输须保证一次调用只发送一次；配置关闭或无效仍按默认入口的规则拒绝。
    pub fn with_transport(
        settings: DangaoshushuConfig,
        transport: Arc<dyn Transport>,
    ) -> ConnectorResult<Self> {
        validate_settings(&settings)?;
        Ok(Self { settings, transport })
    }

    /// 新增下单前，按本次从后台仓储加载的连接记录取得客户端，沿用其供应商身份并共享渠道限流。
    /// 自动下单停用、健康检查不适用于当前配置、连接已删除或技术引用不符时拒绝。
    pub fn connector(&self, connection: &SupplierApiConnection) -> ConnectorResult<DangaoshushuConnector> {
        if connection.stable.status != SupplierApiConnectionStatus::Active {
            return Err(error(
                SupplierFailureClass::BusinessRejected,
                "DGSS_CONNECTION_DISABLED",
                "请先开启该供应商的自动下单",
            ));
        }
        let connector = self.tracking_connector(connection)?;
        if !connection.current_technical_config_is_healthy() {
            return Err(error(
                SupplierFailureClass::BusinessRejected,
                "DGSS_HEALTH_REQUIRED",
                "接口配置尚未通过连接测试，不能用于业务调用",
            ));
        }
        Ok(connector)
    }

    /// 已有订单跟踪、回调与后续履约使用原连接的客户端，即使自动下单已停用也可继续处理。
    /// 删除、供应商协议不符或端点和凭据引用缺失、陈旧时不发请求。
    pub fn tracking_connector(
        &self,
        connection: &SupplierApiConnection,
    ) -> ConnectorResult<DangaoshushuConnector> {
        let target = self.connection_target(connection)?;
        Ok(DangaoshushuConnector {
            binding: self.bound_fingerprint(connection, &target)?,
            connection_id: target.connection_id,
            settings: self.settings.clone(),
            transport: self.transport.clone(),
        })
    }

    /// 已登记订单需要核对外部结果时，用服务端加载的原绑定连接和持久化订单号只读补查；停用自动下单不影响跟踪。
    /// 返回核对渠道和原单号后的观察结果或明确的暂不可见；绑定、协议或上游查询失败仍按原分类返回。
    pub async fn observe_order(
        &self,
        connection: &SupplierApiConnection,
        key: &OrderKey,
    ) -> ConnectorResult<Lookup<Snapshot<OrderSnapshot>>> {
        self.tracking_connector(connection)?.order(key).await
    }

    /// 后台针对已授权连接诊断目录时，通过原技术绑定执行白名单查询，停用自动下单也可显式查询。
    /// 只返回供应商原始资料，不应用到业务实体；引用未绑定或已失效时不访问上游。
    pub async fn read(
        &self,
        connection: &SupplierApiConnection,
        query: &DangaoshushuReadQuery,
    ) -> ConnectorResult<Value> {
        self.tracking_connector(connection)?.read(query).await
    }

    /// 校验后台连接时先核对接口类型、身份及删除状态；ERP 连接和供应商身份无需在配置文件重复登记。
    /// 启用 `references_required` 时再核对端点和凭据引用，拒绝过期配置或从其他连接复制来的引用。
    pub fn validate_binding(
        &self,
        connection: &SupplierApiConnection,
        references_required: bool,
    ) -> ConnectorResult<()> {
        let target = self.connection_target(connection)?;
        if !references_required {
            return Ok(());
        }
        self.bound_fingerprint(connection, &target)?;
        Ok(())
    }

    fn bound_fingerprint(
        &self,
        connection: &SupplierApiConnection,
        target: &SupplierReferenceTarget,
    ) -> ConnectorResult<String> {
        if !connection.endpoint_reference_bound || !connection.credential_reference_bound {
            return Err(binding_error());
        }

        let fingerprint = proof::binding_hash(&self.settings, target)?;
        if connection.endpoint_reference != internal_reference("endpoint", &fingerprint)
            || connection.credential_reference.as_deref()
                != Some(internal_reference("credential", &fingerprint).as_str())
        {
            return Err(binding_error());
        }
        Ok(fingerprint)
    }

    fn connection_target(
        &self,
        connection: &SupplierApiConnection,
    ) -> ConnectorResult<SupplierReferenceTarget> {
        let target = SupplierReferenceTarget::from(connection);
        self.validate_target(&target)?;
        if connection.base.is_deleted() {
            return Err(binding_error());
        }
        Ok(target)
    }

    fn validate_target(&self, target: &SupplierReferenceTarget) -> ConnectorResult<()> {
        if target.connector_type != Some(SupplierConnectorType::Dangaoshushu) {
            return Err(error(
                SupplierFailureClass::CapabilityGap,
                "DGSS_CONNECTOR_TYPE_MISMATCH",
                "连接未选择蛋糕叔叔 API",
            ));
        }
        if target.connection_id.as_ref().is_empty() || target.supplier_id.as_ref().is_empty() {
            return Err(binding_error());
        }
        Ok(())
    }

    /// 后台选择接入资料时，以 `now` 为签发时间生成有效期五分钟的端点和凭据票据，免于手填内部引用。
    /// 两个票据均绑定当前连接、供应商和配置；连接不可用或票据编码失败时返回分类错误。
    pub fn reference_tickets(
        &self,
        connection: &SupplierApiConnection,
        now: Instant,
    ) -> ConnectorResult<SupplierReferenceTickets> {
        self.tickets(&self.connection_target(connection)?, now)
    }

    fn tickets(
        &self,
        target: &SupplierReferenceTarget,
        now: Instant,
    ) -> ConnectorResult<SupplierReferenceTickets> {
        let expires_at = now.unix_secs().checked_add(300).ok_or_else(mapping)?;
        let fingerprint = proof::binding_hash(&self.settings, target)?;

        let ticket = |kind: &str| {
            proof::encode(
                &self.settings.private_key,
                &ReferenceTicket {
                    connection_id: target.connection_id.to_string(),
                    supplier_id: target.supplier_id.to_string(),
                    connector_type: SupplierConnectorType::Dangaoshushu,
                    kind: kind.into(),
                    fingerprint: fingerprint.clone(),
                    expires_at,
                },
            )
        };
        Ok(SupplierReferenceTickets {
            endpoint_ticket: ticket("endpoint")?,
            credential_ticket: ticket("credential")?,
            expires_at,
        })
    }

    fn resolve_ticket(
        &self,
        kind: SupplierReferenceKind,
        payload: &str,
        target: &SupplierReferenceTarget,
        now: Instant,
    ) -> ConnectorResult<ResolvedSupplierReference> {
        let expected = reference_kind(kind)?;
        let ticket: ReferenceTicket = proof::decode(&self.settings.private_key, payload)?;
        if Some(ticket.connector_type) != target.connector_type
            || ticket.kind != expected
            || ticket.connection_id != target.connection_id.as_ref()
            || ticket.supplier_id != target.supplier_id.as_ref()
            || ticket.fingerprint != proof::binding_hash(&self.settings, target)?
            || ticket.expires_at < now.unix_secs()
            || ticket.expires_at > now.unix_secs().saturating_add(300)
        {
            return Err(error(
                SupplierFailureClass::AuthSignature,
                "DGSS_REFERENCE_TICKET_INVALID",
                "本次选择的供应商资料已经失效，请刷新后重新选择",
            ));
        }

        self.validate_target(target)?;
        Ok(ResolvedSupplierReference {
            internal_reference: internal_reference(expected, &ticket.fingerprint),
        })
    }

    fn reference_options(
        &self,
        kind: SupplierReferenceKind,
        target: &SupplierReferenceTarget,
        now: Instant,
    ) -> ConnectorResult<Vec<SupplierReferenceOption>> {
        reference_kind(kind)?;
        if self.validate_target(target).is_err() {
            return Ok(Vec::new());
        }

        let tickets = self.tickets(target, now)?;
        let (reference_id, alias) = match kind {
            SupplierReferenceKind::Endpoint => (tickets.endpoint_ticket, "蛋糕叔叔接口地址"),
            SupplierReferenceKind::Credential => (tickets.credential_ticket, "蛋糕叔叔渠道凭据"),
            SupplierReferenceKind::BusinessProfile => return Err(mapping()),
        };
        let version = "当前部署配置".into();
        Ok(vec![SupplierReferenceOption {
            reference_id,
            alias: alias.into(),
            version,
            expires_at: tickets.expires_at,
        }])
    }
}

fn internal_reference(kind: &str, fingerprint: &str) -> String {
    format!("config://dangaoshushu/{fingerprint}/{kind}")
}

fn validate_settings(settings: &DangaoshushuConfig) -> ConnectorResult<()> {
    settings.validate().map_err(|_| {
        error(
            SupplierFailureClass::MappingError,
            "DGSS_CONFIG_INVALID",
            "蛋糕叔叔接入配置不完整，请联系管理员核对",
        )
    })?;
    if !settings.enabled {
        return Err(error(
            SupplierFailureClass::CapabilityGap,
            "DGSS_DISABLED",
            "尚未登记蛋糕叔叔接入配置，请联系管理员完成配置",
        ));
    }
    Ok(())
}

fn binding_error() -> ConnectorError {
    error(
        SupplierFailureClass::AuthSignature,
        "DGSS_CONNECTION_BINDING",
        "供应商连接与已绑定的技术配置不一致，请在后台重新选择配置",
    )
}
fn reference_kind(kind: SupplierReferenceKind) -> ConnectorResult<&'static str> {
    match kind {
        SupplierReferenceKind::Endpoint => Ok("endpoint"),
        SupplierReferenceKind::Credential => Ok("credential"),
        SupplierReferenceKind::BusinessProfile => Err(error(
            SupplierFailureClass::CapabilityGap,
            "DGSS_BUSINESS_PROFILE_REGISTRY_UNAVAILABLE",
            "业务资料须由采购资料注册表核验",
        )),
    }
}

impl SupplierRuntime for DangaoshushuRuntime {
    fn connector_type(&self) -> SupplierConnectorType {
        SupplierConnectorType::Dangaoshushu
    }

    fn bind(
        &self,
        connection: &SupplierApiConnection,
        purpose: ConnectorPurpose,
    ) -> Result<Arc<dyn SupplierConnector>, ClassifiedError> {
        let connector = match purpose {
            ConnectorPurpose::Creation => self.connector(connection),
            ConnectorPurpose::Catalog
            | ConnectorPurpose::Notification
            | ConnectorPurpose::Fulfillment
            | ConnectorPurpose::AfterSales => self.tracking_connector(connection),
        }
        .map_err(classified)?;
        Ok(Arc::new(connector))
    }

    fn availability_validity(&self, _: &SupplierApiConnection) -> Option<u64> {
        self.settings.availability_validity_seconds
    }
    fn catalog_schedule(
        &self,
        _connection: &SupplierApiConnection,
    ) -> Result<Option<CatalogSyncPolicy>, ClassifiedError> {
        Ok(Some(CatalogSyncPolicy {
            // 第四版拒绝沿用可能混合旧映射的中断检查点。
            mapping_version: 4,
            sync_interval_seconds: self.settings.catalog_sync_interval_seconds,
            full_sync_interval_seconds: self.settings.catalog_full_sync_interval_seconds,
            overlap_seconds: 86_400,
            page_size: NonZeroU32::new(5).expect("5 非零"),
            incremental: IncrementalSupport::Partial,
        }))
    }

    fn check_health<'a>(
        &'a self,
        connection: &'a SupplierApiConnection,
        check_type: SupplierHealthCheckType,
    ) -> Pin<Box<dyn Future<Output = Result<(), ClassifiedError>> + Send + 'a>> {
        Box::pin(async move {
            let connector = self.tracking_connector(connection).map_err(classified)?;
            if check_type == SupplierHealthCheckType::CapabilityMetadata {
                return Err(ClassifiedError {
                    class: SupplierFailureClass::CapabilityGap,
                    code: "DGSS_CAPABILITY_METADATA_UNSUPPORTED".into(),
                    summary: "品牌读取不能形成能力元数据验证证据".into(),
                });
            }
            connector.brands().await.map_err(classified)?;
            Ok(())
        })
    }
}
impl SupplierReferenceRegistry for DangaoshushuRuntime {
    fn description(&self, target: &SupplierReferenceTarget) -> Result<ConnectorDescription, ClassifiedError> {
        self.validate_target(target).map_err(classified)?;
        let connector = DangaoshushuConnector::bind(self.settings.clone(), target, self.transport.clone())
            .map_err(classified)?;
        Ok(ConnectorDescription::from_connector(&connector))
    }

    fn deployment_config(
        &self,
        target: &SupplierReferenceTarget,
    ) -> Result<SupplierDeploymentConfig, ClassifiedError> {
        self.validate_target(target).map_err(classified)?;
        let fingerprint = proof::binding_hash(&self.settings, target).map_err(classified)?;
        Ok(SupplierDeploymentConfig {
            endpoint: internal_reference("endpoint", &fingerprint),
            credential: internal_reference("credential", &fingerprint),
        })
    }

    fn is_available(&self) -> bool {
        true
    }
    fn options<'a>(
        &'a self,
        kind: SupplierReferenceKind,
        target: &'a SupplierReferenceTarget,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<SupplierReferenceOption>, ClassifiedError>> + Send + 'a>>
    {
        Box::pin(async move { self.reference_options(kind, target, Instant::now()).map_err(classified) })
    }
    fn resolve<'a>(
        &'a self,
        kind: SupplierReferenceKind,
        payload_reference: &'a str,
        target: &'a SupplierReferenceTarget,
    ) -> Pin<Box<dyn Future<Output = Result<ResolvedSupplierReference, ClassifiedError>> + Send + 'a>> {
        Box::pin(async move {
            self.resolve_ticket(kind, payload_reference, target, Instant::now()).map_err(classified)
        })
    }
}
fn classified(error: ConnectorError) -> ClassifiedError {
    ClassifiedError { class: error.class, code: error.code, summary: error.summary }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use erp_core::ids::{SupplierAccountId, SupplierApiConnectionId};
    use erp_supply::entity::supplier_api::{
        HealthCheckResult, SupplierApiCapabilityCode, SupplierApiConnectionData, SupplierApiConnectionStatus,
    };
    use erp_supply::ports::connector::common::{Scan, ScanWindow};
    use erp_supply::ports::connector::description::NotificationMode;
    use erp_supply::ports::supplier_api_gateway::SupplierApiGateway;
    use serde_json::{from_value, json};

    use super::super::test_support::{recording_connector, settings};
    use super::*;

    fn connection() -> SupplierApiConnection {
        SupplierApiConnection::new(
            SupplierApiConnectionId::new("connection-1"),
            SupplierApiConnectionData {
                supplier_id: SupplierAccountId::new("supplier-1"),
                connection_code: "dgss-test".into(),
                connector_type: SupplierConnectorType::Dangaoshushu,
                endpoint_reference: "unbound".into(),
                credential_reference: None,
                rate_limit_policy: None,
                status: SupplierApiConnectionStatus::Disabled,
            },
            "actor",
        )
        .unwrap()
    }

    #[test]
    fn injected_transport_preserves_configuration_rejection_without_requests() {
        let (_, transport) = recording_connector(vec![]);
        let mut disabled = settings();
        disabled.enabled = false;
        let mut invalid = settings();
        invalid.channel_no.clear();
        for (config, code) in [(disabled, "DGSS_DISABLED"), (invalid, "DGSS_CONFIG_INVALID")] {
            let original = DangaoshushuRuntime::new(config.clone()).err().unwrap();
            let injected = DangaoshushuRuntime::with_transport(config, transport.clone()).err().unwrap();
            assert_eq!(original.code, code);
            assert_eq!(injected.code, original.code);
            assert_eq!(injected.class, original.class);
            assert_eq!(injected.summary, original.summary);
        }
        assert!(transport.requests.lock().unwrap().is_empty());
    }
    #[test]
    fn catalog_policy_uses_deployment_intervals_without_requiring_bound_credentials() {
        let mut config = settings();
        config.catalog_sync_interval_seconds = 600;
        config.catalog_full_sync_interval_seconds = 7200;
        let runtime = DangaoshushuRuntime::new(config).unwrap();
        let policy = runtime.catalog_policy(&connection()).unwrap().unwrap();
        assert_eq!(policy.mapping_version, 4);
        assert_eq!(policy.sync_interval_seconds, 600);
        assert_eq!(policy.full_sync_interval_seconds, 7200);
        assert_eq!(policy.overlap_seconds, 86_400);
        assert_eq!(policy.page_size.get(), 5);
        assert_eq!(policy.incremental, IncrementalSupport::Partial);
    }

    #[tokio::test]
    async fn automatic_configuration_supports_catalog_before_business_enable_and_invalidates_on_rotation() {
        let (connector, transport) = recording_connector(vec![
            Ok(json!({ "product_list": [
            { "id": "p1", "title": "蛋糕", "specs": [{ "spec_id": "s1", "name": "6寸" }] }
        ] })),
            Ok(json!({"product":{"id":"p1","title":"蛋糕"}, "specs":[{"id":"s1","name":"6寸"}]})),
        ]);
        let runtime = DangaoshushuRuntime { settings: connector.settings, transport: transport.clone() };
        let mut connection = connection();
        let config = runtime.deployment_config(&SupplierReferenceTarget::from(&connection)).unwrap();
        assert!(connection.apply_deployment(&config, "actor").unwrap());
        let version = connection.technical_config_version;
        assert!(!connection.apply_deployment(&config, "actor").unwrap());
        assert_eq!(connection.technical_config_version, version);
        assert!(!connection.is_active());

        let page = runtime
            .catalog_page(
                &connection,
                &Scan { window: ScanWindow::Full, cursor: None, limit: NonZeroU32::new(50).unwrap() },
            )
            .await
            .unwrap();
        assert_eq!(page.items.len(), 1);
        assert!(page.next.is_none());
        assert_eq!(transport.requests.lock().unwrap().len(), 2);

        connection.record_health(HealthCheckResult::Healthy, Instant::now());
        let mut rotated_settings = settings();
        rotated_settings.private_key = "rotated-secret".into();
        let rotated = DangaoshushuRuntime::new(rotated_settings).unwrap();
        assert!(rotated.validate_binding(&connection, true).is_err());
        let rotated_config = rotated.deployment_config(&SupplierReferenceTarget::from(&connection)).unwrap();
        assert!(connection.apply_deployment(&rotated_config, "actor").unwrap());
        assert!(!connection.current_technical_config_is_healthy());
        assert!(rotated.validate_binding(&connection, true).is_ok());
    }

    #[test]
    fn tickets_are_kind_configuration_connection_and_expiry_bound() {
        let runtime = DangaoshushuRuntime::new(settings()).unwrap();
        let connection = connection();
        let target = SupplierReferenceTarget::from(&connection);
        let now = Instant::from_unix_secs(1700000000);
        let tickets = runtime.reference_tickets(&connection, now).unwrap();
        assert!(
            runtime
                .resolve_ticket(SupplierReferenceKind::Endpoint, &tickets.endpoint_ticket, &target, now)
                .is_ok()
        );
        assert!(
            runtime
                .resolve_ticket(SupplierReferenceKind::Credential, &tickets.endpoint_ticket, &target, now)
                .is_err()
        );

        let mut changed_settings = settings();
        changed_settings.base_url = "https://other.dangaoss.cn".into();
        let changed = DangaoshushuRuntime::new(changed_settings).unwrap();
        assert!(
            changed
                .resolve_ticket(SupplierReferenceKind::Endpoint, &tickets.endpoint_ticket, &target, now)
                .is_err()
        );

        assert!(
            runtime
                .resolve_ticket(
                    SupplierReferenceKind::Endpoint,
                    &tickets.endpoint_ticket,
                    &target,
                    Instant::from_unix_secs(1700000301)
                )
                .is_err()
        );

        let wrong_connection = SupplierReferenceTarget {
            connection_id: SupplierApiConnectionId::new("connection-2"),
            ..target.clone()
        };
        assert!(
            runtime
                .resolve_ticket(
                    SupplierReferenceKind::Endpoint,
                    &tickets.endpoint_ticket,
                    &wrong_connection,
                    now
                )
                .is_err()
        );

        let wrong_supplier =
            SupplierReferenceTarget { supplier_id: SupplierAccountId::new("supplier-2"), ..target };
        assert!(
            runtime
                .resolve_ticket(
                    SupplierReferenceKind::Endpoint,
                    &tickets.endpoint_ticket,
                    &wrong_supplier,
                    now
                )
                .is_err()
        );

        let mut wrong = connection.clone();
        wrong.supplier_id = SupplierAccountId::new("other");
        assert!(runtime.validate_binding(&wrong, false).is_ok());
        assert!(runtime.validate_binding(&connection, true).is_err());
    }

    #[test]
    fn ticket_entries_preserve_target_validation_and_option_filtering() {
        let (_, transport) = recording_connector(vec![]);
        let runtime = DangaoshushuRuntime::with_transport(settings(), transport.clone()).unwrap();
        let now = Instant::from_unix_secs(1700000000);
        let valid = connection();
        let tickets = runtime.reference_tickets(&valid, now).unwrap();
        assert_eq!(tickets.expires_at, 1700000300);
        let target = SupplierReferenceTarget::from(&valid);
        for kind in [SupplierReferenceKind::Endpoint, SupplierReferenceKind::Credential] {
            let options = runtime.reference_options(kind, &target, now).unwrap();
            assert_eq!(options.len(), 1);
            let resolved = runtime.resolve_ticket(kind, &options[0].reference_id, &target, now).unwrap();
            assert_eq!(
                resolved.internal_reference,
                internal_reference(
                    reference_kind(kind).unwrap(),
                    &proof::binding_hash(&runtime.settings, &target).unwrap()
                )
            );
        }

        for boundary in ["missing_type", "other_type", "connection", "supplier", "deleted"] {
            let mut invalid = valid.clone();
            match boundary {
                "missing_type" => invalid.connector_type = None,
                "other_type" => invalid.connector_type = Some(SupplierConnectorType::JdVop),
                "connection" => invalid.base.id.clear(),
                "supplier" => invalid.supplier_id = SupplierAccountId::new(""),
                "deleted" => invalid.base.deleted_at = 1,
                _ => unreachable!(),
            }
            let expected = if matches!(boundary, "missing_type" | "other_type") {
                error(
                    SupplierFailureClass::CapabilityGap,
                    "DGSS_CONNECTOR_TYPE_MISMATCH",
                    "连接未选择蛋糕叔叔 API",
                )
            } else {
                binding_error()
            };
            assert_eq!(runtime.reference_tickets(&invalid, now).err().unwrap(), expected);
            if boundary != "deleted" {
                assert!(
                    runtime
                        .reference_options(
                            SupplierReferenceKind::Endpoint,
                            &SupplierReferenceTarget::from(&invalid),
                            now
                        )
                        .unwrap()
                        .is_empty()
                );
            }
        }
        assert!(transport.requests.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn capability_health_cannot_use_brand_read_success_as_metadata_evidence() {
        let (connector, transport) = recording_connector(vec![Ok(json!([])), Ok(json!([]))]);
        let runtime = DangaoshushuRuntime { settings: connector.settings, transport: transport.clone() };
        let mut connection = connection();
        let config = runtime.deployment_config(&SupplierReferenceTarget::from(&connection)).unwrap();
        connection.bind_endpoint_reference(config.endpoint, "actor").unwrap();
        connection.bind_credential_reference(config.credential, "actor").unwrap();
        let error =
            runtime.health_check(&connection, SupplierHealthCheckType::CapabilityMetadata).await.unwrap_err();
        assert_eq!(error.class, SupplierFailureClass::CapabilityGap);
        assert_eq!(error.code, "DGSS_CAPABILITY_METADATA_UNSUPPORTED");
        assert!(transport.requests.lock().unwrap().is_empty());

        for check_type in [SupplierHealthCheckType::Connectivity, SupplierHealthCheckType::Authentication] {
            runtime.health_check(&connection, check_type).await.unwrap();
        }
        assert_eq!(transport.requests.lock().unwrap().len(), 2);
    }
    async fn bind_from_options(runtime: &DangaoshushuRuntime, connection: &mut SupplierApiConnection) {
        let target = SupplierReferenceTarget::from(&*connection);
        for kind in [SupplierReferenceKind::Endpoint, SupplierReferenceKind::Credential] {
            let options = runtime.options(kind, &target).await.unwrap();
            assert_eq!(options.len(), 1);
            assert!(!options[0].alias.contains("test-key"));
            let resolved = runtime.resolve(kind, &options[0].reference_id, &target).await.unwrap();
            match kind {
                SupplierReferenceKind::Endpoint => {
                    connection.bind_endpoint_reference(resolved.internal_reference, "actor").unwrap()
                },
                SupplierReferenceKind::Credential => {
                    connection.bind_credential_reference(resolved.internal_reference, "actor").unwrap()
                },
                SupplierReferenceKind::BusinessProfile => unreachable!(),
            }
        }
    }

    #[tokio::test]
    async fn backend_connections_bind_independently_without_config_ids_and_share_transport() {
        let (_, transport) = recording_connector(vec![Ok(json!([])), Ok(json!([]))]);
        let runtime = DangaoshushuRuntime { settings: settings(), transport: transport.clone() };
        let mut first = connection();
        let mut second = connection();
        second.base.id = "created-in-backend-2".into();
        second.supplier_id = SupplierAccountId::new("chosen-supplier-2");
        bind_from_options(&runtime, &mut first).await;
        bind_from_options(&runtime, &mut second).await;
        assert_ne!(first.endpoint_reference, second.endpoint_reference);
        for value in [&first, &second] {
            runtime.health_check(value, SupplierHealthCheckType::Authentication).await.unwrap();
        }
        assert_eq!(transport.requests.lock().unwrap().len(), 2);

        let first_client = runtime.tracking_connector(&first).unwrap();
        let second_client = runtime.tracking_connector(&second).unwrap();
        assert_eq!(second_client.connection_id.as_ref(), "created-in-backend-2");
        assert_ne!(first_client.binding, second_client.binding);
        assert!(Arc::ptr_eq(&first_client.transport, &second_client.transport));

        second.endpoint_reference = first.endpoint_reference;
        assert!(runtime.tracking_connector(&second).is_err());
    }

    #[tokio::test]
    async fn business_access_obeys_backend_status_and_rebinding_after_config_change() {
        let runtime = DangaoshushuRuntime::new(settings()).unwrap();
        let mut connection = connection();
        bind_from_options(&runtime, &mut connection).await;
        assert_eq!(runtime.connector(&connection).err().unwrap().code, "DGSS_CONNECTION_DISABLED");
        connection.stable.status = SupplierApiConnectionStatus::Active;
        assert_eq!(runtime.connector(&connection).err().unwrap().code, "DGSS_HEALTH_REQUIRED");
        connection.record_health(HealthCheckResult::Healthy, Instant::now());
        assert!(runtime.connector(&connection).is_ok());
        connection.stable.status = SupplierApiConnectionStatus::Disabled;
        assert!(runtime.connector(&connection).is_err());
        assert!(runtime.tracking_connector(&connection).is_ok());

        for purpose in [
            ConnectorPurpose::Catalog,
            ConnectorPurpose::Notification,
            ConnectorPurpose::Fulfillment,
            ConnectorPurpose::AfterSales,
        ] {
            assert!(runtime.bind(&connection, purpose).is_ok());
        }
        assert_eq!(
            runtime.bind(&connection, ConnectorPurpose::Creation).err().unwrap().code,
            "DGSS_CONNECTION_DISABLED"
        );

        let description = runtime.description(&SupplierReferenceTarget::from(&connection)).unwrap();
        assert_eq!(
            description,
            ConnectorDescription::from_connector(&runtime.tracking_connector(&connection).unwrap())
        );
        assert_eq!(description.notification_mode, NotificationMode::Callback);
        assert!(!description.funds_source);
        assert!(!description.operations.contains(&SupplierApiCapabilityCode::Refund));
        assert!(!description.operations.contains(&SupplierApiCapabilityCode::Settlement));
        assert!(
            runtime
                .capabilities(&SupplierReferenceTarget::from(&connection))
                .unwrap()
                .contains(&SupplierApiCapabilityCode::Order)
        );

        let mut changed_settings = settings();
        changed_settings.private_key = "rotated-secret".into();
        let changed = DangaoshushuRuntime::new(changed_settings).unwrap();
        assert!(changed.tracking_connector(&connection).is_err());
        bind_from_options(&changed, &mut connection).await;
        assert!(changed.tracking_connector(&connection).is_ok());

        connection.supplier_id = SupplierAccountId::new("other-supplier");
        assert!(changed.tracking_connector(&connection).is_err());
    }

    // 模拟旧连接仍持有有效引用却没有接口类型，验证专用入口拒绝处理，且不会发出供应商请求。
    #[tokio::test]
    async fn direct_adapter_entries_reject_untyped_connection_without_external_calls() {
        let (_, transport) = recording_connector(vec![]);
        let runtime = DangaoshushuRuntime { settings: settings(), transport: transport.clone() };
        let mut connection = connection();
        bind_from_options(&runtime, &mut connection).await;
        let tickets = runtime.reference_tickets(&connection, Instant::now()).unwrap();
        connection.connector_type = None;
        connection.stable.status = SupplierApiConnectionStatus::Active;
        assert_eq!(runtime.connector(&connection).err().unwrap().code, "DGSS_CONNECTOR_TYPE_MISMATCH");
        let query: DangaoshushuReadQuery = from_value(json!({"kind":"brands"})).unwrap();
        assert_eq!(runtime.read(&connection, &query).await.unwrap_err().code, "DGSS_CONNECTOR_TYPE_MISMATCH");
        assert!(runtime.reference_tickets(&connection, Instant::now()).is_err());
        assert!(runtime.health_check(&connection, SupplierHealthCheckType::Connectivity).await.is_err());
        assert!(
            runtime
                .catalog_page(
                    &connection,
                    &Scan { window: ScanWindow::Full, cursor: None, limit: NonZeroU32::new(50).unwrap() }
                )
                .await
                .is_err()
        );

        let target = SupplierReferenceTarget::from(&connection);
        assert!(runtime.options(SupplierReferenceKind::Endpoint, &target).await.unwrap().is_empty());
        assert!(
            runtime
                .resolve(SupplierReferenceKind::Endpoint, &tickets.endpoint_ticket, &target)
                .await
                .is_err()
        );
        assert!(transport.requests.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn configured_endpoint_is_available_without_connection_environment() {
        for endpoint in ["https://dev.dangaoss.cn", "https://other.dangaoss.cn"] {
            let mut config = settings();
            config.base_url = endpoint.into();
            let runtime = DangaoshushuRuntime::new(config).unwrap();
            let mut connection = connection();
            bind_from_options(&runtime, &mut connection).await;
            assert!(runtime.tracking_connector(&connection).is_ok());
        }
    }

    #[tokio::test]
    async fn disabled_connection_keeps_existing_order_queries_without_enabling_writes() {
        let (connector, transport) = recording_connector(vec![Ok(json!({
            "channel_no":"test-channel","out_order_no":"merchant-1","order_no":"external-1",
            "status":"1"
        }))]);
        let runtime = DangaoshushuRuntime { settings: connector.settings, transport: transport.clone() };
        let mut connection = connection();
        bind_from_options(&runtime, &mut connection).await;
        assert!(!connection.is_active());
        assert_eq!(runtime.connector(&connection).err().unwrap().code, "DGSS_CONNECTION_DISABLED");

        let result =
            runtime.observe_order(&connection, &OrderKey::External("external-1".into())).await.unwrap();
        let Lookup::Found(result) = result else {
            panic!("known order must be returned");
        };
        assert_eq!(result.value.reference.merchant_order_no, "merchant-1");
        assert_eq!(result.stamp.changed_at, None);
        assert_eq!(transport.requests.lock().unwrap().len(), 1);
    }
}
