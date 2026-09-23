//! SettlementParty 目录的身份授权装配。
use std::sync::Arc;

use application_core::AuditActor;
use application_core::directory::DirectoryScope;
use async_trait::async_trait;
use erp_identity::SharedRbacService;
use erp_identity::access_control::ScopeDimension;
use erp_identity::service::access_control::resolve::DataScopeService;
use erp_party::Result;
use erp_party::ports::SettlementPartyDirectoryAccess;
use erp_party::service::directory::SettlementPartyDirectoryService;
use mongodb::Database;
use persistence_core::Executor;

use super::directory_scope::directory_scope;
use super::identity_error::map_identity_error;
map_identity_error!(erp_party);

struct Access(DataScopeService);
#[async_trait]
impl SettlementPartyDirectoryAccess for Access {
    async fn resolve(&self, actor: &AuditActor, executor: &mut dyn Executor) -> Result<DirectoryScope> {
        let access =
            self.0.resolve(actor, "settlement_party", "list", executor).await.map_err(map_identity_error)?;
        directory_scope(access, ScopeDimension::SettlementParty).map_err(map_identity_error)
    }
}

/// 装配独立目录，不附加任何业务结果条件。
/// # 参数
/// `db` 为数据库，`rbac` 为公共权限服务。
/// # 返回
/// 领域目录服务。
/// # 错误
/// 无；授权失败由读取入口返回。
pub fn party_directory(db: Database, rbac: SharedRbacService) -> SettlementPartyDirectoryService {
    SettlementPartyDirectoryService::new(db.clone(), Arc::new(Access(DataScopeService::new(db, rbac))))
}
