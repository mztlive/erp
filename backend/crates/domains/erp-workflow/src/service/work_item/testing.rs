//! 不可用设施证明授权拒绝先于业务读取；不连接数据库。
#![cfg(test)]
use std::sync::Arc;
use std::time::Duration;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_identity::ports::{IdentityAuditPort, PreparedResourceAudit};
use erp_identity::testing::unavailable_authorization;
use erp_identity::{RbacService, Result as IdentityResult};
use mongodb::Client;
use mongodb::options::{ClientOptions, ServerAddress};
use persistence_core::Executor;

use super::WorkItemService;
use crate::ports::FailClosedWorkflowAuthorizationPort;

struct NoAudit;
#[async_trait]
impl IdentityAuditPort for NoAudit {
    fn resource_log(
        &self,
        _: AuditActor,
        _: &str,
        _: &str,
        _: String,
    ) -> IdentityResult<PreparedResourceAudit> {
        panic!("授权拒绝路径不得准备身份审计")
    }
    async fn persist(&self, _: &PreparedResourceAudit, _: &mut dyn Executor) -> IdentityResult<()> {
        panic!("授权拒绝路径不得写入审计")
    }
}

// 构造不连接数据库的任务服务，用于断言授权拒绝发生在业务事实读取之前。
pub(super) fn unavailable_service() -> WorkItemService<FailClosedWorkflowAuthorizationPort> {
    let options = ClientOptions::builder()
        .hosts(vec![ServerAddress::Tcp { host: "127.0.0.1".into(), port: Some(1) }])
        .server_selection_timeout(Duration::from_millis(1))
        .build();
    let db = Client::with_options(options).unwrap().database("work_item_authorization_unit");
    let identity = Arc::new(RbacService::new(db.clone(), Arc::new(NoAudit)));
    WorkItemService::new(db, identity, unavailable_authorization(), FailClosedWorkflowAuthorizationPort)
}
