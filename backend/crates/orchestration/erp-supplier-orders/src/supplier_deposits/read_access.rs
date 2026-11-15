use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_finance::entity::supplier_deposit::SupplierDepositAccount;
use erp_identity::AuthorizationService;
use erp_read_models::purchase_center::PurchaseDepositAccessPort;
use erp_read_models::{Error as ReadError, Result as ReadResult};
use mongodb::Database;
use persistence_core::Executor;

use super::authorize_deposit_account;
use crate::Error;

/// 原采购资金摘要使用相同的财务账号范围资格，禁止全局权限代替主体和组织范围。
pub struct SupplierDepositReadAccess {
    db: Database,
    authorization: Arc<AuthorizationService>,
}

impl SupplierDepositReadAccess {
    /// 绑定应用数据库及当前RBAC服务，不缓存账号授权。
    /// # 参数
    /// `db`、`authorization` 由应用组合根提供。
    /// # 返回
    /// 可注入采购只读查询的财务权限适配器。
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database, authorization: Arc<AuthorizationService>) -> Self {
        Self { db, authorization }
    }
}

#[async_trait]
impl PurchaseDepositAccessPort for SupplierDepositReadAccess {
    async fn authorize(
        &self,
        account_id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> ReadResult<SupplierDepositAccount> {
        authorize_deposit_account(&self.db, &self.authorization, account_id, actor, executor).await.map_err(
            |error| match error {
                Error::Forbidden(message) => ReadError::Forbidden(message),
                Error::NotFound(message) => ReadError::NotFound(message),
                Error::RepositoryError(source) => ReadError::RepositoryError(source),
                Error::BusinessLogicError(message) => ReadError::BusinessLogicError(message),
                _ => ReadError::Internal("财务账号范围资格暂无法核验，请稍后重试".into()),
            },
        )
    }
}
