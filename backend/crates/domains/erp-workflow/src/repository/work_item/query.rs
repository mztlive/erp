use std::num::NonZeroU32;

use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, QueryFilter, Repository, Result, aggregate};

use super::authorization::prefix;
use super::{WorkItemFilter, WorkItemRow, WorkItemScan};
use crate::entity::work_item::{WorkItem, WorkItemStatus, WorkItemType};

/// 工作项集合仓储扩展。
#[allow(async_fn_in_trait)]
pub trait WorkItemRepositoryExt {
    /// 在调用方执行上下文中打开一次稳定排序的候选扫描。
    /// 返回数据库授权后的游标；调用方负责业务筛选、总数和版本复验。
    ///
    /// 数据库查询失败时返回仓储错误。
    ///
    /// `filter`：原队列候选条件；`batch_size`：每次取数的非零批次大小。
    /// `executor`：调用方执行器；不另开会话。
    async fn open_work_item_scan(
        &self,
        filter: &WorkItemFilter,
        batch_size: NonZeroU32,
        executor: &mut dyn Executor,
    ) -> Result<WorkItemScan>;

    /// 按固定批次读取队列候选任务投影。
    ///
    /// 本方法先应用已编译的完整任务授权，再排序和读取指定批次；
    /// Service 完成剩余业务筛选后形成分页和总数。
    ///
    /// 返回当前候选批次，不暴露仓储候选总数为授权总数。
    ///
    /// MongoDB 查询、游标读取或反序列化失败时返回错误。
    ///
    /// `filter`：服务端责任范围与业务筛选；`offset`：未授权候选集的起始偏移。
    async fn scan_work_item_batch(
        &self,
        filter: &WorkItemFilter,
        offset: u64,
        batch_size: NonZeroU32,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItemRow>>;

    /// 在与队列完全相同的授权筛选内查找焦点任务。
    ///
    /// 任务同时满足 ID 与全部 scope/角色/组织/业务筛选时返回完整实体；否则
    /// 返回 `None`，调用方不得退回无过滤的 ID 查询。
    ///
    /// MongoDB 查询或反序列化失败时返回错误。
    ///
    /// `filter`：与队列相同的授权筛选。
    async fn find_visible_by_id(
        &self,
        id: &str,
        filter: &WorkItemFilter,
        executor: &mut dyn Executor,
    ) -> Result<Option<WorkItem>>;

    /// 查询业务对象当前全部开放任务。
    ///
    /// 该查询供强类型服务核对当前责任事实；同一对象与任务类型的开放唯一性由
    /// `uk_work_items_open_object_type` 部分唯一索引保证。
    ///
    /// 按创建时间升序返回该对象当前全部开放任务；没有时为空向量。
    ///
    /// MongoDB 查询或反序列化失败时返回错误。
    ///
    /// `business_object_id`：业务对象稳定 ID。
    async fn list_active_by_object(
        &self,
        business_object_type: &str,
        business_object_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>>;

    /// 按任务类型、对象类型与当前处理人读取开放任务。
    /// 只返回当前开放任务，历史处理人不得命中。
    ///
    /// 返回匹配的开放任务，按创建时间升序。
    ///
    /// MongoDB 查询或反序列化失败时返回错误。
    ///
    /// `owner_user_ids`：当前处理人稳定 ID；空集合不访问数据库。
    async fn list_open_by_type_owners(
        &self,
        work_item_type: WorkItemType,
        business_object_type: &str,
        owner_user_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>>;

    /// 按任务类型、当前责任人和明确状态读取任务，用于受控历史查询。
    /// 返回未删除的匹配任务；空责任人或状态集合返回空集合。
    ///
    /// 数据库查询或反序列化失败时返回错误。
    ///
    /// `work_item_type`、`business_object_type`、`owner_user_ids` 与 `statuses` 限定查询，
    /// `executor` 为调用方执行器。
    async fn list_by_type_owners_statuses(
        &self,
        work_item_type: WorkItemType,
        business_object_type: &str,
        owner_user_ids: &[String],
        statuses: &[WorkItemStatus],
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>>;

    /// 按稳定 ID 读取任意已注册工作项。
    /// 返回未删除工作项；不存在时返回 `None`。
    ///
    /// MongoDB 查询或反序列化失败时返回错误。
    async fn find_work_item(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<WorkItem>>;

    /// 在调用方事务中写回已经关闭的确认任务。
    /// 全部写回成功时无返回值。
    ///
    /// 任一条 `update` 发生版本冲突或 MongoDB 写入失败时返回错误。
    ///
    /// `work_items`：已关闭、待按 CAS 写回的任务；空切片不访问数据库；`executor`：调用方事务执行器。
    async fn persist_closed_confirmation_work_items(
        &self,
        work_items: &mut [WorkItem],
        executor: &mut dyn Executor,
    ) -> Result<()>;

    /// 查找指定业务对象当前开放的供应异常人工任务。
    /// 返回开放的业务异常任务；不存在时返回 `None`。
    ///
    /// MongoDB 查询或反序列化失败时返回错误。
    async fn find_open_business_exception_for_object(
        &self,
        business_object_type: &str,
        business_object_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<WorkItem>>;

    /// 批量读取导入确认引用的正式任务。
    /// 仅查询本仓储拥有的 `work_items` 集合，按主键 `$in` 批量读取，不访问确认事实集合。
    ///
    /// 返回全部匹配的未删除任务；输入为空时返回空列表。
    ///
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    ///
    /// `work_item_ids`：正式任务 ID 列表；`executor`：数据访问执行器，由 Service 决定是否位于事务中。
    async fn list_legacy_import_confirmations_by_ids(
        &self,
        work_item_ids: &[erp_core::ids::WorkItemId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>>;
}

impl WorkItemRepositoryExt for Repository<'_, WorkItem> {
    // 打开原筛选和排序的单个查询游标，沿用调用方执行器。
    // 成功时返回 `WorkItemScan`。
    //
    // 数据库查询失败时返回 `WorkItemScan::open` 的仓储错误。
    //
    // `filter`：原队列候选筛选，含排序字段；`batch_size`：每次取数的非零批次大小。
    // `executor`：调用方执行器；游标沿用该执行器，不另开会话。
    async fn open_work_item_scan(
        &self,
        filter: &WorkItemFilter,
        batch_size: NonZeroU32,
        executor: &mut dyn Executor,
    ) -> Result<WorkItemScan> {
        WorkItemScan::open(self.collection(), filter, batch_size, executor).await
    }

    async fn scan_work_item_batch(
        &self,
        filter: &WorkItemFilter,
        offset: u64,
        batch_size: NonZeroU32,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItemRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(offset)
            .limit(i64::from(batch_size.get()))
            .projection(work_item_projection())
            .build();
        let collection = self.collection().clone_with_type::<WorkItemRow>();
        aggregate::find(&collection, filter.to_doc(), options, prefix(filter, executor)?, executor).await
    }

    async fn find_visible_by_id(
        &self,
        id: &str,
        filter: &WorkItemFilter,
        executor: &mut dyn Executor,
    ) -> Result<Option<WorkItem>> {
        let mut document = filter.to_doc();
        document.insert("id", id);
        let options = FindOptions::builder().limit(1).build();
        Ok(aggregate::find(&self.collection(), document, options, prefix(filter, executor)?, executor)
            .await?
            .pop())
    }

    async fn list_active_by_object(
        &self,
        business_object_type: &str,
        business_object_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>> {
        self.find_many_sorted(
            doc! {
                "business_object_type": business_object_type,
                "business_object_id": business_object_id,
                "status": WorkItemStatus::Open.as_str(),
            },
            doc! { "created_at": 1 },
            executor,
        )
        .await
    }

    async fn list_open_by_type_owners(
        &self,
        work_item_type: WorkItemType,
        business_object_type: &str,
        owner_user_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>> {
        self.list_by_type_owners_statuses(
            work_item_type,
            business_object_type,
            owner_user_ids,
            &[WorkItemStatus::Open],
            executor,
        )
        .await
    }

    async fn list_by_type_owners_statuses(
        &self,
        work_item_type: WorkItemType,
        business_object_type: &str,
        owner_user_ids: &[String],
        statuses: &[WorkItemStatus],
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>> {
        if owner_user_ids.is_empty() || statuses.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many_sorted(
            doc! {
                "work_item_type": work_item_type.as_str(),
                "business_object_type": business_object_type,
                "status": { "$in": statuses.iter().copied().map(WorkItemStatus::as_str).collect::<Vec<_>>() },
                "owner_user_id": { "$in": owner_user_ids },
            },
            doc! { "created_at": 1 },
            executor,
        )
        .await
    }

    async fn find_work_item(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<WorkItem>> {
        self.find_by_id(id, executor).await
    }

    async fn persist_closed_confirmation_work_items(
        &self,
        work_items: &mut [WorkItem],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if work_items.is_empty() {
            return Ok(());
        }
        for item in work_items.iter_mut() {
            self.update(item, executor).await?;
        }
        Ok(())
    }

    async fn find_open_business_exception_for_object(
        &self,
        business_object_type: &str,
        business_object_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<WorkItem>> {
        self.find_one(
            doc! {
                "work_item_type": WorkItemType::BusinessException.as_str(),
                "business_object_type": business_object_type,
                "business_object_id": business_object_id,
                "status": WorkItemStatus::Open.as_str(),
            },
            executor,
        )
        .await
    }

    async fn list_legacy_import_confirmations_by_ids(
        &self,
        work_item_ids: &[erp_core::ids::WorkItemId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>> {
        if work_item_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids = work_item_ids.iter().map(ToString::to_string).collect::<Vec<_>>();
        self.find_many(doc! { "id": { "$in": ids } }, executor).await
    }
}

// 将白名单排序编译为包含稳定 ID 次序的数据库条件。
// 返回主字段方向与 `id` 升序并列键。
//
// `sort_by`：排序字段；`None` 或白名单外字段时使用 `created_at`；`sort_ascending`：为 `true` 时升序。
pub(super) fn sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    let field = match sort_by {
        Some("updated_at") => "updated_at",
        Some("due_at") => "due_at",
        Some("assigned_at") => "assigned_at",
        Some("current_assignment_at") => "current_assignment_at",
        Some("last_activity_at") => "last_activity_at",
        Some("completed_at") => "completed_at",
        Some("closed_at") => "closed_at",
        _ => "created_at",
    };
    doc! { field: direction, "id": 1 }
}

// 工作台扫描与原分页读取复用同一组任务事实字段。
// 返回任务身份、责任、时间与摘要字段的投影文档。
pub(super) fn work_item_projection() -> Document {
    doc! {
        "id": 1,
        "work_item_type": 1,
        "approval_node_execution_id": 1,
        "responsibility_key": 1,
        "business_object_type": 1,
        "business_object_id": 1,
        "subject_version": 1,
        "links": 1,
        "status": 1,
        "owner_role": 1,
        "owner_organization_id": 1,
        "owner_user_id": 1,
        "responsibility_actor_ids": 1,
        "assignment_source": 1,
        "assigned_at": 1,
        "started_at": 1,
        "current_assignment_at": 1,
        "last_activity_at": 1,
        "priority": 1,
        "due_at": 1,
        "reason_code": 1,
        "impact_summary": 1,
        "completed_at": 1,
        "completed_by": 1,
        "closed_at": 1,
        "closed_by": 1,
        "close_reason": 1,
        "version": 1,
        "created_at": 1,
        "updated_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;

    use super::super::WorkItemRow;
    use super::{sort_doc, work_item_projection};
    use crate::entity::work_item::{AssignmentSource, WorkItemPriority, WorkItemStatus, WorkItemType};

    #[test]
    fn sort_doc_is_whitelisted() {
        assert_eq!(sort_doc(None, false), doc! { "created_at": -1, "id": 1 });
        assert_eq!(sort_doc(Some("last_activity_at"), true), doc! { "last_activity_at": 1, "id": 1 });
        assert_eq!(sort_doc(Some("assigned_at"), false), doc! { "assigned_at": -1, "id": 1 });
        assert_eq!(sort_doc(Some("business_object_id"), false), doc! { "created_at": -1, "id": 1 });
    }

    #[test]
    fn sort_doc_always_carries_id_tie_breaker_for_stable_paging() {
        for (sort, ascending) in [(None, false), (Some("due_at"), true), (Some("created_at"), false)] {
            let sorted = sort_doc(sort, ascending);
            assert_eq!(sorted.get_i32("id").expect("稳定排序必须含ID"), 1);
            assert_eq!(sorted.keys().count(), 2);
        }
    }

    #[test]
    fn work_item_projection_includes_approval_references() {
        let projection = work_item_projection();
        assert_eq!(projection.get_i32("approval_node_execution_id").unwrap(), 1);
        assert_eq!(projection.get_i32("responsibility_key").unwrap(), 1);

        let row = WorkItemRow {
            links: Vec::new(),
            id: "wi-1".to_string(),
            work_item_type: WorkItemType::CardFundsDeltaReview,
            approval_node_execution_id: Some("exec-1".to_string()),
            responsibility_key: None,
            business_object_type: "receivable_account".to_string(),
            business_object_id: "account-1".to_string(),
            subject_version: "v1".to_string(),
            status: WorkItemStatus::Open,
            owner_role: "role-finance".to_string(),
            owner_organization_id: "org-1".to_string(),
            owner_user_id: Some("user-1".to_string()),
            responsibility_actor_ids: Vec::new(),
            assignment_source: AssignmentSource::SystemRule,
            assigned_at: None,
            started_at: None,
            current_assignment_at: None,
            last_activity_at: None,
            priority: WorkItemPriority::Normal,
            due_at: None,
            reason_code: None,
            impact_summary: None,
            completed_at: None,
            completed_by: None,
            closed_at: None,
            closed_by: None,
            close_reason: None,
            version: 1,
            created_at: 1,
            updated_at: 1,
        };
        assert_eq!(row.approval_node_execution_id.as_deref(), Some("exec-1"));

        let decoded: WorkItemRow = mongodb::bson::deserialize_from_document(doc! {
            "id": "wi-2",
            "work_item_type": "CARD_FUNDS_DELTA_REVIEW",
            "business_object_type": "receivable_account",
            "business_object_id": "account-1",
            "subject_version": "v1",
            "status": "OPEN",
            "owner_role": "role-finance",
            "owner_organization_id": "org-1",
            "owner_user_id": "user-1",
            "responsibility_actor_ids": [],
            "assignment_source": "SYSTEM_RULE",
            "priority": "normal",
            "version": 1i64,
            "created_at": 1i64,
            "updated_at": 1i64,
        })
        .expect("缺少 approval_node_execution_id 的旧文档仍可反序列化");
        assert_eq!(decoded.approval_node_execution_id, None);
    }
}
