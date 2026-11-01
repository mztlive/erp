//! 演示入口专用的领域与编排合同；具体跨域 adapter 只由应用装配。
use std::sync::Arc;

use erp_catalog::ports::supply::CatalogSupplyQueryPort;
use erp_catalog::{CatalogAccess, CatalogService};
use erp_customer::CustomerAssignmentService;
use erp_identity::service::access_control::AccessControlService;
use erp_identity::service::organization::OrganizationService;
use erp_master_data::{CustomerProfileProcess, SupplierProfileProcess};
use erp_party::PartyService;
use erp_procure_to_pay::procure_to_pay::responsibility::ProcurementResponsibilityProcess;
use erp_supplier_catalog::supply_governance::SupplierOfferingProcess;
use erp_warehouse::{IdentityFactPort, WarehouseService};
use erp_workflow::WorkItemService;
use erp_workflow::ports::WorkflowAuditPort;

/// 仅供演示数据入口使用的已装配服务，禁止普通业务库依赖本合同。
pub struct DemoRuntimePorts<A> {
    /// 主体读写服务。
    pub party: PartyService,
    /// 当前范围、审计和文件资产均已注入的商品服务。
    pub catalog: CatalogService,
    /// 与商品写入服务采用同一授权来源的范围服务。
    pub catalog_access: CatalogAccess,
    /// 已注入客户范围的根资料命令。
    pub customers: CustomerProfileProcess,
    /// 已注入供应商范围及身份资格的根资料命令。
    pub suppliers: SupplierProfileProcess,
    /// 仓库命令服务。
    pub warehouse: WarehouseService,
    /// 核验当前收发经办人的领域消费 Port。
    pub warehouse_identity: Arc<dyn IdentityFactPort>,
    /// 客户归属维护服务。
    pub customer_assignments: CustomerAssignmentService,
    /// 已装配未结业务事实的组织服务。
    pub organization: OrganizationService,
    /// 已装配显式范围目标校验的身份服务。
    pub scopes: AccessControlService,
    /// 财务责任规则与资格服务。
    pub work_items: WorkItemService<A>,
    /// 工作流资格 Port，不导入具体实现。
    pub workflow_auth: A,
    /// 工作流审计 Port。
    pub workflow_audit: Arc<dyn WorkflowAuditPort>,
    /// 已装配范围与工作项合同的供给命令。
    pub offerings: SupplierOfferingProcess,
    /// 采购责任规则与资格命令。
    pub procurement_responsibility: ProcurementResponsibilityProcess,
    /// 与正式销售入口一致的可售 SKU 资格读取 Port。
    pub catalog_supply: Arc<dyn CatalogSupplyQueryPort>,
}
