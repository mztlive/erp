use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use config::DangaoshushuConfig;
use erp_core::common::time::Instant;
use erp_supply::entity::failure::SupplierFailureClass;
use erp_supply::entity::supplier_api::{SupplierApiConnection, SupplierHealthCheckType};
use erp_supply::ports::connector::common::{ConnectorError, ConnectorResult};
use erp_supply::ports::supplier_api_gateway::{ClassifiedError, SupplierApiGateway};
use erp_supply::ports::supplier_reference_registry::{
    ResolvedSupplierReference, SupplierReferenceKind, SupplierReferenceRegistry, SupplierReferenceTarget,
};
use serde::{Deserialize, Serialize};

use super::parsing::mapping;
use super::{DangaoshushuConnector, error, proof};

/// 一个配置快照装配的协议实例和权威技术引用；配置变更须重启并重新绑定引用。
pub struct DangaoshushuRuntime {
    connector: Arc<DangaoshushuConnector>,
    fingerprint: String,
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
    /// 装配连接实例，不发送外部请求。
    /// # 参数
    /// `settings` 是 SafeConfig 提供的供应商配置快照。
    /// # 返回
    /// 可共享的供应商运行时。
    /// # 错误
    /// 无效或关闭配置返回分类错误。
    pub fn new(settings: DangaoshushuConfig) -> ConnectorResult<Self> {
        let connector = Arc::new(DangaoshushuConnector::new(settings)?);
        let fingerprint = connector.binding.clone();
        Ok(Self { connector, fingerprint })
    }

    /// 返回固定绑定的协议实例；业务操作前调用 validate_binding。
    /// # 参数
    /// 无。
    /// # 返回
    /// 共享协议实例。
    /// # 错误
    /// 无。
    pub fn connector(&self) -> Arc<DangaoshushuConnector> {
        Arc::clone(&self.connector)
    }

    /// 核对当前数据库连接与配置，防止跨供应商、跨环境或陈旧技术引用调用。
    /// # 参数
    /// `connection` 为本次读取的连接；`references_required` 控制是否要求技术引用已绑定。
    /// # 返回
    /// 核验通过返回 Ok。
    /// # 错误
    /// 连接身份、环境、引用或软删除状态不一致时返回失败关闭错误。
    pub fn validate_binding(
        &self,
        connection: &SupplierApiConnection,
        references_required: bool,
    ) -> ConnectorResult<()> {
        let settings = &self.connector.settings;
        if connection.base.is_deleted()
            || connection.base.id != settings.connection_id
            || connection.supplier_id.as_ref() != settings.supplier_id
            || connection.environment.as_str() != settings.environment
            || (references_required
                && (!connection.endpoint_reference_bound
                    || !connection.credential_reference_bound
                    || connection.endpoint_reference != self.internal_reference("endpoint")
                    || connection.credential_reference.as_deref()
                        != Some(&self.internal_reference("credential"))))
        {
            return Err(error(
                SupplierFailureClass::AuthSignature,
                "DGSS_CONNECTION_BINDING",
                "供应商连接、环境或技术配置绑定不一致",
            ));
        }
        Ok(())
    }

    /// 为已授权连接签发五分钟技术引用票据；票据不包含地址或密钥正文。
    /// # 参数
    /// `connection` 为当前授权连接；`now` 为签发时间。
    /// # 返回
    /// 端点与凭证绑定票据，供既有连接治理强命令消费。
    /// # 错误
    /// 连接不匹配或编码失败时返回分类错误。
    pub fn reference_tickets(
        &self,
        connection: &SupplierApiConnection,
        now: Instant,
    ) -> ConnectorResult<SupplierReferenceTickets> {
        self.validate_binding(connection, false)?;
        let expires_at = now.unix_secs().checked_add(300).ok_or_else(mapping)?;
        let ticket = |kind: &str| {
            proof::encode(
                &self.connector.settings.private_key,
                &ReferenceTicket {
                    connection_id: connection.base.id.clone(),
                    supplier_id: connection.supplier_id.as_ref().to_owned(),
                    environment: connection.environment.as_str().into(),
                    kind: kind.into(),
                    fingerprint: self.fingerprint.clone(),
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
    fn internal_reference(&self, kind: &str) -> String {
        format!("config://dangaoshushu/{}/{kind}", self.fingerprint)
    }

    fn resolve_ticket(
        &self,
        kind: SupplierReferenceKind,
        payload: &str,
        target: &SupplierReferenceTarget,
        now: Instant,
    ) -> ConnectorResult<ResolvedSupplierReference> {
        let expected = match kind {
            SupplierReferenceKind::Endpoint => "endpoint",
            SupplierReferenceKind::Credential => "credential",
            SupplierReferenceKind::BusinessProfile => {
                return Err(error(
                    SupplierFailureClass::CapabilityGap,
                    "DGSS_BUSINESS_PROFILE_REGISTRY_UNAVAILABLE",
                    "业务资料须由采购资料注册表核验",
                ));
            },
        };
        let ticket: ReferenceTicket = proof::decode(&self.connector.settings.private_key, payload)?;
        if ticket.kind != expected
            || ticket.environment != target.environment.as_str()
            || ticket.environment != self.connector.settings.environment
            || ticket.connection_id != target.connection_id.as_ref()
            || ticket.connection_id != self.connector.settings.connection_id
            || ticket.supplier_id != target.supplier_id.as_ref()
            || ticket.supplier_id != self.connector.settings.supplier_id
            || ticket.fingerprint != self.fingerprint
            || ticket.expires_at < now.unix_secs()
            || ticket.expires_at > now.unix_secs().saturating_add(300)
        {
            return Err(error(
                SupplierFailureClass::AuthSignature,
                "DGSS_REFERENCE_TICKET_INVALID",
                "供应商技术引用票据无效或过期",
            ));
        }
        Ok(ResolvedSupplierReference { internal_reference: self.internal_reference(expected) })
    }
}

impl SupplierApiGateway for DangaoshushuRuntime {
    /// 按运行冻结的种类执行只读检查；品牌请求只证明可达性或鉴权。
    /// # 参数
    /// `connection` 为当前连接；`check_type` 为已启动运行冻结的检查种类。
    /// # 返回
    /// 可达性或鉴权读取成功返回 Ok。
    /// # 错误
    /// 身份不匹配、只读调用失败或能力元数据不受支持时返回分类错误。
    fn health_check<'a>(
        &'a self,
        connection: &'a SupplierApiConnection,
        check_type: SupplierHealthCheckType,
    ) -> Pin<Box<dyn Future<Output = Result<(), ClassifiedError>> + Send + 'a>> {
        Box::pin(async move {
            self.validate_binding(connection, true).map_err(classified)?;
            if check_type == SupplierHealthCheckType::CapabilityMetadata {
                return Err(ClassifiedError {
                    class: SupplierFailureClass::CapabilityGap,
                    code: "DGSS_CAPABILITY_METADATA_UNSUPPORTED".into(),
                    summary: "品牌读取不能形成能力元数据验证证据".into(),
                });
            }
            self.connector.brands().await.map_err(classified)?;
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
        assert!(runtime.validate_binding(&wrong, false).is_err());
        assert!(runtime.validate_binding(&connection, true).is_err());
    }

    #[tokio::test]
    async fn capability_health_cannot_use_brand_read_success_as_metadata_evidence() {
        let (connector, transport) =
            recording_connector(vec![Ok(serde_json::json!([])), Ok(serde_json::json!([]))]);
        let fingerprint = connector.binding.clone();
        let runtime = DangaoshushuRuntime { connector: Arc::new(connector), fingerprint };
        let mut connection = connection();
        connection.bind_endpoint_reference(runtime.internal_reference("endpoint"), "actor").unwrap();
        connection.bind_credential_reference(runtime.internal_reference("credential"), "actor").unwrap();
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
}
