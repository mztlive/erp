//! 仅由演示数据入口使用的依赖装配。
use std::sync::Arc;

use erp_demo_data::runtime_ports::DemoRuntimePorts;
use erp_procure_to_pay::procure_to_pay::responsibility::ProcurementResponsibilityProcess;

use super::AppState;
use crate::wiring::catalog_supply_query::MongoCatalogSupplyQuery;
use crate::wiring::workflow::{WorkflowAuth, work_item_service, workflow_audit, workflow_auth};
use crate::wiring::{
    MongoWarehouseIdentity, catalog_access, organization_service, scope_configuration,
    scoped_customer_assignment_service, scoped_offering_process,
};

impl AppState {
    /// 装配演示入口实际使用的领域合同，不执行业务读写。
    /// # 参数
    /// 无；复用当前数据库、配置与授权源。
    /// # 返回
    /// 只供演示数据 Process 消费的服务集合。
    /// # 错误
    /// 不返回错误。
    pub(super) fn demo_runtime_ports(&self) -> DemoRuntimePorts<WorkflowAuth> {
        DemoRuntimePorts {
            party: self.party_service(),
            catalog: self.catalog_service(),
            catalog_access: catalog_access(self.db(), self.rbac()),
            customers: self.customer_profile_process(),
            suppliers: self.supplier_profile_process(),
            warehouse: self.warehouse_service(),
            warehouse_identity: MongoWarehouseIdentity::shared(self.db(), self.rbac()),
            customer_assignments: scoped_customer_assignment_service(self.db(), self.rbac()),
            organization: organization_service(self.db(), self.rbac()),
            scopes: scope_configuration(self.db(), self.rbac()),
            work_items: work_item_service(self.db(), self.rbac()),
            workflow_auth: workflow_auth(self.db(), self.rbac()),
            workflow_audit: workflow_audit(self.db()),
            offerings: scoped_offering_process(self.db(), self.rbac()),
            procurement_responsibility: ProcurementResponsibilityProcess::new(self.db(), self.rbac()),
            catalog_supply: Arc::new(MongoCatalogSupplyQuery::new(self.db())),
        }
    }
}
