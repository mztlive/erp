//! 管理读取拒绝非事务执行器，授权失败时不访问业务集合。
#![cfg(test)]

use std::time::Duration;

use erp_core::AccountKind;
use erp_identity::testing::unavailable_authorization;
use mongodb::Client;
use mongodb::options::{ClientOptions, ServerAddress};
use persistence_core::NoTransaction;
use persistence_core::testing::SnapshotExecutor;

use super::*;

/// 使用不可用授权设施隔离验证读取入口，不连接数据库。
fn service_and_actor() -> (DocumentRegistryService, AuditActor) {
    let options = ClientOptions::builder()
        .hosts(vec![ServerAddress::Tcp { host: "127.0.0.1".into(), port: Some(1) }])
        .server_selection_timeout(Duration::from_millis(1))
        .build();
    let db = Client::with_options(options).unwrap().database("registry_read_authorization_unit");
    let service = DocumentRegistryService::new(db, unavailable_authorization());
    let actor = AuditActor::new("reader".into(), "reader".into(), AccountKind::Admin);
    (service, actor)
}

/// 执行五个真实步骤入口，保留原执行器供每项读取验证事务合同。
async fn read_errors(
    service: &DocumentRegistryService,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Vec<Error> {
    vec![
        service
            .business_document_list_with(&BusinessDocumentListParams::default(), actor, executor)
            .await
            .unwrap_err(),
        service.business_document_detail_with("document", actor, executor).await.unwrap_err(),
        service
            .workflow_action_list_with(&WorkflowActionListParams::default(), actor, executor)
            .await
            .unwrap_err(),
        service
            .document_relation_list_with(&BusinessDocumentId::new("document"), actor, executor)
            .await
            .unwrap_err(),
        service.document_participant_list_with("participant", actor, executor).await.unwrap_err(),
    ]
}

#[tokio::test]
async fn management_reads_reject_non_snapshot_steps_before_authorization_or_business_reads() {
    let (service, actor) = service_and_actor();
    for error in read_errors(&service, &actor, &mut NoTransaction).await {
        assert!(
            matches!(error, Error::ValidationError(message) if message == "单据注册管理读取必须使用快照事务")
        );
    }
}

#[tokio::test]
async fn management_reads_use_snapshot_authorization_and_stop_before_business_reads_on_failure() {
    let (service, actor) = service_and_actor();
    for error in read_errors(&service, &actor, &mut SnapshotExecutor::default()).await {
        assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
    }
}
