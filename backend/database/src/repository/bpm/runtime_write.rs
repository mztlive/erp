use bpm::ids::ApprovalNodeExecutionId;
use bpm::model::types::{ApprovalCommandKind, ApprovalNodeExecutionStatus, ApprovalProcessInstanceStatus};
use bpm::model::{
    ApprovalCommandReceipt, ApprovalInstanceAssignee, ApprovalNodeExecution, ApprovalProcessInstance,
    IdempotencyKey,
};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, serialize_to_document, Document};

use super::{
    i64_version, merge_documents, ApprovalInstanceListProjection, BpmWorkflowRepository, CasReplaceSpec,
    CasWriteOutcome, ASSIGNEES, EXECUTIONS, INSTANCES, RECEIPTS,
};
use crate::executor::Executor;
use crate::{mongo_ops, Error, Result};

impl<'a> BpmWorkflowRepository<'a> {
    /// 只写入 BPM 运行事实：实例、审批人、首个执行和命令收据。
    ///
    /// 实例插入必须同时写入有界列表投影；列表不得再扫执行历史补全当前节点、
    /// 当前审批人、最近驳回与最近状态变更时间。
    ///
    /// # 错误
    /// 唯一索引冲突或 MongoDB 写入失败时返回错误。
    pub async fn create_bpm_runtime(
        &self,
        instance: &ApprovalProcessInstance,
        assignees: &[ApprovalInstanceAssignee],
        first_execution: &ApprovalNodeExecution,
        receipt: &ApprovalCommandReceipt,
        list_projection: &ApprovalInstanceListProjection,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.create_bpm_runtime_after_receipt(
            instance,
            assignees,
            first_execution,
            list_projection,
            executor,
        )
        .await?;
        self.insert_command_receipt(receipt, executor).await
    }

    /// 在命令收据已先行仲裁后写入启动实例、审批人绑定和首个执行。
    ///
    /// 本方法不写收据、不创建事务。要求并发启动以收据作为第一写的调用方，
    /// 必须先在同一事务调用 [`Self::insert_command_receipt`]，再调用本方法。
    ///
    /// # 错误
    /// 任一运行集合插入失败时返回错误，调用方事务必须整体回滚。
    pub async fn create_bpm_runtime_after_receipt(
        &self,
        instance: &ApprovalProcessInstance,
        assignees: &[ApprovalInstanceAssignee],
        first_execution: &ApprovalNodeExecution,
        list_projection: &ApprovalInstanceListProjection,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self.db.collection(INSTANCES),
            &instance_insert_document(instance, list_projection)?,
            executor,
        )
        .await?;
        mongo_ops::insert_many(&self.db.collection(ASSIGNEES), assignees.to_vec(), executor).await?;
        mongo_ops::insert_one(&self.db.collection(EXECUTIONS), first_execution, executor).await
    }

    /// 按幂等键读取命令收据。
    ///
    /// `scope_id` 只允许调用方传入当前 v3 scope 或其命令协议显式登记的历史
    /// scope；幂等键必须先形成 [`IdempotencyKey`]，仓储不接受或二次规范 raw
    /// 输入。未命中返回 `Ok(None)`，唯一索引竞争后的恢复读取必须由 Service 在
    /// 可用的新会话中编排。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn find_command_receipt(
        &self,
        command_kind: ApprovalCommandKind,
        scope_id: &str,
        idempotency_key: &IdempotencyKey,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalCommandReceipt>> {
        self.receipts()
            .find_one(
                receipt_key_filter(command_kind, scope_id, idempotency_key),
                executor,
            )
            .await
    }

    /// 持久化已由引擎形成的取消实例、结束执行和命令收据。
    ///
    /// 本方法不创建事务；调用方必须传入与业务单据、任务关闭相同的事务执行器。
    /// 实例和执行均按引擎自增后的版本反推加载版本，并以当前令牌和
    /// `ACTIVE|BLOCKED` 状态执行 CAS。
    ///
    /// # 参数
    /// * `instance` - 已进入 `CANCELLED` 的实例快照
    /// * `updated_executions` - 已随实例取消结束的当前执行集合
    /// * `receipt` - 本次取消命令收据
    /// * `executor` - 调用方事务执行器
    ///
    /// # 返回
    /// 全部 BPM 运行事实写入成功时返回 `Ok(())`。
    ///
    /// # 错误
    /// 取消计划缺少执行、版本元数据非法、CAS 未命中或 MongoDB 写入失败时返回错误。
    pub async fn persist_cancelled_runtime(
        &self,
        instance: &ApprovalProcessInstance,
        updated_executions: &[ApprovalNodeExecution],
        receipt: &ApprovalCommandReceipt,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.persist_cancelled_runtime_after_receipt(instance, updated_executions, executor)
            .await?;
        self.insert_command_receipt(receipt, executor).await
    }

    /// 在命令收据已先行仲裁后持久化取消实例与结束执行。
    ///
    /// 本方法不写命令收据、不创建事务。需要以唯一收据作为并发仲裁点的调用方
    /// 必须先在同一事务内调用 [`Self::insert_command_receipt`]，再调用本方法，
    /// 最后写业务单据、任务、通知和审计。旧调用方继续使用
    /// [`Self::persist_cancelled_runtime`]，其行为保持不变。
    ///
    /// # 错误
    /// 取消计划缺少执行、版本元数据非法、CAS 未命中或 MongoDB 写入失败时返回错误。
    pub async fn persist_cancelled_runtime_after_receipt(
        &self,
        instance: &ApprovalProcessInstance,
        updated_executions: &[ApprovalNodeExecution],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let current = updated_executions
            .first()
            .ok_or(Error::EntityMetadataOutOfRange("cancelled_execution"))?;
        let current_id = ApprovalNodeExecutionId::new(current.base.id.clone());
        let expected_instance_version = previous_version(instance.base.version)?;
        require_cas_applied(
            self.advance_instance(
                instance,
                expected_instance_version,
                &current_id,
                &cancelled_instance_projection(instance),
                executor,
            )
            .await?,
        )?;
        for execution in updated_executions {
            let expected_execution_version = previous_version(execution.base.version)?;
            require_cas_applied(
                self.end_cancellable_execution(execution, expected_execution_version, executor)
                    .await?,
            )?;
        }
        Ok(())
    }

    /// 以 `id + expected_instance_version + current_execution_id + RUNNING|BLOCKED` 推进实例。
    ///
    /// # 错误
    /// 元数据越界或 MongoDB 写入失败时返回错误。
    pub async fn advance_instance(
        &self,
        instance: &ApprovalProcessInstance,
        expected_instance_version: u64,
        expected_current_execution_id: &ApprovalNodeExecutionId,
        list_projection: &ApprovalInstanceListProjection,
        executor: &mut dyn Executor,
    ) -> Result<CasWriteOutcome<ApprovalProcessInstance>> {
        let filter = instance_advance_filter(
            &instance.base.id,
            expected_instance_version,
            expected_current_execution_id,
        )?;
        self.cas_replace(
            CasReplaceSpec {
                collection: INSTANCES,
                filter,
                entity: instance,
                expected_version: expected_instance_version,
                extra_set: Some(serialize_to_document(list_projection)?),
            },
            |current| {
                matches!(
                    current.status,
                    ApprovalProcessInstanceStatus::Running | ApprovalProcessInstanceStatus::Blocked
                ) && current.current_node_execution_id.as_ref() == Some(expected_current_execution_id)
            },
            executor,
        )
        .await
    }

    /// 以 `id + expected_execution_version + ACTIVE` 结束活动执行。
    ///
    /// # 错误
    /// 元数据越界或 MongoDB 写入失败时返回错误。
    pub async fn end_active_execution(
        &self,
        execution: &ApprovalNodeExecution,
        expected_execution_version: u64,
        executor: &mut dyn Executor,
    ) -> Result<CasWriteOutcome<ApprovalNodeExecution>> {
        self.cas_end_execution(
            execution,
            expected_execution_version,
            ApprovalNodeExecutionStatus::Active,
            executor,
        )
        .await
    }

    /// 以 `id + expected_execution_version + BLOCKED` 结束受阻执行。
    ///
    /// # 参数
    /// * `execution` - 已由恢复引擎置为 `SUPERSEDED` 的旧执行
    /// * `expected_execution_version` - 调用方持有的受阻执行版本
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回应用、缺失、版本冲突或状态变化的 CAS 分类。
    ///
    /// # 错误
    /// 元数据越界或 MongoDB 写入失败时返回错误。
    ///
    /// # 关键业务约束
    /// 仅人员恢复可使用本端口；不得接受活动执行或任意当前状态。
    pub async fn end_blocked_execution(
        &self,
        execution: &ApprovalNodeExecution,
        expected_execution_version: u64,
        executor: &mut dyn Executor,
    ) -> Result<CasWriteOutcome<ApprovalNodeExecution>> {
        self.cas_end_execution(
            execution,
            expected_execution_version,
            ApprovalNodeExecutionStatus::Blocked,
            executor,
        )
        .await
    }

    /// 以 `id + expected_execution_version + ACTIVE|BLOCKED` 结束可取消执行。
    ///
    /// # 参数
    /// * `execution` - 已由引擎置为 `CANCELLED` 的当前执行
    /// * `expected_execution_version` - 引擎变更前的加载版本
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回应用、缺失、版本冲突或状态变化的 CAS 分类。
    ///
    /// # 错误
    /// 元数据越界或 MongoDB 写入失败时返回错误。
    async fn end_cancellable_execution(
        &self,
        execution: &ApprovalNodeExecution,
        expected_execution_version: u64,
        executor: &mut dyn Executor,
    ) -> Result<CasWriteOutcome<ApprovalNodeExecution>> {
        let filter = cancellable_execution_end_filter(&execution.base.id, expected_execution_version)?;
        self.cas_replace(
            CasReplaceSpec {
                collection: EXECUTIONS,
                filter,
                entity: execution,
                expected_version: expected_execution_version,
                extra_set: None,
            },
            |current| current.status.is_current(),
            executor,
        )
        .await
    }

    /// 插入新的节点执行。原审批人恢复不得更新旧 `CLOSED` 任务对应的旧执行。
    ///
    /// # 错误
    /// 唯一索引冲突或 MongoDB 写入失败时返回错误。
    pub async fn insert_execution(
        &self,
        execution: &ApprovalNodeExecution,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(&self.db.collection(EXECUTIONS), execution, executor).await
    }

    /// 启动实例时一次性冻结全部节点审批人绑定；运行时不得更新或追加。
    ///
    /// # 错误
    /// 唯一索引冲突或 MongoDB 写入失败时返回错误。
    pub async fn insert_assignees(
        &self,
        assignees: &[ApprovalInstanceAssignee],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_many(&self.db.collection(ASSIGNEES), assignees.to_vec(), executor).await
    }

    /// 插入命令收据。唯一键冲突由调用方按同/异载荷回读分类。
    ///
    /// # 错误
    /// 唯一索引冲突或 MongoDB 写入失败时返回错误。
    pub async fn insert_command_receipt(
        &self,
        receipt: &ApprovalCommandReceipt,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(&self.db.collection(RECEIPTS), receipt, executor).await
    }
}

pub(super) fn receipt_key_filter(
    command_kind: ApprovalCommandKind,
    scope_id: &str,
    idempotency_key: &IdempotencyKey,
) -> Document {
    doc! {
        "command_kind": command_kind.as_str(),
        "scope_id": scope_id,
        "idempotency_key": idempotency_key.as_str(),
    }
}

pub(super) fn instance_advance_filter(
    id: &str,
    expected_version: u64,
    expected_current_execution_id: &ApprovalNodeExecutionId,
) -> Result<Document> {
    Ok(doc! {
        "id": id,
        "version": i64_version(expected_version)?,
        "current_node_execution_id": expected_current_execution_id.as_ref(),
        "status": {
            "$in": [
                ApprovalProcessInstanceStatus::Running.as_str(),
                ApprovalProcessInstanceStatus::Blocked.as_str(),
            ]
        },
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    })
}

/// 构造取消当前执行的 `ACTIVE|BLOCKED` CAS 过滤条件。
///
/// # 参数
/// * `id` - 节点执行主键
/// * `expected_version` - 引擎变更前的加载版本
///
/// # 返回
/// 返回同时约束版本、当前状态和软删除标记的查询文档。
///
/// # 错误
/// 版本无法表示为 BSON 整数时返回错误。
pub(super) fn cancellable_execution_end_filter(id: &str, expected_version: u64) -> Result<Document> {
    Ok(doc! {
        "id": id,
        "version": i64_version(expected_version)?,
        "status": {
            "$in": [
                ApprovalNodeExecutionStatus::Active.as_str(),
                ApprovalNodeExecutionStatus::Blocked.as_str(),
            ]
        },
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    })
}

/// 构造取消后的实例列表投影。
///
/// # 参数
/// * `instance` - 已由引擎置为 `CANCELLED` 的实例
///
/// # 返回
/// 返回清空当前节点、审批人与驳回摘要并记录终态时间的有界投影。
///
/// # 错误
/// 无。
pub(super) fn cancelled_instance_projection(
    instance: &ApprovalProcessInstance,
) -> ApprovalInstanceListProjection {
    ApprovalInstanceListProjection {
        current_node_key: None,
        current_node_name: None,
        current_assignee_participant_id: None,
        current_assignee_name: None,
        latest_rejected_execution_id: None,
        latest_rejection_summary: None,
        last_status_changed_at: instance.ended_at.map(|at| at.unix_secs()),
    }
}

pub(super) fn instance_insert_document(
    instance: &ApprovalProcessInstance,
    list_projection: &ApprovalInstanceListProjection,
) -> Result<Document> {
    let mut document = serialize_to_document(instance)?;
    merge_documents(&mut document, serialize_to_document(list_projection)?);
    Ok(document)
}

/// 从引擎变更后的实体版本反推加载时版本。
///
/// # 参数
/// * `current_version` - 引擎规则已经自增后的内存版本
///
/// # 返回
/// 返回执行 CAS 时应匹配的持久化版本。
///
/// # 错误
/// 当前版本为零时返回元数据越界错误。
pub(super) fn previous_version(current_version: u64) -> Result<u64> {
    current_version
        .checked_sub(1)
        .ok_or(Error::EntityMetadataOutOfRange("version"))
}

/// 要求语义写入中的 CAS 已成功应用。
///
/// # 参数
/// * `outcome` - 单次实例或执行 CAS 分类
///
/// # 返回
/// 仅 [`CasWriteOutcome::Applied`] 返回 `Ok(())`。
///
/// # 错误
/// 目标缺失、版本冲突或状态变化时返回乐观锁错误。
pub(super) fn require_cas_applied<T>(outcome: CasWriteOutcome<T>) -> Result<()> {
    if matches!(outcome, CasWriteOutcome::Applied(_)) {
        return Ok(());
    }
    Err(Error::OptimisticLockingError)
}
