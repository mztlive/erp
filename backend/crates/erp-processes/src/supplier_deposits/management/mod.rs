//! 预存款管理用例；主体/组织范围、原业务资格和资金事务共同核验。

use application_core::AuditActor;
use erp_identity::SharedRbacService;
use erp_party::SensitiveDataCodec;
use mongodb::Database;

use crate::adapters::identity::shared_rbac_service;

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
    rbac: SharedRbacService,
    codec: SensitiveDataCodec,
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
    pub fn new(db: Database, codec: SensitiveDataCodec) -> Self {
        Self { rbac: shared_rbac_service(db.clone()), db, codec }
    }

    /// 复用应用共享权限服务，不创建另一个身份缓存。
    /// # 参数
    /// * `rbac` - 当前权限实例。
    /// # 返回
    /// 返回装配后的流程。
    /// # 错误
    /// 无。
    pub fn with_rbac(mut self, rbac: SharedRbacService) -> Self {
        self.rbac = rbac;
        self
    }
}

struct DepositManagementContext {
    db: Database,
    rbac: SharedRbacService,
    codec: SensitiveDataCodec,
    account: String,
    actor: AuditActor,
}
impl SupplierDepositManagementProcess {
    fn command_context(&self, account: &str, actor: &AuditActor) -> DepositManagementContext {
        DepositManagementContext {
            db: self.db.clone(),
            rbac: self.rbac.clone(),
            codec: self.codec.clone(),
            account: account.into(),
            actor: actor.clone(),
        }
    }
}
