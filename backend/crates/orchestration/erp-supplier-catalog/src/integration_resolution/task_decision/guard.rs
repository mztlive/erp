//! 正式任务绑定、权限前置与审计回执重放。
use application_core::AuditActor;
use erp_integration::dto::{IntegrationItemType, IntegrationNonTerminalTaskAction, PreparedWorkItemTarget};
use erp_integration::entity::integration_ops::{IntegrationCommandIdentity, IntegrationReceiptPayload};
use erp_integration::repository::{IntegrationCommandExt, IntegrationCommandRepositoryExt};
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::{WorkItem, WorkItemStatus, WorkItemType};
use mongodb::Database;
use persistence_core::{Executor, ReadExecutor, Transactional};
use serde::Serialize;

use super::super::IntegrationResolutionProcess;
use crate::wiring::CatalogWorkItems;
use crate::{Error, Result};

impl IntegrationResolutionProcess {
    /// 重放仍校验当前账号、责任、完整执行权限和对象绑定，不再套原写版本。
    ///
    /// # 参数
    /// * `work_item_id` - 回执绑定的正式任务。
    /// * `action` - 非终态动作，用于核对对象关联。
    /// * `actor` - 当前认证账号。
    ///
    /// # 返回
    /// 无返回值。校验通过后可以继续重放。
    ///
    /// # 错误
    /// 任务缺失时返回内部错误；当前账号不是责任人时返回 `Forbidden`；对象关联或执行权限不通过，或仓储读取失败时返回对应错误。
    pub(super) async fn ensure_replay_task_access(
        &self,
        work_item_id: &str,
        action: &IntegrationNonTerminalTaskAction,
        actor: &AuditActor,
    ) -> Result<()> {
        let db = self.db.clone();
        let work_items = (self.work_items_factory)();
        let id = work_item_id.to_owned();
        let action = action.clone();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let item = db
                        .work_items()
                        .find_by_id(&id, executor)
                        .await?
                        .ok_or_else(|| Error::Internal("W29 命令回执对应任务缺失".into()))?;
                    authorize_replay_task(&db, work_items.as_ref(), &item, &action, &actor, executor).await
                })
            })
            .await
    }

    /// 按命令身份读取已提交回执，并按当前操作人恢复载荷。
    ///
    /// # 参数
    /// * `receipt` - 领域命令身份。
    /// * `actor` - 当前认证账号。
    ///
    /// # 返回
    /// 已提交回执的载荷；没有回执时为 `None`。
    ///
    /// # 错误
    /// 仓储读取失败，或回执无法按该命令身份和操作人恢复时返回对应错误。
    pub(super) async fn replay_receipt<T: IntegrationReceiptPayload>(
        &self,
        receipt: &IntegrationCommandIdentity,
        actor: &AuditActor,
    ) -> Result<Option<T>> {
        let Some(committed) = self
            .db
            .integration_command_receipts()
            .find_command(receipt.receipt_id(), &mut ReadExecutor::default())
            .await?
        else {
            return Ok(None);
        };
        Ok(Some(committed.recover(receipt, actor.id())?))
    }
}

/// 回放使用当前任务和原快照；不能将任务办理资格降为普通读取资格。
async fn authorize_replay_task(
    db: &Database,
    work_items: &dyn CatalogWorkItems,
    item: &WorkItem,
    action: &IntegrationNonTerminalTaskAction,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
        return Err(Error::ValidationError("任务回放授权必须使用快照事务".into()));
    }
    if item.status != WorkItemStatus::Open {
        return Err(Error::Forbidden("当前账号不是任务的当前责任人".into()));
    }
    work_items.require_assigned(actor, &item.base.id, executor).await?;
    ensure_work_item_association(db, item, action, executor).await?;
    work_items.ensure_domain_decision_access(actor, item, executor).await?;
    Ok(())
}

/// 序列化 W29 命令并构造领域幂等身份。
///
/// # 参数
/// * `actor_id` - 命令操作人
/// * `action` - 稳定动作名
/// * `resource_type` - 审计资源类型
/// * `resource_id` - 审计资源 ID
/// * `idempotency_key` - 客户端幂等键
/// * `command` - 完整强命令载荷
///
/// # 返回
/// 返回不暴露原始幂等键的领域命令身份。
///
/// # 错误
/// 命令无法序列化时返回内部错误。
pub(super) fn command_identity<T: Serialize>(
    actor_id: &str,
    action: &str,
    resource_type: &str,
    resource_id: &str,
    idempotency_key: &str,
    command: &T,
) -> Result<IntegrationCommandIdentity> {
    let payload =
        serde_json::to_vec(command).map_err(|_| Error::Internal("W29 命令无法形成幂等指纹".to_string()))?;
    Ok(IntegrationCommandIdentity::new(
        actor_id,
        action,
        resource_type,
        resource_id,
        idempotency_key,
        &payload,
    ))
}

/// 加载与命令绑定的正式任务，并校验责任归属与版本。
///
/// 任务版本与业务主题版本取自 Prepared 目标（DTO 层单次解析），此处只做
/// typed 比较，不再解析版本字符串。
///
/// # 参数
/// * `db` - 数据库。
/// * `work_items` - 已装配的正式任务授权能力。
/// * `target` - 已解析的任务与主题版本。
/// * `action` - 非终态动作，用于核对对象关联。
/// * `actor` - 当前账号。
/// * `executor` - 读取所用执行器。
///
/// # 返回
/// 仍开放且归属当前人的正式任务。
///
/// # 错误
/// 任务不存在返回 `NotFound`；版本变化或任务不再开放返回 `ConflictError`；不是当前责任人返回 `Forbidden`；关联或仓储读取失败返回对应错误。
pub(super) async fn load_bound_work_item(
    db: &Database,
    work_items: &dyn CatalogWorkItems,
    target: &PreparedWorkItemTarget,
    action: &IntegrationNonTerminalTaskAction,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<WorkItem> {
    let item = db
        .work_items()
        .find_by_id(&target.work_item_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("正式任务不存在".to_string()))?;
    ensure_work_item_version(&item, target.task_version, &target.subject_version)?;
    ensure_work_item_open(&item)?;
    work_items.require_assigned(actor, &item.base.id, executor).await?;
    ensure_work_item_association(db, &item, action, executor).await?;
    Ok(item)
}

/// 校验任务与业务主题版本与调用方冻结的 typed 版本一致（无二次解析）。
fn ensure_work_item_version(item: &WorkItem, task_version: u64, subject_version: &str) -> Result<()> {
    if item.base.version != task_version || item.subject_version != subject_version {
        return Err(Error::ConflictError("任务或业务主题版本已变化，请刷新后重试".to_string()));
    }
    Ok(())
}

fn ensure_work_item_open(item: &WorkItem) -> Result<()> {
    if item.status != WorkItemStatus::Open {
        return Err(Error::ConflictError("任务已不再开放".to_string()));
    }
    Ok(())
}

async fn ensure_work_item_association(
    db: &Database,
    item: &WorkItem,
    action: &IntegrationNonTerminalTaskAction,
    executor: &mut dyn Executor,
) -> Result<()> {
    let expected = match action.item_type {
        IntegrationItemType::ErrorTask => {
            let task = erp_integration::service::task_decision::guard::load_error_task_for_association(
                db,
                &action.item_id,
                executor,
            )
            .await?;
            (
                if task.error_class == erp_integration::entity::integration_ops::ErrorClass::ResultUnknown {
                    WorkItemType::IntegrationResultUnknown
                } else {
                    WorkItemType::BusinessException
                },
                "integration_error_task",
            )
        },
        IntegrationItemType::ReconciliationDifference => {
            (WorkItemType::BusinessException, "reconciliation_difference")
        },
    };
    if item.work_item_type != expected.0
        || item.business_object_type != expected.1
        || item.business_object_id != action.item_id
    {
        return Err(Error::ConflictError("任务与业务项的正式关联不一致".to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod replay_tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use erp_core::AccountKind;
    use erp_core::ids::WorkItemId;
    use erp_integration::dto::IntegrationTaskActionKind;
    use erp_workflow::entity::work_item::{
        AssignmentSource, SupplierPortalReviewIdentity, SupplierPortalReviewTaskData, WorkItemData,
        WorkItemPriority,
    };
    use erp_workflow::{Error as WorkflowError, Result as WorkflowResult};
    use mongodb::Client;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    struct Tasks {
        executor: usize,
        denied: bool,
        calls: Mutex<Vec<&'static str>>,
    }

    impl Tasks {
        fn visit(&self, executor: &mut dyn Executor, step: &'static str) {
            assert_eq!(executor as *mut dyn Executor as *mut () as usize, self.executor);
            assert!(executor.boundary().unwrap().is_snapshot());
            self.calls.lock().unwrap().push(step);
        }
    }

    #[async_trait]
    impl CatalogWorkItems for Tasks {
        async fn require_assigned(
            &self,
            actor: &AuditActor,
            id: &str,
            executor: &mut dyn Executor,
        ) -> WorkflowResult<()> {
            self.visit(executor, "assigned");
            assert_eq!((actor.id(), id), ("actor", "work-item"));
            if self.denied {
                return Err(WorkflowError::Forbidden("当前责任已撤销".into()));
            }
            Ok(())
        }

        async fn ensure_domain_decision_access(
            &self,
            actor: &AuditActor,
            item: &WorkItem,
            executor: &mut dyn Executor,
        ) -> WorkflowResult<()> {
            self.visit(executor, "decision");
            assert_eq!(actor.id(), "actor");
            assert_eq!(item.business_object_id, "difference");
            Ok(())
        }

        async fn create_supplier_portal_review(
            &self,
            _: WorkItemId,
            _: SupplierPortalReviewTaskData,
            _: &mut dyn Executor,
        ) -> WorkflowResult<WorkItem> {
            unreachable!("回放不创建任务")
        }

        async fn validate_supplier_portal_review(
            &self,
            _: SupplierPortalReviewIdentity<'_>,
            _: &AuditActor,
            _: &mut dyn Executor,
        ) -> WorkflowResult<WorkItem> {
            unreachable!("集成回放不读取门户申请")
        }

        async fn complete_supplier_portal_review(
            &self,
            _: SupplierPortalReviewIdentity<'_>,
            _: &AuditActor,
            _: &mut dyn Executor,
        ) -> WorkflowResult<WorkItem> {
            unreachable!("回放不完成任务")
        }

        async fn withdraw_supplier_portal_review(
            &self,
            _: SupplierPortalReviewIdentity<'_>,
            _: &str,
            _: &str,
            _: &mut dyn Executor,
        ) -> WorkflowResult<WorkItem> {
            unreachable!("回放不撤回任务")
        }
    }

    fn task() -> WorkItem {
        WorkItem::new(
            WorkItemId::new("work-item"),
            WorkItemData {
                work_item_type: WorkItemType::BusinessException,
                business_object_type: "reconciliation_difference".into(),
                business_object_id: "difference".into(),
                subject_version: "1".into(),
                owner_role: "finance".into(),
                owner_organization_id: "org".into(),
                owner_user_id: "actor".into(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: None,
                impact_summary: None,
            },
        )
        .unwrap()
    }

    fn action() -> IntegrationNonTerminalTaskAction {
        IntegrationNonTerminalTaskAction {
            item_type: IntegrationItemType::ReconciliationDifference,
            item_id: "difference".into(),
            kind: IntegrationTaskActionKind::AddEvidence,
            operation_id: "operation".into(),
            reason_code: None,
            comment: None,
            evidence_refs: vec![],
        }
    }

    /// 当前分配、真实主题和完整办理资格沿同一快照通过；撤权停止后续检查。
    #[tokio::test]
    async fn replay_authorization_reuses_snapshot_and_stops_on_revocation() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap().database("replay_unit");
        let actor = AuditActor::new("actor".into(), "operator".into(), AccountKind::Admin);
        for denied in [false, true] {
            let mut executor = SnapshotExecutor::default();
            let tasks = Tasks {
                executor: &mut executor as *mut SnapshotExecutor as usize,
                denied,
                calls: Mutex::new(vec![]),
            };
            let result = authorize_replay_task(&db, &tasks, &task(), &action(), &actor, &mut executor).await;
            if denied {
                assert!(matches!(result, Err(Error::Forbidden(message)) if message == "当前责任已撤销"));
                assert_eq!(*tasks.calls.lock().unwrap(), vec!["assigned"]);
            } else {
                result.unwrap();
                assert_eq!(*tasks.calls.lock().unwrap(), vec!["assigned", "decision"]);
            }
        }
    }

    /// 普通读取执行器不能办理精确任务，非开放任务不能回放旧资格。
    #[tokio::test]
    async fn replay_rejects_non_snapshot_closed_task_or_changed_business_association() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap().database("replay_guard");
        let actor = AuditActor::new("actor".into(), "operator".into(), AccountKind::Admin);
        let mut executor = ReadExecutor::default();
        let tasks = Tasks { executor: 0, denied: false, calls: Mutex::new(vec![]) };
        let error =
            authorize_replay_task(&db, &tasks, &task(), &action(), &actor, &mut executor).await.unwrap_err();
        assert!(
            matches!(error, Error::ValidationError(message) if message == "任务回放授权必须使用快照事务")
        );
        assert!(tasks.calls.lock().unwrap().is_empty());
        let mut executor = SnapshotExecutor::default();
        let tasks = Tasks {
            executor: &mut executor as *mut SnapshotExecutor as usize,
            denied: false,
            calls: Mutex::new(vec![]),
        };
        let mut closed = task();
        closed.status = WorkItemStatus::Completed;
        assert!(matches!(
            authorize_replay_task(&db, &tasks, &closed, &action(), &actor, &mut executor).await,
            Err(Error::Forbidden(_))
        ));
        assert!(tasks.calls.lock().unwrap().is_empty());
        let mut changed = task();
        changed.business_object_id = "other-difference".into();
        assert!(matches!(
            authorize_replay_task(&db, &tasks, &changed, &action(), &actor, &mut executor).await,
            Err(Error::ConflictError(_))
        ));
        assert_eq!(*tasks.calls.lock().unwrap(), vec!["assigned"]);
    }
}
