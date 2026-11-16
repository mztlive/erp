//! 应收子账与客户回款读模型。

mod account;
mod approval_query;
pub mod approval_view;
mod customer_receipt;
pub mod invoice_request;
pub mod invoice_request_source;
pub mod snapshot;

/// 只读应收投影，跨越财务、销售与审批事实。
///
/// 命令与事务所有权仍留在财务过账流程。
pub struct ReceivableReadService {
    db: mongodb::Database,
}

impl ReceivableReadService {
    /// 为给定数据库构造应收读模型。
    ///
    /// 构造不读不写。
    ///
    /// # 参数
    /// * `db` - 目标数据库。
    ///
    /// # 返回
    /// 返回应收读模型。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: mongodb::Database) -> Self {
        Self { db }
    }
}
