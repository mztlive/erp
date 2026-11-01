//! 供应商履约任务与交接在原核验位置创建独立身份及同源能力。

use std::sync::Arc;

use erp_fulfillment_execution::supply_execution::{
    SupplierFulfillmentIdentity, SupplierFulfillmentIdentityFactory,
};
use erp_fulfillment_execution::workflow::FulfillmentWorkflowServices;
use mongodb::Database;

use super::identity::shared_rbac_service;
use super::workflow::work_item_service;
use super::{MongoFulfillmentOrderDataScope, approval_runtime};

/// 为每次任务资格、调查阶段或交接命令创建独立的身份能力。
///
/// # 参数
/// * `db` - 原供应商履约命令使用的数据库。
/// # 返回
/// 返回每次独立、组内 RBAC 与工作流及数据范围同源的工厂。
/// # 错误
/// 构造只创建本地能力，不执行 I/O，不返回错误。
pub fn supplier_fulfillment_identity_factory(db: Database) -> SupplierFulfillmentIdentityFactory {
    Arc::new(move || {
        let rbac = shared_rbac_service(db.clone());
        SupplierFulfillmentIdentity {
            workflow: Arc::new(FulfillmentWorkflowServices::new(
                work_item_service(db.clone(), rbac.clone()),
                approval_runtime(db.clone(), rbac.clone()),
            )),
            data_scope: MongoFulfillmentOrderDataScope::shared(db.clone(), rbac.clone()),
            rbac,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn supplier_fulfillment_checks_have_independent_policy_state() {
        // 只构造句柄与实际应用工厂，不向 MongoDB 发送命令。
        let client = mongodb::Client::with_uri_str("mongodb://localhost:27017").await.unwrap();
        let db = client.database("supplier-fulfillment-identity-unit-test");
        let caller = shared_rbac_service(db.clone());
        let factory = supplier_fulfillment_identity_factory(db);
        let first = factory();
        let second = factory();

        assert!(!Arc::ptr_eq(&caller, &first.rbac));
        assert!(!Arc::ptr_eq(&caller, &second.rbac));
        assert!(!Arc::ptr_eq(&first.rbac, &second.rbac));
        assert!(!Arc::ptr_eq(&first.workflow, &second.workflow));
        assert!(!Arc::ptr_eq(&first.data_scope, &second.data_scope));
    }
}
