//! 询价跨领域原子命令；需求和反馈规则由销售域拥有。

pub mod access;
mod assignment;
mod commands;
pub mod dto;
pub mod eligibility;
mod execution;
pub mod expense_read;
pub mod fee_allocation;
pub mod internal_quote;
pub mod read;
mod recovery;
mod tasks;
mod template;

use access::SalesInquiryAccess;
use erp_identity::SharedRbacService;
use mongodb::Database;

/// 协调询价、当前责任任务、审计与强类型命令回执。
#[derive(Clone)]
pub struct SalesInquiryProcess {
    pub(crate) db: Database,
    pub(crate) rbac: SharedRbacService,
}

impl SalesInquiryProcess {
    /// 绑定业务库与授权源，命令每次在写事务中重验。
    ///
    /// # 参数
    /// `db` 为业务库，`rbac` 为当前授权源。
    /// # 返回
    /// 未执行 I/O 的询价流程。
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }

    fn access(&self) -> SalesInquiryAccess {
        SalesInquiryAccess::new(self.db.clone(), self.rbac.clone())
    }
}
