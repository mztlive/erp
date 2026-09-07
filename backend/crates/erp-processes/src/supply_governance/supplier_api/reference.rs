//! 外部引用解析位于两次本域校验之间；提交事务始终重新读取连接。
use super::{
    receipt::{persist_command_receipt, CommandReceiptWrite},
    SupplierApiGovernanceProcess,
};
use crate::{Error, Result};
use application_core::AuditActor;
use erp_supply::{
    dto::supplier_api::SupplierConnectionCommandResult,
    entity::supplier_api::{ConnectionEnvironment, SupplierCommandOutcome, SupplierConnectionAction},
    ports::{
        supplier_api_gateway::ClassifiedError,
        supplier_reference_registry::{ResolvedSupplierReference, SupplierReferenceKind},
    },
    service::supplier_api::{command::CommandIdentity, SupplierApiService},
};
use persistence_core::{NoTransaction, Transactional};
impl SupplierApiGovernanceProcess {
    pub(super) async fn execute_reference_command(
        &self,
        id: &str,
        action: SupplierConnectionAction,
        payload_reference: &str,
        expected_version: u64,
        identity: CommandIdentity,
        actor: &AuditActor,
    ) -> Result<SupplierConnectionCommandResult> {
        execute_reference(ReferenceCommand {
            process: self,
            id,
            action,
            payload_reference,
            expected_version,
            identity,
            actor,
        })
        .await
    }
    async fn commit_reference_command(
        &self,
        id: &str,
        action: SupplierConnectionAction,
        expected_version: u64,
        identity: CommandIdentity,
        resolved: ResolvedSupplierReference,
        actor: &AuditActor,
    ) -> Result<SupplierConnectionCommandResult> {
        let db = self.db.clone();
        let client = db.client().clone();
        let actor = actor.clone();
        let connection_id_value = id.to_string();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let connection = SupplierApiService::new(db.clone())
                        .apply_reference(
                            &connection_id_value,
                            action,
                            expected_version,
                            resolved,
                            actor.id(),
                            session,
                        )
                        .await?;
                    persist_command_receipt(
                        &db,
                        CommandReceiptWrite {
                            connection: &connection,
                            action,
                            identity: &identity,
                            outcome: SupplierCommandOutcome::Succeeded,
                            job_id: None,
                            actor: &actor,
                        },
                        session,
                    )
                    .await
                })
            })
            .await
    }
}
fn reference_error(error: ClassifiedError) -> Error {
    Error::BusinessLogicError(format!("{}: {}", error.code, error.summary))
}
/// 生产与替身共用的预检、外部解析、重新校验提交三段顺序。
trait ReferenceCommandPort: Sync + Send {
    type Checked: Send + Sync;
    type Resolved: Send;
    type Output: Send;
    fn preflight(&self) -> impl std::future::Future<Output = Result<Self::Checked>> + Send;
    fn resolve(
        &self,
        checked: Self::Checked,
    ) -> impl std::future::Future<Output = Result<Self::Resolved>> + Send;
    fn commit(
        self,
        resolved: Self::Resolved,
    ) -> impl std::future::Future<Output = Result<Self::Output>> + Send;
}
async fn execute_reference<P: ReferenceCommandPort>(port: P) -> Result<P::Output> {
    let checked = port.preflight().await?;
    let resolved = port.resolve(checked).await?;
    port.commit(resolved).await
}
/// 实际 provider 保留事务外 NoTransaction 和结果事务内第二次校验。
struct ReferenceCommand<'a> {
    process: &'a SupplierApiGovernanceProcess,
    id: &'a str,
    action: SupplierConnectionAction,
    payload_reference: &'a str,
    expected_version: u64,
    identity: CommandIdentity,
    actor: &'a AuditActor,
}
impl ReferenceCommandPort for ReferenceCommand<'_> {
    type Checked = ConnectionEnvironment;
    type Resolved = ResolvedSupplierReference;
    type Output = SupplierConnectionCommandResult;
    async fn preflight(&self) -> Result<Self::Checked> {
        let connection = self
            .process
            .domain()
            .load_reference_target(self.id, self.expected_version, &mut NoTransaction)
            .await?;
        Ok(connection.environment)
    }
    async fn resolve(&self, environment: Self::Checked) -> Result<Self::Resolved> {
        let kind = match self.action {
            SupplierConnectionAction::UpdateBusinessProfile => SupplierReferenceKind::BusinessProfile,
            SupplierConnectionAction::BindEndpointReference => SupplierReferenceKind::Endpoint,
            SupplierConnectionAction::BindCredentialReference => SupplierReferenceKind::Credential,
            _ => return Err(Error::Internal("引用命令分派错误".to_string())),
        };
        self.process
            .reference_registry
            .resolve(kind, self.payload_reference, environment)
            .await
            .map_err(reference_error)
    }
    async fn commit(self, resolved: Self::Resolved) -> Result<Self::Output> {
        self.process
            .commit_reference_command(
                self.id,
                self.action,
                self.expected_version,
                self.identity,
                resolved,
                self.actor,
            )
            .await
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    struct RecordingReference {
        calls: Arc<Mutex<Vec<&'static str>>>,
        fail_at: Option<usize>,
    }
    impl RecordingReference {
        fn step(&self, name: &'static str) -> Result<()> {
            let mut calls = self.calls.lock().unwrap();
            let position = calls.len();
            calls.push(name);
            if self.fail_at == Some(position) {
                return Err(Error::ConflictError(format!("failed {name}")));
            }
            Ok(())
        }
    }
    impl ReferenceCommandPort for RecordingReference {
        type Checked = u8;
        type Resolved = u8;
        type Output = u8;
        async fn preflight(&self) -> Result<u8> {
            self.step("preflight")?;
            Ok(11)
        }
        async fn resolve(&self, checked: u8) -> Result<u8> {
            assert_eq!(checked, 11);
            self.step("resolve")?;
            Ok(22)
        }
        async fn commit(self, resolved: u8) -> Result<u8> {
            assert_eq!(resolved, 22);
            self.step("commit_revalidation")?;
            Ok(33)
        }
    }
    #[tokio::test]
    async fn reference_resolution_stays_between_preflight_and_commit_revalidation() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let port = RecordingReference {
            calls: Arc::clone(&calls),
            fail_at: None,
        };
        assert_eq!(execute_reference(port).await.unwrap(), 33);
        assert_eq!(
            *calls.lock().unwrap(),
            ["preflight", "resolve", "commit_revalidation"]
        );
    }
    #[tokio::test]
    async fn reference_failures_stop_before_resolve_or_commit_and_preserve_first_error() {
        let expected = ["preflight", "resolve", "commit_revalidation"];
        for fail_at in 0..3 {
            let calls = Arc::new(Mutex::new(Vec::new()));
            let port = RecordingReference {
                calls: Arc::clone(&calls),
                fail_at: Some(fail_at),
            };
            let error = execute_reference(port).await.unwrap_err();
            assert!(
                matches!(error,Error::ConflictError(message) if message==format!("failed {}",expected[fail_at]))
            );
            assert_eq!(*calls.lock().unwrap(), expected[..=fail_at]);
        }
    }
}
