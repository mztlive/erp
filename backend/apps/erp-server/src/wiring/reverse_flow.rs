//! 逆向命令消费的身份、审批运行、采购范围和正式财务责任提供者。

use erp_identity::SharedRbacService;
use erp_reverse_flow::ReverseFlowPorts;
use mongodb::Database;

use super::finance_responsibility::finance_responsibility;
use super::identity::shared_rbac_service;
use super::{MongoPurchaseDataScope, approval_runtime, purchase_access};

/// 从应用已有提供者装配逆向命令的全部外域能力。
/// # 参数
/// `db` 为当前数据库，`rbac` 为当前应用共享权限实例。
/// # 返回
/// 返回未执行I/O的本库能力集合；每个业务步骤沿用调用方执行器。
/// # 错误
/// 构造时不返回错误，资格失败在原业务调用点传播。
pub fn reverse_flow_ports(db: Database, rbac: SharedRbacService) -> ReverseFlowPorts {
    ReverseFlowPorts::new(
        rbac.clone(),
        approval_runtime(db.clone(), rbac.clone()),
        purchase_access(db.clone(), rbac.clone()),
        MongoPurchaseDataScope::shared(db.clone(), rbac.clone()),
        finance_responsibility(db, rbac),
    )
}

/// 为未显式共享应用身份的原退货退款命令创建局部身份边界。
/// # 参数
/// `db` 为当前数据库。
/// # 返回
/// 返回与其他命令和全局权限状态隔离的提供者集合。
/// # 错误
/// 构造不执行I/O，不返回错误。
pub fn local_reverse_flow_ports(db: Database) -> ReverseFlowPorts {
    reverse_flow_ports(db.clone(), shared_rbac_service(db))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[tokio::test]
    async fn default_reverse_commands_are_local_and_explicit_purchase_commands_share_identity() {
        // Construct the exact application factories without querying a database.
        let client = mongodb::Client::with_uri_str("mongodb://localhost:27017").await.unwrap();
        let db = client.database("reverse-construction-unit-test");
        let application = shared_rbac_service(db.clone());
        let explicit_purchase = reverse_flow_ports(db.clone(), application.clone());
        let first = local_reverse_flow_ports(db.clone());
        let second = local_reverse_flow_ports(db);

        assert!(Arc::ptr_eq(&application, &*explicit_purchase));
        assert!(!Arc::ptr_eq(&application, &*first));
        assert!(!Arc::ptr_eq(&*first, &*second));
    }
}
