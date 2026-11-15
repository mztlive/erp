//! 采购付款申请的完整来源授权、提交材料与任务专用读取。

use erp_identity::AuthorizationService;
mod access;
mod actions;
mod approval;
mod candidate_list;
mod candidates;
mod dto;
mod entries;
mod freshness;
mod list;
mod materials;
mod payments;
mod previews;
mod query;
mod review;
mod review_dto;
mod source;
mod submissions;

use std::sync::Arc;

pub use candidate_list::{PaymentCandidateQuery, PaymentCandidateSort};
pub use dto::*;
use erp_finance::ports::funds_scope::FailClosedFundsDataScopePort;
pub use materials::PaymentRequestMaterialReference;
use mongodb::Database;
pub use review_dto::*;
pub use source::freeze_payment_request_source;

use crate::finance::funds_scope::FundsAccess;

/// 只在全部采购来源获准时交付整份申请；正式任务另行证明责任资格。
#[derive(Clone)]
pub struct PurchasePaymentRequestReadService {
    db: Database,
    rbac: Arc<AuthorizationService>,
}

impl PurchasePaymentRequestReadService {
    /// 绑定数据库与当前权限服务。
    ///
    /// # 参数
    /// * `db` / `rbac` - 组合根注入的数据库与身份服务。
    /// # 返回
    /// 返回不缓存资格的读取服务。
    /// # 错误
    /// 不执行 I/O，不返回错误。
    pub fn new(db: Database, rbac: Arc<AuthorizationService>) -> Self {
        Self { db, rbac }
    }

    fn funds_access(&self) -> FundsAccess {
        FundsAccess::new(self.db.clone(), self.rbac.clone(), Arc::new(FailClosedFundsDataScopePort))
    }
}
