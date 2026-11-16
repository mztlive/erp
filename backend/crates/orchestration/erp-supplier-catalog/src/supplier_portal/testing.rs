//! 不可用授权设施只验证拒绝顺序，不执行数据库读取。
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_catalog::{CatalogAccess, CatalogService, EmptyFileAssetFacts, FailClosedAuditPort};
use erp_identity::ports::{IdentityAuditPort, PreparedResourceAudit};
use erp_identity::testing::unavailable_authorization;
use erp_identity::{RbacService, Result as IdentityResult};
use erp_supplier::SupplierAccess;
use erp_supply::service::supplier_offering::{OfferingAccess, SupplierOfferingService};
use erp_workflow::WorkItemService;
use erp_workflow::ports::WorkflowAuthorizationPort;
use mongodb::Client;
use persistence_core::Executor;

use super::SupplierPortalProcess;
use crate::wiring::CatalogServices;

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
        panic!("授权拒绝不得准备身份审计")
    }
    async fn persist(&self, _: &PreparedResourceAudit, _: &mut dyn Executor) -> IdentityResult<()> {
        panic!("授权拒绝不得写入身份审计")
    }
}
#[derive(Clone)]
struct Workflows;
impl WorkflowAuthorizationPort for Workflows {}

pub(super) async fn unavailable_process() -> SupplierPortalProcess {
    let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
        .await
        .unwrap()
        .database("portal_command_authorization_unit");
    let authorization = unavailable_authorization();
    let catalog_access = CatalogAccess::new(db.clone(), authorization.clone());
    let offering_access = OfferingAccess::new(db.clone(), authorization.clone());
    let services = CatalogServices {
        catalog: CatalogService::new(
            db.clone(),
            Arc::new(FailClosedAuditPort),
            Arc::new(EmptyFileAssetFacts),
            catalog_access.clone(),
        ),
        catalog_access,
        offerings: SupplierOfferingService::new(db.clone()).with_data_scope(offering_access.clone()),
        offering_access,
        suppliers: SupplierAccess::new(db.clone(), authorization.clone()),
        work_items: Arc::new(WorkItemService::new(
            db.clone(),
            Arc::new(RbacService::new(db.clone(), Arc::new(NoAudit))),
            authorization.clone(),
            Workflows,
        )),
    };
    SupplierPortalProcess::new(db, authorization, Arc::new(services))
}
