//! 询价跨领域原子命令；需求和反馈规则由销售域拥有。

use std::sync::Arc;

use crate::services::SalesServices;
pub mod access;
mod assignment;
mod authorization;
mod commands;
pub mod dto;
pub mod eligibility;
mod execution;
pub mod expense_read;
pub mod fee_allocation;
pub mod internal_quote;
pub mod quote_import;
mod recovery;
mod renewal;
mod tasks;

use access::SalesInquiryAccess;
use erp_identity::AuthorizationService;
use mongodb::Database;

/// 协调询价、当前责任任务、审计与强类型命令回执。
#[derive(Clone)]
pub struct SalesInquiryProcess {
    services: Arc<SalesServices>,
    pub(crate) db: Database,
    pub(crate) authorization: Arc<AuthorizationService>,
}

impl SalesInquiryProcess {
    /// 绑定业务库与授权源，命令每次在写事务中重验。
    ///
    /// `db` 为业务库，`authorization` 为当前授权源，`services` 为同一政策下装配的领域消费合同。未执行 I/O 的询价流程。
    pub fn new(db: Database, authorization: Arc<AuthorizationService>, services: Arc<SalesServices>) -> Self {
        Self { db, authorization, services }
    }

    fn access(&self) -> SalesInquiryAccess {
        SalesInquiryAccess::new(self.db.clone(), self.authorization.clone())
    }
}
