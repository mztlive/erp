//! 应收财务操作与确定性的命令准备。
//! 跨域事务和工作流命令由财务过账流程持有。

pub mod customer_receipt_commit;
pub mod invoice_commit;
pub mod mapping;

pub mod customer_receipt_posting;
pub mod invoice_posting;
pub mod red_invoice_plan;

mod invoice_query;

/// 仅财务域的应收查询，以及不依赖事务的应用操作。
pub struct ReceivableService {
    db: mongodb::Database,
}

impl ReceivableService {
    /// 用给定数据库句柄构造应收财务服务。
    ///
    /// 构造本身不读取、不写入，也不划定事务边界。
    ///
    /// # 参数
    /// * `db` - 财务领域数据库。
    ///
    /// # 返回
    /// 返回未开始任何读写的服务实例。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: mongodb::Database) -> Self {
        Self { db }
    }
}

pub mod red_invoice_posting;

pub mod initial_account;

pub mod sales_change;

pub mod receipt_reversal;

pub mod customer_refund;
