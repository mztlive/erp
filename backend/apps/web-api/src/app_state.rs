use config::{Config, SafeConfig};
use erp_identity::SharedRbacService;
use erp_integration::ports::evidence::IntegrationEvidenceAuthority;
use erp_party::SensitiveDataCodec;
use erp_processes::adapters::supplier_api::{
    UnavailableSupplierApiGateway, UnavailableSupplierReferenceRegistry,
};
use erp_processes::adapters::supplier_fulfillment_gateway::UnavailableSupplierGateway;
use erp_processes::adapters::workflow::{workflow_audit, workflow_auth, workflow_object_facts, WorkflowAuth};
use erp_processes::approval_dispatch::{ProcessObjectRead, ProcessUpgradeSubject};
use erp_processes::integration_resolution::{
    evidence_adapter::MongoIntegrationEvidenceAuthority, IntegrationResolutionProcess,
};
use erp_processes::ApprovalActionRegistry;
use erp_read_models::integration_center::IntegrationCenterReadService;
use erp_supply::ports::supplier_api_gateway::SupplierApiGateway;
use erp_supply::ports::supplier_gateway::SupplierGateway;
use erp_supply::ports::supplier_reference_registry::SupplierReferenceRegistry;
use erp_supply::service::supplier_api::SupplierApiService;
use erp_supply::service::supplier_fulfillment::SupplierFulfillmentService;
use erp_support::{BulkJobService, FileAssetService, SourceRegistryService};
use erp_workflow::service::approval::execution::ApprovalRuntimeService;
use erp_workflow::ApprovalNotificationOutboxPort;
use mongodb::Database;
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;
use storage::S3Storage;
use tokio::sync::{watch, RwLock};
use tokio::task::JoinHandle;
use tracing::{error, info};

/// worker 轮询间隔。
const OUTBOX_POLL_INTERVAL: Duration = Duration::from_secs(1);

/// 外部连接器装配模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConnectorMode {
    /// 组合根已注入可用实现。
    Configured,
    /// 组合根注入失败关闭实现，任何外部写均不得伪造成功。
    FailClosed,
}

/// 外部连接器 readiness 视图。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ExternalConnectorReadiness {
    pub supplier_api: ConnectorMode,
    pub supplier_reference_registry: ConnectorMode,
    pub supplier_fulfillment: ConnectorMode,
}

impl ExternalConnectorReadiness {
    /// 全部强依赖连接器是否已配置。
    pub const fn is_ready(self) -> bool {
        matches!(self.supplier_api, ConnectorMode::Configured)
            && matches!(self.supplier_reference_registry, ConnectorMode::Configured)
            && matches!(self.supplier_fulfillment, ConnectorMode::Configured)
    }
}

/// 启动组合根注入的外部连接器集合。
#[derive(Clone)]
pub struct ExternalConnectorPorts {
    supplier_api: Arc<dyn SupplierApiGateway>,
    supplier_reference_registry: Arc<dyn SupplierReferenceRegistry>,
    supplier_fulfillment: Arc<dyn SupplierGateway>,
    readiness: ExternalConnectorReadiness,
}

impl ExternalConnectorPorts {
    /// 构造显式配置的连接器集合；生产实现与测试替身均通过此入口注入。
    pub fn configured(
        supplier_api: Arc<dyn SupplierApiGateway>,
        supplier_reference_registry: Arc<dyn SupplierReferenceRegistry>,
        supplier_fulfillment: Arc<dyn SupplierGateway>,
    ) -> Self {
        Self {
            supplier_api,
            supplier_reference_registry,
            supplier_fulfillment,
            readiness: ExternalConnectorReadiness {
                supplier_api: ConnectorMode::Configured,
                supplier_reference_registry: ConnectorMode::Configured,
                supplier_fulfillment: ConnectorMode::Configured,
            },
        }
    }

    /// 构造失败关闭集合；仅用于尚未接入真实连接器的部署。
    pub fn fail_closed() -> Self {
        Self {
            supplier_api: Arc::new(UnavailableSupplierApiGateway),
            supplier_reference_registry: Arc::new(UnavailableSupplierReferenceRegistry),
            supplier_fulfillment: Arc::new(UnavailableSupplierGateway),
            readiness: ExternalConnectorReadiness {
                supplier_api: ConnectorMode::FailClosed,
                supplier_reference_registry: ConnectorMode::FailClosed,
                supplier_fulfillment: ConnectorMode::FailClosed,
            },
        }
    }
}

/// 审批通知 outbox worker 句柄。停止时不再领取新租约。
pub struct ApprovalOutboxWorker {
    /// 置位后 worker 停止领取新租约。
    stop_tx: watch::Sender<bool>,
    /// 后台轮询任务。
    join: JoinHandle<()>,
}

/// 销售选品服务装配句柄（域层 Service 就绪前的组合根占位）。
///
/// TODO(erp-sales-domain): `erp-sales` 选品 Service 落地后删除本句柄，
/// `selection_service()` 直接返回域层 Service。
#[derive(Clone)]
pub struct SelectionServiceHandle {
    /// 业务数据库。
    db: Database,
    /// 商品池供给查询端口（组合层装配的同一实现）。
    catalog_supply_query: Arc<dyn erp_catalog::ports::supply::CatalogSupplyQueryPort>,
    /// 受管对象存储客户端。
    storage: Arc<S3Storage>,
    /// 敏感数据编解码器（启动密钥派生，只驻留内存）。
    sensitive_data: Arc<SensitiveDataCodec>,
}

impl SelectionServiceHandle {
    /// 返回业务数据库。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回数据库实例的克隆。
    ///
    /// # 错误
    /// 无。
    pub fn db(&self) -> Database {
        self.db.clone()
    }

    /// 返回商品池供给查询端口。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回组合层装配的查询端口。
    ///
    /// # 错误
    /// 无。
    pub fn catalog_supply_query(&self) -> Arc<dyn erp_catalog::ports::supply::CatalogSupplyQueryPort> {
        Arc::clone(&self.catalog_supply_query)
    }

    /// 返回受管对象存储客户端。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回共享存储客户端引用。
    ///
    /// # 错误
    /// 无。
    pub fn storage(&self) -> &S3Storage {
        self.storage.as_ref()
    }

    /// 返回敏感数据编解码器。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回进程内单例编解码器。
    ///
    /// # 错误
    /// 无。
    pub fn sensitive_data(&self) -> Arc<SensitiveDataCodec> {
        Arc::clone(&self.sensitive_data)
    }
}

impl ApprovalOutboxWorker {
    /// 停止领取新租约并等待当前批次结束。
    ///
    /// 进程被强制终止时同样不再领取；未完成租约会到期后由其他实例接管。
    pub async fn stop(self) {
        if self.stop_tx.send(true).is_err() {
            info!("审批通知 outbox worker 已退出");
        }
        if let Err(error) = self.join.await {
            error!(error = %error, "等待审批通知 outbox worker 结束失败");
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    db: Database,
    config: SafeConfig,
    jwt_engine: Arc<RwLock<Option<crate::core::auth::JwtEngine>>>,
    rbac: SharedRbacService,
    storage: Arc<S3Storage>,
    sensitive_data: Arc<SensitiveDataCodec>,
    approval_runtime_service: Arc<ApprovalRuntimeService<WorkflowAuth>>,
    approval_outbox: Arc<ApprovalNotificationOutboxPort>,
    external_connectors: ExternalConnectorPorts,
    integration_evidence: Arc<dyn IntegrationEvidenceAuthority>,
    catalog_supply_query: Arc<dyn erp_catalog::ports::supply::CatalogSupplyQueryPort>,
}

impl AppState {
    /// 创建 AppState 实例。
    ///
    /// # 参数
    /// * `db` - 应用启动时建立的数据库连接
    /// * `config` - 配置数据
    /// * `storage` - 启动时已构建的 S3 存储客户端
    ///
    /// # 返回
    /// 返回创建的实例。
    pub fn new(db: Database, config: SafeConfig, storage: S3Storage) -> Self {
        Self::new_with_connectors(db, config, storage, ExternalConnectorPorts::fail_closed())
    }

    /// 使用组合根选定的外部连接器创建应用状态。
    pub fn new_with_connectors(
        db: Database,
        config: SafeConfig,
        storage: S3Storage,
        external_connectors: ExternalConnectorPorts,
    ) -> Self {
        let sensitive_data = Arc::new(SensitiveDataCodec::from_secret(
            config.snapshot().app.secret.as_bytes(),
        ));
        let rbac = erp_processes::adapters::identity::shared_rbac_service(db.clone());
        let approval_action_port = Arc::new(ApprovalActionRegistry::new(db.clone(), Arc::clone(&rbac)));
        let approval_runtime_service = Arc::new(ApprovalRuntimeService::with_ports(
            db.clone(),
            workflow_auth(db.clone(), Arc::clone(&rbac)),
            approval_action_port,
            Arc::new(ProcessObjectRead),
            Arc::new(ProcessUpgradeSubject::new(db.clone())),
            workflow_audit(db.clone()),
            workflow_object_facts(db.clone()),
        ));
        let approval_outbox = Arc::new(ApprovalNotificationOutboxPort::new(db.clone()));
        let integration_evidence: Arc<dyn IntegrationEvidenceAuthority> =
            Arc::new(MongoIntegrationEvidenceAuthority::new(db.clone()));
        let catalog_supply_query: Arc<dyn erp_catalog::ports::supply::CatalogSupplyQueryPort> =
            Arc::new(erp_processes::adapters::catalog_supply_query::MongoCatalogSupplyQuery::new(db.clone()));
        Self {
            db,
            config,
            jwt_engine: Arc::new(RwLock::new(None)),
            rbac,
            storage: Arc::new(storage),
            sensitive_data,
            approval_runtime_service,
            approval_outbox,
            external_connectors,
            integration_evidence,
            catalog_supply_query,
        }
    }

    /// 订阅配置变更通知。
    ///
    /// # 返回
    /// 返回配置变更的接收器。
    pub fn subscribe_config(&self) -> watch::Receiver<Config> {
        self.config.subscribe()
    }

    /// 获取当前配置快照。
    ///
    /// # 返回
    /// 返回当前不可变配置副本。
    pub fn config_snapshot(&self) -> Config {
        self.config.snapshot()
    }

    /// 返回数据库实例。
    ///
    /// # 返回
    /// 返回数据库实例的克隆。
    pub fn db(&self) -> Database {
        self.db.clone()
    }

    /// 返回共享 Casbin RBAC 服务。
    ///
    /// # 返回
    /// 返回共享 RBAC 服务。
    pub fn rbac(&self) -> SharedRbacService {
        Arc::clone(&self.rbac)
    }

    /// Support-domain file asset service with audit and document-registry adapters.
    pub fn file_asset_service(&self) -> FileAssetService {
        FileAssetService::new(
            self.db(),
            erp_processes::adapters::support_audit::MongoSupportAudit::shared(self.db()),
            erp_processes::adapters::support_documents::MongoBusinessDocument::shared(self.db()),
        )
    }

    /// Support-domain bulk job service with audit and document-registry adapters.
    pub fn bulk_job_service(&self) -> BulkJobService {
        BulkJobService::new(
            self.db(),
            erp_processes::adapters::support_audit::MongoSupportAudit::shared(self.db()),
            erp_processes::adapters::support_documents::MongoBusinessDocument::shared(self.db()),
        )
    }

    /// Support-domain source registry service with the audit adapter.
    pub fn source_registry_service(&self) -> SourceRegistryService {
        SourceRegistryService::new(
            self.db(),
            erp_processes::adapters::support_audit::MongoSupportAudit::shared(self.db()),
        )
    }

    /// Composition-root object-read port for approval binding.
    pub fn approval_object_read(&self) -> Arc<dyn erp_workflow::ApprovalObjectReadPort> {
        Arc::new(ProcessObjectRead)
    }

    /// 返回进程内注入的目标审批运行服务。
    ///
    /// 本波次只交付注入点；Handler 改走本访问器归 P3-HTTP owns，不得在此越权改 Handler。
    ///
    /// # 返回
    /// 返回启动时构造的真实 [`ApprovalRuntimeService`]；未 cut-over 类型必须失败关闭。
    pub fn approval_runtime_service(&self) -> Arc<ApprovalRuntimeService<WorkflowAuth>> {
        Arc::clone(&self.approval_runtime_service)
    }

    /// 返回进程内注入的通知 outbox 应用端口。
    ///
    /// # 返回
    /// 返回工作流领域 outbox 端口；HTTP 不得直连审批仓储。
    pub fn approval_outbox_port(&self) -> Arc<ApprovalNotificationOutboxPort> {
        Arc::clone(&self.approval_outbox)
    }

    /// 返回使用统一权威证据提供方的集成治理命令入口。
    pub fn integration_resolution(&self) -> IntegrationResolutionProcess {
        IntegrationResolutionProcess::new(self.db(), Arc::clone(&self.integration_evidence))
    }

    /// 返回使用同一权威证据提供方的集成详情读取入口。
    pub fn integration_center(&self) -> IntegrationCenterReadService {
        IntegrationCenterReadService::new(self.db(), Arc::clone(&self.integration_evidence))
    }

    /// 返回仅处理本域事实的供应商 API 服务。
    pub fn supplier_api_service(&self) -> SupplierApiService {
        SupplierApiService::new(self.db())
    }

    /// 注入原引用注册表和 RBAC，执行连接治理命令。
    pub fn supplier_api_governance_process(
        &self,
    ) -> erp_processes::supply_governance::SupplierApiGovernanceProcess {
        erp_processes::supply_governance::SupplierApiGovernanceProcess::new(self.db())
            .with_reference_registry(Arc::clone(&self.external_connectors.supplier_reference_registry))
            .with_rbac(self.rbac())
    }

    /// 读取按原权限与引用配置投影的供应商连接详情。
    pub fn supplier_api_read_service(
        &self,
    ) -> erp_read_models::supplier_center::supplier_api::SupplierApiReadService {
        erp_read_models::supplier_center::supplier_api::SupplierApiReadService::new(self.db())
            .with_reference_registry(Arc::clone(&self.external_connectors.supplier_reference_registry))
            .with_rbac(self.rbac())
    }

    /// 注入实际任务授权读取，构造不执行授权或查询。
    pub fn work_item_authorization(
        &self,
    ) -> erp_processes::adapters::workflow::work_item_authorization::WorkItemAuthorizationAdapter {
        erp_processes::adapters::workflow::work_item_authorization::WorkItemAuthorizationAdapter::new(
            self.db(),
            self.rbac(),
        )
    }

    /// 商品列表和销售资格共用同一查询提供方实现。
    pub fn catalog_center(&self) -> erp_read_models::catalog_center::CatalogCenterReadService {
        erp_read_models::catalog_center::CatalogCenterReadService::new(Arc::clone(&self.catalog_supply_query))
    }

    /// 返回保留意图提交与外部调用分离的供应商连接执行入口。
    pub fn supplier_connection_execution_process(
        &self,
    ) -> erp_processes::supplier_connection_execution::SupplierConnectionExecutionProcess {
        erp_processes::supplier_connection_execution::SupplierConnectionExecutionProcess::new(
            self.db(),
            Arc::clone(&self.external_connectors.supplier_api),
        )
    }

    /// 返回供应商履约的本域查询与持久化服务。
    pub fn supplier_fulfillment_service(&self) -> SupplierFulfillmentService {
        SupplierFulfillmentService::new(self.db())
    }

    /// 返回已注入原网关的供应商履约跨域执行入口。
    pub fn supplier_fulfillment_process(
        &self,
    ) -> erp_processes::supply_execution::SupplierFulfillmentProcess {
        erp_processes::supply_execution::SupplierFulfillmentProcess::new(
            self.db(),
            Arc::clone(&self.external_connectors.supplier_fulfillment),
        )
    }

    /// 返回连接器配置状态，供 readiness 暴露。
    pub const fn external_connector_readiness(&self) -> ExternalConnectorReadiness {
        self.external_connectors.readiness
    }

    /// 启动审批通知 outbox worker。
    ///
    /// 领取租约后在事务外调用失败关闭发送口；进程停止时不再领取新租约。
    ///
    /// # 返回
    /// 返回可用于显式停止的 worker 句柄。
    pub fn start_approval_outbox_worker(&self) -> ApprovalOutboxWorker {
        let (stop_tx, stop_rx) = watch::channel(false);
        let port = self.approval_outbox_port();
        let worker_id = format!("web-api-{}", id_generator::next_id());
        let join = tokio::spawn(run_approval_outbox_worker(port, worker_id, stop_rx));
        ApprovalOutboxWorker { stop_tx, join }
    }

    /// 返回启动时固定的 S3 存储客户端。
    ///
    /// # 返回
    /// 返回所有上传 handler 共享的单例客户端；S3 配置变更需重启后生效。
    pub fn storage(&self) -> &S3Storage {
        self.storage.as_ref()
    }

    /// 返回启动时固定的敏感数据编解码器。
    ///
    /// # 返回
    /// 返回敏感资料 Service 共享的进程内单例；启动密钥变化后必须先迁移既有密文。
    pub fn sensitive_data(&self) -> Arc<SensitiveDataCodec> {
        Arc::clone(&self.sensitive_data)
    }

    /// 返回销售选品服务装配句柄。
    ///
    /// 选品链接令牌只存哈希与经服务端密钥加密的密文，禁止明文落库；
    /// 加密密钥由启动配置派生、只驻留内存，不得与令牌密文同库保存。
    /// 公开限流器另经 `OnceLock` 常驻并由公开路由以 `Extension` 注入。
    /// TODO(erp-sales-domain): 域层 Service 就绪后改返域层 Service。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回持有数据库、供给查询端口、存储与敏感编解码器的句柄。
    ///
    /// # 错误
    /// 无。
    pub fn selection_service(&self) -> SelectionServiceHandle {
        SelectionServiceHandle {
            db: self.db.clone(),
            catalog_supply_query: Arc::clone(&self.catalog_supply_query),
            storage: Arc::clone(&self.storage),
            sensitive_data: Arc::clone(&self.sensitive_data),
        }
    }

    /// Party identity service with audit and supplier-role adapters.
    pub fn party_service(&self) -> erp_party::PartyService {
        erp_processes::adapters::party_service(self.db())
    }

    /// Party contact service with composition adapters.
    pub fn party_contact_service(&self) -> erp_party::PartyContactService {
        erp_processes::adapters::party_contact_service(self.db(), self.sensitive_data())
    }

    /// Party address service with composition adapters.
    pub fn party_address_service(&self) -> erp_party::PartyAddressService {
        erp_processes::adapters::party_address_service(self.db(), self.sensitive_data())
    }

    /// Party bank-account service with composition adapters.
    pub fn party_bank_account_service(&self) -> erp_party::PartyBankAccountService {
        erp_processes::adapters::party_bank_account_service(self.db(), self.sensitive_data())
    }

    /// Party tax-profile service with composition adapters.
    pub fn party_tax_profile_service(&self) -> erp_party::PartyTaxProfileService {
        erp_processes::adapters::party_tax_profile_service(self.db())
    }

    /// Customer account service with composition adapters.
    pub fn customer_service(&self) -> erp_customer::CustomerService {
        erp_processes::adapters::customer_service(self.db())
    }

    /// Customer assignment service with composition adapters.
    pub fn customer_assignment_service(&self) -> erp_customer::CustomerAssignmentService {
        erp_processes::adapters::customer_assignment_service(self.db())
    }

    /// Customer profile root process.
    pub fn customer_profile_service(&self) -> erp_processes::CustomerProfileService {
        erp_processes::CustomerProfileService::new(self.db(), self.sensitive_data()).with_rbac(self.rbac())
    }

    /// Supplier list/detail service with party facts and reveal tokens.
    pub fn supplier_service(&self) -> erp_supplier::SupplierService {
        erp_processes::adapters::supplier_service_with_sensitive(self.db(), self.sensitive_data())
    }

    /// Supplier profile root process.
    pub fn supplier_profile_service(&self) -> erp_processes::SupplierProfileService {
        erp_processes::SupplierProfileService::new(self.db(), self.sensitive_data())
    }

    /// 绑定供应商后台导入所需数据库、对象存储和密文编解码器。
    ///
    /// 返回导入流程；不执行 I/O，不产生错误。
    pub fn supplier_import_process(&self) -> erp_processes::SupplierImportProcess {
        erp_processes::SupplierImportProcess::new(self.db(), self.storage().clone(), self.sensitive_data())
    }

    /// Catalog domain service with audit and file-asset adapters.
    pub fn catalog_service(&self) -> erp_catalog::CatalogService {
        erp_processes::adapters::catalog_service(self.db())
    }

    /// 产品报价表异步导入流程。
    ///
    /// # 返回
    /// 返回绑定当前数据库、对象存储与内容指纹密钥的导入流程。
    ///
    /// # 错误
    /// 无。
    pub fn product_import_process(&self) -> erp_processes::ProductImportProcess {
        erp_processes::ProductImportProcess::new(
            self.db(),
            self.storage().clone(),
            self.config_snapshot().app.secret.as_bytes(),
        )
    }

    /// Warehouse domain service with identity, audit and fingerprint adapters.
    pub fn warehouse_service(&self) -> erp_warehouse::WarehouseService {
        erp_processes::adapters::warehouse_service(self.db(), self.rbac())
    }

    /// Contract domain service with customer, identity, attachment and audit adapters.
    pub fn contract_service(&self) -> erp_contract::ContractService {
        erp_processes::adapters::contract_service(self.db())
    }

    /// Import-domain query service with bulk-job identity adapter.
    pub fn legacy_import_service(&self) -> erp_import::LegacyImportService {
        erp_processes::adapters::legacy_import_service(self.db())
    }

    /// Import-apply process that owns cross-domain apply/confirmation transactions.
    pub fn import_apply_service(&self) -> erp_processes::ImportApplyService {
        erp_processes::adapters::import_apply_service(self.db())
    }

    /// Inventory query service with authorization and foreign-fact adapters.
    pub fn inventory_service(&self) -> erp_inventory::InventoryService {
        erp_processes::adapters::inventory_service(self.db(), self.rbac())
    }

    /// Inventory-adjustment process that owns approval submit/cancel/post.
    pub fn inventory_adjustment_service(&self) -> erp_processes::InventoryAdjustmentService {
        erp_processes::adapters::inventory_adjustment_service(self.db(), self.rbac())
            .with_object_read(self.approval_object_read())
    }

    /// 使 JWT 引擎缓存失效。
    ///
    /// # 返回
    /// 无返回值。
    pub async fn invalidate_jwt_engine(&self) {
        let mut engine_guard = self.jwt_engine.write().await;
        *engine_guard = None;
    }

    /// 获取 JWT 引擎实例。
    ///
    /// # 返回
    /// 返回执行结果，`Ok` 表示成功，`Err` 表示失败。
    ///
    /// # 错误
    /// 当验证失败或底层操作失败时返回错误。
    pub async fn jwt_engine(
        &self,
    ) -> std::result::Result<crate::core::auth::JwtEngine, crate::core::auth::JwtError> {
        let mut engine_guard = self.jwt_engine.write().await;

        if let Some(engine) = engine_guard.as_ref() {
            return Ok(engine.clone());
        }

        let config = self.config_snapshot();
        let engine = crate::core::auth::JwtEngine::new(config.app.secret)?;
        *engine_guard = Some(engine.clone());
        Ok(engine)
    }
}

/// 运行 outbox worker，直到收到停止信号。
///
/// # 参数
/// * `port` - 工作流领域 outbox 端口
/// * `worker_id` - 本进程租约持有者
/// * `stop_rx` - 停止信号
async fn run_approval_outbox_worker(
    port: Arc<ApprovalNotificationOutboxPort>,
    worker_id: String,
    mut stop_rx: watch::Receiver<bool>,
) {
    info!(worker_id = %worker_id, "审批通知 outbox worker 已启动");
    loop {
        if *stop_rx.borrow() {
            info!(worker_id = %worker_id, "审批通知 outbox worker 停止领取新租约");
            return;
        }
        if let Err(error) = port.process_tick(&worker_id).await {
            error!(worker_id = %worker_id, error = %error, "审批通知 outbox 批次处理失败");
        }
        tokio::select! {
            changed = stop_rx.changed() => {
                if changed.is_err() || *stop_rx.borrow() {
                    info!(worker_id = %worker_id, "审批通知 outbox worker 已停止");
                    return;
                }
            }
            () = tokio::time::sleep(OUTBOX_POLL_INTERVAL) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ConnectorMode, ExternalConnectorReadiness};

    #[test]
    fn readiness_requires_every_external_port() {
        let configured = ExternalConnectorReadiness {
            supplier_api: ConnectorMode::Configured,
            supplier_reference_registry: ConnectorMode::Configured,
            supplier_fulfillment: ConnectorMode::Configured,
        };
        assert!(configured.is_ready());

        assert!(!ExternalConnectorReadiness {
            supplier_api: ConnectorMode::FailClosed,
            ..configured
        }
        .is_ready());
        assert!(!ExternalConnectorReadiness {
            supplier_reference_registry: ConnectorMode::FailClosed,
            ..configured
        }
        .is_ready());
    }
}
