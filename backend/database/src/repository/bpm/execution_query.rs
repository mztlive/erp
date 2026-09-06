use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessInstanceId};
use bpm::model::types::ApprovalNodeExecutionStatus;
use bpm::model::{ApprovalInstanceAssignee, ApprovalNodeExecution};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};

use super::{clamp_limit, find_limited, BpmWorkflowRepository, ASSIGNEES, EXECUTIONS, MAX_EXECUTION_HISTORY};
use persistence_core::Executor;
use persistence_core::{mongo_ops, Result};

impl<'a> BpmWorkflowRepository<'a> {
    /// 按主键读取审批节点执行。
    ///
    /// # 参数
    /// * `execution_id` - 审批节点执行 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配且未软删除的节点执行；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    ///
    /// # 关键业务约束
    /// 本方法不推断当前执行，调用方需要当前令牌时应使用 `find_current_execution`。
    pub async fn find_execution_by_id(
        &self,
        execution_id: &ApprovalNodeExecutionId,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalNodeExecution>> {
        self.executions()
            .find_by_id(execution_id.as_ref(), executor)
            .await
    }

    /// 按主键批量读取审批节点执行。
    ///
    /// # 参数
    /// * `execution_ids` - 已授权工作项引用的审批节点执行 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配且未软删除的节点执行；不存在的 ID 不产生占位记录。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    ///
    /// # 关键业务约束
    /// 本方法只批量解析工作项已经持有的运行身份，不按审批人扩大查询范围。
    pub async fn list_executions_by_ids(
        &self,
        execution_ids: &[ApprovalNodeExecutionId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ApprovalNodeExecution>> {
        if execution_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids = execution_ids.iter().map(ToString::to_string).collect::<Vec<_>>();
        self.executions()
            .find_many(doc! { "id": { "$in": ids } }, executor)
            .await
    }

    /// 查询实例指定节点的当前审批人绑定。
    ///
    /// # 参数
    /// * `instance_id` - 审批流程实例 ID
    /// * `node_key` - 定义内节点键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配且未软删除的实例审批人绑定；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    ///
    /// # 关键业务约束
    /// `(process_instance_id, node_key)` 由唯一索引保证单值语义。
    pub async fn find_assignee_for_node(
        &self,
        instance_id: &ApprovalProcessInstanceId,
        node_key: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalInstanceAssignee>> {
        mongo_ops::find_one(
            &self.db.collection(ASSIGNEES),
            doc! {
                "process_instance_id": instance_id.as_ref(),
                "node_key": node_key,
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            executor,
        )
        .await
    }

    /// 查询实例当前 `ACTIVE|BLOCKED` 执行。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn find_current_execution(
        &self,
        instance_id: &ApprovalProcessInstanceId,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalNodeExecution>> {
        self.executions()
            .find_one(current_execution_filter(instance_id), executor)
            .await
    }

    /// 读取取消用例所需的当前活动或受阻执行。
    ///
    /// # 参数
    /// * `instance_id` - 可取消实例主键
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回该实例当前 `ACTIVE|BLOCKED` 执行；缺失时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn current_execution_for_cancellation(
        &self,
        instance_id: &ApprovalProcessInstanceId,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalNodeExecution>> {
        self.executions()
            .find_one(current_execution_filter(instance_id), executor)
            .await
    }

    /// 按执行序号稳定游标读取实例历史，单次不超过上限。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_execution_history(
        &self,
        instance_id: &ApprovalProcessInstanceId,
        after_execution_no: Option<u32>,
        limit: u32,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ApprovalNodeExecution>> {
        find_limited(
            &self.db.collection(EXECUTIONS),
            execution_history_filter(instance_id, after_execution_no),
            doc! { "execution_no": 1 },
            execution_history_limit(limit),
            executor,
        )
        .await
    }
}

/// 构造实例执行历史的稳定游标过滤条件。
///
/// # 参数
/// * `instance_id` - 所属流程实例
/// * `after_execution_no` - 上一页最后一条执行序号；首页为空
///
/// # 返回
/// 返回含软删除约束、实例主键与可选 `execution_no $gt` 的查询文档。
pub(super) fn execution_history_filter(
    instance_id: &ApprovalProcessInstanceId,
    after_execution_no: Option<u32>,
) -> Document {
    let mut filter = doc! {
        "process_instance_id": instance_id.as_ref(),
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    };
    if let Some(after_execution_no) = after_execution_no {
        filter.insert("execution_no", doc! { "$gt": i64::from(after_execution_no) });
    }
    filter
}

/// 将执行历史请求页大小夹紧到 `[1, MAX_EXECUTION_HISTORY]`。
///
/// # 参数
/// * `limit` - 调用方请求条数
///
/// # 返回
/// 返回可交给 MongoDB `limit` 的有界整数。
pub(super) fn execution_history_limit(limit: u32) -> i64 {
    clamp_limit(limit, MAX_EXECUTION_HISTORY)
}

pub(super) fn current_execution_filter(instance_id: &ApprovalProcessInstanceId) -> Document {
    doc! {
        "process_instance_id": instance_id.as_ref(),
        "status": {
            "$in": [
                ApprovalNodeExecutionStatus::Active.as_str(),
                ApprovalNodeExecutionStatus::Blocked.as_str(),
            ]
        },
    }
}
