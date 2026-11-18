//! 外部引用解析位于两次本域校验之间；提交事务始终重新读取连接。
use application_core::AuditActor;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::Permission;
use erp_supply::dto::supplier_api::SupplierConnectionCommandResult;
use erp_supply::entity::supplier_api::{SupplierCommandOutcome, SupplierConnectionAction};
use erp_supply::ports::supplier_api_gateway::ClassifiedError;
use erp_supply::ports::supplier_reference_registry::{
    ResolvedSupplierReference, SupplierReferenceKind, SupplierReferenceTarget,
};
use erp_supply::service::supplier_api::SupplierApiService;
use erp_supply::service::supplier_api::command::CommandIdentity;
use erp_supply::service::supplier_api::context::action_permission;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::SupplierApiGovernanceProcess;
use super::receipt::{CommandReceiptWrite, persist_command_receipt};
use crate::{Error, Result};
impl SupplierApiGovernanceProcess {
    /// 事务外预检并解析外部引用，再在新事务内重验连接并提交。
    ///
    /// # 参数
    /// * `id` - 供应连接 ID。
    /// * `action` - 资料、端点或凭证引用动作。
    /// * `payload_reference` - 待解析的外部引用。
    /// * `expected_version` - 调用方冻结的连接版本。
    /// * `identity` - 已计算的命令身份。
    /// * `actor` - 当前操作人。
    ///
    /// # 返回
    /// 引用绑定成功后的命令回执。
    ///
    /// # 错误
    /// 连接或版本不可用、动作不是引用命令、外部引用解析失败，或提交时重验与回执写入失败时返回对应错误。解析失败映射为 `BusinessLogicError`。
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
        let authorization = self.authorization.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        let actor = actor.clone();
        let connection_id_value = id.to_string();
        client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    authorization
                        .require(&actor, &action_permission(action), AccessTarget::Operation, executor)
                        .await?;

                    let connection = SupplierApiService::new(db.clone())
                        .apply_reference(
                            &connection_id_value,
                            action,
                            expected_version,
                            resolved,
                            actor.id(),
                            executor,
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
                        executor,
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
    fn preflight(&self) -> impl Future<Output = Result<Self::Checked>> + Send;
    fn resolve(&self, checked: Self::Checked) -> impl Future<Output = Result<Self::Resolved>> + Send;
    fn commit(self, resolved: Self::Resolved) -> impl Future<Output = Result<Self::Output>> + Send;
}
async fn execute_reference<P: ReferenceCommandPort>(port: P) -> Result<P::Output> {
    let checked = port.preflight().await?;
    let resolved = port.resolve(checked).await?;
    port.commit(resolved).await
}
/// 本地预检使用授权快照；外部引用解析在事务外，结果提交时重新授权。
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
    type Checked = SupplierReferenceTarget;
    type Resolved = ResolvedSupplierReference;
    type Output = SupplierConnectionCommandResult;
    async fn preflight(&self) -> Result<Self::Checked> {
        let (process, id, version, action, actor) = (
            self.process.clone(),
            self.id.to_owned(),
            self.expected_version,
            self.action,
            self.actor.clone(),
        );
        self.process
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    reference_target(
                        &process.db,
                        &process.authorization,
                        &actor,
                        &action_permission(action),
                        &id,
                        version,
                        executor,
                    )
                    .await
                })
            })
            .await
    }
    async fn resolve(&self, target: Self::Checked) -> Result<Self::Resolved> {
        let kind = match self.action {
            SupplierConnectionAction::UpdateBusinessProfile => SupplierReferenceKind::BusinessProfile,
            SupplierConnectionAction::BindEndpointReference => SupplierReferenceKind::Endpoint,
            SupplierConnectionAction::BindCredentialReference => SupplierReferenceKind::Credential,
            _ => return Err(Error::Internal("引用命令分派错误".to_string())),
        };
        self.process
            .reference_registry
            .resolve(kind, self.payload_reference, &target)
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
async fn reference_target(
    db: &Database,
    authorization: &AuthorizationService,
    actor: &AuditActor,
    permission: &Permission,
    id: &str,
    version: u64,
    executor: &mut dyn Executor,
) -> Result<SupplierReferenceTarget> {
    authorization.require(actor, permission, AccessTarget::Operation, executor).await?;
    let connection = SupplierApiService::new(db.clone()).load_reference_target(id, version, executor).await?;
    Ok(SupplierReferenceTarget::from(&connection))
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};

    use erp_core::ids::{SupplierAccountId, SupplierApiConnectionId};
    use erp_supply::entity::failure::SupplierFailureClass;
    use erp_supply::entity::supplier_api::{
        SupplierApiConnection, SupplierApiConnectionData, SupplierApiConnectionStatus, SupplierConnectorType,
    };
    use erp_supply::ports::supplier_reference_registry::{
        SupplierReferenceOption, SupplierReferenceRegistry,
    };

    use super::*;
    #[tokio::test]
    async fn reference_preflight_rejects_before_loading_connection_without_snapshot_or_permission() {
        use erp_core::AccountKind;
        use erp_identity::testing::unavailable_authorization;
        use mongodb::Client;
        use persistence_core::ReadExecutor;
        use persistence_core::testing::SnapshotExecutor;

        let db = Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap().database("reference");
        let authorization = unavailable_authorization();
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let permission = action_permission(SupplierConnectionAction::BindEndpointReference);
        assert!(
            matches!(reference_target(&db, &authorization, &actor, &permission, "connection", 1, &mut ReadExecutor::default()).await, Err(Error::ValidationError(message)) if message == "写入授权必须使用快照事务")
        );

        assert!(
            matches!(reference_target(&db, &authorization, &actor, &permission, "connection", 1, &mut SnapshotExecutor::default()).await, Err(Error::Internal(message)) if message == "测试授权设施不可用")
        );
    }

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
        let port = RecordingReference { calls: Arc::clone(&calls), fail_at: None };
        assert_eq!(execute_reference(port).await.unwrap(), 33);
        assert_eq!(*calls.lock().unwrap(), ["preflight", "resolve", "commit_revalidation"]);
    }
    #[tokio::test]
    async fn reference_failures_stop_before_resolve_or_commit_and_preserve_first_error() {
        let expected = ["preflight", "resolve", "commit_revalidation"];
        for fail_at in 0..3 {
            let calls = Arc::new(Mutex::new(Vec::new()));
            let port = RecordingReference { calls: Arc::clone(&calls), fail_at: Some(fail_at) };
            let error = execute_reference(port).await.unwrap_err();
            assert!(
                matches!(error,Error::ConflictError(message) if message==format!("failed {}",expected[fail_at]))
            );
            assert_eq!(*calls.lock().unwrap(), expected[..=fail_at]);
        }
    }

    struct BoundReference(SupplierReferenceTarget);
    impl SupplierReferenceRegistry for BoundReference {
        fn is_available(&self) -> bool {
            true
        }
        fn options<'a>(
            &'a self,
            _: SupplierReferenceKind,
            _: &'a SupplierReferenceTarget,
        ) -> Pin<
            Box<
                dyn Future<Output = std::result::Result<Vec<SupplierReferenceOption>, ClassifiedError>>
                    + Send
                    + 'a,
            >,
        > {
            Box::pin(async { Ok(vec![]) })
        }
        fn resolve<'a>(
            &'a self,
            _: SupplierReferenceKind,
            _: &'a str,
            target: &'a SupplierReferenceTarget,
        ) -> Pin<
            Box<
                dyn Future<Output = std::result::Result<ResolvedSupplierReference, ClassifiedError>>
                    + Send
                    + 'a,
            >,
        > {
            Box::pin(async move {
                if target.connection_id != self.0.connection_id {
                    return Err(ClassifiedError {
                        class: SupplierFailureClass::MappingError,
                        code: "REFERENCE_TARGET_MISMATCH".into(),
                        summary: "target differs".into(),
                    });
                }
                Ok(ResolvedSupplierReference { internal_reference: "bound-reference".into() })
            })
        }
    }
    struct RuntimeReference<'a> {
        registry: &'a dyn SupplierReferenceRegistry,
        target: SupplierReferenceTarget,
        ticket: &'a str,
        commits: Arc<Mutex<Vec<String>>>,
    }
    impl ReferenceCommandPort for RuntimeReference<'_> {
        type Checked = SupplierReferenceTarget;
        type Resolved = ResolvedSupplierReference;
        type Output = ();
        async fn preflight(&self) -> Result<Self::Checked> {
            Ok(self.target.clone())
        }
        async fn resolve(&self, target: Self::Checked) -> Result<Self::Resolved> {
            self.registry
                .resolve(SupplierReferenceKind::Endpoint, self.ticket, &target)
                .await
                .map_err(reference_error)
        }
        async fn commit(self, resolved: Self::Resolved) -> Result<Self::Output> {
            self.commits.lock().unwrap().push(resolved.internal_reference);
            Ok(())
        }
    }
    fn ticket_connection() -> SupplierApiConnection {
        SupplierApiConnection::new(
            SupplierApiConnectionId::new("connection-1"),
            SupplierApiConnectionData {
                supplier_id: SupplierAccountId::new("supplier-1"),
                connection_code: "dgss-test".into(),
                connector_type: SupplierConnectorType::Dangaoshushu,
                endpoint_reference: "unbound".into(),
                credential_reference: None,
                rate_limit_policy: None,
                status: SupplierApiConnectionStatus::Disabled,
            },
            "actor",
        )
        .unwrap()
    }
    #[tokio::test]
    async fn ticket_target_mismatch_stops_before_commit_and_correct_target_commits() {
        let connection = ticket_connection();
        let runtime = BoundReference(SupplierReferenceTarget::from(&connection));
        let target = SupplierReferenceTarget::from(&connection);
        let commits = Arc::new(Mutex::new(Vec::new()));
        let wrong = SupplierReferenceTarget {
            connection_id: SupplierApiConnectionId::new("connection-2"),
            ..target.clone()
        };
        let error = execute_reference(RuntimeReference {
            registry: &runtime,
            target: wrong,
            ticket: "opaque-bound-ticket",
            commits: Arc::clone(&commits),
        })
        .await
        .unwrap_err();
        assert!(
            matches!(error, Error::BusinessLogicError(message) if message.starts_with("REFERENCE_TARGET_MISMATCH:"))
        );
        assert!(commits.lock().unwrap().is_empty());

        execute_reference(RuntimeReference {
            registry: &runtime,
            target,
            ticket: "opaque-bound-ticket",
            commits: Arc::clone(&commits),
        })
        .await
        .unwrap();
        assert_eq!(commits.lock().unwrap().len(), 1);
    }
}
