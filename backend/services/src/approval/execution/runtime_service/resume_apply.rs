//! 原审批人恢复提交、回放与事务写入。

use bpm::engine::{CommitRequired, Eligibility};
use bpm::ids::{ApprovalCommandReceiptId, ApprovalNodeExecutionId, ApprovalProcessInstanceId};
use bpm::model::Timestamp;
use database::repository::bpm::ApprovalInstanceListProjection;
use database::{
    AccessControlExt, ApprovalIntegrationExt, BpmExt, Executor, NoTransaction, Transactional, WorkItemExt,
};
use entities::approval_integration::ApprovalSubjectSnapshot;
use entities::common::time::Instant;
use entities::document_registry::DocumentType;
use entities::work_item::WorkItemStatus;
use id_generator::next_id;
use mongodb::Database;

use super::super::apply_plan::PlannedWrites;
use super::super::authorization::{converge_eligibility, AuthorizationFailure};
use super::super::idempotency::{
    command_may_have_committed, command_recovery_delay, map_receipt_first_write_error,
    normalize_idempotency_key, payload_conflict_error, resume_identity, PreparedCommandIdentity,
    ReceiptBranch,
};
use super::super::resume::prepare_resume;
use super::super::runtime_query::{recovery_options_for, RuntimeRecoveryAction};
use super::super::view::{map_command_view, ApprovalCommandView, OpenTaskSummary};
use super::super::{ExecutionCommandInput, PreparedExecution, ResumeExecutionInput};
use super::notifications::{persist_resume_notifications, ResumeNotificationFacts};
use super::query::{first_open_task, list_projection_from_writes};
use super::read_auth::{
    process_required_separation_policy, revalidate_decision_approver, RevalidateDecisionApproverInput,
};
use super::tasks::{create_open_tasks, CreateOpenTasksInput};
use super::{
    ensure_command_actor, ensure_expected_version, find_receipt_for_identity, hidden_not_found,
    load_exact_runtime_snapshot, persisted_command_view_with_executor, require_cas_applied,
    ApprovalRuntimeService,
};
use crate::approval::business_adapter::{
    adapter_object_read_decision, adapter_spec_of, BindingRevalidationContext,
};
use crate::approval::policy::STATIC_APPROVE_PERMISSION;
use crate::approval::{approval_recovery_scope, ApprovalResumeCommand};
use crate::audit::AuditActor;
use crate::errors::{Error, ErrorCode, Result};
use crate::iam::subject;
use crate::iam::SharedRbacService;

/// 人员恢复时对旧关闭任务执行的只读并发守卫。
struct ClosedTaskGuard {
    task_id: String,
    execution_id: ApprovalNodeExecutionId,
    version: u64,
}

/// 人员恢复事务需要的冻结输入。
struct ResumePersistInput<'a> {
    writes: &'a PlannedWrites,
    ended_execution_id: &'a str,
    expected_instance_version: u64,
    expected_execution_version: u64,
    closed_task_guard: Option<&'a ClosedTaskGuard>,
    new_task_ids: &'a [String],
    list_projection: &'a ApprovalInstanceListProjection,
    audit: &'a entities::audit_log::AuditLog,
    now: Instant,
    owner_role: &'a str,
    owner_organization_id: &'a str,
    subject_version: &'a str,
    business_object_id: &'a str,
    document_type_label: &'a str,
    document_no: &'a str,
    submitted_by: &'a str,
}

impl ApprovalRuntimeService {
    /// 写时重验审批人资格：账号启用、具备 `approval_instance:decide`、能读取
    /// 被审单据。任一失败收敛为对应人员 blocker，不得回滚为空。
    ///
    /// # 参数
    /// * `assignee_id` - 当前或下一节点审批人账号 ID
    /// * `assignee_name` - 定义或执行中的显示名快照
    /// * `snapshot` - 被审单据冻结主体与责任组织
    /// * `spec` - 单据类型审批适配器规格
    /// * `executor` - 调用方持有的数据库快照执行器
    ///
    /// # 返回
    /// 返回 BPM 可消费的有效或结构化受阻资格。
    ///
    /// # 错误
    /// Repository、权限解析、RBAC 或对象读取适配器失败时返回错误。
    ///
    /// # 关键业务约束
    /// 账号后台有效性由实体判断，Service 只编排权限与对象读取 I/O。
    async fn revalidate_approver(
        &self,
        assignee_id: &str,
        assignee_name: &str,
        snapshot: &ApprovalSubjectSnapshot,
        spec: &crate::approval::business_adapter::ApprovalAdapterSpec,
        executor: &mut dyn Executor,
    ) -> Result<Eligibility> {
        if spec.document_type == DocumentType::StockAdjustment {
            return revalidate_decision_approver(
                &self.db,
                &self.rbac,
                RevalidateDecisionApproverInput {
                    assignee_id,
                    assignee_name,
                    authenticated_actor: None,
                    snapshot,
                    spec,
                    separation_policy: process_required_separation_policy(snapshot.document_type)?,
                },
                executor,
            )
            .await;
        }
        let failure = match self
            .db
            .accounts()
            .find_approval_assignee_by_id(assignee_id, executor)
            .await?
        {
            Some(account) if account.is_active_backoffice() => {
                let permission = entities::Permission::parse(STATIC_APPROVE_PERMISSION)
                    .map_err(|error| Error::Internal(format!("静态审批权限不变量损坏: {error}")))?;
                if self
                    .rbac
                    .enforce(&subject(account.kind, &account.base.id), &permission)
                    .await?
                {
                    let context = BindingRevalidationContext {
                        organization_id: snapshot.payload.responsible_org_id.clone(),
                        creator_id: String::new(),
                    };
                    match adapter_object_read_decision(spec, &context, assignee_id)? {
                        Some(true) => None,
                        _ => Some(AuthorizationFailure::CannotReadSubject),
                    }
                } else {
                    Some(AuthorizationFailure::NotEligible)
                }
            }
            _ => Some(AuthorizationFailure::AccountInactive),
        };
        converge_eligibility(assignee_id, assignee_name, failure)
    }

    /// 在原审批人重新合格后恢复当前受阻执行。
    ///
    /// # 参数
    /// * `actor` - 当前认证且具备恢复权限的审计主体
    /// * `command` - 实例、执行、审批人和已关闭任务的期望版本及幂等键
    ///
    /// # 返回
    /// 返回恢复后的审批命令视图；幂等回放返回已持久化视图。
    ///
    /// # 错误
    /// 主体不一致、实例缺失、权限不足、版本冲突、快照不一致或事务写入失败时返回错误。
    ///
    /// # 关键业务约束
    /// 冻结快照必须精确匹配单据类型、主体 ID 和提交版本，事务边界仍由 Service 持有。
    pub async fn resume_current_approver(
        &self,
        actor: &AuditActor,
        command: ApprovalResumeCommand,
    ) -> Result<ApprovalCommandView> {
        ensure_command_actor(actor, &command.actor_id)?;
        let instance_id = command.approval_process_instance_id.clone();
        let idempotency_key = normalize_idempotency_key(&command.idempotency_key)?;
        let identity = resume_identity(
            idempotency_key.clone(),
            &instance_id,
            command.expected_instance_version,
            command.expected_execution_version,
            command.expected_assignment_version,
            command.expected_closed_task_version,
            actor.id(),
        )?;
        if let Some(view) = self.replay_resume(actor, &instance_id, &identity).await? {
            return Ok(view);
        }

        self.require_recovery_action(&instance_id, RuntimeRecoveryAction::ResumeCurrentApprover)
            .await?;
        let instance = self
            .db
            .bpm_workflow()
            .find_instance_by_id(&ApprovalProcessInstanceId::new(&instance_id), &mut NoTransaction)
            .await?
            .ok_or_else(hidden_not_found)?;
        ensure_expected_version(
            "审批实例",
            command.expected_instance_version,
            instance.base.version,
        )?;
        let current = self
            .db
            .bpm_workflow()
            .find_current_execution(&ApprovalProcessInstanceId::new(&instance_id), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::ConflictError("审批实例缺少当前受阻执行".to_string()))?;
        ensure_expected_version(
            "审批执行",
            command.expected_execution_version,
            current.base.version,
        )?;
        let assignee = self
            .db
            .bpm_workflow()
            .find_assignee_for_node(
                &ApprovalProcessInstanceId::new(&instance_id),
                &current.node_key,
                &mut NoTransaction,
            )
            .await?
            .ok_or_else(|| Error::ConflictError("实例缺少当前节点审批人绑定".to_string()))?;
        ensure_expected_version(
            "审批人绑定",
            command.expected_assignment_version,
            assignee.base.version,
        )?;
        let closed_task_guard = self
            .load_resume_task_guard(
                &ApprovalNodeExecutionId::new(current.base.id.clone()),
                command.expected_closed_task_version,
            )
            .await?;
        let snapshot = self
            .db
            .approval_subject_snapshots()
            .find_by_process_instance_id(&instance_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::ConflictError("审批实例缺少冻结业务快照".to_string()))?;
        let document_type =
            entities::approval_integration::document_type_from_subject_kind(instance.subject.subject_kind())
                .map_err(|error| Error::ValidationError(error.to_string()))?;
        snapshot
            .ensure_matches_runtime_subject(
                document_type,
                instance.subject.subject_id(),
                instance.subject_version,
            )
            .map_err(|_| Error::ConflictError("审批实例与冻结业务快照不一致".to_string()))?;
        let recovery_scope = approval_recovery_scope(&self.db, &self.rbac, actor).await?;
        if !recovery_scope.covers(&snapshot.payload.responsible_org_id) {
            return Err(Error::Forbidden("无权恢复该责任组织的审批实例".to_string()));
        }
        let spec = adapter_spec_of(document_type)?;
        let eligibility = self
            .revalidate_approver(
                assignee.current_assignee_participant_id.as_str(),
                &current.assignee_name_snapshot,
                &snapshot,
                &spec,
                &mut NoTransaction,
            )
            .await?;
        ensure_resume_approver_recovered(&eligibility)?;
        let graph = self
            .db
            .bpm_workflow()
            .load_definition_graph(&instance.process_definition_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::ConflictError("审批实例绑定的定义不存在".to_string()))?;
        let now = Instant::now();
        let prepared = prepare_resume(ResumeExecutionInput {
            command: ExecutionCommandInput {
                graph,
                current_eligibility: eligibility.clone(),
                next_eligibility: eligibility,
                receipt: None,
                idempotency_key: idempotency_key.clone(),
                now: Timestamp::from_utc(now.as_utc()),
            },
            instance,
            current: current.clone(),
            assignee: assignee.clone(),
            expected_instance_version: command.expected_instance_version,
            expected_execution_version: command.expected_execution_version,
            expected_assignment_version: command.expected_assignment_version,
            expected_closed_task_version: command.expected_closed_task_version,
            next_execution_id: ApprovalNodeExecutionId::new(next_id()),
            next_execution_no: current.execution_no.saturating_add(1),
            receipt_id: ApprovalCommandReceiptId::new(next_id()),
            actor_id: actor.id().to_string(),
        })?;
        let PreparedExecution::Apply(writes) = prepared else {
            return self
                .persisted_command_view(&instance_id, CommitRequired::Proceed, true)
                .await;
        };
        let writes = *writes;
        let new_task_ids = writes.create_tasks.iter().map(|_| next_id()).collect::<Vec<_>>();
        let list_projection = list_projection_from_writes(&writes, &current.base.id, None, now);
        let audit = actor.clone().resource_log_with_message(
            "approval.resume_current_approver",
            "approval_process_instance",
            instance_id.clone(),
            Some(format!("execution={}", current.base.id)),
        )?;
        let db = self.db.clone();
        let client = self.db.client().clone();
        let owner_role = spec.owner_role.as_str().to_string();
        let owner_organization_id = snapshot.payload.responsible_org_id.clone();
        let subject_version = snapshot.subject_version.to_string();
        let business_object_id = snapshot.business_object_id.clone();
        let document_type_label = document_type.label().to_string();
        let document_no = snapshot.payload.document_no.clone();
        let submitted_by = snapshot.payload.submitted_by.clone();
        let ended_execution_id = current.base.id.clone();
        let rbac = self.rbac.clone();
        let stock_resume_revalidation = (document_type == DocumentType::StockAdjustment).then(|| {
            (
                assignee.current_assignee_participant_id.as_str().to_string(),
                current.assignee_name_snapshot.clone(),
                snapshot.clone(),
                spec.clone(),
            )
        });
        let recovery_identity = identity.clone();
        let recovery_instance_id = instance_id.clone();
        let view = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    if let Some((assignee_id, assignee_name, snapshot, spec)) =
                        stock_resume_revalidation.as_ref()
                    {
                        let eligibility = revalidate_decision_approver(
                            &db,
                            &rbac,
                            RevalidateDecisionApproverInput {
                                assignee_id,
                                assignee_name,
                                authenticated_actor: None,
                                snapshot,
                                spec,
                                separation_policy: process_required_separation_policy(
                                    snapshot.document_type,
                                )?,
                            },
                            session,
                        )
                        .await?;
                        ensure_resume_approver_recovered(&eligibility)?;
                    }
                    persist_resume_writes(
                        &db,
                        ResumePersistInput {
                            writes: &writes,
                            ended_execution_id: &ended_execution_id,
                            expected_instance_version: command.expected_instance_version,
                            expected_execution_version: command.expected_execution_version,
                            closed_task_guard: closed_task_guard.as_ref(),
                            new_task_ids: &new_task_ids,
                            list_projection: &list_projection,
                            audit: &audit,
                            now,
                            owner_role: &owner_role,
                            owner_organization_id: &owner_organization_id,
                            subject_version: &subject_version,
                            business_object_id: &business_object_id,
                            document_type_label: &document_type_label,
                            document_no: &document_no,
                            submitted_by: &submitted_by,
                        },
                        session,
                    )
                    .await?;
                    Ok::<ApprovalCommandView, crate::errors::Error>(map_command_view(
                        &writes.instance,
                        writes.created_executions.last(),
                        None,
                        None,
                        first_open_task(&writes, &new_task_ids),
                        writes.commit,
                        false,
                    ))
                })
            })
            .await;
        match view {
            Ok(view) => Ok(view),
            Err(error) if command_may_have_committed(&error) => {
                self.recover_resume_after_competing_commit(
                    actor,
                    recovery_instance_id,
                    recovery_identity,
                    error,
                )
                .await
            }
            Err(error) => Err(error),
        }
    }

    /// 在独立事务快照内按当前权限回放原审批人恢复结果。
    async fn replay_resume(
        &self,
        actor: &AuditActor,
        instance_id: &str,
        identity: &PreparedCommandIdentity,
    ) -> Result<Option<ApprovalCommandView>> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let actor = actor.clone();
        let instance_id = instance_id.to_string();
        let identity = identity.clone();
        self.db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    replay_resume_in_transaction(&db, &rbac, &actor, &instance_id, &identity, session).await
                })
            })
            .await
    }

    /// 唯一键竞争、瞬态事务错误或提交结果未知后，以新会话有限回读胜者。
    async fn recover_resume_after_competing_commit(
        &self,
        actor: &AuditActor,
        instance_id: String,
        identity: PreparedCommandIdentity,
        original_error: Error,
    ) -> Result<ApprovalCommandView> {
        const RECOVERY_ATTEMPTS: usize = 8;
        for attempt in 0..RECOVERY_ATTEMPTS {
            match self.replay_resume(actor, &instance_id, &identity).await {
                Ok(Some(view)) => return Ok(view),
                Ok(None) => {}
                Err(error) if command_may_have_committed(&error) => {}
                Err(error) => return Err(error),
            }
            if attempt + 1 < RECOVERY_ATTEMPTS {
                tokio::time::sleep(command_recovery_delay(attempt)).await;
            }
        }
        Err(original_error)
    }

    async fn load_resume_task_guard(
        &self,
        execution_id: &ApprovalNodeExecutionId,
        expected_closed_task_version: Option<u64>,
    ) -> Result<Option<ClosedTaskGuard>> {
        let tasks = self
            .db
            .work_items()
            .approval_tasks_for_execution(execution_id, &mut NoTransaction)
            .await?;
        if tasks.len() > 1 {
            return Err(Error::ConflictError("受阻执行关联多个历史审批任务".to_string()));
        }
        let Some(task) = tasks.into_iter().next() else {
            if expected_closed_task_version.is_some() {
                return Err(Error::ConflictError(
                    "调用方声明了关闭任务版本，但历史任务不存在".to_string(),
                ));
            }
            return Ok(None);
        };
        if task.status != WorkItemStatus::Closed {
            return Err(Error::ConflictError("人员恢复要求原审批任务已经关闭".to_string()));
        }
        if let Some(expected) = expected_closed_task_version {
            ensure_expected_version("已关闭审批任务", expected, task.base.version)?;
        }
        Ok(Some(ClosedTaskGuard {
            task_id: task.base.id,
            execution_id: execution_id.clone(),
            version: task.base.version,
        }))
    }

    async fn persisted_command_view(
        &self,
        instance_id: &str,
        commit: CommitRequired,
        replay: bool,
    ) -> Result<ApprovalCommandView> {
        let instance = self
            .db
            .bpm_workflow()
            .find_instance_by_id(&ApprovalProcessInstanceId::new(instance_id), &mut NoTransaction)
            .await?
            .ok_or_else(hidden_not_found)?;
        let current = self
            .db
            .bpm_workflow()
            .find_current_execution(&ApprovalProcessInstanceId::new(instance_id), &mut NoTransaction)
            .await?;
        let next_open_task = match current.as_ref() {
            Some(execution) => {
                let tasks = self
                    .db
                    .work_items()
                    .open_approval_tasks_for_execution(
                        &ApprovalNodeExecutionId::new(execution.base.id.clone()),
                        &mut NoTransaction,
                    )
                    .await?;
                if tasks.len() > 1 {
                    return Err(Error::ConflictError("当前执行关联多个开放审批任务".to_string()));
                }
                tasks.into_iter().next().map(|task| OpenTaskSummary {
                    work_item_id: task.base.id,
                    task_version: task.base.version.to_string(),
                    owner_user_id: task.owner_user_id.unwrap_or_default(),
                })
            }
            None => None,
        };
        Ok(map_command_view(
            &instance,
            current.as_ref(),
            None,
            None,
            next_open_task,
            commit,
            replay,
        ))
    }

    async fn require_recovery_action(&self, instance_id: &str, wanted: RuntimeRecoveryAction) -> Result<()> {
        let instance = self
            .db
            .bpm_workflow()
            .find_instance_by_id(&ApprovalProcessInstanceId::new(instance_id), &mut NoTransaction)
            .await?
            .ok_or_else(hidden_not_found)?;
        let blocked = instance.status == bpm::model::types::ApprovalProcessInstanceStatus::Blocked;
        if recovery_options_for(blocked, instance.blocker_code).contains(&wanted) {
            return Ok(());
        }
        Err(Error::ConflictError("当前 blocker 不允许该恢复动作".to_string()))
    }
}

/// 原审批人恢复回放先按当前账号与责任组织授权，再允许读取和比较收据。
async fn replay_resume_in_transaction(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    instance_id: &str,
    identity: &PreparedCommandIdentity,
    session: &mut mongodb::ClientSession,
) -> Result<Option<ApprovalCommandView>> {
    let instance = db
        .bpm_workflow()
        .find_instance_by_id(&ApprovalProcessInstanceId::new(instance_id), session)
        .await?
        .ok_or_else(hidden_not_found)?;
    let (_, snapshot) = load_exact_runtime_snapshot(db, &instance, session, true).await?;
    let recovery_scope = approval_recovery_scope(db, rbac, actor).await?;
    if !recovery_scope.covers(&snapshot.payload.responsible_org_id) {
        return Err(Error::Forbidden("无权恢复该责任组织的审批实例".to_string()));
    }
    let Some(receipt) = find_receipt_for_identity(db, identity, session).await? else {
        return Ok(None);
    };
    if receipt.result_ref != instance_id {
        return Err(Error::ConflictError("恢复收据结果引用与实例不一致".to_string()));
    }
    match identity.classify(Some(&receipt)) {
        ReceiptBranch::SamePayload(_) => {}
        ReceiptBranch::Fresh => unreachable!("receipt was loaded"),
        ReceiptBranch::PayloadConflict => return Err(payload_conflict_error()),
    }
    persisted_command_view_with_executor(db, instance_id, CommitRequired::Proceed, true, session)
        .await
        .map(Some)
}

/// 在一个 MongoDB 事务内应用人员恢复全部正式事实。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `input` - 恢复计划、CAS 版本、任务元数据与审计
/// * `session` - 唯一事务会话
///
/// # 返回
/// 实例、执行、收据、新任务、通知与审计全部写入时返回 `Ok(())`。
///
/// # 错误
/// 任一历史任务守卫、CAS、唯一索引或实体写入失败时返回错误并回滚。
///
/// # 关键业务约束
/// 旧关闭任务保持不可变；恢复只能结束旧受阻执行并为新执行创建新任务。
async fn persist_resume_writes(
    db: &Database,
    input: ResumePersistInput<'_>,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let [new_execution] = input.writes.created_executions.as_slice() else {
        return Err(Error::Internal(
            "原审批人恢复必须且只能创建一个新执行".to_string(),
        ));
    };
    if let Some(guard) = input.closed_task_guard {
        let task = db
            .work_items()
            .find_document_approval_by_id(&guard.task_id, session)
            .await?
            .ok_or_else(|| Error::ConflictError("原关闭审批任务不存在".to_string()))?;
        if task.status != WorkItemStatus::Closed
            || task.base.version != guard.version
            || task.approval_node_execution_id.as_ref() != Some(&guard.execution_id)
        {
            return Err(Error::ConflictError(
                "原关闭审批任务已变化，请刷新后重试".to_string(),
            ));
        }
    }
    // 收据是完成全部只读验证后的第一笔物理写，用唯一身份仲裁同键并发。
    db.bpm_workflow()
        .insert_command_receipt(&input.writes.receipt, session)
        .await
        .map_err(map_receipt_first_write_error)?;
    let expected_execution_id = ApprovalNodeExecutionId::new(input.ended_execution_id);
    require_cas_applied(
        db.bpm_workflow()
            .advance_instance(
                &input.writes.instance,
                input.expected_instance_version,
                &expected_execution_id,
                input.list_projection,
                session,
            )
            .await?,
        "审批实例",
    )?;
    for execution in &input.writes.updated_executions {
        if execution.base.id != input.ended_execution_id {
            return Err(Error::Internal("恢复计划包含非当前旧执行更新".to_string()));
        }
        require_cas_applied(
            db.bpm_workflow()
                .end_blocked_execution(execution, input.expected_execution_version, session)
                .await?,
            "受阻审批执行",
        )?;
    }
    if !input.writes.created_assignees.is_empty() {
        return Err(Error::Internal("原审批人恢复不得修改实例审批人绑定".to_string()));
    }
    db.bpm_workflow().insert_execution(new_execution, session).await?;
    create_open_tasks(
        db,
        CreateOpenTasksInput {
            writes: input.writes,
            new_task_ids: input.new_task_ids,
            owner_role: input.owner_role,
            owner_organization_id: input.owner_organization_id,
            subject_version: input.subject_version,
            business_object_id: input.business_object_id,
            now: input.now,
        },
        session,
    )
    .await?;
    persist_resume_notifications(
        db,
        input.writes,
        ResumeNotificationFacts {
            new_execution,
            submitted_by: input.submitted_by,
            document_type_label: input.document_type_label,
            document_no: input.document_no,
        },
        input.now,
        session,
    )
    .await?;
    db.audit_logs().create(input.audit, session).await?;
    Ok(())
}

/// 恢复端口只接受已重新满足全部资格的原审批人。
fn ensure_resume_approver_recovered(eligibility: &Eligibility) -> Result<()> {
    if eligibility.blocked_code().is_some() {
        return Err(Error::from_approval_code(
            ErrorCode::ApprovalCurrentApproverNotRecovered,
        ));
    }
    Ok(())
}
