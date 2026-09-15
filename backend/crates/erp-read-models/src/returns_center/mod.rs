//! 退货、退款与冲正的详情和分页读模型。
mod approval;
mod customer_refund;
mod customer_refund_list;
pub mod dto;
mod payment_reversal;
mod purchase_return;
pub use purchase_return::PurchaseReturnListView;
mod receipt_reversal;
#[cfg(test)]
mod repository;
mod sales_return;
mod supplier_refund;
use erp_procurement::{FailClosedPurchaseDataScopePort, PurchaseDataScopePort};
use mongodb::Database;
use std::sync::Arc;

/// 组合逆向本域事实与只读审批摘要的查询入口。
pub struct ReturnsReadService {
    db: Database,
    purchase_scope: Arc<dyn PurchaseDataScopePort>,
}
impl ReturnsReadService {
    /// 使用组合根提供的数据库读取退货与审批事实。
    ///
    /// # 参数
    /// * `db` - MongoDB 数据库
    ///
    /// # 返回
    /// 返回未注入采购范围 Port 的服务。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 采购退货列表、详情必须改用 `with_purchase_scope`，未注入时失败关闭。
    pub fn new(db: Database) -> Self {
        Self {
            db,
            purchase_scope: FailClosedPurchaseDataScopePort::shared(),
        }
    }

    /// 注入采购范围 Port，供采购退货沿来源采购单授权。
    ///
    /// # 参数
    /// * `purchase_scope` - 组合层注入的采购范围 Port
    ///
    /// # 返回
    /// 返回可解析采购退货范围的服务。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 不得把 RBAC 直接交给退货域解释原始范围。
    pub fn with_purchase_scope(mut self, purchase_scope: Arc<dyn PurchaseDataScopePort>) -> Self {
        self.purchase_scope = purchase_scope;
        self
    }

    /// 构造绑定当前采购 Port 的访问器。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回无授权缓存的采购访问器。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 采购退货必须沿来源采购单责任接入。
    fn purchase_access(&self) -> crate::purchase_center::access::PurchaseAccess {
        crate::purchase_center::access::PurchaseAccess::new(self.db.clone(), Arc::clone(&self.purchase_scope))
    }
}
