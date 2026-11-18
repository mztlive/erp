//! 逆向命令消费共享统一授权、审批运行和正式财务责任提供者。

use std::sync::Arc;

use erp_identity::{AuthorizationService, SharedRbacService};
use erp_reverse_flow::ReverseFlowPorts;
use mongodb::Database;

use super::approval_runtime;
use super::finance_responsibility::finance_responsibility;
use super::identity::shared_rbac_service;

/// 从应用已有提供者装配逆向命令的全部外域能力。
/// 返回未执行I/O的本库能力集合；每个业务步骤沿用调用方执行器。
pub fn reverse_flow_ports(
    db: Database,
    rbac: SharedRbacService,
    authorization: Arc<AuthorizationService>,
) -> ReverseFlowPorts {
    ReverseFlowPorts::new(
        authorization.clone(),
        approval_runtime(db.clone(), rbac.clone(), authorization.clone()),
        finance_responsibility(db, rbac, authorization),
    )
}

/// 沿原退货退款入口装配审批设施并复用共享统一授权服务。
/// 返回与采购命令使用相同统一授权服务的逆向能力集合。
pub fn local_reverse_flow_ports(db: Database, authorization: Arc<AuthorizationService>) -> ReverseFlowPorts {
    reverse_flow_ports(db.clone(), shared_rbac_service(db), authorization)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wiring::authorization::build;

    #[tokio::test]
    async fn reverse_commands_reuse_the_injected_shared_authorization() {
        let client = mongodb::Client::with_uri_str("mongodb://localhost:27017").await.unwrap();
        let db = client.database("reverse-construction-unit-test");
        let rbac = shared_rbac_service(db.clone());
        let authorization = build(db.clone(), rbac.clone()).unwrap();
        let explicit = reverse_flow_ports(db.clone(), rbac, authorization.clone());
        let local = local_reverse_flow_ports(db, authorization.clone());
        assert!(Arc::ptr_eq(&authorization, &*explicit));
        assert!(Arc::ptr_eq(&authorization, &*local));
    }
}
