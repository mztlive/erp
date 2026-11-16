//! 回放先在原快照核验完整命令资格及当前责任，再读取稳定收据。
use std::marker::PhantomData;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_identity::AccessTarget;
use erp_integration::dto::IntegrationNonTerminalTaskAction;
use erp_integration::entity::integration_ops::{IntegrationCommandIdentity, IntegrationReceiptPayload};
use erp_integration::repository::{IntegrationCommandExt, IntegrationCommandRepositoryExt};
use erp_permissions::integration_ops;
use erp_workflow::WorkItemExt;
use persistence_core::{Executor, Transactional};

use super::super::IntegrationResolutionProcess;
use super::direct::ensure_no_work_item;
use super::guard::authorize_replay_task;
use super::{TASK_ACTION_AUDIT, TASK_COMPLETION_AUDIT};
use crate::{Error, Result};

/// 回放目标只由当前强类型业务命令确定。
pub(super) enum ReplayTarget {
    Task(IntegrationNonTerminalTaskAction),
    Difference,
}

impl IntegrationResolutionProcess {
    /// 原命令资格、真实目标及回执必须共用同一事务快照。
    pub(super) async fn replay_receipt<T: IntegrationReceiptPayload + 'static>(
        &self,
        receipt: &IntegrationCommandIdentity,
        actor: &AuditActor,
        target: ReplayTarget,
    ) -> Result<Option<T>> {
        let port = MongoReplay::<T> {
            process: self.clone(),
            receipt: receipt.clone(),
            actor: actor.clone(),
            target,
            payload: PhantomData,
        };
        self.db
            .client()
            .with_transaction(move |executor| Box::pin(async move { replay(&port, executor).await }))
            .await
    }
}

#[async_trait]
trait ReplayPort: Sync {
    type Payload: Send;

    async fn authorize(&self, executor: &mut dyn Executor) -> Result<()>;
    async fn receipt(&self, executor: &mut dyn Executor) -> Result<Option<Self::Payload>>;
}

/// 未授权、无快照或当前责任变化时，禁止读取收据并返回旧结果。
async fn replay<P: ReplayPort>(port: &P, executor: &mut dyn Executor) -> Result<Option<P::Payload>> {
    if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
        return Err(Error::ValidationError("命令回放必须使用快照事务".into()));
    }
    port.authorize(executor).await?;
    port.receipt(executor).await
}

struct MongoReplay<T> {
    process: IntegrationResolutionProcess,
    receipt: IntegrationCommandIdentity,
    actor: AuditActor,
    target: ReplayTarget,
    payload: PhantomData<T>,
}

impl<T> MongoReplay<T> {
    /// 已完成结果沿用完整执行资格及当前责任，不套用原开放状态和冻结业务版本。
    async fn authorize_task(
        &self,
        action: &IntegrationNonTerminalTaskAction,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let permission = match self.receipt.action() {
            TASK_ACTION_AUDIT => integration_ops::INTEGRATION_TASK_ACTION,
            TASK_COMPLETION_AUDIT => integration_ops::INTEGRATION_TASK_COMPLETION,
            _ => return Err(Error::ValidationError("未知任务回放命令".into())),
        };
        self.process
            .authorization
            .require(&self.actor, &permission, AccessTarget::Operation, executor)
            .await?;
        let item = self
            .process
            .db
            .work_items()
            .find_by_id(self.receipt.resource_id(), executor)
            .await?
            .ok_or_else(|| Error::Internal("W29 命令回执对应任务缺失".into()))?;
        let work_items = (self.process.work_items_factory)();
        authorize_replay_task(&self.process.db, work_items.as_ref(), &item, action, &self.actor, executor)
            .await
    }
}

#[async_trait]
impl<T: IntegrationReceiptPayload> ReplayPort for MongoReplay<T> {
    type Payload = T;

    async fn authorize(&self, executor: &mut dyn Executor) -> Result<()> {
        match &self.target {
            ReplayTarget::Task(action) => self.authorize_task(action, executor).await,
            ReplayTarget::Difference => {
                self.process
                    .data_scope
                    .require_decide(&self.actor, self.receipt.resource_id(), executor)
                    .await?;
                ensure_no_work_item(&self.process.db, self.receipt.resource_id(), executor).await
            },
        }
    }

    async fn receipt(&self, executor: &mut dyn Executor) -> Result<Option<T>> {
        let Some(committed) = self
            .process
            .db
            .integration_command_receipts()
            .find_command(self.receipt.receipt_id(), executor)
            .await?
        else {
            return Ok(None);
        };
        Ok(Some(committed.recover(&self.receipt, self.actor.id())?))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use erp_integration::dto::{IntegrationItemType, IntegrationTaskActionKind};
    use persistence_core::testing::SnapshotExecutor;
    use persistence_core::{ExecutionBoundary, ReadExecutor};

    use super::super::super::testing::{actor, denied, process};
    use super::super::guard::command_identity;
    use super::*;

    struct Replay {
        allowed: bool,
        payload: Option<&'static str>,
        calls: Mutex<Vec<&'static str>>,
        boundary: Mutex<Option<ExecutionBoundary>>,
    }

    #[async_trait]
    impl ReplayPort for Replay {
        type Payload = &'static str;

        async fn authorize(&self, executor: &mut dyn Executor) -> Result<()> {
            self.calls.lock().unwrap().push("authorize");
            if !self.allowed {
                return Err(Error::Forbidden("当前命令或责任已撤销".into()));
            }
            *self.boundary.lock().unwrap() = executor.boundary();
            Ok(())
        }

        async fn receipt(&self, executor: &mut dyn Executor) -> Result<Option<Self::Payload>> {
            assert!(self.boundary.lock().unwrap().as_ref().unwrap().is_current(executor));
            self.calls.lock().unwrap().push("receipt");
            Ok(self.payload)
        }
    }

    #[tokio::test]
    async fn replay_uses_one_snapshot_and_returns_only_currently_authorized_receipts() {
        for payload in [Some("original-result"), None] {
            let port = Replay { allowed: true, payload, calls: Mutex::default(), boundary: Mutex::default() };
            assert_eq!(replay(&port, &mut SnapshotExecutor::default()).await.unwrap(), payload);
            assert_eq!(*port.calls.lock().unwrap(), ["authorize", "receipt"]);
        }
        let port = Replay {
            allowed: false,
            payload: Some("old-result"),
            calls: Mutex::default(),
            boundary: Mutex::default(),
        };
        assert!(matches!(replay(&port, &mut SnapshotExecutor::default()).await, Err(Error::Forbidden(_))));
        assert_eq!(*port.calls.lock().unwrap(), ["authorize"]);
        port.calls.lock().unwrap().clear();
        assert!(matches!(replay(&port, &mut ReadExecutor::default()).await, Err(Error::ValidationError(_))));
        assert!(port.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn task_replay_checks_each_original_entry_before_reading_task_or_receipt() {
        let process = process().await;
        let action = IntegrationNonTerminalTaskAction {
            item_type: IntegrationItemType::ReconciliationDifference,
            item_id: "difference".into(),
            kind: IntegrationTaskActionKind::AddEvidence,
            operation_id: "operation".into(),
            reason_code: None,
            comment: None,
            evidence_refs: vec![],
        };
        for entry in [TASK_ACTION_AUDIT, TASK_COMPLETION_AUDIT] {
            let port = MongoReplay::<String> {
                process: process.clone(),
                receipt: command_identity("actor", entry, "work_item", "task", "key", &action).unwrap(),
                actor: actor(),
                target: ReplayTarget::Task(action.clone()),
                payload: PhantomData,
            };
            denied::<()>(port.authorize_task(&action, &mut SnapshotExecutor::default()).await);
        }
    }
}
