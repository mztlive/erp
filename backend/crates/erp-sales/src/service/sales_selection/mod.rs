//! 销售选品用例编排。

mod create;
mod lifecycle;
mod mapper;
mod prepare;
mod prepare_queue;
mod publish;
mod query;
mod session;

use erp_core::ids::SalesSelectionBookletId;
use mongodb::Database;

use crate::entity::sales_selection::IdempotencyOperation;

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
pub struct SalesSelectionService {
    db: Database,
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
        Self { db }
    }
}
