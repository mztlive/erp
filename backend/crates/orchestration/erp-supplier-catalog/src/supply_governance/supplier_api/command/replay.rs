//! 连接命令回放的资格、收据和任务编号共用一个事务快照。
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_supply::dto::supplier_api::{SupplierConnectionCommand, SupplierConnectionCommandResult};
use erp_supply::entity::supplier_api::{SupplierApiConnectionId, SupplierConnectionCommandReceipt};
use erp_supply::repository::SupplierApiExt;
use erp_supply::service::supplier_api::command::CommandIdentity;
use erp_supply::service::supplier_api::context::action_permission;
use erp_support::BulkJobExt;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::SupplierApiGovernanceProcess;
use crate::{Error, Result};

impl SupplierApiGovernanceProcess {
    /// 在原命令资格成立的同一快照中完整回放收据。
    /// # 参数
    /// id、command 和 actor 为当前连接命令及认证人。
    /// # 返回
    /// 已提交命令结果，未命中时返回 None，后续写入仍须重新授权。
    /// # 错误
    /// 撤权、同键异载荷或收据及任务读取失败时返回原错误。
    pub(super) async fn replay_command(
        &self,
        id: &str,
        command: &SupplierConnectionCommand,
        actor: &AuditActor,
    ) -> Result<Option<SupplierConnectionCommandResult>> {
        let identity = CommandIdentity::new(id, actor.id(), command)?;
        let port = MongoReplay {
            db: self.db.clone(),
            authorization: self.authorization.clone(),
            actor: actor.clone(),
        };
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { replay(&port, &identity, executor).await })
            })
            .await
    }
}

#[async_trait]
trait ReplayPort: Sync {
    async fn authorize(&self, identity: &CommandIdentity, executor: &mut dyn Executor) -> Result<()>;
    async fn receipt(
        &self,
        identity: &CommandIdentity,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierConnectionCommandReceipt>>;
    async fn job_no(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<String>>;
}

async fn replay(
    port: &impl ReplayPort,
    identity: &CommandIdentity,
    executor: &mut dyn Executor,
) -> Result<Option<SupplierConnectionCommandResult>> {
    port.authorize(identity, executor).await?;
    let Some(receipt) = port.receipt(identity, executor).await? else { return Ok(None) };
    if receipt.request_fingerprint != identity.fingerprint {
        return Err(Error::ConflictError(
            "这次操作已用于提交其他内容，请核对原操作结果后再发起新的操作".into(),
        ));
    }

    let job_no = match receipt.job_id.as_deref() {
        Some(id) => port.job_no(id, executor).await?,
        None => None,
    };
    Ok(Some(SupplierConnectionCommandResult {
        outcome: receipt.outcome,
        action: receipt.action,
        operation_id: receipt.base.id,
        connection_version: receipt.connection_version,
        job_id: receipt.job_id,
        job_no,
        audit_event_id: receipt.audit_event_id,
    }))
}

struct MongoReplay {
    db: Database,
    authorization: Arc<AuthorizationService>,
    actor: AuditActor,
}

#[async_trait]
impl ReplayPort for MongoReplay {
    async fn authorize(&self, identity: &CommandIdentity, executor: &mut dyn Executor) -> Result<()> {
        self.authorization
            .require(&self.actor, &action_permission(identity.action), AccessTarget::Operation, executor)
            .await?;
        Ok(())
    }

    async fn receipt(
        &self,
        identity: &CommandIdentity,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierConnectionCommandReceipt>> {
        Ok(self
            .db
            .supplier_api()
            .command_receipt(
                &SupplierApiConnectionId::new(&identity.connection_id),
                identity.action,
                &identity.actor_id,
                &identity.idempotency_hash,
                executor,
            )
            .await?)
    }

    async fn job_no(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<String>> {
        Ok(self.db.background_jobs().find_by_id(id, executor).await?.map(|job| job.job_no))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use erp_supply::entity::supplier_api::{
        SupplierCommandOutcome, SupplierConnectionAction, SupplierConnectionCommandReceiptData,
    };
    use mongodb::Client;
    use persistence_core::testing::SnapshotExecutor;
    use persistence_core::{ExecutionBoundary, ReadExecutor};

    use super::*;

    struct Replay {
        allowed: bool,
        receipt: Option<SupplierConnectionCommandReceipt>,
        calls: Mutex<Vec<&'static str>>,
        boundary: Mutex<Option<ExecutionBoundary>>,
    }

    impl Replay {
        fn new(allowed: bool, receipt: Option<SupplierConnectionCommandReceipt>) -> Self {
            Self { allowed, receipt, calls: Mutex::default(), boundary: Mutex::default() }
        }

        fn record(&self, name: &'static str, executor: &mut dyn Executor) {
            self.calls.lock().unwrap().push(name);
            assert!(self.boundary.lock().unwrap().as_ref().unwrap().is_current(executor));
        }
    }

    #[async_trait]
    impl ReplayPort for Replay {
        async fn authorize(&self, _: &CommandIdentity, executor: &mut dyn Executor) -> Result<()> {
            self.calls.lock().unwrap().push("authorize");
            let Some(boundary) = executor.boundary().filter(ExecutionBoundary::is_snapshot) else {
                return Err(Error::ValidationError("写入授权必须使用快照事务".into()));
            };
            if !self.allowed {
                return Err(Error::Forbidden("连接动作权限不足".into()));
            }
            *self.boundary.lock().unwrap() = Some(boundary);
            Ok(())
        }

        async fn receipt(
            &self,
            _: &CommandIdentity,
            executor: &mut dyn Executor,
        ) -> Result<Option<SupplierConnectionCommandReceipt>> {
            self.record("receipt", executor);
            Ok(self.receipt.clone())
        }

        async fn job_no(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<String>> {
            assert_eq!(id, "job");
            self.record("job_no", executor);
            Ok(Some("JOB-1".into()))
        }
    }

    fn identity() -> CommandIdentity {
        CommandIdentity::new(
            "connection",
            "actor",
            &SupplierConnectionCommand {
                action: SupplierConnectionAction::StartCatalogSync,
                expected_version: 1,
                payload_reference: None,
                reason_code: None,
                check_type: None,
                idempotency_key: "key".into(),
            },
        )
        .unwrap()
    }

    fn receipt(identity: &CommandIdentity, job: Option<&str>) -> SupplierConnectionCommandReceipt {
        SupplierConnectionCommandReceipt::new(
            identity.receipt_id.clone(),
            SupplierConnectionCommandReceiptData {
                connection_id: SupplierApiConnectionId::new(&identity.connection_id),
                action: identity.action,
                actor_id: identity.actor_id.clone(),
                idempotency_key_hash: identity.idempotency_hash.clone(),
                request_fingerprint: identity.fingerprint.clone(),
                outcome: SupplierCommandOutcome::Processing,
                connection_version: 3,
                job_id: job.map(str::to_owned),
                audit_event_id: identity.audit_id.clone(),
            },
        )
        .unwrap()
    }

    #[tokio::test]
    async fn replay_qualification_receipt_and_job_share_the_original_snapshot() {
        let identity = identity();
        for job in [Some("job"), None] {
            let port = Replay::new(true, Some(receipt(&identity, job)));
            let result = replay(&port, &identity, &mut SnapshotExecutor::default()).await.unwrap().unwrap();
            assert_eq!(result.operation_id, identity.receipt_id);
            assert_eq!(result.connection_version, 3);
            assert_eq!(result.job_no.as_deref(), job.map(|_| "JOB-1"));
            let expected = if job.is_some() {
                vec!["authorize", "receipt", "job_no"]
            } else {
                vec!["authorize", "receipt"]
            };
            assert_eq!(*port.calls.lock().unwrap(), expected);
        }
    }

    #[tokio::test]
    async fn denial_stops_before_receipt_and_payload_conflict_stops_before_job() {
        let identity = identity();
        let denied = Replay::new(false, Some(receipt(&identity, Some("job"))));
        assert!(matches!(
            replay(&denied, &identity, &mut SnapshotExecutor::default()).await,
            Err(Error::Forbidden(_))
        ));
        assert_eq!(*denied.calls.lock().unwrap(), ["authorize"]);

        let mut changed = receipt(&identity, Some("job"));
        changed.request_fingerprint = "different".into();
        let conflict = Replay::new(true, Some(changed));
        assert!(
            matches!(replay(&conflict, &identity, &mut SnapshotExecutor::default()).await, Err(Error::ConflictError(message)) if message == "这次操作已用于提交其他内容，请核对原操作结果后再发起新的操作")
        );
        assert_eq!(*conflict.calls.lock().unwrap(), ["authorize", "receipt"]);

        let absent = Replay::new(true, None);
        assert!(replay(&absent, &identity, &mut SnapshotExecutor::default()).await.unwrap().is_none());
        assert_eq!(*absent.calls.lock().unwrap(), ["authorize", "receipt"]);
    }

    #[tokio::test]
    async fn real_authorization_rejects_invalid_snapshot_or_qualification_before_database_reads() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap().database("replay");
        let port = MongoReplay {
            db,
            authorization: unavailable_authorization(),
            actor: AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin),
        };
        assert!(
            matches!(replay(&port, &identity(), &mut ReadExecutor::default()).await, Err(Error::ValidationError(message)) if message == "写入授权必须使用快照事务")
        );
        assert!(
            matches!(replay(&port, &identity(), &mut SnapshotExecutor::default()).await, Err(Error::Internal(message)) if message == "测试授权设施不可用")
        );
    }
}
