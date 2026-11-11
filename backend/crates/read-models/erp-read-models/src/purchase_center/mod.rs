//! 采购列表、对象中心、创建依据与责任规则的跨域只读组合。
pub mod access;
mod approval;
pub(crate) mod approval_materials;
mod approval_query;
pub(crate) mod approval_supplier;
mod change;
pub mod change_bounds;
pub(crate) mod creation_authorization;
mod creation_basis;
pub use creation_authorization::require_creation_task;
mod delivery_options;
pub mod direct_return;
pub mod dto;
pub mod expense_source;
pub mod freight_follow_up;
mod payments;
pub mod procurement_responsibility;
mod query;
pub use query::deposit_funds::PurchaseDepositAccessPort;

pub use crate::finance::purchase_payments::PurchasePaymentView;
pub mod repository;
mod scope;
pub mod source_impact;
pub(crate) mod source_sales;
use std::sync::Arc;

pub use change::query::PurchaseChangeListView;
use erp_identity::SharedRbacService;
use erp_procurement::PurchaseDataScopePort;
pub use scope::{PurchaseListParams, PurchaseListView};
pub use source_sales::PurchaseSalesMaterialReference;

/// 使用提供方公开事实装配采购视图；构造本身不访问数据库。
#[derive(Clone)]
pub struct PurchaseOrderReadService {
    db: mongodb::Database,
    data_scope: Arc<dyn PurchaseDataScopePort>,
    rbac: SharedRbacService,
}
impl PurchaseOrderReadService {
    /// 装配采购读取所需的数据库、身份设施和对象范围。
    ///
    /// # 参数
    /// * `db` - 采购事实数据库。
    /// * `rbac` - 当前共享身份设施。
    /// * `data_scope` - 已装配的采购对象范围端口。
    ///
    /// # 返回
    /// 不缓存授权结论的采购读取服务。
    ///
    /// # 错误
    /// 构造不执行 I/O。
    pub fn new(
        db: mongodb::Database,
        rbac: SharedRbacService,
        data_scope: Arc<dyn PurchaseDataScopePort>,
    ) -> Self {
        Self { db, rbac, data_scope }
    }

    /// 构造绑定当前 Port 的采购访问器。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回无授权缓存的访问器。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 不得在此回退构造身份域 Service。
    fn access(&self) -> access::PurchaseAccess {
        access::PurchaseAccess::new(self.db.clone(), Arc::clone(&self.data_scope))
    }
}

#[cfg(test)]
mod test_support;

pub mod payment_arrangement;
