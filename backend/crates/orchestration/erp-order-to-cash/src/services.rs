//! 销售编排实际消费的领域服务及窄 Port，由应用配置具体实现。

use std::sync::Arc;

use erp_approval_runtime::ApprovalRuntimePort;
use erp_catalog::ports::supply::CatalogSupplyQueryPort;
use erp_contract::ContractAccess;
use erp_customer::{CustomerAccess, CustomerService};
use erp_identity::AuthorizationService;
use erp_sales::service::sales_selection::SelectionAccess;
use erp_supplier::SupplierAccess;
use erp_support::FileAssetService;

/// 应用使用同一数据库和授权政策构造的销售消费合同。
pub struct SalesServices {
    /// 进程共享授权服务；交接双方资格使用本次执行器读取。
    pub authorization: Arc<AuthorizationService>,
    /// 客户事实读取，不替代独立对象范围核验。
    pub customers: Arc<CustomerService>,
    /// 客户对象范围及创建资格。
    pub customer_access: CustomerAccess,
    /// 关联合同对象范围。
    pub contracts: ContractAccess,
    /// 采购内部报价的供应商对象范围。
    pub suppliers: SupplierAccess,
    /// 选品册和提案的范围核验。
    pub selections: SelectionAccess,
    /// 公司商品池的当前可售资格与价格读取。
    pub catalog: Arc<dyn CatalogSupplyQueryPort>,
    /// 选品图片快照引用的文件资产服务。
    pub files: Arc<FileAssetService>,
    /// 采购联动需要的财务正式任务责任分配。
    pub finance_responsibility: Arc<dyn erp_workflow::ports::FinanceResponsibilityPort>,
    /// 审批材料、绑定与抄送的共有执行步骤。
    pub approvals: Arc<dyn ApprovalRuntimePort>,
}
