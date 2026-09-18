use std::num::NonZeroU32;

use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, QueryFilter, Repository, Result, mongo_ops};

use super::{WorkItemFilter, WorkItemRow};
use crate::entity::work_item::{WorkItem, WorkItemStatus, WorkItemType};

/// 工作项集合仓储扩展。
#[allow(async_fn_in_trait)]
pub trait WorkItemRepositoryExt {
    /// 按固定批次读取队列候选任务投影。
    ///
    /// 本方法不执行未授权候选总数统计；Service 必须逐批加载权威
    /// 业务对象事实，完成参与权过滤后再形成分页和总数。
    ///
    /// # 参数
    /// * `filter` - 服务端责任范围与业务筛选
    /// * `offset` - 未授权候选集的起始偏移
    /// * `batch_size` - 非零固定批次大小
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回当前候选批次，不暴露仓储候选总数为授权总数。
    ///
    /// # 错误
    /// MongoDB 查询、游标读取或反序列化失败时返回错误。
    async fn scan_work_item_batch(
        &self,
        filter: &WorkItemFilter,
        offset: u64,
        batch_size: NonZeroU32,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItemRow>>;

    /// 在与队列完全相同的授权筛选内查找焦点任务。
    ///
    /// # 返回
    /// 任务同时满足 ID 与全部 scope/角色/组织/业务筛选时返回完整实体；否则
    /// 返回 `None`，调用方不得退回无过滤的 ID 查询。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
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
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    async fn list_active_by_object(
        &self,
        business_object_type: &str,
        business_object_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>>;

    /// 按任务类型、对象类型与当前处理人读取开放任务。
    ///
    /// # 参数
    /// * `work_item_type` - 任务类型
    /// * `business_object_type` - 业务对象类型
    /// * `owner_user_ids` - 当前处理人稳定 ID；空集合不访问数据库
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回匹配的开放任务，按创建时间升序。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    ///
    /// # 关键业务约束
    /// 只返回当前开放任务，历史处理人不得命中。
    async fn list_open_by_type_owners(
        &self,
        work_item_type: WorkItemType,
        business_object_type: &str,
        owner_user_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>>;

    /// 按稳定 ID 读取任意已注册工作项。
    ///
    /// # 参数
    /// * `id` - 工作项 ID
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回未删除工作项；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    async fn find_work_item(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<WorkItem>>;

    /// Persist already-closed confirmation work items in the caller transaction.
    ///
    /// # Parameters
    /// * `work_items` - closed work items to write back with CAS
    /// * `executor` - caller transaction executor
    ///
    /// # Errors
    /// Version conflict or MongoDB write failures.
    async fn persist_closed_confirmation_work_items(
        &self,
        work_items: &mut [WorkItem],
        executor: &mut dyn Executor,
    ) -> Result<()>;

    /// 查找指定业务对象当前开放的供应异常人工任务。
    ///
    /// # 参数
    /// * `business_object_type` - 业务对象类型
    /// * `business_object_id` - 业务对象稳定 ID
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回开放的业务异常任务；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    async fn find_open_business_exception_for_object(
        &self,
        business_object_type: &str,
        business_object_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<WorkItem>>;

    /// 批量读取导入确认引用的正式任务。
    ///
    /// # 参数
    /// * `work_item_ids` - 正式任务 ID 列表
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配的未删除任务；输入为空时返回空列表。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    ///
    /// # 约束
    /// 仅查询本仓储拥有的 `work_items` 集合，按主键 `$in` 批量读取，不访问确认事实集合。
    async fn list_legacy_import_confirmations_by_ids(
        &self,
        work_item_ids: &[erp_core::ids::WorkItemId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>>;
}

impl WorkItemRepositoryExt for Repository<'_, WorkItem> {
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
        mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await
    }

    async fn find_visible_by_id(
        &self,
        id: &str,
        filter: &WorkItemFilter,
        executor: &mut dyn Executor,
    ) -> Result<Option<WorkItem>> {
        let mut document = filter.to_doc();
        document.insert("id", id);
        mongo_ops::find_one(&self.collection(), document, executor).await
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
        if owner_user_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many_sorted(
            doc! {
                "work_item_type": work_item_type.as_str(),
                "business_object_type": business_object_type,
                "status": WorkItemStatus::Open.as_str(),
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

fn sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
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

fn work_item_projection() -> Document {
    doc! {
        "id": 1,
        "work_item_type": 1,
        "approval_node_execution_id": 1,
        "business_object_type": 1,
        "business_object_id": 1,
        "subject_version": 1,
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
    fn work_item_projection_includes_approval_node_execution_id() {
        let projection = work_item_projection();
        assert_eq!(projection.get_i32("approval_node_execution_id").unwrap(), 1);
        let row = WorkItemRow {
            id: "wi-1".to_string(),
            work_item_type: WorkItemType::CardFundsDeltaReview,
            approval_node_execution_id: Some("exec-1".to_string()),
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
