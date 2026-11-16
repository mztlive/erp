//! 调查回放先核验当前对象或精确任务，再沿同一快照读取回执和证据。
use std::sync::Arc;

use application_core::AuditActor;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::{internal, supplier_fulfillment};
use erp_supply::command_receipt::repository::{SupplyCommandReceiptExt, SupplyCommandReceiptReadExt};
use erp_supply::command_receipt::{SupplyCommandReceipt, SupplyCommandResult};
use erp_supply::entity::supplier_fulfillment::SupplierFulfillmentOrder;
use erp_supply::repository::SupplierFulfillmentExt;
use erp_supply::service::supplier_fulfillment::investigate::parse_investigation_evidence;
use erp_workflow::WorkItemExt;
use erp_workflow::repository::prelude::WorkItemRepositoryExt;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::super::{SupplierFulfillmentProcess, legacy, task_authorization};
use super::{InvestigationTaskContext, SupplierOrderInvestigationResultView, investigation_result};
use crate::supply_execution::receipt::{InvestigationReceipt, stable_digest};
use crate::{Error, Result};

/// 回放保留真实调用身份；旧请求版本仅用于匹配原回执，不代替当前任务版本。
struct InvestigationReplay {
    command_id: String,
    fingerprint: String,
    operation_id: String,
    task: Option<InvestigationTaskContext>,
    scope_id: String,
    actor: AuditActor,
    idempotency_key: String,
}

impl SupplierFulfillmentProcess {
    /// 在当前动作授权的原快照中恢复已提交调查结果。
    /// # 参数
    /// 命令身份、原载荷、操作号、任务或对象定位及真实操作人。
    /// # 返回
    /// 原成功结果；未命中回执时返回 None。
    /// # 错误
    /// 当前资格、目标、回执身份或证据失效时拒绝。
    pub(super) async fn replay_investigation(
        &self,
        command_id: &str,
        expected_fingerprint: &str,
        expected_operation_id: &str,
        task: Option<&InvestigationTaskContext>,
        scope_id: &str,
        identity: (&AuditActor, &str),
    ) -> Result<Option<SupplierOrderInvestigationResultView>> {
        let replay = InvestigationReplay {
            command_id: command_id.into(),
            fingerprint: expected_fingerprint.into(),
            operation_id: expected_operation_id.into(),
            task: task.cloned(),
            scope_id: scope_id.into(),
            actor: identity.0.clone(),
            idempotency_key: identity.1.into(),
        };
        let (db, authorization) = (self.db.clone(), self.identity.rbac.clone());
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { replay.load(&db, &authorization, executor).await })
            })
            .await
    }
}

impl InvestigationReplay {
    /// 当前资格与来源成立后才读取原回执，未命中时后续提交仍重新授权。
    async fn load(
        &self,
        db: &Database,
        authorization: &Arc<AuthorizationService>,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierOrderInvestigationResultView>> {
        let order = self.authorized_order(db, authorization, executor).await?;
        legacy::ensure(db, &order.base.id, executor).await?;
        let Some(stored) = db.supply_command_receipts().find_command(&self.command_id, executor).await?
        else {
            return Ok(None);
        };
        let receipt = self.receipt(stored, &order.base.id)?;
        let evidence = db
            .supplier_order_actions()
            .find_by_id(&receipt.evidence_id, executor)
            .await?
            .ok_or_else(|| Error::Internal("W26 调查幂等证据不存在".into()))?;
        let record = parse_investigation_evidence(&evidence)?;
        if evidence.supplier_fulfillment_order_id.as_ref() != order.base.id {
            return Err(Error::Internal("W26 调查幂等证据对象不一致".into()));
        }
        if record.operation_id() != self.operation_id {
            return Err(Error::ConflictError("请求标识已用于不同的调查命令".into()));
        }
        Ok(Some(investigation_result(
            order,
            evidence,
            record,
            self.task.as_ref().map(|task| task.work_item_id.as_str()),
            receipt.task_version,
        )))
    }

    /// 对象回放使用原动作范围；任务回放先取得完整资格，再校验当前精确任务。
    async fn authorized_order(
        &self,
        db: &Database,
        authorization: &Arc<AuthorizationService>,
        executor: &mut dyn Executor,
    ) -> Result<SupplierFulfillmentOrder> {
        let id = if let Some(task) = &self.task {
            authorization
                .require(
                    &self.actor,
                    &internal::SUPPLIER_FULFILLMENT_COMPLETED_RESULT_READ,
                    AccessTarget::Operation,
                    executor,
                )
                .await?;
            let item = db
                .work_items()
                .find_work_item(&task.work_item_id, executor)
                .await?
                .ok_or_else(|| Error::NotFound("调查任务不存在".into()))?;
            task_authorization::require_assignee(db, authorization, &self.actor, &item, executor).await?;
            item.business_object_id
        } else {
            authorization
                .require(
                    &self.actor,
                    &supplier_fulfillment::ORDER_INVESTIGATION,
                    AccessTarget::record(&self.scope_id)?,
                    executor,
                )
                .await?;
            self.scope_id.clone()
        };
        db.supplier_fulfillment_orders()
            .find_by_id(&id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("供应商订单不存在".into()))
    }

    /// 原操作人、目标、操作号及载荷均须精确匹配，不使用旧任务版本拒绝重放。
    fn receipt(&self, stored: SupplyCommandReceipt, order_id: &str) -> Result<InvestigationReceipt> {
        let action = if self.task.is_some() {
            "supplier_fulfillment.task_investigate"
        } else {
            "supplier_fulfillment.investigate"
        };
        stored.verify_identity(
            &self.command_id,
            self.actor.id(),
            action,
            &self.scope_id,
            &stable_digest(self.idempotency_key.trim()),
        )?;
        stored.verify(&self.fingerprint, None, "请求标识已用于不同的调查命令")?;
        if stored.resource_id != order_id {
            return Err(Error::Internal("W26 调查幂等收据对象不一致".into()));
        }
        let SupplyCommandResult::Investigation(receipt) = stored.result else {
            return Err(Error::Internal("W26 调查幂等收据身份非法".into()));
        };
        if self.task.is_some() != receipt.task_version.is_some() {
            return Err(Error::ConflictError("请求标识已用于不同的调查入口".into()));
        }
        Ok(receipt)
    }
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use mongodb::Client;
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    fn fixture(task: bool) -> (InvestigationReplay, SupplyCommandReceipt) {
        let scope = if task { "task" } else { "order" };
        let replay = InvestigationReplay {
            command_id: "command".into(),
            fingerprint: stable_digest("original-payload"),
            operation_id: "operation".into(),
            task: task.then(|| InvestigationTaskContext {
                work_item_id: "task".into(),
                expected_task_version: 1,
                expected_subject_version: "old-order-version".into(),
            }),
            scope_id: scope.into(),
            actor: AuditActor::new("actor".into(), "operator".into(), AccountKind::Admin),
            idempotency_key: "key".into(),
        };
        let stored = SupplyCommandReceipt {
            base: BaseModel::new("command".into()),
            schema_version: 1,
            actor_id: "actor".into(),
            action: if task {
                "supplier_fulfillment.task_investigate"
            } else {
                "supplier_fulfillment.investigate"
            }
            .into(),
            resource_type: "SUPPLIER_FULFILLMENT_ORDER".into(),
            resource_id: "order".into(),
            scope_id: scope.into(),
            idempotency_key_hash: stable_digest("key"),
            fingerprint: stable_digest("original-payload"),
            fingerprint_algorithm: "sha256-serialized-v1".into(),
            result: SupplyCommandResult::Investigation(InvestigationReceipt {
                evidence_id: "evidence".into(),
                order_version: 7,
                task_version: task.then_some(9),
            }),
            audit_event_id: "audit".into(),
        };
        (replay, stored)
    }

    #[tokio::test]
    async fn current_authorization_precedes_task_order_receipt_and_evidence_reads_for_each_entry() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("investigation_replay");
        let authorization = unavailable_authorization();
        for task in [false, true] {
            let (replay, _) = fixture(task);
            assert!(matches!(
                replay.load(&db, &authorization, &mut SnapshotExecutor::default()).await,
                Err(Error::Internal(message)) if message == "测试授权设施不可用"
            ));
            assert!(matches!(
                replay.load(&db, &authorization, &mut ReadExecutor::default()).await,
                Err(Error::ValidationError(_))
            ));
        }
    }

    #[test]
    fn original_receipt_replays_without_old_task_version_but_rejects_other_target_or_payload() {
        for task in [false, true] {
            let (replay, stored) = fixture(task);
            let receipt = replay.receipt(stored, "order").unwrap();
            assert_eq!(receipt.evidence_id, "evidence");
            assert_eq!(receipt.task_version, task.then_some(9));
            let (replay, stored) = fixture(task);
            assert!(matches!(replay.receipt(stored, "other-order"), Err(Error::Internal(_))));
            let (mut replay, stored) = fixture(task);
            replay.fingerprint = stable_digest("other-payload");
            assert!(matches!(replay.receipt(stored, "order"), Err(Error::ConflictError(_))));
        }
    }
}
