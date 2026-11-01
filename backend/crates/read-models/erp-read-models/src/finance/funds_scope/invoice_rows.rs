//! 发票与开票申请的数据范围响应行，保留原票面事实及命令所需身份。

use erp_core::common::time::BusinessDate;
use erp_core::money::Amount;
use erp_finance::dto::payable::PurchaseInvoiceAllocationView;
use erp_finance::dto::receivable::SalesInvoiceAllocationView;
use erp_finance::entity::receivable::{
    InvoiceDirection, InvoiceKind, InvoiceRequestData, InvoiceRequestStatus, InvoiceStatus,
};
use serde::Serialize;

/// M08 销项发票范围行：整单金额与完整凭证部分授权为 null。
#[derive(Debug, Clone, Serialize)]
pub struct ScopedInvoiceRow {
    /// 发票主键。
    pub id: String,
    /// 发票乐观锁版本；部分授权为 null。
    pub version: Option<u64>,
    /// 客户或供应商主体；部分授权为 null。
    pub party_id: Option<String>,
    /// 原票面发票代码；未采集或部分授权为 null。
    pub invoice_code: Option<String>,
    /// 发票号码。
    pub invoice_no: Option<String>,
    /// 发票方向。
    pub invoice_direction: InvoiceDirection,
    /// 蓝红类型。
    pub invoice_kind: InvoiceKind,
    /// 发票状态。
    pub status: InvoiceStatus,
    /// 开票日期。
    pub invoice_date: Option<BusinessDate>,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 获授权分配份额合计。
    pub visible_allocated_share: Amount,
    /// 整单含税金额；部分授权为 null。
    pub gross_amount: Option<Amount>,
    /// 原票面不含税金额；附件登记未采集或部分授权为 null。
    pub net_amount: Option<Amount>,
    /// 原票面税额；附件登记未采集或部分授权为 null，不推导为零。
    pub tax_amount: Option<Amount>,
    /// 红票关联的原蓝票；无关联或部分授权为 null。
    pub original_invoice_id: Option<String>,
    /// 整单已分配合计；部分授权为 null。
    pub allocated_total: Option<Amount>,
    /// 未分配余额；部分授权为 null。
    pub unallocated_amount: Option<Amount>,
    /// 获授权分配行；部分授权仅含获授权份额行。
    pub allocations: Option<Vec<SalesInvoiceAllocationView>>,
    /// 进项方向获授权分配行；销项发票为空，部分授权仅含获授权份额行。
    pub purchase_allocations: Option<Vec<PurchaseInvoiceAllocationView>>,
    /// 部分受限时为 true。
    pub permission_limited: bool,
}

/// M08 开票申请范围行：负责销售/申请人/当前开票处理人分别查询，不改变正式开票准入。
#[derive(Debug, Clone, Serialize)]
pub struct ScopedInvoiceRequestRow {
    /// 申请主键。
    pub id: String,
    /// 当前申请版本；撤回、重新提交及附件开票命令必须回传此版本。
    pub version: u64,
    /// 申请单号。
    pub request_no: String,
    /// 申请固定绑定的应收子账；供来源核对及原申请重新提交使用。
    pub receivable_account_id: String,
    /// 关联销售单。
    pub sales_order_id: String,
    /// 关联销售单业务单号。
    pub sales_order_no: String,
    /// 申请状态。
    pub status: InvoiceRequestStatus,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 申请人（创建人）。
    pub applicant_user_id: String,
    /// 当前开票处理人（关联工作项当前负责人）；无工作项为空。
    pub handler_user_id: Option<String>,
    /// 当前申请绑定的开票任务；未批准或尚未创建任务时为空。
    pub work_item_id: Option<String>,
    /// 申请金额；部分授权仍返回本行申请金额（行级事实），整单汇总为 null 由汇总视图承担。
    pub amount: Amount,
    /// 申请保存的开票资料；列表与详情均返回原申请快照，不使用当前主体档案替换。
    pub data: InvoiceRequestData,
    /// 已使用的申请额度；实际开票后增加，批准申请本身不增加。
    pub invoiced_amount: Amount,
    /// 部分受限时为 true。
    pub permission_limited: bool,
    /// 关联销售单当前负责人。
    pub sales_owner_user_id: Option<String>,
    /// 关联销售单当前业务组织。
    pub business_org_unit_id: Option<String>,
}
