//! 履约与验收任务在原资格核验位置独立创建身份，不复用调用方的策略状态。

use std::sync::Arc;

use erp_fulfillment_execution::fulfillment_execution::task::FulfillmentIdentityFactory;
use mongodb::Database;

use super::identity::shared_rbac_service;

/// 为每次履约或验收任务资格核验保留原有的独立身份构造边界。
///
/// # 参数
/// * `db` - 原履约任务所用的业务及身份数据库。
///
/// # 返回
/// 返回在任务创建、活动及完成核验时创建全新 RBAC 状态的工厂。
///
/// # 错误
/// 构造不执行 I/O，不返回错误。
pub fn fulfillment_identity_factory(db: Database) -> FulfillmentIdentityFactory {
    Arc::new(move || shared_rbac_service(db.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fulfillment_task_authorizations_have_independent_policy_state() {
        // 仅构造句柄与实际应用工厂，不向 MongoDB 发出命令。
        let client = mongodb::Client::with_uri_str("mongodb://localhost:27017").await.unwrap();
        let db = client.database("fulfillment-task-identity-unit-test");
        let caller = shared_rbac_service(db.clone());
        let factory = fulfillment_identity_factory(db);
        let first = factory();
        let second = factory();

        assert!(!Arc::ptr_eq(&caller, &first));
        assert!(!Arc::ptr_eq(&caller, &second));
        assert!(!Arc::ptr_eq(&first, &second));
    }
}
