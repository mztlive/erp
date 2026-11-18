//! 由 SafeConfig 装配京东账号，未取得认证实现时明确拒绝远端调用。
use std::collections::BTreeSet;
use std::future::Future;
use std::num::NonZeroU32;
use std::pin::Pin;
use std::sync::Arc;

use config::JdVopConfig;
use erp_party::SensitiveDataCodec;
use erp_supply::entity::failure::SupplierFailureClass;
use erp_supply::entity::supplier_api::{
    SupplierApiConnection, SupplierApiConnectionStatus, SupplierConnectorType, SupplierHealthCheckType,
};
use erp_supply::ports::catalog_sync::CatalogSyncPolicy;
use erp_supply::ports::connector::SupplierConnector;
use erp_supply::ports::connector::common::{ConnectorError, ConnectorResult};
use erp_supply::ports::connector::description::ConnectorDescription;
use erp_supply::ports::connector::offer::IncrementalSupport;
use erp_supply::ports::connector::runtime::{ConnectorPurpose, SupplierRuntime};
use erp_supply::ports::supplier_api_gateway::ClassifiedError;
use erp_supply::ports::supplier_reference_registry::{
    ResolvedSupplierReference, SupplierDeploymentConfig, SupplierReferenceKind, SupplierReferenceRegistry,
    SupplierReferenceTarget,
};
use hmac::{Hmac, KeyInit, Mac};
use serde_json::{json, to_vec};
use sha2::Sha256;

use super::messages::MessageRuntime;
use super::transport::{JdVopReadRequest, JdVopReadTransport, JdVopWriteTransport, ReadClient, WriteClient};
use super::wire::Pools;
use super::{
    JdVopConnector, JdVopExecutionProfile, JdVopOfferMapper, classified, error, mapping, protocol_required,
};

/// 一个部署账号的共享读取配额和配置身份；不依赖其他供应商运行时。
pub struct JdVopRuntime {
    settings: JdVopConfig,
    reader: Option<Arc<ReadClient>>,
    writer: Option<Arc<WriteClient>>,
    profile: Option<Arc<JdVopExecutionProfile>>,
    evidence_codec: Option<Arc<SensitiveDataCodec>>,
    message_runtime: Option<Arc<MessageRuntime>>,
}

impl JdVopRuntime {
    /// 发起页面授权前，应用应传入已按操作人权限加载的原连接，离线核对京东接口类型及当前部署的端点和凭据引用。
    /// 连接已删除或绑定不符时拒绝，不访问供应商，也不把授权转给其他账号。
    pub fn verify_authorization_binding(&self, connection: &SupplierApiConnection) -> ConnectorResult<()> {
        self.validate_binding(connection)
    }
    /// 启动时登记 SafeConfig 中已启用的京东技术配置，供后续绑定账号和接入传输。
    /// 配置关闭或无效时报错；未注入已验收传输的运行时不发请求，也不声明远端读写能力。
    pub fn new(settings: JdVopConfig) -> ConnectorResult<Self> {
        settings.validate().map_err(|_| mapping("京东接入设置不完整，请联系管理员核对连接配置"))?;
        if !settings.enabled {
            return Err(error(
                SupplierFailureClass::CapabilityGap,
                "JD_VOP_DISABLED",
                "尚未登记京东接入设置，请联系管理员完成配置",
            ));
        }
        Ok(Self {
            settings,
            reader: None,
            writer: None,
            profile: None,
            evidence_codec: None,
            message_runtime: None,
        })
    }

    /// 组合根在 G0 固定向量和账号授权验收完成后，注入受信的读取实现供目录等查询使用。
    /// 不得从用户载荷构造传输；此处校验并登记配置，不发请求验收，也不开放下单、确认、消息删除或资金操作。
    pub fn with_verified_transport(
        settings: JdVopConfig,
        transport: Arc<dyn JdVopReadTransport>,
    ) -> ConnectorResult<Self> {
        let mut runtime = Self::new(settings)?;
        runtime.reader = Some(Arc::new(ReadClient::new(
            transport,
            runtime.settings.max_concurrent_reads,
            runtime.settings.requests_per_second,
            runtime.settings.timeout_seconds,
        )));
        Ok(runtime)
    }

    /// 组合根取得 G0 验收的账号执行资料、财务核实的发票资料和单次发送传输后，装配订单执行及证据处理能力。
    /// 构造不访问网络，缺失必要资料时报错。未配置确认资金规则时，不开放新增下单及原单确认，仍可查询、取消和调查原单。
    pub fn with_verified_execution(
        settings: JdVopConfig,
        read: Arc<dyn JdVopReadTransport>,
        write: Arc<dyn JdVopWriteTransport>,
        profile: JdVopExecutionProfile,
        evidence_codec: Arc<SensitiveDataCodec>,
    ) -> ConnectorResult<Self> {
        profile.validate()?;
        let mut runtime = Self::with_verified_transport(settings, read)?;
        let reader = runtime.reader.as_ref().ok_or_else(protocol_required)?.clone();
        runtime.writer = Some(Arc::new(WriteClient { transport: write, reader }));
        runtime.profile = Some(Arc::new(profile));
        runtime.evidence_codec = Some(evidence_codec);
        Ok(runtime)
    }

    // 启用独立轮询时，用已有读取客户端及传输登记已核实的消息类型、渠道和轮询间隔。
    // 没有读取客户端则拒绝；消息删除所需的发送器不赋予订单写能力。
    pub(super) fn configure_messages(
        &mut self,
        transport: Arc<dyn JdVopWriteTransport>,
        types: BTreeSet<u32>,
        source_type: u32,
        interval: u64,
    ) -> ConnectorResult<()> {
        let reader = self.reader.as_ref().ok_or_else(protocol_required)?.clone();
        self.message_runtime = Some(Arc::new(MessageRuntime {
            writer: Arc::new(WriteClient { transport, reader }),
            types,
            source_type,
            interval,
        }));
        Ok(())
    }

    /// 新增下单或履约前，从服务端加载的连接取得已核对技术绑定的京东客户端。
    /// 自动下单关闭、当前配置健康检查未通过、绑定失效或未核实确认资金规则时拒绝。
    pub fn connector(&self, connection: &SupplierApiConnection) -> ConnectorResult<JdVopConnector> {
        let connector = self.tracking_connector(connection)?;
        if connection.stable.status != SupplierApiConnectionStatus::Active
            || !connection.current_technical_config_is_healthy()
        {
            return Err(error(
                SupplierFailureClass::BusinessRejected,
                "JD_VOP_CREATION_DISABLED",
                "请先启用自动下单并完成连接检查",
            ));
        }
        self.profile.as_ref().ok_or_else(protocol_required)?.ensure_confirmation_funds()?;
        Ok(connector)
    }

    /// 调查既有订单时按原连接和部署账号取客户端，不因关闭自动下单而换账号或中断跟踪。
    /// 连接删除、协议不符或技术引用缺失、陈旧时拒绝。
    pub fn tracking_connector(&self, connection: &SupplierApiConnection) -> ConnectorResult<JdVopConnector> {
        self.validate_binding(connection)?;
        self.connector_for_target(&SupplierReferenceTarget::from(connection))
    }

    fn connector_for_target(&self, target: &SupplierReferenceTarget) -> ConnectorResult<JdVopConnector> {
        Ok(JdVopConnector {
            connection_id: target.connection_id.clone(),
            account_scope: self.settings.account_scope.clone(),
            binding: self.binding(target)?,
            cursor_secret: self.settings.app_secret.clone(),
            reader: self.reader.clone(),
            mapper: JdVopOfferMapper { image_base_url: self.settings.image_base_url.clone() },
            writer: self.writer.clone(),
            profile: self.profile.clone(),
            evidence_codec: self.evidence_codec.clone(),
            message_runtime: self.message_runtime.clone(),
        })
    }

    fn validate_target(&self, target: &SupplierReferenceTarget) -> ConnectorResult<()> {
        if target.connector_type != Some(SupplierConnectorType::JdVop)
            || target.connection_id.as_ref().is_empty()
            || target.supplier_id.as_ref().is_empty()
        {
            return Err(binding_error());
        }
        Ok(())
    }

    fn binding(&self, target: &SupplierReferenceTarget) -> ConnectorResult<String> {
        self.validate_target(target)?;
        let profile = self.profile.as_ref().map(|profile| profile.binding_content()).transpose()?;
        let body = to_vec(&json!({"gateway":self.settings.gateway_url,"app_key":self.settings.app_key,"account_scope":self.settings.account_scope,"credential_id":self.settings.credential_id,"source_type":self.settings.source_type,"image_base_url":self.settings.image_base_url,"connection":target.connection_id,"supplier":target.supplier_id,"execution_profile":profile})).map_err(|_| mapping("京东配置引用生成失败"))?;

        let mut mac = Hmac::<Sha256>::new_from_slice(self.settings.app_secret.as_bytes())
            .expect("HMAC接受任意长度密钥");
        mac.update(b"erp-jd-vop-configuration-binding-v1\0");
        mac.update(&body);
        Ok(hex::encode(mac.finalize().into_bytes()))
    }

    fn references(&self, target: &SupplierReferenceTarget) -> ConnectorResult<SupplierDeploymentConfig> {
        let binding = self.binding(target)?;
        Ok(SupplierDeploymentConfig {
            endpoint: format!("config://jd-vop/{binding}/endpoint"),
            credential: format!("config://jd-vop/{binding}/credential"),
        })
    }

    fn validate_binding(&self, connection: &SupplierApiConnection) -> ConnectorResult<()> {
        let references = self.references(&SupplierReferenceTarget::from(connection))?;
        if connection.base.is_deleted()
            || !connection.endpoint_reference_bound
            || !connection.credential_reference_bound
            || connection.endpoint_reference != references.endpoint
            || connection.credential_reference.as_deref() != Some(references.credential.as_str())
        {
            return Err(binding_error());
        }
        Ok(())
    }
}

impl JdVopRuntime {
    fn validate_funds_binding(&self, connection: &SupplierApiConnection) -> ConnectorResult<()> {
        if let Some(profile) = &self.profile
            && connection.supplier_deposit_external_account_ref.as_deref()
                != Some(profile.verified_external_funds_account_ref.as_str())
        {
            return Err(error(
                SupplierFailureClass::BusinessRejected,
                "JD_VOP_FUNDS_ACCOUNT_MISMATCH",
                "预存款账户与已核实的京东资金账号不一致",
            ));
        }
        Ok(())
    }
}

impl SupplierRuntime for JdVopRuntime {
    fn connector_type(&self) -> SupplierConnectorType {
        SupplierConnectorType::JdVop
    }

    fn bind(
        &self,
        connection: &SupplierApiConnection,
        purpose: ConnectorPurpose,
    ) -> Result<Arc<dyn SupplierConnector>, ClassifiedError> {
        if matches!(purpose, ConnectorPurpose::Creation | ConnectorPurpose::Fulfillment) {
            self.validate_funds_binding(connection).map_err(classified)?;
        }
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

    fn catalog_schedule(
        &self,
        connection: &SupplierApiConnection,
    ) -> Result<Option<CatalogSyncPolicy>, ClassifiedError> {
        self.validate_target(&SupplierReferenceTarget::from(connection)).map_err(classified)?;
        Ok(self.reader.as_ref().map(|_| CatalogSyncPolicy {
            mapping_version: 2,
            sync_interval_seconds: self.settings.catalog_sync_interval_seconds,
            full_sync_interval_seconds: self.settings.catalog_full_sync_interval_seconds,
            overlap_seconds: 0,
            page_size: NonZeroU32::new(20).expect("20非零"),
            incremental: IncrementalSupport::Unsupported,
        }))
    }

    fn check_health<'a>(
        &'a self,
        connection: &'a SupplierApiConnection,
        check_type: SupplierHealthCheckType,
    ) -> Pin<Box<dyn Future<Output = Result<(), ClassifiedError>> + Send + 'a>> {
        Box::pin(async move {
            self.validate_binding(connection).map_err(classified)?;
            if check_type == SupplierHealthCheckType::CapabilityMetadata {
                return Err(classified(error(
                    SupplierFailureClass::CapabilityGap,
                    "JD_VOP_CAPABILITY_METADATA_UNSUPPORTED",
                    "商品池读取不能代替完整接口授权核验",
                )));
            }

            let reader = self.reader.as_ref().ok_or_else(|| classified(protocol_required()))?;
            let _: Pools = reader.read(JdVopReadRequest::Pools).await.map_err(classified)?;
            Ok(())
        })
    }
}

impl SupplierReferenceRegistry for JdVopRuntime {
    fn description(&self, target: &SupplierReferenceTarget) -> Result<ConnectorDescription, ClassifiedError> {
        let connector = self.connector_for_target(target).map_err(classified)?;
        Ok(ConnectorDescription::from_connector(&connector))
    }

    fn deployment_config(
        &self,
        target: &SupplierReferenceTarget,
    ) -> Result<SupplierDeploymentConfig, ClassifiedError> {
        self.references(target).map_err(classified)
    }

    fn is_available(&self) -> bool {
        true
    }

    fn resolve<'a>(
        &'a self,
        _: SupplierReferenceKind,
        _: &'a str,
        _: &'a SupplierReferenceTarget,
    ) -> Pin<Box<dyn Future<Output = Result<ResolvedSupplierReference, ClassifiedError>> + Send + 'a>> {
        Box::pin(async {
            Err(classified(error(
                SupplierFailureClass::CapabilityGap,
                "JD_VOP_MANUAL_REFERENCE_UNSUPPORTED",
                "京东接入设置由管理员维护，请联系管理员核对连接配置",
            )))
        })
    }
}

fn binding_error() -> ConnectorError {
    error(
        SupplierFailureClass::MappingError,
        "JD_VOP_BINDING_INVALID",
        "京东接口账号或配置绑定已变化，请重新核对连接",
    )
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use erp_core::ids::{SupplierAccountId, SupplierApiConnectionId};
    use erp_supply::entity::supplier_api::SupplierApiConnectionData;
    use erp_supply::ports::supplier_api_gateway::SupplierApiGateway;

    use super::*;

    fn runtime() -> JdVopRuntime {
        let settings = toml_config();
        JdVopRuntime::new(settings).unwrap()
    }

    fn toml_config() -> JdVopConfig {
        serde_json::from_value(json!({"enabled":true,"gateway_url":"https://gateway.example/router","app_key":"app-1","app_secret":"secret-1","credential_id":"jd-account-1-token","credential_encryption_key":"offline-test-only-independent-key-at-least-32","account_scope":"account-1","source_type":3})).unwrap()
    }

    fn connection(runtime: &JdVopRuntime) -> SupplierApiConnection {
        let mut connection = SupplierApiConnection::new(
            SupplierApiConnectionId::new("jd-connection"),
            SupplierApiConnectionData {
                supplier_id: SupplierAccountId::new("supplier-1"),
                connection_code: "jd".into(),
                connector_type: SupplierConnectorType::JdVop,
                endpoint_reference: String::new(),
                credential_reference: None,
                rate_limit_policy: None,
                status: SupplierApiConnectionStatus::Disabled,
            },
            "actor",
        )
        .unwrap();
        let references = runtime.references(&SupplierReferenceTarget::from(&connection)).unwrap();
        connection.endpoint_reference = references.endpoint;
        connection.credential_reference = Some(references.credential);
        connection.endpoint_reference_bound = true;
        connection.credential_reference_bound = true;
        connection
    }

    #[tokio::test]
    async fn unverified_runtime_never_claims_remote_capability_or_health() {
        let runtime = runtime();
        let connection = connection(&runtime);
        let connector = runtime.tracking_connector(&connection).unwrap();
        assert!(connector.capabilities().is_empty());
        assert!(connector.offers().is_none());
        assert!(connector.orders().is_none());
        assert!(connector.payment_confirmation().is_none());

        let description = runtime.description(&SupplierReferenceTarget::from(&connection)).unwrap();
        assert!(description.operations.is_empty());
        assert_eq!(description, ConnectorDescription::from_connector(&connector));
        assert!(!description.address_candidates);
        assert!(!description.funds_source);
        assert!(runtime.catalog_policy(&connection).unwrap().is_none());

        assert_eq!(
            runtime
                .health_check(&connection, SupplierHealthCheckType::Authentication)
                .await
                .unwrap_err()
                .code,
            "JD_VOP_PROTOCOL_REQUIRED"
        );
        assert_eq!(runtime.connector(&connection).err().unwrap().code, "JD_VOP_CREATION_DISABLED");
    }

    #[test]
    fn shared_connector_construction_preserves_read_capabilities_and_binding_failures() {
        struct Offline;
        #[async_trait]
        impl JdVopReadTransport for Offline {
            async fn read(&self, _: &JdVopReadRequest) -> ConnectorResult<Vec<u8>> {
                panic!("构造连接和能力描述不得发送请求")
            }
        }

        let runtime = JdVopRuntime::with_verified_transport(toml_config(), Arc::new(Offline)).unwrap();
        let mut connection = connection(&runtime);
        let target = SupplierReferenceTarget::from(&connection);
        let connector = runtime.tracking_connector(&connection).unwrap();
        let description = runtime.description(&target).unwrap();
        assert_eq!(description, ConnectorDescription::from_connector(&connector));
        assert!(description.address_candidates);
        assert!(connector.offers().is_some());
        assert!(connector.orders().is_none());
        assert_eq!(connector.connection_id().as_ref(), connection.base.id.as_str());
        assert_eq!(connector.binding, runtime.binding(&target).unwrap());

        connection.credential_reference = Some("stale".into());
        assert_eq!(runtime.tracking_connector(&connection).err().unwrap(), binding_error());
        let mut invalid = target;
        invalid.connector_type = None;
        assert_eq!(runtime.description(&invalid).unwrap_err(), classified(binding_error()));
    }

    #[test]
    fn configuration_binding_rejects_cross_connection_and_changed_accounts() {
        let runtime = runtime();
        let mut connection = connection(&runtime);
        connection.supplier_id = SupplierAccountId::new("other-supplier");
        assert_eq!(runtime.tracking_connector(&connection).err().unwrap().code, "JD_VOP_BINDING_INVALID");

        let mut other = toml_config();
        other.account_scope = "other-account".into();
        assert!(JdVopRuntime::new(other).unwrap().tracking_connector(&connection).is_err());
    }

    #[test]
    fn channel_changes_binding_but_credential_cipher_key_is_not_business_identity() {
        let runtime = runtime();
        let connection = connection(&runtime);
        let mut settings = toml_config();
        settings.source_type = 4;
        assert!(JdVopRuntime::new(settings).unwrap().tracking_connector(&connection).is_err());

        let mut cipher_rotated = toml_config();
        cipher_rotated.credential_encryption_key = "another-offline-independent-test-key-at-least-32".into();
        assert!(JdVopRuntime::new(cipher_rotated).unwrap().tracking_connector(&connection).is_ok());
    }
}
