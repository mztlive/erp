//! 销售、询价与选品消费方的应用装配。

use std::sync::Arc;

use erp_identity::SharedRbacService;
use erp_order_to_cash::services::SalesServices;
use erp_support::FileAssetService;
use mongodb::Database;

use super::approval_runtime::approval_runtime;
use super::catalog_supply_query::MongoCatalogSupplyQuery;
use super::identity::shared_rbac_service;
use super::support_audit::MongoSupportAudit;
use super::support_documents::MongoBusinessDocument;
use super::{contract_access, customer_access, customer_service, selection_access, supplier_access};

/// 使用原共享身份政策装配销售业务实际消费的服务与 Port。
/// # 参数
/// `db` 为应用数据库，`rbac` 为当前共享权限来源。
/// # 返回
/// 不执行 I/O 的销售消费合同。
/// # 错误
/// 不返回错误；资格与版本均在业务原事务中重验。
pub fn sales_services(db: Database, rbac: SharedRbacService) -> Arc<SalesServices> {
    let handover_db = db.clone();
    Arc::new(SalesServices {
        handover_identity: Arc::new(move || shared_rbac_service(handover_db.clone())),
        customers: Arc::new(customer_service(db.clone())),
        customer_access: customer_access(db.clone(), rbac.clone()),
        contracts: contract_access(db.clone(), rbac.clone()),
        suppliers: supplier_access(db.clone(), rbac.clone()),
        selections: selection_access(db.clone(), rbac.clone()),
        catalog: Arc::new(MongoCatalogSupplyQuery::new(db.clone())),
        files: Arc::new(FileAssetService::new(
            db.clone(),
            MongoSupportAudit::shared(db.clone()),
            MongoBusinessDocument::shared(db.clone()),
        )),
        approvals: approval_runtime(db.clone(), rbac.clone()),
        finance_responsibility: super::finance_responsibility(db, rbac),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn handover_target_revalidation_has_fresh_identity_per_call() {
        // Construct application providers only; no database command is sent.
        let client = mongodb::Client::with_uri_str("mongodb://localhost:27017").await.unwrap();
        let db = client.database("sales-handover-identity-unit-test");
        let caller = shared_rbac_service(db.clone());
        let services = sales_services(db, caller.clone());

        let first = (services.handover_identity)();
        let second = (services.handover_identity)();

        assert!(!Arc::ptr_eq(&caller, &first));
        assert!(!Arc::ptr_eq(&caller, &second));
        assert!(!Arc::ptr_eq(&first, &second));
    }
}
