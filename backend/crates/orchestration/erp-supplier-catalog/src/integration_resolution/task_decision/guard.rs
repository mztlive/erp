//! 正式任务绑定、权限前置与审计回执重放。
use application_core::AuditActor;
use async_trait::async_trait;
use erp_integration::dto::{IntegrationItemType, IntegrationNonTerminalTaskAction, PreparedWorkItemTarget};
use erp_integration::entity::integration_ops::{ErrorClass, IntegrationCommandIdentity};
use erp_integration::repository::IntegrationOpsExt;
use erp_integration::service::task_decision::guard::load_error_task_for_association;
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::{WorkItem, WorkItemStatus, WorkItemType};
use mongodb::Database;
use persistence_core::Executor;
use serde::Serialize;

use crate::wiring::CatalogWorkItems;
use crate::{Error, Result};

// 回放使用当前任务和原快照；不能将任务办理资格降为普通读取资格。
pub(super) async fn authorize_replay_task(
    db: &Database,
    work_items: &dyn CatalogWorkItems,
    item: &WorkItem,
    action: &IntegrationNonTerminalTaskAction,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    authorize_replay_with_source(&MongoAssociation(db), work_items, item, action, actor, executor).await
}

// 先验证完整命令资格，再验证当前真实来源；回放不套用旧业务状态或版本。
async fn authorize_replay_with_source(
    source: &impl AssociationSource,
    work_items: &dyn CatalogWorkItems,
    item: &WorkItem,
    action: &IntegrationNonTerminalTaskAction,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
        return Err(Error::ValidationError("任务回放授权必须使用快照事务".into()));
    }
    work_items.require_result_access(actor, &item.base.id, executor).await?;
    ensure_association(source, item, action, executor).await?;
    Ok(())
}

// 序列化 W29 命令并构造领域幂等身份。
//
// - `actor_id`：命令操作人
// - `action`：稳定动作名
// - `resource_type`：审计资源类型
// - `resource_id`：审计资源 ID
// - `idempotency_key`：客户端幂等键
// - `command`：完整强命令载荷
//
// 返回不暴露原始幂等键的领域命令身份。
//
// 命令无法序列化时返回内部错误。
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

// 加载与命令绑定的正式任务，并校验责任归属与版本。
//
// 任务版本与业务主题版本取自 Prepared 目标（DTO 层单次解析），此处只做
// typed 比较，不再解析版本字符串。
//
// - `work_items`：已装配的正式任务授权能力。
// - `target`：已解析的任务与主题版本。
// - `action`：非终态动作，用于核对对象关联。
// - `actor`：当前账号。
// - `executor`：读取所用执行器。
//
// 仍开放且归属当前人的正式任务。
//
// 任务不存在返回 `NotFound`；版本变化或任务不再开放返回 `ConflictError`；不是当前责任人返回 `Forbidden`；关联或仓储读取失败返回对应错误。
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
    authorize_bound_work_item(work_items, &item, target, actor, executor).await?;
    ensure_work_item_association(db, &item, action, executor).await?;
    Ok(item)
}

// 当前完整任务资格先成立，再向调用方报告版本或业务状态冲突。
async fn authorize_bound_work_item(
    work_items: &dyn CatalogWorkItems,
    item: &WorkItem,
    target: &PreparedWorkItemTarget,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    work_items.require_result_access(actor, &item.base.id, executor).await?;
    ensure_work_item_version(item, target.task_version, &target.subject_version)?;
    ensure_work_item_open(item)
}

// 校验任务与业务主题版本与调用方冻结的 typed 版本一致（无二次解析）。
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

/// 当前正式关联只核对真实存在性和身份，不核对业务状态和冻结版本。
struct AssociationFact {
    id: String,
    deleted: bool,
    task_type: WorkItemType,
    object_type: &'static str,
}

#[async_trait]
trait AssociationSource: Sync {
    async fn load(
        &self,
        action: &IntegrationNonTerminalTaskAction,
        executor: &mut dyn Executor,
    ) -> Result<AssociationFact>;
}

struct MongoAssociation<'a>(&'a Database);

#[async_trait]
impl AssociationSource for MongoAssociation<'_> {
    async fn load(
        &self,
        action: &IntegrationNonTerminalTaskAction,
        executor: &mut dyn Executor,
    ) -> Result<AssociationFact> {
        match action.item_type {
            IntegrationItemType::ErrorTask => {
                let task = load_error_task_for_association(self.0, &action.item_id, executor).await?;
                Ok(AssociationFact {
                    deleted: task.base.is_deleted(),
                    id: task.base.id,
                    task_type: if task.error_class == ErrorClass::ResultUnknown {
                        WorkItemType::IntegrationResultUnknown
                    } else {
                        WorkItemType::BusinessException
                    },
                    object_type: "integration_error_task",
                })
            },
            IntegrationItemType::ReconciliationDifference => {
                let difference = self
                    .0
                    .reconciliation_differences()
                    .find_by_id(&action.item_id, executor)
                    .await?
                    .ok_or_else(|| Error::NotFound("对账差异不存在".into()))?;
                Ok(AssociationFact {
                    deleted: difference.base.is_deleted(),
                    id: difference.base.id,
                    task_type: WorkItemType::BusinessException,
                    object_type: "reconciliation_difference",
                })
            },
        }
    }
}

async fn ensure_work_item_association(
    db: &Database,
    item: &WorkItem,
    action: &IntegrationNonTerminalTaskAction,
    executor: &mut dyn Executor,
) -> Result<()> {
    ensure_association(&MongoAssociation(db), item, action, executor).await
}

async fn ensure_association(
    source: &impl AssociationSource,
    item: &WorkItem,
    action: &IntegrationNonTerminalTaskAction,
    executor: &mut dyn Executor,
) -> Result<()> {
    let fact = source.load(action, executor).await?;
    if fact.deleted {
        return Err(Error::NotFound("任务业务项不存在".into()));
    }
    if item.work_item_type != fact.task_type
        || item.business_object_type != fact.object_type
        || item.business_object_id != action.item_id
        || fact.id != action.item_id
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
    use persistence_core::testing::SnapshotExecutor;
    use persistence_core::{ExecutionBoundary, ReadExecutor};

    use super::*;

    struct ExistingDifference;

    #[async_trait]
    impl AssociationSource for ExistingDifference {
        async fn load(
            &self,
            action: &IntegrationNonTerminalTaskAction,
            executor: &mut dyn Executor,
        ) -> Result<AssociationFact> {
            assert!(executor.boundary().unwrap().is_snapshot());
            assert_eq!(action.item_id, "difference");
            Ok(AssociationFact {
                id: "difference".into(),
                deleted: false,
                task_type: WorkItemType::BusinessException,
                object_type: "reconciliation_difference",
            })
        }
    }

    struct DifferenceSource {
        boundary: ExecutionBoundary,
        id: Option<&'static str>,
        deleted: bool,
    }

    #[async_trait]
    impl AssociationSource for DifferenceSource {
        async fn load(
            &self,
            _: &IntegrationNonTerminalTaskAction,
            executor: &mut dyn Executor,
        ) -> Result<AssociationFact> {
            assert!(self.boundary.is_current(executor));
            let id = self.id.ok_or_else(|| Error::NotFound("对账差异不存在".into()))?;
            Ok(AssociationFact {
                id: id.into(),
                deleted: self.deleted,
                task_type: WorkItemType::BusinessException,
                object_type: "reconciliation_difference",
            })
        }
    }

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
        async fn require_result_access(
            &self,
            actor: &AuditActor,
            id: &str,
            executor: &mut dyn Executor,
        ) -> WorkflowResult<()> {
            self.visit(executor, "result_access");
            assert_eq!((actor.id(), id), ("actor", "work-item"));
            if self.denied {
                return Err(WorkflowError::Forbidden("当前责任已撤销".into()));
            }
            Ok(())
        }
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

    #[tokio::test]
    async fn bound_task_authorization_precedes_version_and_open_state_errors() {
        let actor = AuditActor::new("actor".into(), "operator".into(), AccountKind::Admin);
        for denied in [false, true] {
            for (stale, closed) in [(false, false), (true, false), (false, true)] {
                let mut executor = SnapshotExecutor::default();
                let tasks = Tasks {
                    executor: &mut executor as *mut SnapshotExecutor as usize,
                    denied,
                    calls: Mutex::default(),
                };
                let mut item = task();
                if closed {
                    item.status = WorkItemStatus::Completed;
                }
                let target = PreparedWorkItemTarget {
                    work_item_id: item.base.id.clone(),
                    task_version: item.base.version + u64::from(stale),
                    subject_version: item.subject_version.clone(),
                };
                let result = authorize_bound_work_item(&tasks, &item, &target, &actor, &mut executor).await;
                if denied {
                    assert!(matches!(result, Err(Error::Forbidden(message)) if message == "当前责任已撤销"));
                } else if stale || closed {
                    assert!(matches!(result, Err(Error::ConflictError(_))));
                } else {
                    result.unwrap();
                }
                assert_eq!(*tasks.calls.lock().unwrap(), ["result_access"]);
            }
        }
    }

    // 验证：当前分配、真实主题和完整办理资格沿同一快照通过；撤权停止后续检查。
    #[tokio::test]
    async fn replay_authorization_reuses_snapshot_and_stops_on_revocation() {
        let actor = AuditActor::new("actor".into(), "operator".into(), AccountKind::Admin);
        for denied in [false, true] {
            let mut executor = SnapshotExecutor::default();
            let tasks = Tasks {
                executor: &mut executor as *mut SnapshotExecutor as usize,
                denied,
                calls: Mutex::new(vec![]),
            };
            let result = authorize_replay_with_source(
                &ExistingDifference,
                &tasks,
                &task(),
                &action(),
                &actor,
                &mut executor,
            )
            .await;
            if denied {
                assert!(matches!(result, Err(Error::Forbidden(message)) if message == "当前责任已撤销"));
                assert_eq!(*tasks.calls.lock().unwrap(), vec!["result_access"]);
            } else {
                result.unwrap();
                assert_eq!(*tasks.calls.lock().unwrap(), vec!["result_access"]);
            }
        }
    }

    // 验证：已完成任务保留当前责任下的结果回放；普通执行器和错误关联仍拒绝。
    #[tokio::test]
    async fn completed_replay_keeps_qualification_and_rejects_invalid_boundary_or_association() {
        let actor = AuditActor::new("actor".into(), "operator".into(), AccountKind::Admin);
        let mut executor = ReadExecutor::default();
        let tasks = Tasks { executor: 0, denied: false, calls: Mutex::new(vec![]) };
        let error = authorize_replay_with_source(
            &ExistingDifference,
            &tasks,
            &task(),
            &action(),
            &actor,
            &mut executor,
        )
        .await
        .unwrap_err();
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
        authorize_replay_with_source(&ExistingDifference, &tasks, &closed, &action(), &actor, &mut executor)
            .await
            .unwrap();
        assert_eq!(*tasks.calls.lock().unwrap(), vec!["result_access"]);

        tasks.calls.lock().unwrap().clear();
        let mut changed = task();
        changed.business_object_id = "other-difference".into();
        assert!(matches!(
            authorize_replay_with_source(
                &ExistingDifference,
                &tasks,
                &changed,
                &action(),
                &actor,
                &mut executor
            )
            .await,
            Err(Error::ConflictError(_))
        ));
        assert_eq!(*tasks.calls.lock().unwrap(), vec!["result_access"]);
    }

    // 验证：完成后的稳定结果仍要求同快照中的真实未删除来源和精确来源身份。
    #[tokio::test]
    async fn completed_replay_requires_the_real_current_difference_without_old_version_checks() {
        let actor = AuditActor::new("actor".into(), "operator".into(), AccountKind::Admin);
        for (id, deleted) in
            [(Some("difference"), false), (None, false), (Some("difference"), true), (Some("other"), false)]
        {
            let mut executor = SnapshotExecutor::default();
            let source = DifferenceSource { boundary: executor.boundary().unwrap(), id, deleted };
            let tasks = Tasks {
                executor: &mut executor as *mut SnapshotExecutor as usize,
                denied: false,
                calls: Mutex::default(),
            };
            let mut completed = task();
            completed.status = WorkItemStatus::Completed;
            completed.subject_version = "original-frozen-version".into();
            let result =
                authorize_replay_with_source(&source, &tasks, &completed, &action(), &actor, &mut executor)
                    .await;
            match (id, deleted) {
                (Some("difference"), false) => result.unwrap(),
                (None, _) | (_, true) => assert!(matches!(result, Err(Error::NotFound(_)))),
                _ => assert!(matches!(result, Err(Error::ConflictError(_)))),
            }
            assert_eq!(*tasks.calls.lock().unwrap(), ["result_access"]);
        }
    }
}
