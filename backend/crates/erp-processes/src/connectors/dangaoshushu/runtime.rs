use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use config::DangaoshushuConfig;
use erp_core::common::time::Instant;
use erp_supply::entity::failure::SupplierFailureClass;
use erp_supply::entity::supplier_api::{
    ConnectionEnvironment, SupplierApiConnection, SupplierApiConnectionStatus, SupplierHealthCheckType,
};
use erp_supply::ports::connector::common::{ConnectorError, ConnectorResult};
use erp_supply::ports::supplier_api_gateway::{ClassifiedError, SupplierApiGateway};
use erp_supply::ports::supplier_reference_registry::{
    ResolvedSupplierReference, SupplierReferenceKind, SupplierReferenceOption, SupplierReferenceRegistry,
    SupplierReferenceTarget,
};
use reqwest::Url;
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
    environment: String,
    kind: String,
    fingerprint: String,
    expires_at: i64,
}

impl DangaoshushuRuntime {
    /// 登记 SafeConfig 技术参数，不选择 ERP 供应商或连接，不发送请求。
    /// # 参数
    /// `settings` 为技术配置启动快照。
    /// # 返回
    /// 可供后台连接绑定的配置注册表。
    /// # 错误
    /// 关闭或无效配置、HTTP 客户端创建失败时返回分类错误。
    pub fn new(settings: DangaoshushuConfig) -> ConnectorResult<Self> {
        settings.validate().map_err(|_| {
            error(SupplierFailureClass::MappingError, "DGSS_CONFIG_INVALID", "蛋糕叔叔技术配置无效")
        })?;
        if !settings.enabled {
            return Err(error(
                SupplierFailureClass::CapabilityGap,
                "DGSS_DISABLED",
                "蛋糕叔叔技术配置未登记",
            ));
        }
        let transport = Arc::new(HttpTransport::new(&settings)?);
        Ok(Self { settings, transport })
    }

    /// 为已启用的后台连接取得协议客户端；连接身份、供应商与环境均来自记录。
    /// # 参数
    /// `connection` 为本次从后台连接仓储读取的记录。
    /// # 返回
    /// 与该记录及已绑定技术引用一致的客户端，共享渠道限流。
    /// # 错误
    /// 停用、删除、环境或技术绑定不符时拒绝业务调用。
    pub fn connector(&self, connection: &SupplierApiConnection) -> ConnectorResult<DangaoshushuConnector> {
        if connection.stable.status != SupplierApiConnectionStatus::Active {
            return Err(error(
                SupplierFailureClass::BusinessRejected,
                "DGSS_CONNECTION_DISABLED",
                "请先在后台启用供应商 API 连接",
            ));
        }
        self.bound_connector(connection)
    }

    fn bound_connector(&self, connection: &SupplierApiConnection) -> ConnectorResult<DangaoshushuConnector> {
        self.validate_binding(connection, true)?;
        DangaoshushuConnector::bind(
            self.settings.clone(),
            &SupplierReferenceTarget::from(connection),
            Arc::clone(&self.transport),
        )
    }

    /// 按当前后台连接读取协议目录；停用连接仍允许显式诊断查询。
    /// # 参数
    /// `connection` 为授权连接，`query` 为固定白名单查询。
    /// # 返回
    /// 未应用到业务实体的供应商原始资料。
    /// # 错误
    /// 未绑定、配置失效或上游读取失败时返回分类错误。
    pub async fn read(
        &self,
        connection: &SupplierApiConnection,
        query: &DangaoshushuReadQuery,
    ) -> ConnectorResult<Value> {
        self.bound_connector(connection)?.read(query).await
    }

    /// 核对后台记录及其技术引用；不要求配置文件重复登记 ERP 身份。
    /// # 参数
    /// `connection` 为后台连接；`references_required` 指示是否须完成技术绑定。
    /// # 返回
    /// 当前记录与所选技术配置一致时返回 Ok。
    /// # 错误
    /// 删除、环境不适用或引用陈旧、跨连接复制时返回绑定错误。
    pub fn validate_binding(
        &self,
        connection: &SupplierApiConnection,
        references_required: bool,
    ) -> ConnectorResult<()> {
        let target = SupplierReferenceTarget::from(connection);
        self.validate_target(&target)?;
        if connection.base.is_deleted()
            || (references_required
                && (!connection.endpoint_reference_bound
                    || !connection.credential_reference_bound
                    || connection.endpoint_reference != self.internal_reference("endpoint", &target)?
                    || connection.credential_reference.as_deref()
                        != Some(self.internal_reference("credential", &target)?.as_str())))
        {
            return Err(binding_error());
        }
        Ok(())
    }

    fn validate_target(&self, target: &SupplierReferenceTarget) -> ConnectorResult<()> {
        let url = Url::parse(&self.settings.base_url).map_err(|_| mapping())?;
        if target.connection_id.as_ref().is_empty()
            || target.supplier_id.as_ref().is_empty()
            || (target.environment == ConnectionEnvironment::Production
                && url.host_str() == Some("dev.dangaoss.cn"))
        {
            return Err(binding_error());
        }
        Ok(())
    }

    /// 为后台连接签发五分钟技术引用票据，无需手填内部 ID。
    /// # 参数
    /// `connection` 为授权连接记录，`now` 为签发时间。
    /// # 返回
    /// 绑定连接、供应商、环境及当前配置的两个票据。
    /// # 错误
    /// 连接不可用、环境不符或编码失败时返回分类错误。
    pub fn reference_tickets(
        &self,
        connection: &SupplierApiConnection,
        now: Instant,
    ) -> ConnectorResult<SupplierReferenceTickets> {
        self.validate_binding(connection, false)?;
        self.tickets(&SupplierReferenceTarget::from(connection), now)
    }

    fn tickets(
        &self,
        target: &SupplierReferenceTarget,
        now: Instant,
    ) -> ConnectorResult<SupplierReferenceTickets> {
        self.validate_target(target)?;
        let expires_at = now.unix_secs().checked_add(300).ok_or_else(mapping)?;
        let ticket = |kind: &str| {
            proof::encode(
                &self.settings.private_key,
                &ReferenceTicket {
                    connection_id: target.connection_id.to_string(),
                    supplier_id: target.supplier_id.to_string(),
                    environment: target.environment.as_str().into(),
                    kind: kind.into(),
                    fingerprint: proof::binding_hash(&self.settings, target)?,
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

    fn internal_reference(&self, kind: &str, target: &SupplierReferenceTarget) -> ConnectorResult<String> {
        Ok(format!("config://dangaoshushu/{}/{kind}", proof::binding_hash(&self.settings, target)?))
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
        if ticket.kind != expected
            || ticket.environment != target.environment.as_str()
            || ticket.connection_id != target.connection_id.as_ref()
            || ticket.supplier_id != target.supplier_id.as_ref()
            || ticket.fingerprint != proof::binding_hash(&self.settings, target)?
            || ticket.expires_at < now.unix_secs()
            || ticket.expires_at > now.unix_secs().saturating_add(300)
        {
            return Err(error(
                SupplierFailureClass::AuthSignature,
                "DGSS_REFERENCE_TICKET_INVALID",
                "供应商技术引用票据无效或过期",
            ));
        }
        self.validate_target(target)?;
        Ok(ResolvedSupplierReference { internal_reference: self.internal_reference(expected, target)? })
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
        let version = match target.environment {
            ConnectionEnvironment::Testing => "测试环境",
            ConnectionEnvironment::Production => "生产环境",
        };
        Ok(vec![SupplierReferenceOption {
            reference_id,
            alias: alias.into(),
            version: version.into(),
            expires_at: tickets.expires_at,
        }])
    }
}

fn binding_error() -> ConnectorError {
    error(
        SupplierFailureClass::AuthSignature,
        "DGSS_CONNECTION_BINDING",
        "供应商连接环境或已绑定的技术配置不一致，请在后台重新选择配置",
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

impl SupplierApiGateway for DangaoshushuRuntime {
    fn health_check<'a>(
        &'a self,
        connection: &'a SupplierApiConnection,
        check_type: SupplierHealthCheckType,
    ) -> Pin<Box<dyn Future<Output = Result<(), ClassifiedError>> + Send + 'a>> {
        Box::pin(async move {
            let connector = self.bound_connector(connection).map_err(classified)?;
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
    fn catalog_sync<'a>(
        &'a self,
        _connection: &'a SupplierApiConnection,
    ) -> Pin<Box<dyn Future<Output = Result<(), ClassifiedError>> + Send + 'a>> {
        Box::pin(async {
            Err(ClassifiedError {
                class: SupplierFailureClass::CapabilityGap,
                code: "DGSS_CATALOG_APPLY_NOT_CONFIGURED".into(),
                summary: "目录读取不等于正式供给同步；须先绑定公司 SKU 并配置来源证据应用".into(),
            })
        })
    }
}
impl SupplierReferenceRegistry for DangaoshushuRuntime {
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
    use erp_core::ids::{SupplierAccountId, SupplierApiConnectionId};
    use erp_supply::entity::supplier_api::{
        ConnectionEnvironment, SupplierApiConnectionData, SupplierApiConnectionStatus,
    };

    use super::super::test_support::{recording_connector, settings};
    use super::*;

    fn connection() -> SupplierApiConnection {
        SupplierApiConnection::new(
            SupplierApiConnectionId::new("connection-1"),
            SupplierApiConnectionData {
                supplier_id: SupplierAccountId::new("supplier-1"),
                connection_code: "dgss-test".into(),
                environment: ConnectionEnvironment::Testing,
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
    fn tickets_are_kind_environment_connection_and_expiry_bound() {
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
        assert!(
            runtime
                .resolve_ticket(
                    SupplierReferenceKind::Endpoint,
                    &tickets.endpoint_ticket,
                    &SupplierReferenceTarget {
                        environment: ConnectionEnvironment::Production,
                        ..target.clone()
                    },
                    now
                )
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

    #[tokio::test]
    async fn capability_health_cannot_use_brand_read_success_as_metadata_evidence() {
        let (connector, transport) =
            recording_connector(vec![Ok(serde_json::json!([])), Ok(serde_json::json!([]))]);
        let runtime = DangaoshushuRuntime { settings: connector.settings, transport: transport.clone() };
        let mut connection = connection();
        connection
            .bind_endpoint_reference(
                runtime.internal_reference("endpoint", &SupplierReferenceTarget::from(&connection)).unwrap(),
                "actor",
            )
            .unwrap();
        connection
            .bind_credential_reference(
                runtime
                    .internal_reference("credential", &SupplierReferenceTarget::from(&connection))
                    .unwrap(),
                "actor",
            )
            .unwrap();
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
        let (_, transport) = recording_connector(vec![Ok(serde_json::json!([])), Ok(serde_json::json!([]))]);
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
        let first_client = runtime.bound_connector(&first).unwrap();
        let second_client = runtime.bound_connector(&second).unwrap();
        assert_eq!(second_client.connection_id.as_ref(), "created-in-backend-2");
        assert_ne!(first_client.binding, second_client.binding);
        assert!(Arc::ptr_eq(&first_client.transport, &second_client.transport));
        second.endpoint_reference = first.endpoint_reference;
        assert!(runtime.bound_connector(&second).is_err());
    }

    #[tokio::test]
    async fn business_access_obeys_backend_status_and_rebinding_after_config_change() {
        let runtime = DangaoshushuRuntime::new(settings()).unwrap();
        let mut connection = connection();
        bind_from_options(&runtime, &mut connection).await;
        assert_eq!(runtime.connector(&connection).err().unwrap().code, "DGSS_CONNECTION_DISABLED");
        connection.stable.status = SupplierApiConnectionStatus::Active;
        assert!(runtime.connector(&connection).is_ok());
        connection.stable.status = SupplierApiConnectionStatus::Disabled;
        assert!(runtime.connector(&connection).is_err());
        let mut changed_settings = settings();
        changed_settings.private_key = "rotated-secret".into();
        let changed = DangaoshushuRuntime::new(changed_settings).unwrap();
        assert!(changed.bound_connector(&connection).is_err());
        bind_from_options(&changed, &mut connection).await;
        assert!(changed.bound_connector(&connection).is_ok());
        connection.supplier_id = SupplierAccountId::new("other-supplier");
        assert!(changed.bound_connector(&connection).is_err());
    }

    #[tokio::test]
    async fn testing_endpoint_is_not_offered_or_callable_for_production_connection() {
        let runtime = DangaoshushuRuntime::new(settings()).unwrap();
        let mut connection = connection();
        connection.environment = ConnectionEnvironment::Production;
        let options = runtime
            .options(SupplierReferenceKind::Endpoint, &SupplierReferenceTarget::from(&connection))
            .await
            .unwrap();
        assert!(options.is_empty());
        assert!(runtime.reference_tickets(&connection, Instant::now()).is_err());
        assert!(runtime.health_check(&connection, SupplierHealthCheckType::Authentication).await.is_err());
    }
}
