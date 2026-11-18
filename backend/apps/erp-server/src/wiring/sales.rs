//! 销售、询价与选品消费方的应用装配。

use std::sync::Arc;

use erp_identity::{AuthorizationService, SharedRbacService};
use erp_order_to_cash::services::SalesServices;
use erp_support::FileAssetService;
use mongodb::Database;

use super::approval_runtime::approval_runtime;
use super::catalog_supply_query::MongoCatalogSupplyQuery;
use super::support_audit::MongoSupportAudit;
use super::support_documents::MongoBusinessDocument;
use super::{
    contract_access, customer_access, finance_responsibility, scoped_customer_service, selection_access,
    supplier_access,
};

/// 装配复用进程统一授权服务的销售业务服务与 Port。
/// 不执行 I/O 的销售消费合同。
/// 不返回错误；资格与版本均在业务原事务中重验。
pub fn sales_services(
    db: Database,
    rbac: SharedRbacService,
    authorization: Arc<AuthorizationService>,
) -> Arc<SalesServices> {
    Arc::new(SalesServices {
        authorization: authorization.clone(),
        customers: Arc::new(scoped_customer_service(db.clone(), authorization.clone())),
        customer_access: customer_access(db.clone(), authorization.clone()),
        contracts: contract_access(db.clone(), authorization.clone()),
        suppliers: supplier_access(db.clone(), authorization.clone()),
        selections: selection_access(db.clone(), authorization.clone()),
        catalog: Arc::new(MongoCatalogSupplyQuery::new(db.clone())),
        files: Arc::new(FileAssetService::new(
            db.clone(),
            MongoSupportAudit::shared(db.clone()),
            MongoBusinessDocument::shared(db.clone()),
            authorization.clone(),
        )),
        approvals: approval_runtime(db.clone(), rbac.clone(), authorization.clone()),
        finance_responsibility: finance_responsibility(db, rbac, authorization),
    })
}
