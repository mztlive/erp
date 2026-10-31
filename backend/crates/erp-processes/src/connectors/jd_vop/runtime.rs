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
    SupplierApiCapabilityCode, SupplierApiConnection, SupplierApiConnectionStatus, SupplierConnectorType,
    SupplierHealthCheckType,
};
use erp_supply::ports::catalog_sync::CatalogSyncPolicy;
use erp_supply::ports::connector::SupplierConnector;
use erp_supply::ports::connector::common::{ConnectorError, ConnectorResult, Scan};
use erp_supply::ports::connector::offer::{IncrementalSupport, OfferSource};
use erp_supply::ports::supplier_api_gateway::{
    CatalogPage, CatalogRead, CatalogScopeRead, ClassifiedError, SupplierApiGateway,
};
use erp_supply::ports::supplier_reference_registry::{
    ResolvedSupplierReference, SupplierDeploymentConfig, SupplierReferenceKind, SupplierReferenceRegistry,
    SupplierReferenceTarget,
};
use hmac::{Hmac, KeyInit, Mac};
use serde_json::{json, to_vec};
use sha2::Sha256;

use super::super::registry::SupplierFulfillmentFactory;
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
    /// 核对页面授权使用当前部署账号绑定的原京东连接，不执行供应商调用。
    /// # 参数
    /// `connection` 为按当前操作人范围加载的连接。
    /// # 返回
    /// 固定供应商协议及部署引用一致时成功。
    /// # 错误
    /// 连接删除、协议或技术绑定不符时拒绝，不转移账号身份。
    pub fn verify_authorization_binding(&self, connection: &SupplierApiConnection) -> ConnectorResult<()> {
        self.validate_binding(connection)
    }
    /// 登记有效技术配置，不猜测认证协议、不发送请求、不启用京东写操作。
    /// # 参数
    /// `settings` 为 SafeConfig 启动快照。
    /// # 返回
    /// 未注入已验收认证实现的运行时，能力清单为空。
    /// # 错误
    /// 配置无效或尚未开启技术登记时返回分类错误。
    pub fn new(settings: JdVopConfig) -> ConnectorResult<Self> {
        settings.validate().map_err(|_| mapping("京东 VOP 技术配置无效"))?;
        if !settings.enabled {
            return Err(error(
                SupplierFailureClass::CapabilityGap,
                "JD_VOP_DISABLED",
                "京东 VOP 技术配置未登记",
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

    /// 注入已完成 G0 固定向量及账号授权验收的只读认证实现。
    /// # 参数
    /// `settings` 为技术快照，`transport` 为服务端受信认证实现；禁止从用户载荷构造。
    /// # 返回
    /// 可独立读取目录的运行时，仍不提供下单、确认、消息删除或资金操作。
    /// # 错误
    /// 配置无效或关闭时拒绝登记；本构造不执行网络验收。
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

    /// 注入已完成 G0 的账号执行资料及一次发送传输，不由部署布尔开关自行启用。
    /// # 参数
    /// 技术快照、已验读写认证实现及财务核实的账号发票资料。
    /// # 返回
    /// 已注入传输的运行时；缺少确认资金规则时仅保留原单查询、取消及调查。
    /// # 错误
    /// 缺失验收引用、发票资料或技术参数时拒绝，构造过程不发网络请求。
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

    /// 装配独立消息通道，不把消息确认传输暴露为订单写能力。
    /// # 参数
    /// 受控写传输、已核实类型、调用渠道及轮询间隔。
    /// # 返回
    /// 消息通道已登记。
    /// # 错误
    /// 缺少读取传输时拒绝。
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

    /// 按当前连接资格取得允许新增业务调用的协议客户端。
    /// # 参数
    /// `connection` 为服务端加载的连接记录。
    /// # 返回
    /// 连接身份及技术引用已核对的客户端。
    /// # 错误
    /// 自动下单关闭、健康检查过期、配置绑定失效或确认资金规则未核实时拒绝。
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

    /// 取得既有订单调查客户端；自动下单关闭不改变原账号身份。
    /// # 参数
    /// `connection` 为服务端加载的原连接。
    /// # 返回
    /// 与部署账号及业务连接绑定的客户端。
    /// # 错误
    /// 已删除、协议不匹配、技术引用缺失或陈旧时拒绝。
    pub fn tracking_connector(&self, connection: &SupplierApiConnection) -> ConnectorResult<JdVopConnector> {
        self.validate_binding(connection)?;
        let target = SupplierReferenceTarget::from(connection);
        Ok(JdVopConnector {
            connection_id: target.connection_id.clone(),
            account_scope: self.settings.account_scope.clone(),
            binding: self.binding(&target)?,
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

impl SupplierFulfillmentFactory for JdVopRuntime {
    fn notification_connector(
        &self,
        connection: &SupplierApiConnection,
    ) -> Result<Arc<dyn SupplierConnector>, ClassifiedError> {
        self.tracking_connector(connection)
            .map(|connector| Arc::new(connector) as Arc<dyn SupplierConnector>)
            .map_err(classified)
    }

    fn creation_connector(
        &self,
        connection: &SupplierApiConnection,
    ) -> Result<Arc<dyn SupplierConnector>, ClassifiedError> {
        self.validate_funds_binding(connection).map_err(classified)?;
        self.connector(connection)
            .map(|connector| Arc::new(connector) as Arc<dyn SupplierConnector>)
            .map_err(classified)
    }

    fn fulfillment_connector(
        &self,
        connection: &SupplierApiConnection,
    ) -> Result<Arc<dyn SupplierConnector>, ClassifiedError> {
        self.validate_funds_binding(connection).map_err(classified)?;
        self.tracking_connector(connection)
            .map(|connector| Arc::new(connector) as Arc<dyn SupplierConnector>)
            .map_err(classified)
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

impl SupplierApiGateway for JdVopRuntime {
    fn catalog_scope<'a>(&'a self, connection: &'a SupplierApiConnection) -> CatalogScopeRead<'a> {
        Box::pin(async move {
            self.tracking_connector(connection).map_err(classified)?.scope().await.map_err(classified)
        })
    }

    fn catalog_connector_types(&self) -> Vec<SupplierConnectorType> {
        vec![SupplierConnectorType::JdVop]
    }

    fn catalog_policy(
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

    fn catalog_page<'a>(&'a self, connection: &'a SupplierApiConnection, scan: &'a Scan) -> CatalogRead<'a> {
        Box::pin(async move {
            let page = self
                .tracking_connector(connection)
                .map_err(classified)?
                .scan_scoped(scan)
                .await
                .map_err(classified)?;
            Ok(CatalogPage {
                items: page.page.items,
                next: page.page.next,
                skipped: page.skipped,
                membership: page.membership,
            })
        })
    }

    fn health_check<'a>(
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
    fn capabilities(
        &self,
        target: &SupplierReferenceTarget,
    ) -> Result<Vec<SupplierApiCapabilityCode>, ClassifiedError> {
        self.validate_target(target).map_err(classified)?;
        let connector = JdVopConnector {
            connection_id: target.connection_id.clone(),
            account_scope: self.settings.account_scope.clone(),
            binding: self.binding(target).map_err(classified)?,
            cursor_secret: self.settings.app_secret.clone(),
            reader: self.reader.clone(),
            mapper: JdVopOfferMapper { image_base_url: self.settings.image_base_url.clone() },
            writer: self.writer.clone(),
            profile: self.profile.clone(),
            evidence_codec: self.evidence_codec.clone(),
            message_runtime: self.message_runtime.clone(),
        };
        Ok(connector.capabilities())
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
                "京东技术配置由服务端部署绑定，请联系管理员",
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
    use erp_core::ids::{SupplierAccountId, SupplierApiConnectionId};
    use erp_supply::entity::supplier_api::SupplierApiConnectionData;

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
