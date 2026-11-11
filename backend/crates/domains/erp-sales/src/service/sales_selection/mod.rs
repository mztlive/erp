//! 销售选品用例编排。

pub mod access;
mod copy;
mod create;
mod lifecycle;
mod mapper;
mod prepare;
mod prepare_queue;
mod pricing;
mod public_access;
mod publish;
mod query;
mod session;

pub use access::SelectionAccess;
use application_core::AuditActor;
use erp_core::ids::SalesSelectionBookletId;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::sales_selection::IdempotencyOperation;
use crate::{Error, Result};

/// 幂等写入上下文对象。
#[derive(Debug)]
pub struct IdempotencyStoreInput<'a, T> {
    /// 操作域。
    pub operation: IdempotencyOperation,
    /// 作用域。
    pub scope_id: &'a str,
    /// 幂等键。
    pub key: &'a str,
    /// 请求哈希。
    pub hash: &'a str,
    /// 待持久化的结果视图。
    pub result: &'a T,
    /// 令牌版本。
    pub token_version: Option<u32>,
    /// 关联选品册。
    pub booklet_id: Option<SalesSelectionBookletId>,
}

/// 销售选品服务.
#[derive(Clone)]
pub struct SalesSelectionService {
    db: Database,
    authorization: Option<(SelectionAccess, AuditActor)>,
}

impl SalesSelectionService {
    /// 创建服务。
    ///
    /// # 参数
    /// * `db` - 数据库
    ///
    /// # 返回
    /// 返回服务。
    ///
    /// # 错误
    /// 无。
    pub fn new(db: Database) -> Self {
        Self { db, authorization: None }
    }
    /// 绑定内部管理命令的统一授权能力。
    /// # 参数
    /// access 为组合根装配的授权器，actor 为当前操作身份。
    /// # 返回
    /// 在实际写事务内重验对象资格的服务。
    /// # 错误
    /// 构造不执行 I/O。
    pub fn with_authorization(mut self, access: SelectionAccess, actor: AuditActor) -> Self {
        self.authorization = Some((access, actor));
        self
    }

    async fn authorize_booklet(&self, id: &str, action: &str, executor: &mut dyn Executor) -> Result<()> {
        let (access, actor) = self
            .authorization
            .as_ref()
            .ok_or_else(|| Error::Internal("选品管理命令未装配统一授权".into()))?;
        access.require_booklet(actor, action, id, executor).await?;
        Ok(())
    }
}
