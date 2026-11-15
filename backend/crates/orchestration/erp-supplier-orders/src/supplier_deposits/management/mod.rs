//! 预存款管理用例；主体/组织范围、原业务资格和资金事务共同核验。

use std::sync::Arc;

use application_core::AuditActor;
use erp_identity::AuthorizationService;
use erp_party::SensitiveDataCodec;
use mongodb::Database;

use super::OrdersPaymentTasks;

mod authorization;
mod dto;
mod execution;
mod reads;
mod recharge;
mod resolution;

pub use dto::*;

/// 持有身份授权及敏感资料解码器的预存款跨域管理入口。
pub struct SupplierDepositManagementProcess {
    db: Database,
    authorization: Arc<AuthorizationService>,
    codec: SensitiveDataCodec,
    payment_tasks: Arc<dyn OrdersPaymentTasks>,
}

impl SupplierDepositManagementProcess {
    /// 绑定应用拥有的数据、账号身份和预算解码能力。
    /// # 参数
    /// * `db` - 应用数据库。
    /// * `codec` - 组合根共享的敏感资料编解码器。
    /// # 返回
    /// 返回受权管理流程。
    /// # 错误
    /// 无。
    pub fn new(
        db: Database,
        codec: SensitiveDataCodec,
        authorization: Arc<AuthorizationService>,
        payment_tasks: Arc<dyn OrdersPaymentTasks>,
    ) -> Self {
        Self { authorization, db, codec, payment_tasks }
    }
}

struct DepositManagementContext {
    db: Database,
    authorization: Arc<AuthorizationService>,
    codec: SensitiveDataCodec,
    payment_tasks: Arc<dyn OrdersPaymentTasks>,
    account: String,
    actor: AuditActor,
}
impl SupplierDepositManagementProcess {
    fn command_context(&self, account: &str, actor: &AuditActor) -> DepositManagementContext {
        DepositManagementContext {
            db: self.db.clone(),
            authorization: self.authorization.clone(),
            codec: self.codec.clone(),
            payment_tasks: self.payment_tasks.clone(),
            account: account.into(),
            actor: actor.clone(),
        }
    }
}
