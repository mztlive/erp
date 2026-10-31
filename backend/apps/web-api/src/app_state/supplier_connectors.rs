//! 使用同一启动快照和应用数据库装配协议，京东令牌存在不构成写操作授权。
use std::sync::Arc;
use std::time::Duration;

use config::{Config, JdVopConfig};
use erp_party::SensitiveDataCodec;
use erp_processes::connectors::dangaoshushu::DangaoshushuRuntime;
use erp_processes::connectors::jd_vop::JdVopRuntime;
use erp_processes::connectors::jd_vop::credential::MongoJdVopTokenProvider;
use erp_processes::connectors::jd_vop::http_transport::JdVopHttpTransport;
use erp_processes::connectors::jd_vop::token::JdVopTokenProvider;
use erp_processes::connectors::registry::SupplierConnectorRegistry;
use erp_processes::supplier_catalog_callbacks::SupplierCatalogCallbackProcess;
use erp_processes::{Error as ProcessError, Result as ProcessResult, supplier_callbacks};
use erp_supply::entity::failure::SupplierFailureClass;
use erp_supply::entity::supplier_api::SupplierConnectorType;
use erp_supply::ports::connector::callback::{CallbackReply, CallbackRequest};
use erp_supply::ports::connector::common::{ConnectorError, ConnectorResult};
use erp_supply::ports::supplier_api_gateway::ClassifiedError;
use erp_supply::ports::supplier_reference_registry::SupplierReferenceRegistry;
use mongodb::Database;

use super::{AppState, ConnectorMode, ExternalConnectorPorts};

/// 同一部署账号的后台凭据维护，复用业务 HTTP 所持的同一提供器。
pub(crate) struct JdVopCredentialMaintenance {
    tokens: Arc<dyn JdVopTokenProvider>,
    credential_id: String,
    timeout: Duration,
}

impl JdVopCredentialMaintenance {
    fn new(settings: &JdVopConfig, tokens: Arc<dyn JdVopTokenProvider>) -> Self {
        // HTTP 自身仍受配置的短超时控制；此边界留给 Mongo 领取、发送登记及提交恢复。
        Self { tokens, credential_id: settings.credential_id.clone(), timeout: Duration::from_secs(300) }
    }

    /// 检查本账号的有效凭据，续期时点和唯一发送由提供器负责。
    /// # 参数
    /// 无；稳定身份、提供器及截止预算来自同一启动快照。
    /// # 返回
    /// 已取得可用令牌时成功，不把令牌交付给调度日志。
    /// # 错误
    /// 读取、续期或完整编排超时时失败；已经发送的未知结果不得自动重放。
    pub(crate) async fn run_once(&self) -> ConnectorResult<()> {
        tokio::time::timeout(self.timeout, self.tokens.access_token(&self.credential_id)).await.map_err(
            |_| {
                let mut failure = unavailable(
                    "JD_VOP_CREDENTIAL_MAINTENANCE_TIMEOUT",
                    "京东授权维护结果待核实，请核对原续期状态",
                );
                failure.class = SupplierFailureClass::ResultUnknown;
                failure
            },
        )??;
        Ok(())
    }

    /// 等待已开始的续期保存完成，不在停机阶段发起新的续期。
    /// # 参数
    /// 无；与当前运行时共享同一提供器的在途任务。
    /// # 返回
    /// 已开始的原续期已完成持久化或有界失败处理。
    /// # 错误
    /// 提供器无法收敛原续期时返回脱敏错误，不重发请求。
    pub(crate) async fn settle_refresh(&self) -> ConnectorResult<()> {
        self.tokens.settle_refresh().await
    }
}

impl AppState {
    /// 按注册表接收供应商回调并可靠保存证据。
    /// # 参数
    /// 协议、连接和原始 HTTP 请求。
    /// # 返回
    /// 已持久化后的协议应答。
    /// # 错误
    /// 未装配、验签、绑定或持久化失败时拒绝。
    pub async fn receive_supplier_callback(
        &self,
        kind: SupplierConnectorType,
        connection: &str,
        request: &CallbackRequest<'_>,
    ) -> ProcessResult<CallbackReply> {
        let registry = self
            .external_connectors
            .order_connectors
            .as_ref()
            .ok_or_else(|| ProcessError::NotFound("供应商回调未配置".into()))?;
        supplier_callbacks::receive(&self.db(), registry, &self.sensitive_data(), kind, connection, request)
            .await
    }

    /// 返回商品回调的补查及来源应用消费者。
    /// # 参数
    /// 无，复用当前部署的协议注册表与权限服务。
    /// # 返回
    /// 配置协议后返回流程，否则返回 None。
    /// # 错误
    /// 不返回错误。
    pub fn supplier_catalog_callback_process(&self) -> Option<SupplierCatalogCallbackProcess> {
        self.external_connectors
            .order_connectors
            .as_ref()
            .map(|runtime| SupplierCatalogCallbackProcess::new(self.db(), Arc::clone(runtime), self.rbac()))
    }

    /// 返回独立于新增下单开关的账号凭据维护任务。
    /// # 参数
    /// 无；仅使用技术启用时已装配的同一个凭据提供器。
    /// # 返回
    /// 已启用京东返回维护对象；停用技术配置时返回 None。
    /// # 错误
    /// 不执行 I/O，不返回错误。
    pub(crate) fn jd_vop_credential_maintenance(&self) -> Option<Arc<JdVopCredentialMaintenance>> {
        self.external_connectors.jd_vop_credentials.clone()
    }
}

impl ExternalConnectorPorts {
    /// 装配不需要数据库凭据的供应商，禁止为已启用京东登记空传输。
    /// # 参数
    /// `config` 是 SafeConfig 的同一启动快照。
    /// # 返回
    /// 返回原供应商或失败关闭端口；京东必须改用 from_database。
    /// # 错误
    /// 已启用京东而缺少 Mongo 凭据提供器，或协议配置无效时拒绝。
    pub fn from_config(config: &Config) -> ConnectorResult<Self> {
        Self::with_jd_tokens(config, None)
    }

    /// 在应用数据库就绪后装配京东认证、已核实执行资料及已有供应商协议。
    /// # 参数
    /// `config` 为启动快照，`db` 为应用当前数据库；不单独建立凭据数据库连接。
    /// # 返回
    /// 京东缺省只读；有受控消息或执行资料时装配对应能力。
    /// # 错误
    /// 凭据、加密密钥、调用渠道或 HTTP 构造无效时返回脱敏错误。
    pub fn from_database(config: &Config, db: Database) -> ConnectorResult<Self> {
        let tokens = config
            .jd_vop
            .as_ref()
            .filter(|settings| settings.enabled)
            .map(|settings| {
                MongoJdVopTokenProvider::new(db, settings)
                    .map(|provider| Arc::new(provider) as Arc<dyn JdVopTokenProvider>)
            })
            .transpose()?;
        Self::with_jd_tokens(config, tokens)
    }

    fn with_jd_tokens(config: &Config, tokens: Option<Arc<dyn JdVopTokenProvider>>) -> ConnectorResult<Self> {
        let mut ports = Self::fail_closed();
        let mut registry = SupplierConnectorRegistry::default();
        if let Some(settings) = config.dangaoshushu.as_ref().filter(|settings| settings.enabled) {
            let runtime = Arc::new(DangaoshushuRuntime::new(settings.clone())?);
            registry
                .register(SupplierConnectorType::Dangaoshushu, runtime.clone())
                .map_err(registry_error)?;
            ports.dangaoshushu = Some(runtime);
            ports.readiness.supplier_api = ConnectorMode::Configured;
            ports.readiness.supplier_reference_registry = ConnectorMode::Configured;
        }
        if let Some(settings) = config.jd_vop.as_ref().filter(|settings| settings.enabled) {
            let tokens = tokens.ok_or_else(|| {
                unavailable(
                    "JD_VOP_MONGO_PROVIDER_REQUIRED",
                    "京东只读连接须在应用数据库就绪后装配凭据提供器",
                )
            })?;
            ports.jd_vop_credentials =
                Some(Arc::new(JdVopCredentialMaintenance::new(settings, tokens.clone())));
            let runtime = Arc::new(jd_runtime(config, settings, tokens)?);
            registry.register(SupplierConnectorType::JdVop, runtime).map_err(registry_error)?;
            // Configured 仅表达已注入读取实现；账号授权仍由真实只读健康检查核验。
            ports.readiness.supplier_api = ConnectorMode::Configured;
            ports.readiness.supplier_reference_registry = ConnectorMode::Configured;
        }
        let registry = Arc::new(registry);
        if registry.is_available() {
            ports.order_connectors = Some(registry.clone());
        }
        ports.supplier_api = registry.clone();
        ports.supplier_reference_registry = registry;
        Ok(ports)
    }
}

fn jd_runtime(
    config: &Config,
    settings: &JdVopConfig,
    tokens: Arc<dyn JdVopTokenProvider>,
) -> ConnectorResult<JdVopRuntime> {
    settings.validate().map_err(|_| unavailable("JD_VOP_CONFIG_INVALID", "京东启动配置不完整"))?;
    let transport = Arc::new(JdVopHttpTransport::new(settings.clone(), tokens)?);
    JdVopRuntime::from_configured_transport(
        settings.clone(),
        transport.clone(),
        transport,
        Arc::new(SensitiveDataCodec::from_secret(config.app.secret.as_bytes())),
    )
}

fn unavailable(code: &str, summary: &str) -> ConnectorError {
    ConnectorError {
        class: SupplierFailureClass::CapabilityGap,
        code: code.into(),
        summary: summary.into(),
        retry_after: None,
    }
}

fn registry_error(error: ClassifiedError) -> ConnectorError {
    ConnectorError { class: error.class, code: error.code, summary: error.summary, retry_after: None }
}

#[cfg(test)]
mod tests;
