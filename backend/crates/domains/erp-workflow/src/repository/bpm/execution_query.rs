use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessInstanceId};
use bpm::model::types::ApprovalNodeExecutionStatus;
use bpm::model::{ApprovalInstanceAssignee, ApprovalNodeExecution};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{Document, doc};
use persistence_core::{Executor, Result, mongo_ops};

use super::{
    ASSIGNEES, BpmWorkflowRepository, EXECUTIONS, ExecutionHistoryOrder, MAX_EXECUTION_HISTORY, clamp_limit,
    find_limited,
};

impl<'a> BpmWorkflowRepository<'a> {
    /// 按主键读取审批节点执行。
    /// 本方法不推断当前执行，调用方需要当前令牌时应使用 `find_current_execution`。
    ///
    /// 返回匹配且未软删除的节点执行；不存在时返回 `None`。
    ///
    /// MongoDB 查询或反序列化失败时返回错误。
    ///
    /// `execution_id`：审批节点执行 ID；`executor`：数据访问执行器，由 Service 决定是否位于事务中。
    pub async fn find_execution_by_id(
        &self,
        execution_id: &ApprovalNodeExecutionId,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalNodeExecution>> {
        self.executions().find_by_id(execution_id.as_ref(), executor).await
    }

    /// 按主键批量读取审批节点执行。
    /// 本方法只批量解析工作项已经持有的运行身份，不按审批人扩大查询范围。
    ///
    /// 返回匹配且未软删除的节点执行；不存在的 ID 不产生占位记录。
    ///
    /// MongoDB 查询或反序列化失败时返回错误。
    ///
    /// `execution_ids`：已授权工作项引用的审批节点执行 ID；`executor`：数据访问执行器，由 Service 决定是否位于事务中。
    pub async fn list_executions_by_ids(
        &self,
        execution_ids: &[ApprovalNodeExecutionId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ApprovalNodeExecution>> {
        if execution_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids = execution_ids.iter().map(ToString::to_string).collect::<Vec<_>>();
        self.executions().find_many(doc! { "id": { "$in": ids } }, executor).await
    }

    /// 查询实例指定节点的当前审批人绑定。
    /// `(process_instance_id, node_key)` 由唯一索引保证单值语义。
    ///
    /// 返回匹配且未软删除的实例审批人绑定；不存在时返回 `None`。
    ///
    /// MongoDB 查询或反序列化失败时返回错误。
    ///
    /// `instance_id`：审批流程实例 ID；`executor`：数据访问执行器，由 Service 决定是否位于事务中。
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
    /// 返回当前活动或受阻执行；没有时返回 `None`。
    ///
    /// MongoDB 查询或反序列化失败时返回错误。
    ///
    /// `instance_id`：审批流程实例 ID。
    pub async fn find_current_execution(
        &self,
        instance_id: &ApprovalProcessInstanceId,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalNodeExecution>> {
        self.executions().find_one(current_execution_filter(instance_id), executor).await
    }

    /// 读取实例最后一次真实驳回，不受详情首屏历史的页大小影响。
    /// 返回执行序号最大的驳回执行，没有驳回时为空。
    ///
    /// 仓储读取或反序列化失败时返回错误。
    ///
    /// `instance_id`：已授权单据对应的审批实例；`executor`：原单据读取执行器。
    pub async fn find_latest_rejected_execution(
        &self,
        instance_id: &ApprovalProcessInstanceId,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalNodeExecution>> {
        let mut rows = find_limited(
            &self.db.collection(EXECUTIONS),
            doc! { "process_instance_id": instance_id.as_ref(), "status": ApprovalNodeExecutionStatus::Rejected.as_str(), "deleted_at": NOT_DELETED_TIMESTAMP_BSON },
            doc! { "execution_no": -1 },
            1,
            executor,
        ).await?;
        Ok(rows.pop())
    }

    /// 按执行序号稳定游标读取实例历史，单次不超过上限。
    /// 按 `execution_no` 升序返回本页历史执行。
    ///
    /// MongoDB 查询或反序列化失败时返回错误。
    ///
    /// `instance_id`：审批流程实例 ID；`after_execution_no`：上一页最后一条执行序号；首页为 `None`。
    /// `limit`：请求条数；夹紧到 `[1, MAX_EXECUTION_HISTORY]`。
    pub async fn list_execution_history(
        &self,
        instance_id: &ApprovalProcessInstanceId,
        after_execution_no: Option<u32>,
        limit: u32,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ApprovalNodeExecution>> {
        self.list_execution_history_ordered(
            instance_id,
            after_execution_no,
            u32::try_from(execution_history_limit(limit)).expect("历史上限为 50，必定落入 u32"),
            ExecutionHistoryOrder::Asc,
            executor,
        )
        .await
    }

    /// 按指定方向读取历史；为最多 100 条的 HTTP 分页保留一条额外记录。
    /// 返回含当前未完成执行的有界历史列表，使用已有实例与执行序号索引。
    ///
    /// MongoDB 查询或反序列化失败时返回错误。
    ///
    /// `instance_id`：审批流程实例 ID；`cursor`：上一页最后执行序号；首页为空。
    /// `limit`：本次读取条数，夹紧至 `[1, 101]`；`order`：执行序号排序方向，同时决定游标比较方向。
    pub async fn list_execution_history_ordered(
        &self,
        instance_id: &ApprovalProcessInstanceId,
        cursor: Option<u32>,
        limit: u32,
        order: ExecutionHistoryOrder,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ApprovalNodeExecution>> {
        find_limited(
            &self.db.collection(EXECUTIONS),
            execution_history_filter(instance_id, cursor, order),
            execution_history_sort(order),
            execution_history_page_limit(limit),
            executor,
        )
        .await
    }
}

// 构造实例执行历史的稳定游标过滤条件。
// 返回含软删除约束、实例主键与指定方向游标的查询文档。
//
// `after_execution_no`：上一页最后一条执行序号；首页为空；`order`：排序方向，升序使用 `$gt`，降序使用 `$lt`。
pub(super) fn execution_history_filter(
    instance_id: &ApprovalProcessInstanceId,
    after_execution_no: Option<u32>,
    order: ExecutionHistoryOrder,
) -> Document {
    let mut filter = doc! {
        "process_instance_id": instance_id.as_ref(),
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    };
    if let Some(after_execution_no) = after_execution_no {
        let operator = match order {
            ExecutionHistoryOrder::Asc => "$gt",
            ExecutionHistoryOrder::Desc => "$lt",
        };
        filter.insert("execution_no", doc! { operator: i64::from(after_execution_no) });
    }
    filter
}

// 构造与历史游标方向一致的排序。
// 返回现有唯一执行序号索引支持的稳定排序。
pub(super) fn execution_history_sort(order: ExecutionHistoryOrder) -> Document {
    let direction = match order {
        ExecutionHistoryOrder::Asc => 1,
        ExecutionHistoryOrder::Desc => -1,
    };
    doc! { "execution_no": direction }
}

// 限制分页读取上限，并为最多 100 条记录保留一条后续页判断记录。
// 返回 `[1, 101]` 内的 MongoDB 读取上限。
//
// `limit`：包含后续页判断记录的请求条数。
pub(super) fn execution_history_page_limit(limit: u32) -> i64 {
    clamp_limit(limit, 101)
}

// 将执行历史请求页大小夹紧到 `[1, MAX_EXECUTION_HISTORY]`。
// 返回可交给 MongoDB `limit` 的有界整数。
pub(super) fn execution_history_limit(limit: u32) -> i64 {
    clamp_limit(limit, MAX_EXECUTION_HISTORY)
}

// 构造当前 `ACTIVE|BLOCKED` 执行过滤；软删除由 `find_one` 追加。
// 返回含实例主键与当前状态集合的查询文档。
//
// `instance_id`：审批流程实例 ID。
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
