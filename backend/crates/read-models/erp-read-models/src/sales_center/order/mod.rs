//! 销售单列表与详情：组合销售、财务、采购与审批公开事实。
use std::sync::Arc;

use erp_identity::AuthorizationService;
mod adapter;
mod approval_query;
mod current_work;
pub mod dto;
mod evidence;
mod procurement;
mod query;
mod scope;
pub use scope::{SalesListParams, SalesListView};
mod status;

use erp_sales::entity::sales_order::BusinessType;
use erp_workflow::entity::document_registry::DocumentType;
use mongodb::Database;

use crate::Result;
use crate::approval_runtime::read_subject_ref;

/// 销售单列表与详情的跨域只读组合服务。
pub struct SalesOrderReadService {
    db: Database,
    authorization: Arc<AuthorizationService>,
}
impl SalesOrderReadService {
    /// 使用数据库句柄及共享授权服务构造只读查询服务。
    ///
    /// # 参数
    /// * `db` - 业务数据库。
    /// * `authorization` - 应用启动时完整装配的共享授权服务。
    ///
    /// # 返回
    /// 返回可执行完整授权读取的服务；构造不执行查询。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database, authorization: Arc<AuthorizationService>) -> Self {
        Self { db, authorization }
    }
    fn sales(&self) -> erp_sales::service::sales_order::SalesOrderService {
        erp_sales::service::sales_order::SalesOrderService::new(self.db.clone())
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
    read_subject_ref(document_type_of_sales_business(business_type), id)
}
