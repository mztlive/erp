//! 授权失败不得探测原合同、版本、状态或最大修订序号。
use std::sync::Arc;

use async_trait::async_trait;
use erp_core::AccountKind;
use erp_core::ids::{CustomerAccountId, PartyId};
use erp_identity::entity::authorization::ObjectFacts;
use erp_identity::testing::unavailable_authorization;
use mongodb::Client;
use persistence_core::NoTransaction;
use persistence_core::testing::SnapshotExecutor;

use super::*;
use crate::authorization::ContractAccess;
use crate::dto::contract::{ContractView, CreateContractRequest};
use crate::ports::{
    ContractCreationPort, CustomerAccountFact, CustomerFactsPort, FailClosedAuditPort,
    FailClosedCustomerFactsPort, SigningCompanyFact, SigningCompanyPort,
};
use crate::service::contract::plan_first_archive;

struct NoCreation;

#[async_trait]
impl ContractCreationPort for NoCreation {
    async fn candidate(&self, _: &AuditActor, _: &str, _: &mut dyn Executor) -> Result<ObjectFacts> {
        panic!("既有合同命令不读取创建候选")
    }
}

struct NoCompanies;

#[async_trait]
impl SigningCompanyPort for NoCompanies {
    async fn company(&self, _: &str, _: &mut dyn Executor) -> Result<Option<SigningCompanyFact>> {
        panic!("授权失败不得读取公司")
    }
}

async fn service() -> ContractService {
    let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
        .await
        .unwrap()
        .database("contract_command_authorization");
    ContractService::new(
        db.clone(),
        Arc::new(FailClosedAuditPort),
        Arc::new(FailClosedCustomerFactsPort),
        Arc::new(NoCompanies),
        ContractAccess::new(db, unavailable_authorization(), Arc::new(NoCreation)),
    )
}

fn archive_request(version: u64) -> ArchiveContractRevisionRequest {
    serde_json::from_value(serde_json::json!({
        "version": version, "contract_pdf_file_id":"file", "customer_name":"customer",
        "settlement_party_name":"company", "payment_term_code":"prepaid",
        "payment_term_name":"prepaid", "invoice_type":"normal", "tax_point":"0",
        "valid_from":"2026-01-01", "signed_at":"2026-01-01"
    }))
    .unwrap()
}

#[tokio::test]
async fn archive_and_terminate_authorize_before_original_version_state_or_revision_reads() {
    let service = service().await;
    let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
    for version in [1, 999] {
        let results = [
            service
                .archive_stage(
                    "unread-contract",
                    archive_request(version),
                    &actor,
                    &mut SnapshotExecutor::default(),
                )
                .await,
            service
                .terminate_stage(
                    "unread-contract",
                    TerminateContractRequest { version },
                    &actor,
                    &mut SnapshotExecutor::default(),
                )
                .await,
        ];
        for result in results {
            assert!(matches!(result, Err(Error::Internal(message)) if message == "测试授权设施不可用"));
        }
    }
}

#[tokio::test]
async fn commands_reject_non_snapshot_before_any_business_read() {
    let service = service().await;
    let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
    for permission in [contract::ARCHIVE_REVISION, contract::TERMINATE] {
        let error = service
            .command_contract(&actor, &permission, "unread-contract", &mut NoTransaction)
            .await
            .expect_err("写入必须使用原快照事务");
        assert!(matches!(error, Error::ValidationError(message) if message == "写入授权必须使用快照事务"));
    }
}

struct Customers(Option<bool>);

#[async_trait]
impl CustomerFactsPort for Customers {
    async fn find_by_id(&self, _: &CustomerAccountId) -> Result<Option<CustomerAccountFact>> {
        panic!("创建资格不得使用事务外客户预读")
    }

    async fn find_by_ids(
        &self,
        ids: &[CustomerAccountId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<CustomerAccountFact>> {
        assert_eq!(ids, &[CustomerAccountId::new("customer")]);
        assert!(executor.boundary().is_some_and(|boundary| boundary.is_snapshot()));
        Ok(self
            .0
            .map(|is_active| CustomerAccountFact {
                id: "customer".into(),
                customer_no: "C-1".into(),
                party_id: PartyId::new("party"),
                is_active,
            })
            .into_iter()
            .collect())
    }
}

#[tokio::test]
async fn creation_rechecks_current_customer_eligibility_in_original_snapshot() {
    let mut service = service().await;
    for active in [Some(true), Some(false), None] {
        service.customers = Arc::new(Customers(active));
        let result = service
            .ensure_active_customer(&CustomerAccountId::new("customer"), &mut SnapshotExecutor::default())
            .await;
        match active {
            Some(true) => assert!(result.unwrap().is_active),
            Some(false) => assert!(matches!(result, Err(Error::BusinessLogicError(_)))),
            None => assert!(matches!(result, Err(Error::NotFound(_)))),
        }
    }
}

#[test]
fn creation_response_preserves_saved_revision_pointer_and_version() {
    let request: CreateContractRequest = serde_json::from_value(serde_json::json!({
        "contract_no":"CT-NEW", "customer_id":"customer", "settlement_party_id":"company",
        "contract_pdf_file_id":"file", "customer_name":"customer", "settlement_party_name":"company",
        "payment_term_code":"prepaid", "payment_term_name":"prepaid", "invoice_type":"normal",
        "tax_point":"0", "valid_from":"2026-01-01", "signed_at":"2026-01-01"
    }))
    .unwrap();
    let planned = plan_first_archive(request, "actor").unwrap();
    assert!(planned.contract.stable.current_revision_id.is_none());
    let revision_id = planned.revision.base.id;
    let mut saved = planned.contract;
    saved.attach_revision(&revision_id, "actor");
    // 仓储成功写入后的元数据作为视图输入；不在本测试模拟数据库的 CAS 实现。
    saved.base.version = 2;
    let view = ContractView::from(saved);
    assert_eq!(view.contract_no, "CT-NEW");
    assert_eq!(view.current_revision_id.as_deref(), Some(revision_id.as_str()));
    assert_eq!(view.version, 2);
}
