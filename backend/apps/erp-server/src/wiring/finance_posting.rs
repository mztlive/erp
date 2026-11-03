//! 财务过账消费的身份、审批、来源访问和正式任务提供者。

use std::sync::Arc;

use erp_finance_posting::{FinancePostingPorts, FinanceTaskPort};
use erp_identity::SharedRbacService;
use erp_read_models::finance::payment_request::PurchasePaymentRequestReadService;
use mongodb::Database;

use super::finance_responsibility::finance_tasks;
use super::identity::shared_rbac_service;
use super::{MongoPurchaseDataScope, approval_runtime, funds_access_with_rbac, purchase_access};

/// 装配财务命令提供者，并为付款保持独立的原局部权限事务边界。
/// # 参数
/// `db` 为当前数据库，`rbac` 为应用共享授权读取实例。
/// # 返回
/// 返回不执行I/O的本库能力集合；局部事务身份不与共享实例合并。
/// # 错误
/// 构造时不返回错误；每个业务调用点继续验证当前资格。
pub fn finance_posting_ports(db: Database, rbac: SharedRbacService) -> FinancePostingPorts {
    let task_db = db.clone();
    FinancePostingPorts::new(
        rbac.clone(),
        shared_rbac_service(db.clone()),
        approval_runtime(db.clone(), rbac.clone()),
        purchase_access(db.clone(), rbac.clone()),
        funds_access_with_rbac(db.clone(), rbac.clone()),
        PurchasePaymentRequestReadService::new(
            db.clone(),
            rbac.clone(),
            MongoPurchaseDataScope::shared(db.clone(), rbac.clone()),
        ),
        finance_tasks(db, rbac),
    )
    .with_fresh_tasks_factory(Arc::new(move || -> Arc<dyn FinanceTaskPort> {
        finance_tasks(task_db.clone(), shared_rbac_service(task_db.clone()))
    }))
}

/// 为客户往来命令创建原有的独立身份与权限事务边界。
/// # 参数
/// `db` 为当前数据库，不接收应用全局身份实例。
/// # 返回
/// 返回每次调用独立的局部权限提供者；内部新命令也重新创建独立实例。
/// # 错误
/// 构造不执行I/O，不返回错误。
pub fn receivable_posting_ports(db: Database) -> FinancePostingPorts {
    finance_posting_ports(db.clone(), shared_rbac_service(db.clone()))
        .with_receivable_factory(Arc::new(move || receivable_posting_ports(db.clone())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn receivable_commands_and_nested_commands_have_independent_policy_state() {
        // Only construct handles and providers; this test sends no MongoDB command.
        let client = mongodb::Client::with_uri_str("mongodb://localhost:27017").await.unwrap();
        let db = client.database("receivable-construction-unit-test");
        let application = shared_rbac_service(db.clone());
        let shared_command = finance_posting_ports(db.clone(), application.clone());
        let first = receivable_posting_ports(db.clone());
        let second = receivable_posting_ports(db);
        let nested = first.fresh_receivable().unwrap();

        assert!(Arc::ptr_eq(&application, &*shared_command));
        for local in [&first, &second, &nested] {
            assert!(!Arc::ptr_eq(&application, &**local));
        }
        assert!(!Arc::ptr_eq(&*first, &*second));
        assert!(!Arc::ptr_eq(&*first, &*nested));
    }
}
