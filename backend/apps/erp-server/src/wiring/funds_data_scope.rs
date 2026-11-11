//! 资金授权辅助 adapter：展开目录条件；资金读取必须经真实来源授权入口。

use std::collections::BTreeSet;
use std::sync::Arc;

use async_trait::async_trait;
use erp_finance::ports::funds_scope::FundsDataScopePort;
use erp_identity::SharedRbacService;
use erp_read_models::Result as ReadResult;
use erp_read_models::ports::invoice_grants::InvoiceGrantReadiness;
use erp_resource_grants::ResourceGrantProcess;
use mongodb::Database;
use persistence_core::Executor;

use super::identity_error::map_identity_error;
use super::scope_support;

map_identity_error!(erp_finance);

/// 组合层资金范围 adapter，持有身份域解析所需依赖。
#[derive(Clone)]
pub struct MongoFundsDataScope {
    db: Database,
}

impl MongoFundsDataScope {
    /// 绑定组织事实所用数据库。
    ///
    /// # 参数
    /// * `db` - 组织集合所在数据库。
    ///
    /// # 返回
    /// 返回未执行 I/O 的 adapter。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 包装为资金域可注入的共享 Port。
    ///
    /// # 参数
    /// * `db` - 组织集合所在数据库。
    ///
    /// # 返回
    /// 返回只读取组织事实的 Port。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn shared(db: Database) -> Arc<dyn FundsDataScopePort> {
        Arc::new(Self::new(db))
    }
}

#[async_trait]
impl FundsDataScopePort for MongoFundsDataScope {
    async fn expand_org_units(
        &self,
        org_unit_ids: &[String],
        include_descendants: bool,
        executor: &mut dyn Executor,
    ) -> erp_finance::Result<BTreeSet<String>> {
        scope_support::expand_org_units(
            &self.db,
            org_unit_ids,
            include_descendants,
            executor,
            erp_finance::Error::RepositoryError,
            map_identity_error,
        )
        .await
    }
}

/// 构造绑定同一 RBAC 的资金访问器；HTTP 与命名 Process 必须经此入口。
///
/// # 参数
/// * `db` - 资金与身份集合所在数据库。
/// * `rbac` - 交给资金访问器的 RBAC 快照；范围 Port 本身不保存它。
///
/// # 返回
/// 返回已注入本 adapter 的资金访问器。
///
/// # 错误
/// 不返回错误。
pub fn funds_access_with_rbac(
    db: Database,
    rbac: SharedRbacService,
) -> erp_read_models::finance::funds_scope::FundsAccess {
    let scope = MongoFundsDataScope::shared(db.clone());
    erp_read_models::finance::funds_scope::FundsAccess::new(db.clone(), rbac.clone(), scope)
        .with_grant_readiness(Arc::new(InvoiceReadiness { db, rbac }))
}

struct InvoiceReadiness {
    db: Database,
    rbac: SharedRbacService,
}
#[async_trait]
impl InvoiceGrantReadiness for InvoiceReadiness {
    async fn pending(
        &self,
        request_id: &str,
        recipient: &str,
        executor: &mut dyn Executor,
    ) -> ReadResult<bool> {
        Ok(ResourceGrantProcess::new(self.db.clone(), self.rbac.clone())
            .invoice_pending(request_id, recipient, executor)
            .await?)
    }
}
