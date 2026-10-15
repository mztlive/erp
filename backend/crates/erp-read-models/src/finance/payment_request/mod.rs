//! 采购付款申请的完整来源授权、提交材料与任务专用读取。

mod access;
mod actions;
mod approval;
mod candidates;
mod dto;
mod entries;
mod freshness;
mod list;
mod materials;
mod payments;
mod previews;
mod query;
mod source;
mod submissions;

use std::sync::Arc;

pub use dto::*;
use erp_finance::ports::funds_scope::FailClosedFundsDataScopePort;
use erp_identity::SharedRbacService;
use erp_procurement::{PurchaseAccess, PurchaseDataScopePort};
pub use materials::PaymentRequestMaterialReference;
use mongodb::Database;
pub use source::freeze_payment_request_source;

use crate::finance::funds_scope::FundsAccess;

/// 只在全部采购来源获准时交付整份申请；正式任务另行证明责任资格。
#[derive(Clone)]
pub struct PurchasePaymentRequestReadService {
    db: Database,
    rbac: SharedRbacService,
    purchase_scope: Arc<dyn PurchaseDataScopePort>,
}

impl PurchasePaymentRequestReadService {
    /// 绑定数据库、当前权限服务与采购来源范围。
    ///
    /// # 参数
    /// * `db` / `rbac` / `purchase_scope` - 组合根注入的事实与范围服务。
    /// # 返回
    /// 返回不缓存资格的读取服务。
    /// # 错误
    /// 不执行 I/O，不返回错误。
    pub fn new(
        db: Database,
        rbac: SharedRbacService,
        purchase_scope: Arc<dyn PurchaseDataScopePort>,
    ) -> Self {
        Self { db, rbac, purchase_scope }
    }

    fn funds_access(&self) -> FundsAccess {
        FundsAccess::new(self.db.clone(), self.rbac.clone(), Arc::new(FailClosedFundsDataScopePort))
    }

    fn purchase_access(&self) -> PurchaseAccess {
        PurchaseAccess::new(self.db.clone(), Arc::clone(&self.purchase_scope))
    }
}
