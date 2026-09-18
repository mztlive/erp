use bpm::ApprovalNodeExecutionId;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use persistence_core::{Error, Executor, Repository, Result, mongo_ops};

use super::super::bpm::CasWriteOutcome;
use crate::entity::work_item::{WorkItem, WorkItemStatus, WorkItemType};

mod cas;
mod query;

/// 本人开放审批任务仓储页。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentApprovalWorkItemPage {
    /// 按分派时间与任务 ID 稳定倒序的当前页。
    pub items: Vec<WorkItem>,
    /// 不受游标影响的完整过滤集合总数。
    pub total: u64,
    /// 当前页之后是否仍有数据。
    pub has_more: bool,
    /// 存在后续页时返回的 `(assigned_at, work_item_id)`。
    pub next_cursor: Option<(i64, String)>,
    /// 不受游标影响的完整过滤集合完整性冲突事实。
    pub integrity_conflicts: Vec<DocumentApprovalWorkItemIntegrityConflict>,
}

/// 开放审批任务与 BPM execution/instance 的持久化完整性冲突事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentApprovalWorkItemIntegrityConflict {
    /// 同一 execution 挂接多条开放审批任务。
    MultipleOpenTasksForExecution {
        /// 冲突的审批节点 execution ID。
        approval_node_execution_id: String,
        /// 完整过滤集合内的开放任务数。
        open_work_item_count: u64,
    },
    /// 同一 instance 被多个不同 execution 的开放审批任务挂接。
    MultipleOpenExecutionsForInstance {
        /// 冲突的审批流程 instance ID。
        approval_process_instance_id: String,
        /// 完整过滤集合内的不同 execution 数。
        open_execution_count: u64,
    },
}
/// 工作项审批任务集合仓储扩展。
#[allow(async_fn_in_trait)]
pub trait WorkItemRepositoryApprovalExt {
    /// 按主键读取单据审批任务。
    ///
    /// # 参数
    /// * `id` - 待办任务 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配且未软删除的单据审批任务；不存在或类型不符时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    ///
    /// # 关键业务约束
    /// 本查询固定限制 `DOCUMENT_APPROVAL`，禁止运行时命令误消费其它任务类型。
    async fn find_document_approval_by_id(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<WorkItem>>;

    /// 统计指定节点执行当前开放的单据审批任务。
    ///
    /// # 参数
    /// * `execution_id` - 审批节点执行 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回开放审批任务数量。
    ///
    /// # 错误
    /// MongoDB 统计失败时返回错误。
    ///
    /// # 关键业务约束
    /// 统计固定限制任务类型、开放状态与未删除标记，供 BPM 开放任务不变量校验。
    async fn count_open_document_approval_by_execution(
        &self,
        execution_id: &ApprovalNodeExecutionId,
        executor: &mut dyn Executor,
    ) -> Result<u64>;

    /// 分页查询指定账号当前开放的单据审批任务。
    ///
    /// # 参数
    /// * `owner_user_id` - 当前责任人账号 ID
    /// * `business_object_type` - 可选业务对象类型稳定码
    /// * `query` - 可选字面量检索；由 Service 完成空白规范化
    /// * `cursor` - 上一页最后一条任务的首次分派时间与任务 ID
    /// * `limit` - 非零页大小
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页、总数、下一页游标与完整过滤集合的完整性冲突事实；任务按
    /// `assigned_at desc, id desc` 稳定排序，`total` 不含游标，下一页游标为
    /// `(assigned_at, work_item_id)`。
    ///
    /// # 错误
    /// 页大小为零或溢出、MongoDB 聚合失败、反序列化失败或计数越界时返回错误。
    ///
    /// # 关键业务约束
    /// 基础范围固定为当前 owner 的 `OPEN + DOCUMENT_APPROVAL + 未删除`。检索在
    /// MongoDB 分页前执行；快照单号仅允许通过 execution、instance 与不可变
    /// subject 三元组完全一致的快照命中。Repository 不解释 RBAC 或业务授权。
    async fn page_open_document_approval_owned_by(
        &self,
        owner_user_id: &str,
        business_object_type: Option<&str>,
        query: Option<&str>,
        cursor: Option<(i64, &str)>,
        limit: u32,
        executor: &mut dyn Executor,
    ) -> Result<DocumentApprovalWorkItemPage>;

    /// 查询指定账号当前开放的单据审批任务。
    ///
    /// # 参数
    /// * `owner_user_id` - 当前责任人账号 ID
    /// * `business_object_type` - 可选业务对象类型稳定码
    /// * `limit` - 最大返回条数；为零时直接返回空集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按创建时间升序排列的开放单据审批任务。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    ///
    /// # 关键业务约束
    /// 查询固定限制当前责任人、`DOCUMENT_APPROVAL` 与 `OPEN`，不得返回历史任务。
    async fn list_open_document_approval_owned_by(
        &self,
        owner_user_id: &str,
        business_object_type: Option<&str>,
        limit: u32,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>>;

    /// 批量查询多个业务对象当前开放的审批任务。
    ///
    /// 查询使用业务对象类型与 ID 的精确组合，避免不同单据类型之间形成交叉命中；
    /// 只返回 `DOCUMENT_APPROVAL + OPEN` 任务。
    ///
    /// # 参数
    /// * `business_objects` - `(业务对象类型, 业务对象 ID)` 集合
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回按创建时间升序排列的开放审批任务；输入为空时返回空集合。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    async fn list_active_approval_by_objects(
        &self,
        business_objects: &[(String, String)],
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>>;

    /// 读取指定节点执行当前关联的开放审批任务。
    ///
    /// 查询同时约束 `DOCUMENT_APPROVAL + OPEN + approval_node_execution_id`，并按
    /// 创建时间升序返回全部命中，用于调用方识别零任务或重复任务的不一致事实。
    ///
    /// # 参数
    /// * `execution_id` - 当前审批节点执行
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回指定执行关联的全部开放审批任务。
    ///
    /// # 错误
    /// MongoDB 查询、游标读取或反序列化失败时返回错误。
    async fn open_approval_tasks_for_execution(
        &self,
        execution_id: &ApprovalNodeExecutionId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>>;

    /// 读取指定节点执行关联的全部审批任务。
    ///
    /// 本查询不限制任务状态，供人员恢复命令核对旧任务已经关闭且版本未漂移；
    /// 调用方不得用本接口重新打开或修改历史任务。
    ///
    /// # 参数
    /// * `execution_id` - 原受阻节点执行
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回按创建时间升序排列的全部单据审批任务。
    ///
    /// # 错误
    /// MongoDB 查询、游标读取或反序列化失败时返回错误。
    ///
    /// # 关键业务约束
    /// 人员恢复必须创建绑定新执行的新任务，不得把本查询返回的旧任务改回开放状态。
    async fn approval_tasks_for_execution(
        &self,
        execution_id: &ApprovalNodeExecutionId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>>;

    /// 持久化已由实体规则形成的审批取消关闭任务。
    ///
    /// 每条任务继续使用加载时版本和节点执行引用执行 `OPEN` CAS；调用方必须传入
    /// 同一事务执行器，保证任务关闭与 BPM 运行事实、业务单据写回原子提交。
    ///
    /// # 参数
    /// * `items` - 已由 `WorkItem::close_all_for_approval_cancellation` 关闭的任务快照
    /// * `executor` - 调用方事务执行器
    ///
    /// # 返回
    /// 全部任务 CAS 写入成功时返回 `Ok(())`。
    ///
    /// # 错误
    /// 任务缺少节点执行引用、版本溢出、CAS 未命中或 MongoDB 写入失败时返回错误。
    async fn persist_cancelled_approval_tasks(
        &self,
        items: &[WorkItem],
        executor: &mut dyn Executor,
    ) -> Result<()>;

    /// 持久化已由实体规则批量终结的审批任务。
    ///
    /// 每条任务使用自身加载版本及节点执行引用执行 `OPEN` CAS。调用方必须传入
    /// 与 BPM 运行事实、命令收据、outbox 和审计相同的事务执行器。
    ///
    /// # 参数
    /// * `items` - 已由 WorkItem 批量规则形成的终态任务快照
    /// * `executor` - 调用方事务执行器
    ///
    /// # 返回
    /// 全部任务 CAS 写入成功时返回 `Ok(())`；空集合不执行写入。
    ///
    /// # 错误
    /// 任务缺少节点执行引用、CAS 未命中或 MongoDB 写入失败时返回错误。
    async fn persist_ended_approval_tasks(
        &self,
        items: &[WorkItem],
        executor: &mut dyn Executor,
    ) -> Result<()>;

    /// 以 `id + OPEN + expected_task_version + approval_node_execution_id` 关闭审批任务。
    ///
    /// 原审批人恢复不得更新旧 `CLOSED` 任务，只能为新执行插入新任务。
    ///
    /// # 参数
    /// * `item` - 已完成实体状态变更的审批任务
    /// * `expected_task_version` - 加载时任务版本
    /// * `approval_node_execution_id` - 任务绑定的节点执行
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回应用、缺失、版本冲突或状态变化的 CAS 分类。
    ///
    /// # 错误
    /// 元数据越界或 MongoDB 更新失败时返回错误。
    async fn close_approval_task(
        &self,
        item: &WorkItem,
        expected_task_version: u64,
        approval_node_execution_id: &ApprovalNodeExecutionId,
        executor: &mut dyn Executor,
    ) -> Result<CasWriteOutcome<WorkItem>>;
}

impl WorkItemRepositoryApprovalExt for Repository<'_, WorkItem> {
    async fn find_document_approval_by_id(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<WorkItem>> {
        mongo_ops::find_one(
            &self.collection(),
            doc! {
                "id": id,
                "work_item_type": WorkItemType::DocumentApproval.as_str(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            executor,
        )
        .await
    }

    async fn count_open_document_approval_by_execution(
        &self,
        execution_id: &ApprovalNodeExecutionId,
        executor: &mut dyn Executor,
    ) -> Result<u64> {
        mongo_ops::count_documents(
            &self.collection(),
            doc! {
                "approval_node_execution_id": execution_id.as_ref(),
                "work_item_type": WorkItemType::DocumentApproval.as_str(),
                "status": WorkItemStatus::Open.as_str(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            executor,
        )
        .await
    }

    async fn page_open_document_approval_owned_by(
        &self,
        owner_user_id: &str,
        business_object_type: Option<&str>,
        query: Option<&str>,
        cursor: Option<(i64, &str)>,
        limit: u32,
        executor: &mut dyn Executor,
    ) -> Result<DocumentApprovalWorkItemPage> {
        query::page_open_document_approval_owned_by(
            &self.collection(),
            owner_user_id,
            business_object_type,
            query,
            cursor,
            limit,
            executor,
        )
        .await
    }

    async fn list_open_document_approval_owned_by(
        &self,
        owner_user_id: &str,
        business_object_type: Option<&str>,
        limit: u32,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let mut filter = doc! {
            "owner_user_id": owner_user_id,
            "work_item_type": WorkItemType::DocumentApproval.as_str(),
            "status": WorkItemStatus::Open.as_str(),
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        };
        if let Some(business_object_type) = business_object_type {
            filter.insert("business_object_type", business_object_type);
        }
        let options =
            FindOptions::builder().sort(doc! { "created_at": 1, "id": 1 }).limit(i64::from(limit)).build();
        mongo_ops::find_many(&self.collection(), filter, options, executor).await
    }

    async fn list_active_approval_by_objects(
        &self,
        business_objects: &[(String, String)],
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>> {
        if business_objects.is_empty() {
            return Ok(Vec::new());
        }
        let object_filters = business_objects
            .iter()
            .map(|(object_type, object_id)| {
                doc! {
                    "business_object_type": object_type,
                    "business_object_id": object_id,
                }
            })
            .collect::<Vec<_>>();
        self.find_many_sorted(
            doc! {
                "work_item_type": WorkItemType::DocumentApproval.as_str(),
                "status": WorkItemStatus::Open.as_str(),
                "$or": object_filters,
            },
            doc! { "created_at": 1 },
            executor,
        )
        .await
    }

    async fn open_approval_tasks_for_execution(
        &self,
        execution_id: &ApprovalNodeExecutionId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>> {
        self.find_many_sorted(
            query::open_approval_execution_filter(execution_id),
            doc! { "created_at": 1, "id": 1 },
            executor,
        )
        .await
    }

    async fn approval_tasks_for_execution(
        &self,
        execution_id: &ApprovalNodeExecutionId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>> {
        self.find_many_sorted(
            doc! {
                "approval_node_execution_id": execution_id.as_ref(),
                "work_item_type": WorkItemType::DocumentApproval.as_str(),
            },
            doc! { "created_at": 1, "id": 1 },
            executor,
        )
        .await
    }

    async fn persist_cancelled_approval_tasks(
        &self,
        items: &[WorkItem],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.persist_ended_approval_tasks(items, executor).await
    }

    async fn persist_ended_approval_tasks(
        &self,
        items: &[WorkItem],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        for item in items {
            let execution_id = item
                .approval_node_execution_id
                .as_ref()
                .ok_or(Error::EntityMetadataOutOfRange("approval_node_execution_id"))?;
            let outcome = self.close_approval_task(item, item.base.version, execution_id, executor).await?;
            if !matches!(outcome, CasWriteOutcome::Applied(_)) {
                return Err(Error::OptimisticLockingError);
            }
        }
        Ok(())
    }

    async fn close_approval_task(
        &self,
        item: &WorkItem,
        expected_task_version: u64,
        approval_node_execution_id: &ApprovalNodeExecutionId,
        executor: &mut dyn Executor,
    ) -> Result<CasWriteOutcome<WorkItem>> {
        cas::persist_open_approval_task(
            self,
            item,
            expected_task_version,
            approval_node_execution_id,
            executor,
        )
        .await
    }
}
