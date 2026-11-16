//! 共享资源与业务能力工厂；不持有 HTTP 状态，不启动后台循环。
use std::sync::Arc;

use config::{Config, SafeConfig};
use erp_connector_dangaoshushu::DangaoshushuRuntime;
use erp_contract::ContractTemplateService;
use erp_contract_recognition::{AliyunContractOcr, OpenAiContractExtractor};
use erp_customer::CustomerAccess;
use erp_data_import::contract_import::{ContractImportProcess, ContractImportServices, RecognitionProviders};
use erp_data_import::product_import::ProductImportServices;
use erp_data_import::sales_inquiry_import::SalesInquiryImportProcess;
use erp_identity::{AuthorizationService, SharedRbacService};
use erp_integration::IntegrationAccess;
use erp_integration::ports::evidence::IntegrationEvidenceAuthority;
use erp_notification::NotificationService;
use erp_order_to_cash::sales_inquiry::SalesInquiryProcess;
use erp_party::SensitiveDataCodec;
use erp_read_models::approval_subject::ApprovalUpgradeReadModel;
use erp_read_models::customer_center::profile::CustomerProfileReadService;
use erp_read_models::integration_center::IntegrationCenterReadService;
use erp_read_models::supplier_center::SupplierFulfillmentListReadService;
use erp_supplier_catalog::integration_resolution::IntegrationResolutionProcess;
use erp_supplier_catalog::integration_resolution::evidence_adapter::MongoIntegrationEvidenceAuthority;
use erp_supplier_catalog::wiring::CatalogWorkItems;
use erp_supplier_orders::supplier_deposits::{SupplierDepositFundsAdapter, SupplierFundsInvestigationWorker};
use erp_supplier_orders::supplier_messages::SupplierMessageWorker;
use erp_supply::connector_registry::SupplierConnectorRegistry;
use erp_supply::ports::supplier_api_gateway::SupplierApiGateway;
use erp_supply::ports::supplier_gateway::SupplierGateway;
use erp_supply::ports::supplier_reference_registry::{
    SupplierReferenceRegistry, UnavailableSupplierReferenceRegistry,
};
use erp_supply::service::supplier_api::SupplierApiService;
use erp_supply::service::supplier_fulfillment::SupplierFulfillmentService;
use erp_support::{BulkJobService, FileAssetService, SourceRegistryService};
use erp_workflow::service::approval::execution::ApprovalRuntimeService;
use erp_workflow::{ApprovalNotificationOutboxPort, WorkItemService};
use mongodb::Database;
use serde::Serialize;
use storage::S3Storage;
use tokio::sync::watch;

use super::notification::ApprovalNotifications;
use crate::wiring::approval_dispatch::ApprovalActionRegistry;
use crate::wiring::catalog_supply_query::MongoCatalogSupplyQuery;
use crate::wiring::identity::shared_rbac_service;
use crate::wiring::supplier_api::UnavailableSupplierApiGateway;
use crate::wiring::supplier_fulfillment_gateway::UnavailableSupplierGateway;
use crate::wiring::workflow::{WorkflowAuth, work_item_service, workflow_audit, workflow_auth};
use crate::wiring::{contract_template_service, supplier_access};

mod demo_data;
mod supplier_connectors;
mod supplier_media;
pub(crate) use supplier_connectors::JdVopCredentialMaintenance;

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
    dangaoshushu: Option<Arc<DangaoshushuRuntime>>,
    order_connectors: Option<Arc<SupplierConnectorRegistry>>,
    jd_vop_credentials: Option<Arc<JdVopCredentialMaintenance>>,
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
            dangaoshushu: None,
            order_connectors: None,
            jd_vop_credentials: None,
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
            dangaoshushu: None,
            order_connectors: None,
            jd_vop_credentials: None,
        }
    }
}

#[derive(Clone)]
pub struct Application {
    db: Database,
    config: SafeConfig,
    rbac: SharedRbacService,
    storage: Arc<S3Storage>,
    sensitive_data: Arc<SensitiveDataCodec>,
    approval_runtime_service: Arc<ApprovalRuntimeService<WorkflowAuth>>,
    approval_outbox: Arc<ApprovalNotificationOutboxPort>,
    notifications: NotificationService,
    external_connectors: ExternalConnectorPorts,
    integration_evidence: Arc<dyn IntegrationEvidenceAuthority>,
    catalog_supply_query: Arc<dyn erp_catalog::ports::supply::CatalogSupplyQueryPort>,
}

impl Application {
    /// 返回统一权限入口，使用服务端固定登记。
    /// # 参数
    /// 无。
    /// # 返回
    /// 账号与功能资格服务；对象及查询由所属领域登记。
    /// # 错误
    /// 不执行 I/O。
    pub fn authorization(&self) -> AuthorizationService {
        super::authorization::authorization_service(self.db(), self.rbac())
    }

    /// 返回启动时装配的蛋糕叔叔连接，缺省配置保持关闭。
    /// # 参数
    /// 无。
    /// # 返回
    /// 已配置协议运行时，或 None。
    /// # 错误
    /// 无。
    pub fn dangaoshushu_runtime(&self) -> Option<Arc<DangaoshushuRuntime>> {
        self.external_connectors.dangaoshushu.clone()
    }
    /// 创建 Application 实例。
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
        let sensitive_data =
            Arc::new(SensitiveDataCodec::from_secret(config.snapshot().app.secret.as_bytes()));
        let rbac = crate::wiring::identity::shared_rbac_service(db.clone());
        let approval_action_port = Arc::new(ApprovalActionRegistry::new(db.clone(), Arc::clone(&rbac)));
        let approval_runtime_service = Arc::new(ApprovalRuntimeService::with_ports(
            db.clone(),
            rbac.clone(),
            workflow_auth(db.clone(), Arc::clone(&rbac)),
            approval_action_port,
            Arc::new(ApprovalUpgradeReadModel::new(db.clone())),
            workflow_audit(db.clone()),
        ));
        let notifications = NotificationService::new(db.clone(), Default::default());
        let approval_outbox = Arc::new(ApprovalNotificationOutboxPort::with_sender(
            db.clone(),
            Arc::new(ApprovalNotifications::new(notifications.clone())),
        ));
        let integration_evidence: Arc<dyn IntegrationEvidenceAuthority> =
            Arc::new(MongoIntegrationEvidenceAuthority::new(db.clone()));
        let catalog_supply_query: Arc<dyn erp_catalog::ports::supply::CatalogSupplyQueryPort> =
            Arc::new(crate::wiring::catalog_supply_query::MongoCatalogSupplyQuery::new(db.clone()));
        Self {
            db,
            config,
            rbac,
            storage: Arc::new(storage),
            sensitive_data,
            approval_runtime_service,
            approval_outbox,
            notifications,
            external_connectors,
            integration_evidence,
            catalog_supply_query,
        }
    }

    /// 获取统一通知入口；首期未配置站外渠道。
    /// # 参数
    /// 无。
    /// # 返回
    /// 使用应用共享数据库的通知服务。
    /// # 错误
    /// 不执行 I/O。
    pub fn notifications(&self) -> NotificationService {
        self.notifications.clone()
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

    /// 装配询价命令及当前授权依赖。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回共享业务库与当前权限服务的询价流程。
    /// # 错误
    /// 不执行 I/O，不返回错误。
    pub fn sales_inquiry_process(&self) -> SalesInquiryProcess {
        SalesInquiryProcess::new(
            self.db(),
            self.rbac(),
            crate::wiring::sales::sales_services(self.db(), self.rbac()),
        )
    }

    /// Support-domain file asset service with audit and document-registry adapters.
    pub fn file_asset_service(&self) -> FileAssetService {
        FileAssetService::new(
            self.db(),
            crate::wiring::support_audit::MongoSupportAudit::shared(self.db()),
            crate::wiring::support_documents::MongoBusinessDocument::shared(self.db()),
        )
    }

    /// 装配带统一后台任务授权、审计和业务单据事实的批量任务服务。
    /// # 参数
    /// 无。
    /// # 返回
    /// 使用应用共享身份设施的任务服务。
    /// # 错误
    /// 不执行 I/O；固定授权目录非法时启动失败。
    pub fn bulk_job_service(&self) -> BulkJobService {
        BulkJobService::new(
            self.db(),
            crate::wiring::support_audit::MongoSupportAudit::shared(self.db()),
            crate::wiring::support_documents::MongoBusinessDocument::shared(self.db()),
            self.rbac(),
        )
    }

    /// Support-domain source registry service with the audit adapter.
    pub fn source_registry_service(&self) -> SourceRegistryService {
        SourceRegistryService::new(
            self.db(),
            crate::wiring::support_audit::MongoSupportAudit::shared(self.db()),
        )
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

    /// 装配共享权威证据与本域统一授权的集成治理命令。
    /// # 参数
    /// 无；使用应用共享设施。
    /// # 返回
    /// 保留独立任务身份工厂的集成治理流程。
    /// # 错误
    /// 构造不执行 I/O。
    pub fn integration_resolution(&self) -> IntegrationResolutionProcess {
        let work_items = integration_work_items(self.db());
        IntegrationResolutionProcess::new(
            self.db(),
            Arc::clone(&self.integration_evidence),
            IntegrationAccess::new(self.db(), self.rbac()),
            Arc::new(integration_identity(self.db())),
            Arc::new(move || -> Arc<dyn CatalogWorkItems> { Arc::new(work_items()) }),
        )
    }

    /// 装配共享权威证据与本域统一授权的集成详情读取。
    /// # 参数
    /// 无；使用应用共享设施。
    /// # 返回
    /// 在原读取阶段执行授权与复验的集成读取服务。
    /// # 错误
    /// 构造不执行 I/O。
    pub fn integration_center(&self) -> IntegrationCenterReadService {
        IntegrationCenterReadService::new(
            self.db(),
            Arc::clone(&self.integration_evidence),
            IntegrationAccess::new(self.db(), self.rbac()),
        )
    }

    /// 返回仅处理本域事实的供应商 API 服务。
    pub fn supplier_api_service(&self) -> SupplierApiService {
        SupplierApiService::new(self.db())
    }

    /// 注入原引用注册表和 RBAC，执行连接治理命令。
    pub fn supplier_api_governance_process(
        &self,
    ) -> erp_supplier_catalog::supply_governance::SupplierApiGovernanceProcess {
        erp_supplier_catalog::supply_governance::SupplierApiGovernanceProcess::new(
            self.db(),
            self.rbac(),
            crate::wiring::catalog_services(self.db(), self.rbac()),
        )
        .with_reference_registry(Arc::clone(&self.external_connectors.supplier_reference_registry))
        .with_gateway(Arc::clone(&self.external_connectors.supplier_api))
    }

    /// 读取按原权限与引用配置投影的供应商连接详情。
    pub fn supplier_api_read_service(
        &self,
    ) -> erp_read_models::supplier_center::supplier_api::SupplierApiReadService {
        erp_read_models::supplier_center::supplier_api::SupplierApiReadService::new(self.db(), self.rbac())
            .with_reference_registry(Arc::clone(&self.external_connectors.supplier_reference_registry))
            .with_gateway(Arc::clone(&self.external_connectors.supplier_api))
            .with_supplier_access(supplier_access(self.db(), self.rbac()))
    }

    /// 注入实际任务授权读取，构造不执行授权或查询。
    pub fn work_item_authorization(
        &self,
    ) -> crate::wiring::workflow::work_item_authorization::WorkItemAuthorizationAdapter {
        crate::wiring::workflow::work_item_authorization::WorkItemAuthorizationAdapter::new(
            self.db(),
            self.rbac(),
        )
    }

    /// 商品列表和销售资格复用查询提供方与本域授权能力。
    /// # 参数
    /// 无；使用应用共享设施。
    /// # 返回
    /// 已装配商品统一授权的读取服务。
    /// # 错误
    /// 构造不执行 I/O。
    pub fn catalog_center(&self) -> erp_read_models::catalog_center::CatalogCenterReadService {
        erp_read_models::catalog_center::CatalogCenterReadService::new(
            self.db(),
            Arc::clone(&self.catalog_supply_query),
            crate::wiring::catalog_access(self.db(), self.rbac()),
            erp_read_models::catalog_center::MongoProductProcurementOwners::shared(self.db()),
        )
    }

    /// 返回保留意图提交与外部调用分离的供应商连接执行入口。
    pub fn supplier_connection_execution_process(
        &self,
    ) -> erp_supplier_catalog::supplier_connection_execution::SupplierConnectionExecutionProcess {
        erp_supplier_catalog::supplier_connection_execution::SupplierConnectionExecutionProcess::new(
            self.db(),
            Arc::clone(&self.external_connectors.supplier_api),
            crate::wiring::scoped_catalog_service(self.db(), self.rbac()),
        )
    }

    /// 返回正式采购的真实供应商 API 分步执行入口。
    /// # 参数
    /// 无；使用应用当前协议运行时及加密器。
    /// # 返回
    /// 已配置协议返回流程，未配置返回 None。
    /// # 错误
    /// 不返回错误。
    pub fn supplier_api_order_process(
        &self,
    ) -> Option<erp_supplier_orders::supplier_api_orders::SupplierApiOrderProcess> {
        self.external_connectors.order_connectors.as_ref().map(|runtime| {
            erp_supplier_orders::supplier_api_orders::SupplierApiOrderProcess::new(
                self.db(),
                Arc::clone(runtime),
                self.sensitive_data(),
                Arc::new(SupplierDepositFundsAdapter::new(self.db())),
            )
        })
    }

    /// 装配统一退款与结算处理器。
    /// # 参数
    /// 无。
    /// # 返回
    /// 已配置运行时返回处理器。
    /// # 错误
    /// 无。
    pub fn supplier_after_sales_process(
        &self,
    ) -> Option<erp_supplier_orders::supplier_after_sales::SupplierAfterSalesProcess> {
        self.external_connectors.order_connectors.as_ref().map(|runtime| {
            erp_supplier_orders::supplier_after_sales::SupplierAfterSalesProcess::new(
                self.db(),
                Arc::clone(runtime),
            )
        })
    }

    /// 返回供应商订单的真实协议补查与履约应用入口。
    /// # 参数
    /// 无。
    /// # 返回
    /// 已配置协议返回流程，未配置返回 None。
    /// # 错误
    /// 不返回错误。
    pub fn supplier_order_tracking_process(
        &self,
    ) -> Option<erp_supplier_orders::supplier_order_tracking::SupplierOrderTrackingProcess> {
        self.external_connectors.order_connectors.as_ref().map(|runtime| {
            erp_supplier_orders::supplier_order_tracking::SupplierOrderTrackingProcess::new(
                self.db(),
                Arc::clone(runtime),
                crate::wiring::fulfillment_identity_factory(self.db()),
            )
        })
    }

    /// 返回按账号独立领取的供应商资金调查入口。
    /// # 参数
    /// 无；账号、真实资金来源及处理权限来自服务端绑定。
    /// # 返回
    /// 已登记协议时返回共享调查器；缺失实际资金能力的连接不会发起请求。
    /// # 错误
    /// 不执行 I/O，不返回错误。
    pub fn supplier_funds_worker(&self) -> Option<Arc<SupplierFundsInvestigationWorker>> {
        self.external_connectors.order_connectors.as_ref().map(|registry| {
            Arc::new(SupplierFundsInvestigationWorker::new(
                self.db(),
                registry.clone(),
                self.supplier_payment_tasks(),
            ))
        })
    }

    /// 返回拉取消息调度入口；只按实际消息能力领取账号，不推导回调身份。
    /// # 参数
    /// 无。
    /// # 返回
    /// 已注册协议返回调度器；未配置时返回 None。
    /// # 错误
    /// 不执行 I/O，不返回错误。
    pub fn supplier_message_worker(&self) -> Option<SupplierMessageWorker> {
        self.external_connectors.order_connectors.as_ref().map(|registry| {
            SupplierMessageWorker::new(
                self.db(),
                registry.clone(),
                self.sensitive_data().as_ref().clone(),
                self.supplier_message_catalog(),
            )
        })
    }

    /// 返回供应商履约的本域查询与持久化服务。
    pub fn supplier_fulfillment_service(&self) -> SupplierFulfillmentService {
        crate::wiring::scoped_fulfillment_service(self.db(), self.rbac())
            .with_registry(Arc::clone(&self.external_connectors.supplier_reference_registry))
    }

    /// 装配原网关、统一授权与异常处理事实的供应商履约流程。
    /// # 参数
    /// 无；使用应用共享设施。
    /// # 返回
    /// 保留任务事实端口和身份工厂的跨域执行入口。
    /// # 错误
    /// 构造不执行 I/O。
    pub fn supplier_fulfillment_process(
        &self,
    ) -> erp_fulfillment_execution::supply_execution::SupplierFulfillmentProcess {
        erp_fulfillment_execution::supply_execution::SupplierFulfillmentProcess::new(
            self.db(),
            Arc::clone(&self.external_connectors.supplier_fulfillment),
            crate::wiring::fulfillment_order_access(self.db(), self.rbac()),
            crate::wiring::MongoFulfillmentExceptionHandlers::shared(self.db()),
            crate::wiring::supplier_fulfillment_identity_factory(self.db()),
        )
        .with_registry(Arc::clone(&self.external_connectors.supplier_reference_registry))
    }

    /// 返回连接器配置状态，供 readiness 暴露。
    pub const fn external_connector_readiness(&self) -> ExternalConnectorReadiness {
        self.external_connectors.readiness
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

    /// Party identity service with audit and supplier-role adapters.
    pub fn party_service(&self) -> erp_party::PartyService {
        crate::wiring::party_service(self.db())
    }

    /// Party contact service with composition adapters.
    pub fn party_contact_service(&self) -> erp_party::PartyContactService {
        crate::wiring::party_contact_service(self.db(), self.sensitive_data())
    }

    /// Party address service with composition adapters.
    pub fn party_address_service(&self) -> erp_party::PartyAddressService {
        crate::wiring::party_address_service(self.db(), self.sensitive_data())
    }

    /// Party bank-account service with composition adapters.
    pub fn party_bank_account_service(&self) -> erp_party::PartyBankAccountService {
        crate::wiring::party_bank_account_service(self.db(), self.sensitive_data())
    }

    /// Party tax-profile service with composition adapters.
    pub fn party_tax_profile_service(&self) -> erp_party::PartyTaxProfileService {
        crate::wiring::party_tax_profile_service(self.db())
    }

    /// Customer account service with composition adapters.
    pub fn customer_service(&self) -> erp_customer::CustomerService {
        crate::wiring::scoped_customer_service(self.db(), self.rbac())
    }

    /// Customer assignment service with composition adapters.
    pub fn customer_assignment_service(&self) -> erp_customer::CustomerAssignmentService {
        crate::wiring::scoped_customer_assignment_service(self.db(), self.rbac())
    }

    /// Customer profile root process.
    pub fn customer_profile_process(&self) -> erp_master_data::CustomerProfileProcess {
        erp_master_data::CustomerProfileProcess::new(
            self.db(),
            self.sensitive_data(),
            crate::wiring::customer_access(self.db(), self.rbac()),
        )
    }

    /// 装配客户、主体及账号名称的授权详情读模型。
    /// # 参数
    /// 无；复用应用共享数据库与敏感数据设施。
    /// # 返回
    /// 不缓存授权结果的客户资料读取入口。
    /// # 错误
    /// 构造不执行 I/O。
    pub fn customer_profile_read_service(&self) -> CustomerProfileReadService {
        let authorization = Arc::new(self.authorization());
        CustomerProfileReadService::new(
            self.db(),
            self.sensitive_data(),
            CustomerAccess::new(self.db(), authorization.clone()),
            authorization,
        )
    }

    /// 装配履约订单、账号姓名与当前 W26 处理人的授权列表。
    /// # 参数
    /// 无；复用已装配的供应链领域服务。
    /// # 返回
    /// 不缓存范围结果的履约列表读模型。
    /// # 错误
    /// 构造不执行 I/O。
    pub fn supplier_fulfillment_list_read_service(&self) -> SupplierFulfillmentListReadService {
        SupplierFulfillmentListReadService::new(self.db(), Arc::new(self.supplier_fulfillment_service()))
    }

    /// Supplier list/detail service with party facts and reveal tokens.
    pub fn supplier_service(&self) -> erp_supplier::SupplierService {
        crate::wiring::scoped_supplier_service_with_sensitive(self.db(), self.rbac(), self.sensitive_data())
    }

    /// Supplier profile root process.
    pub fn supplier_profile_process(&self) -> erp_master_data::SupplierProfileProcess {
        erp_master_data::SupplierProfileProcess::new(
            self.db(),
            self.sensitive_data(),
            self.rbac(),
            crate::wiring::supplier_access(self.db(), self.rbac()),
        )
    }

    /// Catalog domain service with audit and file-asset adapters.
    pub fn catalog_service(&self) -> erp_catalog::CatalogService {
        crate::wiring::scoped_catalog_service(self.db(), self.rbac())
    }

    /// Warehouse domain service with identity, audit and fingerprint adapters.
    pub fn warehouse_service(&self) -> erp_warehouse::WarehouseService {
        crate::wiring::warehouse_service(self.db(), self.rbac())
    }

    /// 演示主数据服务。是否允许写入由配置 `demo.master_data` 决定。
    ///
    /// # 返回
    /// 返回绑定当前数据库和授权快照的服务。
    pub fn demo_master_data_process(
        &self,
    ) -> erp_demo_data::demo_master_data::DemoMasterDataProcess<WorkflowAuth> {
        erp_demo_data::demo_master_data::DemoMasterDataProcess::new(
            self.db(),
            self.demo_runtime_ports(),
            self.rbac(),
            self.config_snapshot().demo.master_data_enabled(),
            self.storage().clone(),
            self.config_snapshot().app.secret.as_bytes(),
        )
    }

    /// 装配现有合同查询与终止服务。
    pub fn contract_service(&self) -> erp_contract::ContractService {
        crate::wiring::scoped_contract_service(self.db(), self.rbac())
    }

    /// 装配合同模板和领号服务。
    /// # 参数
    /// 无。
    /// # 返回
    /// 本域服务及公司、审计 Port。
    /// # 错误
    /// 无。
    pub fn contract_template_service(&self) -> ContractTemplateService {
        contract_template_service(self.db())
    }

    /// Import-domain query service with bulk-job identity adapter.
    pub fn legacy_import_service(&self) -> erp_import::LegacyImportService {
        crate::wiring::legacy_import_service(self.db())
    }

    /// Inventory query service with authorization and foreign-fact adapters.
    pub fn inventory_service(&self) -> erp_inventory::InventoryService {
        crate::wiring::inventory_service(self.db(), self.rbac())
    }

    /// Inventory-adjustment process that owns approval submit/cancel/post.
    pub fn inventory_adjustment_process(&self) -> erp_fulfillment_execution::InventoryAdjustmentProcess {
        crate::wiring::inventory_adjustment_process(self.db(), self.rbac())
    }
}

fn integration_identity(db: Database) -> impl Fn() -> SharedRbacService + Send + Sync {
    move || shared_rbac_service(db.clone())
}

fn integration_work_items(db: Database) -> impl Fn() -> WorkItemService<WorkflowAuth> + Send + Sync {
    let identity = integration_identity(db.clone());
    move || work_item_service(db.clone(), identity())
}

#[cfg(test)]
mod tests {
    use super::{Config, ConnectorMode, ExternalConnectorPorts, ExternalConnectorReadiness};

    #[tokio::test]
    async fn integration_commands_and_replays_have_independent_policy_state() {
        use std::sync::Arc;

        use super::{integration_identity, integration_work_items, shared_rbac_service, work_item_service};

        // Construct providers only; no database command is sent.
        let client = mongodb::Client::with_uri_str("mongodb://localhost:27017").await.unwrap();
        let db = client.database("integration-identity-factory-unit-test");
        let caller = shared_rbac_service(db.clone());
        let identities = integration_identity(db.clone());
        let first_identity = identities();
        let second_identity = identities();
        assert!(!Arc::ptr_eq(&caller, &first_identity));
        assert!(!Arc::ptr_eq(&caller, &second_identity));
        assert!(!Arc::ptr_eq(&first_identity, &second_identity));

        let caller_tasks = work_item_service(db.clone(), caller);
        let first_direct = work_item_service(db.clone(), first_identity);
        let tasks = integration_work_items(db);
        let first_command = tasks();
        let replay = tasks();
        assert!(!first_command.auth.shares_policy_state(&caller_tasks.auth));
        assert!(!replay.auth.shares_policy_state(&caller_tasks.auth));
        assert!(!first_command.auth.shares_policy_state(&replay.auth));
        assert!(!first_command.auth.shares_policy_state(&first_direct.auth));
    }

    pub(super) fn connector_config(supplier: &str) -> Config {
        Config::from_toml_str(&format!(
            r#"
            [xxl_job]
admin_url = "http://admin.invalid/xxl-job-admin"
executor_url = "http://worker.invalid:10002"
app_name = "erp-test"
access_token = "test-token-123456"

[app]
            port=10001
            secret="test-secret-that-is-at-least-32-bytes"
            [database]
            uri="mongodb://localhost:27017"
            db_name="test"
            [s3]
            bucket="erp-assets"
            region="cn-south-1"
            endpoint="https://s3.example.com"
            access_key_id="test-access-key"
            secret_access_key="test-secret-key"
            key_prefix="erp/uploads"
            public_base_url="https://cdn.example.com"
            {supplier}
        "#
        ))
        .unwrap()
    }

    #[test]
    fn supplier_config_assembles_protocol_ports_without_enabling_legacy_dispatch() {
        for supplier in ["", "[dangaoshushu]\nenabled=false"] {
            let ports = ExternalConnectorPorts::from_config(&connector_config(supplier)).unwrap();
            assert!(ports.dangaoshushu.is_none());
            assert_eq!(ports.readiness.supplier_api, ConnectorMode::FailClosed);
        }
        let config = connector_config(
            "[dangaoshushu]\nenabled=true\nchannel_no='test-channel'\nprivate_key='test-key'",
        );
        let ports = ExternalConnectorPorts::from_config(&config).unwrap();
        assert!(ports.dangaoshushu.is_some());
        assert_eq!(ports.readiness.supplier_api, ConnectorMode::Configured);
        assert_eq!(ports.readiness.supplier_reference_registry, ConnectorMode::Configured);
        assert_eq!(ports.readiness.supplier_fulfillment, ConnectorMode::FailClosed);
        assert!(!ports.readiness.is_ready());
    }

    #[test]
    fn readiness_requires_every_external_port() {
        let configured = ExternalConnectorReadiness {
            supplier_api: ConnectorMode::Configured,
            supplier_reference_registry: ConnectorMode::Configured,
            supplier_fulfillment: ConnectorMode::Configured,
        };
        assert!(configured.is_ready());

        assert!(
            !ExternalConnectorReadiness { supplier_api: ConnectorMode::FailClosed, ..configured }.is_ready()
        );
        assert!(
            !ExternalConnectorReadiness {
                supplier_reference_registry: ConnectorMode::FailClosed,
                ..configured
            }
            .is_ready()
        );
    }
}

mod supplier_operations;

mod imports;
