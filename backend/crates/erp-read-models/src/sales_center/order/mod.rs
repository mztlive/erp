//! 销售单列表与详情：组合销售、财务、采购与审批公开事实。
mod adapter;
mod approval_query;
pub mod dto;
mod query;
mod scope;
pub use scope::{SalesListParams, SalesListView};
mod status;

use erp_identity::SharedRbacService;
use erp_sales::entity::sales_order::BusinessType;
use erp_workflow::entity::document_registry::DocumentType;
use mongodb::Database;

use crate::{Error, Result};

/// 销售单列表与详情的跨域只读组合服务。
pub struct SalesOrderReadService {
    db: Database,
    rbac: Option<SharedRbacService>,
}
impl SalesOrderReadService {
    /// 使用数据库句柄构造只读查询服务。
    ///
    /// 返回未注入 RBAC 的服务；采购动作投影需要重验权限时返回缺少授权源错误。
    /// 构造本身不执行查询，也不产生错误。
    pub fn new(db: Database) -> Self {
        Self { db, rbac: None }
    }
    /// 使用数据库句柄和当前 RBAC 服务构造只读查询服务。
    ///
    /// 返回可投影采购动作访问资格的服务；构造不执行查询，也不产生错误。
    pub fn with_rbac(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac: Some(rbac) }
    }
    fn sales(&self) -> erp_sales::service::sales_order::SalesOrderService {
        erp_sales::service::sales_order::SalesOrderService::new(self.db.clone())
    }
    /// 获取采购访问投影使用的授权源；未注入时保持原错误语义并拒绝放行。
    fn require_rbac(&self) -> Result<&SharedRbacService> {
        self.rbac.as_ref().ok_or_else(|| Error::Internal("销售单审批绑定需要授权源".into()))
    }
}
/// 按销售业务性质显式选择审批对象类型，保留实物服务与卡券的独立主体。
fn document_type_of_sales_business(business_type: BusinessType) -> DocumentType {
    match business_type {
        BusinessType::GoodsService => DocumentType::SalesOrder,
        BusinessType::Voucher => DocumentType::VoucherSalesOrder,
    }
}
/// 由销售身份构造审批读取主体；非法身份按原销售校验错误传播。
fn subject_ref_for_sales_business(business_type: BusinessType, id: &str) -> Result<bpm::SubjectRef> {
    erp_workflow::entity::approval_integration::subject_ref_for(
        document_type_of_sales_business(business_type),
        id,
    )
    .map_err(|error| Error::ValidationError(error.to_string()))
}
