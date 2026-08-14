//! INTERNAL 审批运行时：唯一事务内推进实例、步骤、待办和强类型业务事实。

use std::{future::Future, pin::Pin, sync::Arc};

use database::{AccessControlExt, ApprovalExt, Executor, NoTransaction, Transactional, WorkItemExt};
use entities::{
    approval::{
        ApprovalDecision, ApprovalDefinition, ApprovalInstance, ApprovalInstanceData, ApprovalInstanceStatus,
        ApprovalStepDefinition, ApprovalStepInstance, ApprovalStepInstanceData, ApprovalStepStatus,
    },
    common::time::Instant,
    work_item::{
        AssignmentMode, AssignmentSource, WorkItem, WorkItemCloseData, WorkItemData, WorkItemId,
        WorkItemPriority, WorkItemStatus,
    },
    ApprovalDefinitionId, ApprovalInstanceId, ApprovalStepInstanceId, AuditLog, AuditLogData,
};
use id_generator::next_id;
use mongodb::Database;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::errors::{Error, Result};

use super::{
    action::{recovery_action_context, ApprovalActionContext, ApprovalDomainActionPort},
    dto::{
        ApprovalRuntimeView, BlockedApprovalListParams, BlockedApprovalPage, BlockedApprovalView,
        CancelApprovalCommand, RecoverApprovalCommand, StartApprovalCommand, SubmitDecisionCommand,
    },
    registry::{
        cancel_action, decision_action, definition as registered_definition, recovery_validation_action,
        start_action, OPERATIONS_APPROVAL, SALES_MANAGER_APPROVAL,
    },
    resolver::{ApprovalAssigneeResolver, ResolvedAssignment},
    scope::{ensure_recovery_authorization, ApprovalManagementScope},
};

/// 稳定运行时端口的异步返回类型。
pub type ApprovalRuntimeFuture<'a> = Pin<Box<dyn Future<Output = Result<ApprovalRuntimeView>> + Send + 'a>>;

/// 审批业务只依赖的稳定运行时端口。
///
/// 四个方法的请求合同不暴露 INTERNAL/BPM 差异；当前实现仅提供 INTERNAL。
/// 每次调用必须建立唯一原子边界，不允许 Handler 自行创建步骤、待办或下一节点。
pub trait ApprovalRuntimePort: Send + Sync {
    /// 启动已注册的审批定义。
    fn start_approval<'a>(&'a self, command: StartApprovalCommand) -> ApprovalRuntimeFuture<'a>;

    /// 提交当前唯一活动步骤的正式决定。
    fn submit_decision<'a>(&'a self, command: SubmitDecisionCommand) -> ApprovalRuntimeFuture<'a>;

    /// 取消仍满足领域撤回规则的审批。
    fn cancel_approval<'a>(&'a self, command: CancelApprovalCommand) -> ApprovalRuntimeFuture<'a>;

    /// 仅以 `RETRY_CURRENT_STEP` 恢复原阻塞步骤。
    fn recover_approval<'a>(&'a self, command: RecoverApprovalCommand) -> ApprovalRuntimeFuture<'a>;
}

/// ERP 内部 MongoDB 事务审批运行时。
#[derive(Clone)]
pub struct InternalApprovalRuntime {
    db: Database,
    action_port: Arc<dyn ApprovalDomainActionPort>,
    resolver: ApprovalAssigneeResolver,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApprovalCommandKind {
    SubmitDecision,
    CancelApproval,
    RecoverApproval,
}

impl ApprovalCommandKind {
    fn action(self) -> &'static str {
        match self {
            Self::SubmitDecision => "approval.submit_decision",
            Self::CancelApproval => "approval.cancel",
            Self::RecoverApproval => "approval.recover",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ApprovalCommandReceipt {
    id: String,
    kind: ApprovalCommandKind,
    approval_instance_id: String,
    actor_id: String,
    payload_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ApprovalCommandReceiptPayload {
    payload_hash: String,
    result: ApprovalRuntimeView,
}

impl ApprovalCommandReceipt {
    fn new(
        kind: ApprovalCommandKind,
        approval_instance_id: &str,
        actor_id: &str,
        idempotency_key: &str,
        payload_hash: String,
    ) -> Self {
        let id = format!(
            "approval-command-{}",
            stable_digest(&[kind.action(), approval_instance_id, actor_id, idempotency_key,])
        );
        Self {
            id,
            kind,
            approval_instance_id: approval_instance_id.to_string(),
            actor_id: actor_id.to_string(),
            payload_hash,
        }
    }
}

impl InternalApprovalRuntime {
    /// 创建绑定强类型领域动作端口的 INTERNAL 运行时。
    ///
    /// # 返回
    /// 返回共享同一数据库与事务执行器的运行时实例。
    pub fn new(db: Database, action_port: Arc<dyn ApprovalDomainActionPort>) -> Self {
        Self {
            resolver: ApprovalAssigneeResolver::new(db.clone()),
            db,
            action_port,
        }
    }

    /// 在调用方已建立的唯一外层事务中创建审批实例、全部步骤和首个待办。
    ///
    /// 该入口不重复执行业务提交动作；销售单 Service 必须先在同一 `executor`
    /// 写提交事实，再调用本方法。DIRECT 解析失败会落下 BLOCKED 实例和步骤，
    /// 并返回阻塞视图，而不是回滚或猜测责任人。
    ///
    /// # 错误
    /// 定义未发布、注册表漂移、重复终态提交或持久化失败时返回服务错误。
    pub async fn start_approval_in_transaction(
        &self,
        command: StartApprovalCommand,
        executor: &mut dyn Executor,
    ) -> Result<ApprovalRuntimeView> {
        validate_start_command(&command)?;
        if let Some(view) = self.replay_start_request(&command, executor).await? {
            return Ok(view);
        }
        if let Some(existing) = self
            .db
            .approval_instances()
            .find_non_terminal_by_subject(
                &command.definition_key,
                &command.business_object_type,
                &command.business_object_id,
                &command.subject_version,
                executor,
            )
            .await?
        {
            return Err(Error::ConflictError(format!(
                "该业务提交已由幂等键 {} 启动审批",
                existing.start_idempotency_key
            )));
        }

        let (definition, step_definitions) = self.load_definition(&command, executor).await?;
        let now = Instant::now();
        let instance_id = ApprovalInstanceId::new(command.deterministic_instance_id());
        let mut steps = build_step_instances(&instance_id, &step_definitions)?;
        let current_step_id = ApprovalStepInstanceId::new(steps[0].base.id.clone());
        let mut instance = ApprovalInstance::new(
            instance_id,
            ApprovalInstanceData {
                definition_key: definition.definition_key,
                definition_version: definition.definition_version,
                runtime_kind: definition.runtime_kind,
                business_object_type: command.business_object_type,
                business_object_id: command.business_object_id,
                subject_version: command.subject_version,
                owner_organization_id: command.owner_organization_id,
                start_idempotency_key: command.idempotency_key,
                current_step_instance_id: current_step_id,
                external_instance_id: None,
                started_by: command.started_by,
                started_at: now,
            },
        )?;
        let assignment = self
            .resolver
            .resolve_excluding(
                &step_definitions[0].assignee_resolver_key,
                &instance.owner_organization_id,
                &[instance.started_by.as_str()],
                executor,
            )
            .await?;
        let work_item = match assignment {
            Ok(assignment) => Some(build_work_item(
                &instance,
                &steps[0],
                &step_definitions[0],
                assignment,
                now,
            )?),
            Err(blocker) => {
                instance.block(blocker.code(), now)?;
                steps[0].block(blocker.code(), now)?;
                None
            }
        };
        self.persist_new_runtime(&instance, &steps, work_item.as_ref(), executor)
            .await?;
        self.append_runtime_audit(
            &instance.started_by,
            "approval.start",
            &instance.base.id,
            Some(format!("step_sha256={}", stable_digest(&[&steps[0].base.id]))),
            executor,
        )
        .await?;
        let transition_action = if work_item.is_some() {
            "approval.assign"
        } else {
            "approval.block"
        };
        self.append_runtime_audit(
            &instance.started_by,
            transition_action,
            &instance.base.id,
            runtime_transition_message(&steps[0], instance.blocker_code.as_deref()),
            executor,
        )
        .await?;
        Ok(runtime_view(instance, steps.remove(0), work_item))
    }

    /// 按当前授权组织分页查询阻塞审批。
    ///
    /// # 错误
    /// 查询参数非法、实例缺少阻塞当前步骤或仓储读取失败时返回错误。
    pub async fn blocked_approvals(
        &self,
        params: &BlockedApprovalListParams,
        owner_organization_ids: Option<&[String]>,
        recovery_scope: Option<&ApprovalManagementScope>,
    ) -> Result<BlockedApprovalPage> {
        validate_blocked_query(params)?;
        let page = self
            .db
            .approval_instances()
            .list_blocked(
                owner_organization_ids,
                params.page,
                params.page_size,
                &mut NoTransaction,
            )
            .await?;
        let mut items = Vec::with_capacity(page.items.len());
        for instance in page.items {
            let can_recover =
                recovery_scope.is_some_and(|scope| scope.covers(&instance.owner_organization_id));
            let step = self.current_step(&instance, &mut NoTransaction).await?;
            let work_item = self.work_item_for_step(&step.base.id, &mut NoTransaction).await?;
            items.push(blocked_view(instance, step, work_item, can_recover)?);
        }
        Ok(BlockedApprovalPage {
            items,
            total: u64::try_from(page.total).unwrap_or(0),
            page: params.page.max(1),
            page_size: params.page_size,
        })
    }

    /// 查询单条阻塞审批并重验其冻结责任组织位于管理范围内。
    ///
    /// # 错误
    /// 实例不存在、已不再阻塞、越出管理范围或持久化读取失败时返回服务错误。
    pub async fn blocked_approval(
        &self,
        approval_instance_id: &str,
        scope: &ApprovalManagementScope,
        can_recover: bool,
    ) -> Result<BlockedApprovalView> {
        let instance = self
            .db
            .approval_instances()
            .find_by_id(approval_instance_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("阻塞审批实例不存在".to_string()))?;
        if !scope.covers(&instance.owner_organization_id) {
            return Err(Error::Forbidden("审批实例不在当前管理范围内".to_string()));
        }
        if instance.status != ApprovalInstanceStatus::Blocked {
            return Err(Error::ConflictError("审批实例已不再阻塞".to_string()));
        }
        let step = self.current_step(&instance, &mut NoTransaction).await?;
        let work_item = self.work_item_for_step(&step.base.id, &mut NoTransaction).await?;
        blocked_view(instance, step, work_item, can_recover)
    }

    /// 重验审批实例的冻结责任组织位于当前管理范围内。
    ///
    /// 本方法不要求实例仍为 `BLOCKED`，因此恢复成功后的同幂等键重试仍可进入
    /// 运行时 receipt 回放；它只承担 HTTP 边界不可绕过的对象级范围授权。
    ///
    /// # 错误
    /// 实例不存在、越出管理范围或仓储读取失败时返回服务错误。
    pub async fn ensure_approval_in_management_scope(
        &self,
        approval_instance_id: &str,
        scope: &ApprovalManagementScope,
    ) -> Result<()> {
        let instance = self
            .db
            .approval_instances()
            .find_by_id(approval_instance_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("审批实例不存在".to_string()))?;
        if !scope.covers(&instance.owner_organization_id) {
            return Err(Error::Forbidden("审批实例不在当前管理范围内".to_string()));
        }
        Ok(())
    }

    async fn load_definition(
        &self,
        command: &StartApprovalCommand,
        executor: &mut dyn Executor,
    ) -> Result<(ApprovalDefinition, Vec<ApprovalStepDefinition>)> {
        let registered = registered_definition(&command.definition_key)
            .ok_or_else(|| Error::ValidationError("审批定义未注册".to_string()))?;
        let definition = self
            .db
            .approval_definitions()
            .find_published_by_key(&command.definition_key, executor)
            .await?
            .ok_or_else(|| Error::BusinessLogicError("审批定义尚未发布".to_string()))?;
        if definition.definition_version != registered.version
            || definition.runtime_kind != entities::approval::ApprovalRuntimeKind::Internal
            || definition.external_definition_id.is_some()
        {
            return Err(Error::BusinessLogicError(
                "审批定义与当前代码注册版本不一致".to_string(),
            ));
        }
        let steps = self
            .db
            .approval_step_definitions()
            .list_by_definition(&ApprovalDefinitionId::new(definition.base.id.clone()), executor)
            .await?;
        verify_frozen_steps(&steps, registered.steps)?;
        Ok((definition, steps))
    }

    async fn persist_new_runtime(
        &self,
        instance: &ApprovalInstance,
        steps: &[ApprovalStepInstance],
        work_item: Option<&WorkItem>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db.approval_instances().create(instance, executor).await?;
        for step in steps {
            self.db.approval_step_instances().create(step, executor).await?;
        }
        if let Some(work_item) = work_item {
            self.db.work_items().create(work_item, executor).await?;
        }
        Ok(())
    }

    async fn runtime_view(
        &self,
        instance: ApprovalInstance,
        executor: &mut dyn Executor,
    ) -> Result<ApprovalRuntimeView> {
        let (step, work_item) = if instance.current_step_instance_id.is_some() {
            let step = self.current_step(&instance, executor).await?;
            let work_item = self.work_item_for_step(&step.base.id, executor).await?;
            (step, work_item)
        } else {
            let step = self
                .db
                .approval_step_instances()
                .list_by_instance(&ApprovalInstanceId::new(instance.base.id.clone()), executor)
                .await?
                .into_iter()
                .filter(|step| step.is_terminal())
                .max_by_key(|step| step.sequence_no)
                .ok_or_else(|| Error::Internal("终态审批实例缺少步骤事实".to_string()))?;
            let work_item = self.any_work_item_for_step(&step.base.id, executor).await?;
            (step, work_item)
        };
        Ok(runtime_view(instance, step, work_item))
    }

    async fn current_step(
        &self,
        instance: &ApprovalInstance,
        executor: &mut dyn Executor,
    ) -> Result<ApprovalStepInstance> {
        let step_id = instance
            .current_step_instance_id
            .as_ref()
            .ok_or_else(|| Error::BusinessLogicError("审批实例已无当前步骤".to_string()))?;
        self.db
            .approval_step_instances()
            .find_by_id(step_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::Internal("审批实例当前步骤不存在".to_string()))
    }

    async fn work_item_for_step(
        &self,
        step_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<WorkItem>> {
        self.db
            .work_items()
            .find_one(
                mongodb::bson::doc! {
                    "approval_step_instance_id": step_id,
                    "status": WorkItemStatus::Open.as_str(),
                },
                executor,
            )
            .await
            .map_err(Into::into)
    }

    async fn any_work_item_for_step(
        &self,
        step_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<WorkItem>> {
        self.db
            .work_items()
            .find_one(
                mongodb::bson::doc! { "approval_step_instance_id": step_id },
                executor,
            )
            .await
            .map_err(Into::into)
    }

    async fn replay_start_request(
        &self,
        command: &StartApprovalCommand,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalRuntimeView>> {
        let Some(existing) = self
            .db
            .approval_instances()
            .find_by_start_idempotency_key(&command.definition_key, &command.idempotency_key, executor)
            .await?
        else {
            return Ok(None);
        };
        ensure_same_start_request(&existing, command)?;
        self.runtime_view(existing, executor).await.map(Some)
    }

    async fn command_receipt_log(
        &self,
        receipt: &ApprovalCommandReceipt,
        result: &ApprovalRuntimeView,
        executor: &mut dyn Executor,
    ) -> Result<AuditLog> {
        let account = self
            .db
            .accounts()
            .find_by_id(&receipt.actor_id, executor)
            .await?
            .ok_or_else(|| Error::Unauthenticated("审批操作人账号不存在".to_string()))?;
        let message = command_receipt_message(receipt, result)?;
        AuditLog::new(
            receipt.id.clone(),
            AuditLogData {
                actor_id: receipt.actor_id.clone(),
                actor_account: account.secret.account().to_string(),
                actor_type: account.kind,
                action: receipt.kind.action().to_string(),
                resource_type: "approval_instance".to_string(),
                resource_id: Some(stable_digest(&[&receipt.approval_instance_id])),
                success: true,
                message: Some(message),
            },
        )
        .map_err(Into::into)
    }

    async fn append_runtime_audit(
        &self,
        actor_id: &str,
        action: &str,
        approval_instance_id: &str,
        message: Option<String>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let account = self
            .db
            .accounts()
            .find_by_id(actor_id, executor)
            .await?
            .ok_or_else(|| Error::Unauthenticated("审批操作人账号不存在".to_string()))?;
        let audit = AuditLog::new(
            next_id(),
            AuditLogData {
                actor_id: actor_id.to_string(),
                actor_account: account.secret.account().to_string(),
                actor_type: account.kind,
                action: action.to_string(),
                resource_type: "approval_instance".to_string(),
                resource_id: Some(stable_digest(&[approval_instance_id])),
                success: true,
                message,
            },
        )?;
        self.db.audit_logs().create(&audit, executor).await?;
        Ok(())
    }

    async fn replay_command_receipt(
        &self,
        receipt: &ApprovalCommandReceipt,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalRuntimeView>> {
        let Some(audit) = self.db.audit_logs().find_by_id(&receipt.id, executor).await? else {
            return Ok(None);
        };
        let expected_resource_id = stable_digest(&[&receipt.approval_instance_id]);
        let payload = audit
            .message
            .as_deref()
            .ok_or_else(|| Error::ConflictError("审批命令幂等回执格式无效".to_string()))
            .and_then(parse_command_receipt_message)?;
        if audit.actor_id != receipt.actor_id
            || audit.action != receipt.kind.action()
            || audit.resource_type != "approval_instance"
            || audit.resource_id.as_deref() != Some(expected_resource_id.as_str())
            || payload.payload_hash != receipt.payload_hash
        {
            return Err(Error::ConflictError("审批命令幂等键已用于不同请求".to_string()));
        }
        Ok(Some(payload.result))
    }

    async fn append_command_receipt(
        &self,
        receipt: &ApprovalCommandReceipt,
        result: &ApprovalRuntimeView,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let audit = self.command_receipt_log(receipt, result, executor).await?;
        self.db.audit_logs().create(&audit, executor).await?;
        Ok(())
    }

    async fn recover_receipt_after_conflict(
        &self,
        receipt: &ApprovalCommandReceipt,
        error: Error,
    ) -> Result<ApprovalRuntimeView> {
        match self.replay_command_receipt(receipt, &mut NoTransaction).await? {
            Some(view) => Ok(view),
            None => Err(error),
        }
    }

    async fn start_owned_transaction(&self, command: StartApprovalCommand) -> Result<ApprovalRuntimeView> {
        validate_start_command(&command)?;
        let db = self.db.clone();
        let runtime = self.clone();
        let action_port = Arc::clone(&self.action_port);
        let replay_command = command.clone();
        let action = start_action(&command.definition_key)
            .ok_or_else(|| Error::ValidationError("审批启动处理器未注册".to_string()))?;
        let action_context = ApprovalActionContext {
            definition_key: command.definition_key.clone(),
            approval_instance_id: command.deterministic_instance_id(),
            approval_step_instance_id: None,
            work_item_id: None,
            business_object_type: command.business_object_type.clone(),
            business_object_id: command.business_object_id.clone(),
            subject_version: command.subject_version.clone(),
            actor_id: command.started_by.clone(),
            reason: None,
            idempotency_key: command.idempotency_key.clone(),
        };
        let result = db
            .client()
            .clone()
            .with_transaction(move |session| {
                Box::pin(async move {
                    if let Some(view) = runtime.replay_start_request(&command, session).await? {
                        return Ok(view);
                    }
                    action_port.execute(action, &action_context, session).await?;
                    runtime.start_approval_in_transaction(command, session).await
                })
            })
            .await;
        match result {
            Ok(view) => Ok(view),
            Err(error @ Error::ConflictError(_)) => {
                if let Some(existing) = self
                    .db
                    .approval_instances()
                    .find_by_start_idempotency_key(
                        &replay_command.definition_key,
                        &replay_command.idempotency_key,
                        &mut NoTransaction,
                    )
                    .await?
                {
                    ensure_same_start_request(&existing, &replay_command)?;
                    return self.runtime_view(existing, &mut NoTransaction).await;
                }
                let deterministic_id = replay_command.deterministic_instance_id();
                if let Some(existing) = self
                    .db
                    .approval_instances()
                    .find_by_id(&deterministic_id, &mut NoTransaction)
                    .await?
                {
                    ensure_same_start_request(&existing, &replay_command)?;
                    return self.runtime_view(existing, &mut NoTransaction).await;
                }
                Err(error)
            }
            Err(error) => Err(error),
        }
    }

    async fn submit_owned_transaction(&self, command: SubmitDecisionCommand) -> Result<ApprovalRuntimeView> {
        let receipt = submit_receipt(&command);
        let replay_receipt = receipt.clone();
        let runtime = self.clone();
        let result = self
            .db
            .client()
            .clone()
            .with_transaction(move |session| {
                Box::pin(async move { runtime.submit_in_transaction(command, session).await })
            })
            .await;
        match result {
            Ok(view) => Ok(view),
            Err(error @ Error::ConflictError(_)) => {
                self.recover_receipt_after_conflict(&replay_receipt, error).await
            }
            Err(error) => Err(error),
        }
    }

    async fn submit_in_transaction(
        &self,
        command: SubmitDecisionCommand,
        executor: &mut dyn Executor,
    ) -> Result<ApprovalRuntimeView> {
        validate_decision_command(&command)?;
        let receipt = submit_receipt(&command);
        if let Some(view) = self.replay_command_receipt(&receipt, executor).await? {
            return Ok(view);
        }
        let mut instance = self
            .load_instance(&command.approval_instance_id, executor)
            .await?;
        ensure_version(
            instance.base.version,
            command.expected_instance_version,
            "审批实例",
        )?;
        ensure_subject_version(&instance, &command.expected_subject_version)?;
        if instance.status == ApprovalInstanceStatus::Blocked {
            return Err(Error::BusinessLogicError(
                "审批实例处于阻塞状态，只能由管理员恢复原当前步骤".to_string(),
            ));
        }
        if instance.status != ApprovalInstanceStatus::Running {
            return Err(Error::ConflictError("审批实例当前不可提交决定".to_string()));
        }
        let mut current_step = self.current_step(&instance, executor).await?;
        if current_step.base.id != command.approval_step_instance_id {
            return Err(Error::ConflictError("审批当前步骤已变化".to_string()));
        }
        self.ensure_separation_of_duties(&instance, &current_step, &command.actor_id, executor)
            .await?;
        ensure_version(
            current_step.base.version,
            command.expected_step_version,
            "审批步骤",
        )?;
        let mut work_item = self
            .db
            .work_items()
            .find_by_id(&command.work_item_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("当前审批任务不存在".to_string()))?;
        ensure_version(work_item.base.version, command.expected_task_version, "审批任务")?;
        ensure_current_work_item(&work_item, &current_step, &command.actor_id)?;
        if !self
            .resolver
            .user_is_eligible_for_assignment(
                &command.actor_id,
                &work_item.owner_role,
                &work_item.owner_organization_id,
                executor,
            )
            .await?
        {
            return Err(Error::Forbidden(
                "当前处理人已不具备审批角色或组织范围资格".to_string(),
            ));
        }
        let step_definition = self.step_definition(&instance, &current_step, executor).await?;
        if !step_definition.allows(command.decision) {
            return Err(Error::ValidationError("当前步骤不允许该审批决定".to_string()));
        }
        let action = decision_action(&instance.definition_key, &current_step.step_key, command.decision)
            .ok_or_else(|| Error::BusinessLogicError("审批决定处理器未注册".to_string()))?;
        let now = Instant::now();
        current_step.decide(command.decision, command.reason.clone(), &command.actor_id, now)?;
        self.db
            .approval_step_instances()
            .update(&mut current_step, executor)
            .await?;
        let action_context = decision_action_context(&command, &instance, &current_step, &work_item);
        self.action_port
            .execute(action, &action_context, executor)
            .await?;
        work_item.complete_by_domain_command(&command.actor_id, now)?;
        self.db.work_items().update(&mut work_item, executor).await?;
        let view = match command.decision {
            ApprovalDecision::Approve => {
                self.advance_after_approval(instance, current_step, work_item, executor)
                    .await
            }
            ApprovalDecision::RejectToApplicant => {
                instance.reject(now)?;
                self.db
                    .approval_instances()
                    .update(&mut instance, executor)
                    .await?;
                Ok(runtime_view(instance, current_step, Some(work_item)))
            }
            ApprovalDecision::TerminateApproval => {
                instance.terminate(now)?;
                self.db
                    .approval_instances()
                    .update(&mut instance, executor)
                    .await?;
                Ok(runtime_view(instance, current_step, Some(work_item)))
            }
        }?;
        self.append_command_receipt(&receipt, &view, executor).await?;
        Ok(view)
    }

    async fn advance_after_approval(
        &self,
        mut instance: ApprovalInstance,
        decided_step: ApprovalStepInstance,
        completed_work_item: WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<ApprovalRuntimeView> {
        let mut steps = self
            .db
            .approval_step_instances()
            .list_by_instance(&ApprovalInstanceId::new(instance.base.id.clone()), executor)
            .await?;
        let Some(mut next_step) = steps
            .drain(..)
            .find(|step| step.sequence_no == decided_step.sequence_no + 1)
        else {
            instance.approve(Instant::now())?;
            self.db
                .approval_instances()
                .update(&mut instance, executor)
                .await?;
            return Ok(runtime_view(instance, decided_step, Some(completed_work_item)));
        };
        let step_definition = self.step_definition(&instance, &next_step, executor).await?;
        let assignment = self
            .resolver
            .resolve_excluding(
                &step_definition.assignee_resolver_key,
                &instance.owner_organization_id,
                &[
                    instance.started_by.as_str(),
                    decided_step.decided_by.as_deref().unwrap_or(""),
                ],
                executor,
            )
            .await?;
        let now = Instant::now();
        let transition_actor = decided_step
            .decided_by
            .as_deref()
            .ok_or_else(|| Error::Internal("已决定步骤缺少决定人".to_string()))?
            .to_string();
        match assignment {
            Ok(assignment) => {
                next_step.activate()?;
                instance.advance_to(ApprovalStepInstanceId::new(next_step.base.id.clone()))?;
                let work_item = build_work_item(&instance, &next_step, &step_definition, assignment, now)?;
                self.db
                    .approval_step_instances()
                    .update(&mut next_step, executor)
                    .await?;
                self.db
                    .approval_instances()
                    .update(&mut instance, executor)
                    .await?;
                self.db.work_items().create(&work_item, executor).await?;
                self.append_runtime_audit(
                    &transition_actor,
                    "approval.assign",
                    &instance.base.id,
                    runtime_transition_message(&next_step, None),
                    executor,
                )
                .await?;
                Ok(runtime_view(instance, next_step, Some(work_item)))
            }
            Err(blocker) => {
                next_step.block(blocker.code(), now)?;
                instance.advance_to(ApprovalStepInstanceId::new(next_step.base.id.clone()))?;
                instance.block(blocker.code(), now)?;
                self.db
                    .approval_step_instances()
                    .update(&mut next_step, executor)
                    .await?;
                self.db
                    .approval_instances()
                    .update(&mut instance, executor)
                    .await?;
                self.append_runtime_audit(
                    &transition_actor,
                    "approval.block",
                    &instance.base.id,
                    runtime_transition_message(&next_step, Some(blocker.code())),
                    executor,
                )
                .await?;
                Ok(runtime_view(instance, next_step, None))
            }
        }
    }

    async fn cancel_owned_transaction(&self, command: CancelApprovalCommand) -> Result<ApprovalRuntimeView> {
        let receipt = cancel_receipt(&command);
        let replay_receipt = receipt.clone();
        let runtime = self.clone();
        let result = self
            .db
            .client()
            .clone()
            .with_transaction(move |session| {
                Box::pin(async move { runtime.cancel_in_transaction(command, session).await })
            })
            .await;
        match result {
            Ok(view) => Ok(view),
            Err(error @ Error::ConflictError(_)) => {
                self.recover_receipt_after_conflict(&replay_receipt, error).await
            }
            Err(error) => Err(error),
        }
    }

    async fn cancel_in_transaction(
        &self,
        command: CancelApprovalCommand,
        executor: &mut dyn Executor,
    ) -> Result<ApprovalRuntimeView> {
        validate_cancel_command(&command)?;
        let receipt = cancel_receipt(&command);
        if let Some(view) = self.replay_command_receipt(&receipt, executor).await? {
            return Ok(view);
        }
        let mut instance = self
            .load_instance(&command.approval_instance_id, executor)
            .await?;
        ensure_version(
            instance.base.version,
            command.expected_instance_version,
            "审批实例",
        )?;
        ensure_subject_version(&instance, &command.expected_subject_version)?;
        let mut current_step = self.current_step(&instance, executor).await?;
        if current_step.base.id != command.current_step_instance_id {
            return Err(Error::ConflictError("审批当前步骤已变化".to_string()));
        }
        ensure_version(
            current_step.base.version,
            command.expected_step_version,
            "审批步骤",
        )?;
        let mut work_item = self.work_item_for_step(&current_step.base.id, executor).await?;
        ensure_optional_task_identity(work_item.as_ref(), command.current_work_item_id.as_deref())?;
        ensure_optional_task_version(work_item.as_ref(), command.expected_task_version)?;
        let context = cancel_action_context(&command, &instance, &current_step, work_item.as_ref());
        let action = cancel_action(&instance.definition_key)
            .ok_or_else(|| Error::BusinessLogicError("审批取消处理器未注册".to_string()))?;
        self.action_port.execute(action, &context, executor).await?;
        let now = Instant::now();
        current_step.cancel()?;
        instance.cancel(now)?;
        let mut pending_steps = self
            .db
            .approval_step_instances()
            .list_by_instance(&ApprovalInstanceId::new(instance.base.id.clone()), executor)
            .await?
            .into_iter()
            .filter(|step| step.base.id != current_step.base.id)
            .filter(|step| {
                matches!(
                    step.status,
                    ApprovalStepStatus::Waiting | ApprovalStepStatus::Active | ApprovalStepStatus::Blocked
                )
            })
            .collect::<Vec<_>>();
        for step in &mut pending_steps {
            step.cancel()?;
            self.db.approval_step_instances().update(step, executor).await?;
        }
        if let Some(item) = &mut work_item {
            item.close(
                &command.actor_id,
                WorkItemCloseData {
                    close_reason: command.reason,
                },
                now,
            )?;
            self.db.work_items().update(item, executor).await?;
        }
        self.db
            .approval_step_instances()
            .update(&mut current_step, executor)
            .await?;
        self.db
            .approval_instances()
            .update(&mut instance, executor)
            .await?;
        let view = runtime_view(instance, current_step, work_item);
        self.append_command_receipt(&receipt, &view, executor).await?;
        Ok(view)
    }

    async fn recover_owned_transaction(
        &self,
        command: RecoverApprovalCommand,
    ) -> Result<ApprovalRuntimeView> {
        let receipt = recover_receipt(&command);
        let replay_receipt = receipt.clone();
        let runtime = self.clone();
        let result = self
            .db
            .client()
            .clone()
            .with_transaction(move |session| {
                Box::pin(async move { runtime.recover_in_transaction(command, session).await })
            })
            .await;
        match result {
            Ok(view) => Ok(view),
            Err(error @ Error::ConflictError(_)) => {
                self.recover_receipt_after_conflict(&replay_receipt, error).await
            }
            Err(error) => Err(error),
        }
    }

    async fn recover_in_transaction(
        &self,
        command: RecoverApprovalCommand,
        executor: &mut dyn Executor,
    ) -> Result<ApprovalRuntimeView> {
        validate_recover_command(&command)?;
        let receipt = recover_receipt(&command);
        let mut instance = self
            .load_instance(&command.approval_instance_id, executor)
            .await?;
        let authorization = command
            .authorization
            .as_ref()
            .ok_or_else(|| Error::Forbidden("审批恢复命令缺少可信授权锚点".to_string()))?;
        ensure_recovery_authorization(
            &self.db,
            authorization,
            &command.actor_id,
            &instance.owner_organization_id,
            executor,
        )
        .await?;
        if let Some(view) = self.replay_command_receipt(&receipt, executor).await? {
            return Ok(view);
        }
        ensure_version(
            instance.base.version,
            command.expected_instance_version,
            "审批实例",
        )?;
        if instance.status != ApprovalInstanceStatus::Blocked {
            return Err(Error::ConflictError("审批实例已不再阻塞".to_string()));
        }
        let mut step = self.current_step(&instance, executor).await?;
        if step.base.id != command.current_step_instance_id || step.status != ApprovalStepStatus::Blocked {
            return Err(Error::ConflictError("审批阻塞步骤已变化".to_string()));
        }
        if step.blocker_code != instance.blocker_code || step.blocked_at != instance.blocked_at {
            return Err(Error::Internal("审批实例与当前步骤的阻塞事实不一致".to_string()));
        }
        ensure_version(step.base.version, command.expected_step_version, "审批步骤")?;
        let step_definition = self.step_definition(&instance, &step, executor).await?;
        let assignment = self
            .resolver
            .resolve_excluding(
                &step_definition.assignee_resolver_key,
                &instance.owner_organization_id,
                &[instance.started_by.as_str()],
                executor,
            )
            .await?;
        let assignment = assignment
            .map_err(|blocker| Error::BusinessLogicError(format!("阻塞原因尚未消除: {}", blocker.code())))?;
        let mut work_item = self.work_item_for_step(&step.base.id, executor).await?;
        ensure_optional_task_version(work_item.as_ref(), command.expected_task_version)?;
        let forbidden_owner_ids = self
            .forbidden_assignment_owners(&instance, &step, executor)
            .await?;
        let validation_context = recovery_action_context(
            &instance,
            &step,
            work_item.as_ref(),
            &command.actor_id,
            &command.reason,
            &command.idempotency_key,
        );
        let validation_action = recovery_validation_action(&instance.definition_key)
            .ok_or_else(|| Error::BusinessLogicError("审批恢复校验处理器未注册".to_string()))?;
        self.action_port
            .execute(validation_action, &validation_context, executor)
            .await?;
        ensure_recovery_authorization(
            &self.db,
            authorization,
            &command.actor_id,
            &instance.owner_organization_id,
            executor,
        )
        .await?;
        let now = Instant::now();
        match &mut work_item {
            Some(item) => {
                if self
                    .recover_existing_assignment_if_needed(
                        item,
                        &assignment,
                        &forbidden_owner_ids,
                        now,
                        executor,
                    )
                    .await?
                {
                    self.db.work_items().update(item, executor).await?;
                }
            }
            None => {
                let item = build_work_item(&instance, &step, &step_definition, assignment, now)?;
                self.db.work_items().create(&item, executor).await?;
                work_item = Some(item);
            }
        }
        step.recover()?;
        instance.recover()?;
        self.db
            .approval_step_instances()
            .update(&mut step, executor)
            .await?;
        self.db
            .approval_instances()
            .update(&mut instance, executor)
            .await?;
        let view = runtime_view(instance, step, work_item);
        self.append_runtime_audit(
            &command.actor_id,
            "approval.recover_audit",
            &command.approval_instance_id,
            Some(command.reason.clone()),
            executor,
        )
        .await?;
        self.append_command_receipt(&receipt, &view, executor).await?;
        Ok(view)
    }

    async fn recover_existing_assignment_if_needed(
        &self,
        item: &mut WorkItem,
        assignment: &ResolvedAssignment,
        forbidden_owner_ids: &[String],
        at: Instant,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        if item.owner_role != assignment.owner_role
            || item.owner_organization_id != assignment.owner_organization_id
        {
            return Err(Error::Internal(
                "阻塞审批原待办责任角色或组织与冻结步骤不一致".to_string(),
            ));
        }
        match assignment.owner_user_id.as_deref() {
            Some(_) if item.assignment_mode != AssignmentMode::Direct => Err(Error::Internal(
                "DIRECT 审批解析结果与原待办分派模式不一致".to_string(),
            )),
            Some(user_id) if item.owner_user_id.as_deref() == Some(user_id) => Ok(false),
            Some(user_id) => {
                item.recover_assignment(user_id, at)?;
                Ok(true)
            }
            None => {
                if item.assignment_mode != AssignmentMode::Pool {
                    return Err(Error::Internal(
                        "POOL 审批解析结果与原待办分派模式不一致".to_string(),
                    ));
                }
                let Some(current_owner) = item.owner_user_id.clone() else {
                    return Ok(false);
                };
                let violates_separation = owner_violates_separation(&current_owner, forbidden_owner_ids);
                if !violates_separation
                    && self
                        .resolver
                        .user_is_eligible_for_assignment(
                            &current_owner,
                            &item.owner_role,
                            &item.owner_organization_id,
                            executor,
                        )
                        .await?
                {
                    return Ok(false);
                }
                item.recover_to_pool(at)?;
                Ok(true)
            }
        }
    }

    async fn forbidden_assignment_owners(
        &self,
        instance: &ApprovalInstance,
        step: &ApprovalStepInstance,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let mut forbidden = vec![instance.started_by.clone()];
        match step.step_key.as_str() {
            SALES_MANAGER_APPROVAL => {}
            OPERATIONS_APPROVAL => {
                let manager_actor = self
                    .db
                    .approval_step_instances()
                    .list_by_instance(&ApprovalInstanceId::new(instance.base.id.clone()), executor)
                    .await?
                    .into_iter()
                    .find(|candidate| candidate.step_key == SALES_MANAGER_APPROVAL)
                    .and_then(|manager_step| manager_step.decided_by)
                    .ok_or_else(|| Error::Internal("运营审批缺少销售领导正式决定人".to_string()))?;
                forbidden.push(manager_actor);
            }
            _ => {
                return Err(Error::BusinessLogicError(
                    "当前审批步骤未注册岗位分离策略".to_string(),
                ));
            }
        }
        forbidden.sort();
        forbidden.dedup();
        Ok(forbidden)
    }

    async fn load_instance(&self, id: &str, executor: &mut dyn Executor) -> Result<ApprovalInstance> {
        let instance = self
            .db
            .approval_instances()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("审批实例不存在".to_string()))?;
        ensure_internal_runtime_identity(instance.runtime_kind, instance.external_instance_id.as_deref())?;
        Ok(instance)
    }

    async fn ensure_separation_of_duties(
        &self,
        instance: &ApprovalInstance,
        current_step: &ApprovalStepInstance,
        actor_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        match current_step.step_key.as_str() {
            SALES_MANAGER_APPROVAL => ensure_actor_separated(&instance.started_by, None, actor_id),
            OPERATIONS_APPROVAL => {
                let manager_step = self
                    .db
                    .approval_step_instances()
                    .list_by_instance(&ApprovalInstanceId::new(instance.base.id.clone()), executor)
                    .await?
                    .into_iter()
                    .find(|step| step.step_key == SALES_MANAGER_APPROVAL)
                    .ok_or_else(|| Error::Internal("运营审批缺少销售领导前置步骤".to_string()))?;
                let manager_actor = manager_step
                    .decided_by
                    .as_deref()
                    .ok_or_else(|| Error::Internal("运营审批缺少销售领导正式决定人".to_string()))?;
                ensure_actor_separated(&instance.started_by, Some(manager_actor), actor_id)
            }
            _ => Err(Error::BusinessLogicError(
                "当前审批步骤未注册岗位分离策略".to_string(),
            )),
        }
    }

    async fn step_definition(
        &self,
        instance: &ApprovalInstance,
        step: &ApprovalStepInstance,
        executor: &mut dyn Executor,
    ) -> Result<ApprovalStepDefinition> {
        let definition = self
            .db
            .approval_definitions()
            .find_by_key_version(&instance.definition_key, instance.definition_version, executor)
            .await?
            .ok_or_else(|| Error::Internal("冻结审批定义不存在".to_string()))?;
        self.db
            .approval_step_definitions()
            .list_by_definition(&ApprovalDefinitionId::new(definition.base.id.clone()), executor)
            .await?
            .into_iter()
            .find(|definition| definition.step_key == step.step_key)
            .ok_or_else(|| Error::Internal("冻结审批步骤定义不存在".to_string()))
    }
}

fn ensure_actor_separated(started_by: &str, sales_manager_actor: Option<&str>, actor_id: &str) -> Result<()> {
    if actor_id == started_by {
        return Err(Error::Forbidden("审批处理人不得与提交人为同一账号".to_string()));
    }
    if sales_manager_actor == Some(actor_id) {
        return Err(Error::Forbidden(
            "运营审批处理人不得与销售领导决定人为同一账号".to_string(),
        ));
    }
    Ok(())
}

fn ensure_internal_runtime_identity(
    runtime_kind: entities::approval::ApprovalRuntimeKind,
    external_instance_id: Option<&str>,
) -> Result<()> {
    if runtime_kind != entities::approval::ApprovalRuntimeKind::Internal || external_instance_id.is_some() {
        return Err(Error::BusinessLogicError(
            "当前审批实例不属于 INTERNAL 运行时，禁止由本地运行时推进".to_string(),
        ));
    }
    Ok(())
}

fn owner_violates_separation(owner_user_id: &str, forbidden_owner_ids: &[String]) -> bool {
    forbidden_owner_ids
        .iter()
        .any(|forbidden| forbidden == owner_user_id)
}

impl ApprovalRuntimePort for InternalApprovalRuntime {
    fn start_approval<'a>(&'a self, command: StartApprovalCommand) -> ApprovalRuntimeFuture<'a> {
        Box::pin(async move { self.start_owned_transaction(command).await })
    }

    fn submit_decision<'a>(&'a self, command: SubmitDecisionCommand) -> ApprovalRuntimeFuture<'a> {
        Box::pin(async move { self.submit_owned_transaction(command).await })
    }

    fn cancel_approval<'a>(&'a self, command: CancelApprovalCommand) -> ApprovalRuntimeFuture<'a> {
        Box::pin(async move { self.cancel_owned_transaction(command).await })
    }

    fn recover_approval<'a>(&'a self, command: RecoverApprovalCommand) -> ApprovalRuntimeFuture<'a> {
        Box::pin(async move { self.recover_owned_transaction(command).await })
    }
}

fn validate_start_command(command: &StartApprovalCommand) -> Result<()> {
    for (value, message) in [
        (&command.definition_key, "审批定义不能为空"),
        (&command.business_object_type, "业务对象类型不能为空"),
        (&command.business_object_id, "业务对象ID不能为空"),
        (&command.subject_version, "业务版本不能为空"),
        (&command.owner_organization_id, "责任组织不能为空"),
        (&command.started_by, "审批启动人不能为空"),
        (&command.idempotency_key, "幂等键不能为空"),
    ] {
        if value.trim().is_empty() {
            return Err(Error::ValidationError(message.to_string()));
        }
        if value != value.trim() {
            return Err(Error::ValidationError(format!("{message}，且不得包含首尾空白")));
        }
    }
    Ok(())
}

fn ensure_same_start_request(instance: &ApprovalInstance, command: &StartApprovalCommand) -> Result<()> {
    let same_payload = instance.definition_key == command.definition_key
        && instance.business_object_type == command.business_object_type
        && instance.business_object_id == command.business_object_id
        && instance.subject_version == command.subject_version
        && instance.owner_organization_id == command.owner_organization_id
        && instance.started_by == command.started_by
        && instance.start_idempotency_key == command.idempotency_key;
    if !same_payload {
        return Err(Error::ConflictError(
            "审批启动幂等键已用于不同的冻结请求".to_string(),
        ));
    }
    Ok(())
}

fn validate_decision_command(command: &SubmitDecisionCommand) -> Result<()> {
    for (value, label) in [
        (&command.work_item_id, "当前待办ID"),
        (&command.approval_instance_id, "审批实例ID"),
        (&command.approval_step_instance_id, "审批步骤实例ID"),
        (&command.expected_subject_version, "业务版本"),
        (&command.actor_id, "决定人"),
        (&command.idempotency_key, "幂等键"),
    ] {
        validate_command_text(value, label, 256)?;
    }
    if matches!(
        command.decision,
        ApprovalDecision::RejectToApplicant | ApprovalDecision::TerminateApproval
    ) && command
        .reason
        .as_deref()
        .is_none_or(|reason| reason.trim().is_empty())
    {
        return Err(Error::ValidationError("驳回或终止必须填写原因".to_string()));
    }
    Ok(())
}

fn validate_cancel_command(command: &CancelApprovalCommand) -> Result<()> {
    for (value, label, max_len) in [
        (&command.approval_instance_id, "审批实例ID", 256),
        (&command.current_step_instance_id, "审批步骤实例ID", 256),
        (&command.expected_subject_version, "业务版本", 256),
        (&command.actor_id, "撤回人", 128),
        (&command.reason, "撤回原因", 512),
        (&command.idempotency_key, "幂等键", 256),
    ] {
        validate_command_text(value, label, max_len)?;
    }
    if let Some(work_item_id) = &command.current_work_item_id {
        validate_command_text(work_item_id, "当前待办ID", 256)?;
    }
    Ok(())
}

fn validate_recover_command(command: &RecoverApprovalCommand) -> Result<()> {
    for (value, label, max_len) in [
        (&command.approval_instance_id, "审批实例ID", 256),
        (&command.current_step_instance_id, "审批步骤实例ID", 256),
        (&command.actor_id, "恢复人", 128),
        (&command.reason, "恢复原因", 256),
        (&command.idempotency_key, "幂等键", 256),
    ] {
        validate_command_text(value, label, max_len)?;
    }
    if command.recovery_action != super::dto::ApprovalRecoveryAction::RetryCurrentStep {
        return Err(Error::ValidationError(
            "阻塞审批只允许 RETRY_CURRENT_STEP".to_string(),
        ));
    }
    Ok(())
}

fn validate_command_text(value: &str, label: &str, max_len: usize) -> Result<()> {
    if value.trim().is_empty() {
        return Err(Error::ValidationError(format!("{label}不能为空")));
    }
    if value != value.trim() {
        return Err(Error::ValidationError(format!("{label}不得包含首尾空白")));
    }
    if value.chars().count() > max_len {
        return Err(Error::ValidationError(format!(
            "{label}不能超过 {max_len} 个字符"
        )));
    }
    Ok(())
}

fn validate_blocked_query(params: &BlockedApprovalListParams) -> Result<()> {
    if params.status != Some(ApprovalInstanceStatus::Blocked) {
        return Err(Error::ValidationError(
            "阻塞审批列表仅允许 status=BLOCKED".to_string(),
        ));
    }
    if params.page == 0 {
        return Err(Error::ValidationError("page 必须大于 0".to_string()));
    }
    if params.page_size == 0 || params.page_size > 100 {
        return Err(Error::ValidationError(
            "page_size 必须在 1 到 100 之间".to_string(),
        ));
    }
    Ok(())
}

fn build_step_instances(
    instance_id: &ApprovalInstanceId,
    definitions: &[ApprovalStepDefinition],
) -> Result<Vec<ApprovalStepInstance>> {
    definitions
        .iter()
        .enumerate()
        .map(|(index, definition)| {
            ApprovalStepInstance::new(
                ApprovalStepInstanceId::new(next_id()),
                ApprovalStepInstanceData {
                    approval_instance_id: instance_id.clone(),
                    step_key: definition.step_key.clone(),
                    sequence_no: definition.sequence_no,
                    initial_status: if index == 0 {
                        ApprovalStepStatus::Active
                    } else {
                        ApprovalStepStatus::Waiting
                    },
                    external_activity_id: None,
                },
            )
            .map_err(Into::into)
        })
        .collect()
}

fn build_work_item(
    instance: &ApprovalInstance,
    step: &ApprovalStepInstance,
    definition: &ApprovalStepDefinition,
    assignment: ResolvedAssignment,
    at: Instant,
) -> Result<WorkItem> {
    WorkItem::new_at(
        WorkItemId::new(next_id()),
        WorkItemData {
            work_item_type: definition.work_item_type,
            approval_step_instance_id: Some(step.base.id.clone()),
            business_object_type: instance.business_object_type.clone(),
            business_object_id: instance.business_object_id.clone(),
            subject_version: instance.subject_version.clone(),
            assignment_mode: match definition.assignment_mode {
                entities::approval::ApprovalAssignmentMode::Direct => AssignmentMode::Direct,
                entities::approval::ApprovalAssignmentMode::Pool => AssignmentMode::Pool,
            },
            owner_role: assignment.owner_role,
            owner_organization_id: assignment.owner_organization_id,
            owner_user_id: assignment.owner_user_id,
            assignment_source: AssignmentSource::StepResolver,
            priority: WorkItemPriority::High,
            due_at: None,
            reason_code: Some(format!("{}_ACTIVE", definition.step_key)),
            impact_summary: Some("审批等待处理".to_string()),
        },
        at,
    )
    .map_err(Into::into)
}

fn verify_frozen_steps(
    actual: &[ApprovalStepDefinition],
    registered: &[super::registry::RegisteredApprovalStep],
) -> Result<()> {
    if actual.len() != registered.len() {
        return Err(Error::Internal("已发布审批定义步骤数量漂移".to_string()));
    }
    for (actual, expected) in actual.iter().zip(registered) {
        if actual.step_key != expected.step_key
            || actual.sequence_no != expected.sequence_no
            || actual.work_item_type != expected.work_item_type
            || actual.handler_key != expected.handler_key
            || actual.assignment_mode != expected.assignment_mode
            || actual.assignee_resolver_key != expected.resolver_key
            || actual.allowed_decisions != expected.allowed_decisions
        {
            return Err(Error::Internal("已发布审批定义内容漂移".to_string()));
        }
    }
    Ok(())
}

fn runtime_view(
    instance: ApprovalInstance,
    step: ApprovalStepInstance,
    work_item: Option<WorkItem>,
) -> ApprovalRuntimeView {
    ApprovalRuntimeView {
        instance: instance.into(),
        step: step.into(),
        work_item: work_item.map(Into::into),
    }
}

fn blocked_view(
    instance: ApprovalInstance,
    step: ApprovalStepInstance,
    work_item: Option<WorkItem>,
    can_recover: bool,
) -> Result<BlockedApprovalView> {
    let blocker_code = instance
        .blocker_code
        .clone()
        .ok_or_else(|| Error::Internal("阻塞审批实例缺少 blocker_code".to_string()))?;
    let blocked_at = instance
        .blocked_at
        .ok_or_else(|| Error::Internal("阻塞审批实例缺少 blocked_at".to_string()))?;
    if step.status != ApprovalStepStatus::Blocked
        || step.blocker_code.as_deref() != Some(blocker_code.as_str())
        || step.blocked_at != Some(blocked_at)
    {
        return Err(Error::Internal("审批实例与当前步骤的阻塞事实不一致".to_string()));
    }
    Ok(BlockedApprovalView {
        approval_instance_id: instance.base.id,
        instance_version: instance.base.version.to_string(),
        current_step_instance_id: step.base.id,
        step_version: step.base.version.to_string(),
        business_object_label: instance.business_object_id.clone(),
        blocker_message: blocker_message(&blocker_code).to_string(),
        blocker_code,
        blocked_at,
        work_item: work_item.map(Into::into),
        allowed_actions: can_recover
            .then_some(super::dto::ApprovalRecoveryAction::RetryCurrentStep)
            .into_iter()
            .collect(),
    })
}

fn blocker_message(code: &str) -> &'static str {
    match code {
        "APPROVAL_DIRECT_ASSIGNEE_NOT_UNIQUE" => "未能确定唯一审批负责人",
        "APPROVAL_ORGANIZATION_SCOPE_UNPROVEN" => "审批责任组织范围尚未配置完整",
        "APPROVAL_OWNER_ROLE_UNAVAILABLE" => "审批责任角色当前不可用",
        "APPROVAL_RESOLVER_NOT_REGISTERED" => "审批责任规则尚未注册",
        _ => "审批推进条件尚未满足",
    }
}

fn ensure_version(actual: u64, expected: u64, object: &str) -> Result<()> {
    if actual != expected {
        return Err(Error::ConflictError(format!("{object}已更新，请刷新后重试")));
    }
    Ok(())
}

fn ensure_subject_version(instance: &ApprovalInstance, expected: &str) -> Result<()> {
    if instance.subject_version != expected {
        return Err(Error::ConflictError("业务版本已更新，请刷新后重试".to_string()));
    }
    Ok(())
}

fn ensure_current_work_item(work_item: &WorkItem, step: &ApprovalStepInstance, actor_id: &str) -> Result<()> {
    if work_item.status != WorkItemStatus::Open
        || work_item.approval_step_instance_id.as_deref() != Some(&step.base.id)
        || work_item.owner_user_id.as_deref() != Some(actor_id)
    {
        return Err(Error::Forbidden("当前用户不再拥有该审批任务处理权".to_string()));
    }
    Ok(())
}

fn ensure_optional_task_version(item: Option<&WorkItem>, expected: Option<u64>) -> Result<()> {
    match (item, expected) {
        (Some(item), Some(expected)) => ensure_version(item.base.version, expected, "审批任务"),
        (None, None) => Ok(()),
        _ => Err(Error::ConflictError(
            "审批任务事实已变化，请刷新后重试".to_string(),
        )),
    }
}

fn ensure_optional_task_identity(item: Option<&WorkItem>, expected_id: Option<&str>) -> Result<()> {
    match (item, expected_id) {
        (Some(item), Some(expected)) if item.base.id == expected => Ok(()),
        (None, None) => Ok(()),
        _ => Err(Error::ConflictError(
            "审批任务身份已变化，请刷新后重试".to_string(),
        )),
    }
}

fn decision_action_context(
    command: &SubmitDecisionCommand,
    instance: &ApprovalInstance,
    step: &ApprovalStepInstance,
    work_item: &WorkItem,
) -> ApprovalActionContext {
    ApprovalActionContext {
        definition_key: instance.definition_key.clone(),
        approval_instance_id: instance.base.id.clone(),
        approval_step_instance_id: Some(step.base.id.clone()),
        work_item_id: Some(work_item.base.id.clone()),
        business_object_type: instance.business_object_type.clone(),
        business_object_id: instance.business_object_id.clone(),
        subject_version: instance.subject_version.clone(),
        actor_id: command.actor_id.clone(),
        reason: command.reason.clone(),
        idempotency_key: command.idempotency_key.clone(),
    }
}

fn cancel_action_context(
    command: &CancelApprovalCommand,
    instance: &ApprovalInstance,
    step: &ApprovalStepInstance,
    work_item: Option<&WorkItem>,
) -> ApprovalActionContext {
    ApprovalActionContext {
        definition_key: instance.definition_key.clone(),
        approval_instance_id: instance.base.id.clone(),
        approval_step_instance_id: Some(step.base.id.clone()),
        work_item_id: work_item.map(|item| item.base.id.clone()),
        business_object_type: instance.business_object_type.clone(),
        business_object_id: instance.business_object_id.clone(),
        subject_version: instance.subject_version.clone(),
        actor_id: command.actor_id.clone(),
        reason: Some(command.reason.clone()),
        idempotency_key: command.idempotency_key.clone(),
    }
}

fn submit_receipt(command: &SubmitDecisionCommand) -> ApprovalCommandReceipt {
    let expected_task_version = command.expected_task_version.to_string();
    let expected_instance_version = command.expected_instance_version.to_string();
    let expected_step_version = command.expected_step_version.to_string();
    let reason = command.reason.as_deref().unwrap_or("");
    let payload_hash = stable_digest(&[
        &command.work_item_id,
        &command.approval_instance_id,
        &command.approval_step_instance_id,
        &expected_task_version,
        &expected_instance_version,
        &expected_step_version,
        &command.expected_subject_version,
        command.decision.as_str(),
        if command.reason.is_some() { "SOME" } else { "NONE" },
        reason,
        &command.actor_id,
    ]);
    ApprovalCommandReceipt::new(
        ApprovalCommandKind::SubmitDecision,
        &command.approval_instance_id,
        &command.actor_id,
        &command.idempotency_key,
        payload_hash,
    )
}

fn cancel_receipt(command: &CancelApprovalCommand) -> ApprovalCommandReceipt {
    let expected_instance_version = command.expected_instance_version.to_string();
    let expected_step_version = command.expected_step_version.to_string();
    let expected_task_version = command
        .expected_task_version
        .map_or_else(|| "NONE".to_string(), |version| format!("SOME:{version}"));
    let payload_hash = stable_digest(&[
        &command.approval_instance_id,
        &command.current_step_instance_id,
        command.current_work_item_id.as_deref().unwrap_or(""),
        if command.current_work_item_id.is_some() {
            "SOME"
        } else {
            "NONE"
        },
        &expected_instance_version,
        &expected_step_version,
        &expected_task_version,
        &command.expected_subject_version,
        &command.actor_id,
        &command.reason,
    ]);
    ApprovalCommandReceipt::new(
        ApprovalCommandKind::CancelApproval,
        &command.approval_instance_id,
        &command.actor_id,
        &command.idempotency_key,
        payload_hash,
    )
}

fn recover_receipt(command: &RecoverApprovalCommand) -> ApprovalCommandReceipt {
    let expected_instance_version = command.expected_instance_version.to_string();
    let expected_step_version = command.expected_step_version.to_string();
    let expected_task_version = command
        .expected_task_version
        .map_or_else(|| "NONE".to_string(), |version| format!("SOME:{version}"));
    let payload_hash = stable_digest(&[
        &command.approval_instance_id,
        &command.current_step_instance_id,
        &expected_instance_version,
        &expected_step_version,
        &expected_task_version,
        "RETRY_CURRENT_STEP",
        &command.reason,
        &command.actor_id,
    ]);
    ApprovalCommandReceipt::new(
        ApprovalCommandKind::RecoverApproval,
        &command.approval_instance_id,
        &command.actor_id,
        &command.idempotency_key,
        payload_hash,
    )
}

fn stable_digest(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        let bytes = part.as_bytes();
        hasher.update(u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_be_bytes());
        hasher.update(bytes);
    }
    format!("{:x}", hasher.finalize())
}

fn command_receipt_message(receipt: &ApprovalCommandReceipt, result: &ApprovalRuntimeView) -> Result<String> {
    serde_json::to_string(&ApprovalCommandReceiptPayload {
        payload_hash: receipt.payload_hash.clone(),
        result: result.clone(),
    })
    .map_err(|error| Error::Internal(format!("审批命令回执序列化失败: {error}")))
}

fn parse_command_receipt_message(message: &str) -> Result<ApprovalCommandReceiptPayload> {
    serde_json::from_str(message).map_err(|_| Error::ConflictError("审批命令幂等回执格式无效".to_string()))
}

fn runtime_transition_message(step: &ApprovalStepInstance, blocker_code: Option<&str>) -> Option<String> {
    let step_digest = stable_digest(&[&step.base.id]);
    Some(match blocker_code {
        Some(blocker_code) => format!("step_sha256={step_digest};blocker={blocker_code}"),
        None => format!("step_sha256={step_digest}"),
    })
}

#[cfg(test)]
mod tests {
    use entities::approval::{
        ApprovalDecision, ApprovalInstanceStatus, ApprovalRuntimeKind, ApprovalStepStatus,
    };
    use entities::common::time::Instant;
    use entities::work_item::{AssignmentMode, WorkItemStatus, WorkItemType};

    use crate::approval::{
        ApprovalInstanceView, ApprovalRecoveryAction, ApprovalRuntimeView, ApprovalStepInstanceView,
        ApprovalWorkItemView,
    };

    use super::{
        command_receipt_message, ensure_actor_separated, ensure_internal_runtime_identity,
        owner_violates_separation, parse_command_receipt_message, recover_receipt, validate_blocked_query,
        validate_recover_command, ApprovalCommandKind, ApprovalCommandReceipt, BlockedApprovalListParams,
        RecoverApprovalCommand,
    };

    #[test]
    fn internal_runtime_rejects_bpm_or_external_instance_identity() {
        assert!(ensure_internal_runtime_identity(ApprovalRuntimeKind::Internal, None).is_ok());
        assert!(ensure_internal_runtime_identity(ApprovalRuntimeKind::Bpm, None).is_err());
        assert!(ensure_internal_runtime_identity(ApprovalRuntimeKind::Internal, Some("external-1")).is_err());
    }

    #[test]
    fn blocked_query_rejects_missing_status_and_unbounded_page() {
        let missing = BlockedApprovalListParams {
            status: None,
            page: 1,
            page_size: 20,
        };
        assert!(validate_blocked_query(&missing).is_err());
        let too_large = BlockedApprovalListParams {
            status: Some(ApprovalInstanceStatus::Blocked),
            page: 1,
            page_size: 101,
        };
        assert!(validate_blocked_query(&too_large).is_err());
        let zero_page = BlockedApprovalListParams {
            status: Some(ApprovalInstanceStatus::Blocked),
            page: 0,
            page_size: 20,
        };
        assert!(validate_blocked_query(&zero_page).is_err());
    }

    #[test]
    fn recovery_command_requires_reason_actor_and_idempotency() {
        let command = RecoverApprovalCommand {
            approval_instance_id: "instance-1".to_string(),
            current_step_instance_id: "step-1".to_string(),
            expected_instance_version: 1,
            expected_step_version: 1,
            expected_task_version: None,
            recovery_action: ApprovalRecoveryAction::RetryCurrentStep,
            reason: " ".to_string(),
            idempotency_key: "request-1".to_string(),
            actor_id: "admin-1".to_string(),
            authorization: None,
        };
        assert!(validate_recover_command(&command).is_err());
    }

    #[test]
    fn command_receipt_reuses_key_identity_but_hashes_the_full_payload() {
        let mut command = RecoverApprovalCommand {
            approval_instance_id: "instance-1".to_string(),
            current_step_instance_id: "step-1".to_string(),
            expected_instance_version: 1,
            expected_step_version: 1,
            expected_task_version: None,
            recovery_action: ApprovalRecoveryAction::RetryCurrentStep,
            reason: "修复责任范围".to_string(),
            idempotency_key: "request-1".to_string(),
            actor_id: "admin-1".to_string(),
            authorization: None,
        };
        let first = recover_receipt(&command);
        command.reason = "改为另一个原因".to_string();
        let changed = recover_receipt(&command);
        assert_eq!(first.id, changed.id);
        assert_ne!(first.payload_hash, changed.payload_hash);
    }

    #[test]
    fn command_receipt_freezes_the_original_runtime_view() {
        let at = Instant::from_unix_secs(1_700_000_000);
        let original = ApprovalRuntimeView {
            instance: ApprovalInstanceView {
                id: "instance-1".to_string(),
                definition_key: "CARD_SALES".to_string(),
                definition_version: 1,
                runtime_kind: ApprovalRuntimeKind::Internal,
                business_object_type: "sales_order".to_string(),
                business_object_id: "sales-order-1".to_string(),
                subject_version: "submission-1".to_string(),
                owner_organization_id: "company".to_string(),
                status: ApprovalInstanceStatus::Running,
                current_step_instance_id: Some("step-2".to_string()),
                instance_version: "2".to_string(),
                blocker_code: None,
                blocked_at: None,
                started_by: "sales-1".to_string(),
                started_at: at,
                ended_at: None,
            },
            step: ApprovalStepInstanceView {
                id: "step-1".to_string(),
                approval_instance_id: "instance-1".to_string(),
                step_key: "MANAGER".to_string(),
                sequence_no: 1,
                status: ApprovalStepStatus::Approved,
                step_version: "2".to_string(),
                decision: Some(ApprovalDecision::Approve),
                decision_reason: None,
                decided_by: Some("manager-1".to_string()),
                decided_at: Some(at),
                blocker_code: None,
                blocked_at: None,
            },
            work_item: Some(ApprovalWorkItemView {
                id: "work-item-2".to_string(),
                work_item_type: WorkItemType::CardSalesOperationApproval,
                approval_step_instance_id: Some("step-2".to_string()),
                status: WorkItemStatus::Open,
                assignment_mode: AssignmentMode::Pool,
                owner_role: "role-operations".to_string(),
                owner_organization_id: "company".to_string(),
                owner_user_id: None,
                task_version: "1".to_string(),
            }),
        };
        let receipt = ApprovalCommandReceipt::new(
            ApprovalCommandKind::SubmitDecision,
            "instance-1",
            "manager-1",
            "request-1",
            "payload-hash".to_string(),
        );
        let message = command_receipt_message(&receipt, &original).unwrap();
        let mut later = original.clone();
        later.instance.status = ApprovalInstanceStatus::Approved;

        let replayed = parse_command_receipt_message(&message).unwrap().result;
        assert_eq!(replayed, original);
        assert_ne!(replayed, later);
    }

    #[test]
    fn separation_rejects_submitter_and_previous_sales_manager() {
        assert!(ensure_actor_separated("submitter", None, "submitter").is_err());
        assert!(ensure_actor_separated("submitter", Some("manager"), "manager").is_err());
        assert!(ensure_actor_separated("submitter", Some("manager"), "operator").is_ok());
        let forbidden = vec!["submitter".to_string(), "manager".to_string()];
        assert!(owner_violates_separation("submitter", &forbidden));
        assert!(owner_violates_separation("manager", &forbidden));
        assert!(!owner_violates_separation("operator", &forbidden));
    }
}
