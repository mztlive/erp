//! 域 D03 `work_item` 仓储：责任队列查询与责任池原子开始处理。

use std::num::NonZeroU32;

use bpm::ApprovalNodeExecutionId;
use entities::common::time::Instant;
use entities::work_item::{AssignmentSource, WorkItem, WorkItemPriority, WorkItemStatus, WorkItemType};
use entity_core::{HasBaseModel, NOT_DELETED_TIMESTAMP_BSON};
use mongodb::bson::{doc, to_document, Bson, Document};
use mongodb::options::FindOptions;
use serde::{Deserialize, Serialize};

use super::bpm::{approval_task_cas_filter, classify_cas_miss, CasWriteOutcome};
use super::{Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Error, Result};

/// 队列列表的最小任务事实投影。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkItemRow {
    /// 任务 ID。
    pub id: String,
    /// 固定任务类型。
    pub work_item_type: WorkItemType,
    /// 类型化审批节点执行；审批任务存在，独立任务为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_node_execution_id: Option<String>,
    /// 业务对象类型。
    pub business_object_type: String,
    /// 业务对象 ID。
    pub business_object_id: String,
    /// 被处理的业务版本。
    pub subject_version: String,
    /// 生命周期状态。
    pub status: WorkItemStatus,
    /// 责任角色。
    pub owner_role: String,
    /// 责任组织。
    pub owner_organization_id: String,
    /// 当前个人责任人。
    pub owner_user_id: Option<String>,
    /// 曾形成个人责任的用户 ID；仅供服务端历史范围过滤。
    pub responsibility_actor_ids: Vec<String>,
    /// 当前或最近责任来源。
    pub assignment_source: AssignmentSource,
    /// 首次形成个人责任的时间。
    pub assigned_at: Option<Instant>,
    /// 首次正式处理时间。
    pub started_at: Option<Instant>,
    /// 当前个人责任生效时间。
    pub current_assignment_at: Option<Instant>,
    /// 最近一次活动时间。
    pub last_activity_at: Option<Instant>,
    /// 优先级。
    pub priority: WorkItemPriority,
    /// 时限。
    pub due_at: Option<Instant>,
    /// 产生原因代码。
    pub reason_code: Option<String>,
    /// 影响摘要。
    pub impact_summary: Option<String>,
    /// 正式完成时间。
    pub completed_at: Option<Instant>,
    /// 正式完成人。
    pub completed_by: Option<String>,
    /// 关闭时间。
    pub closed_at: Option<Instant>,
    /// 关闭操作人。
    pub closed_by: Option<String>,
    /// 关闭原因。
    pub close_reason: Option<String>,
    /// 持久化乐观锁版本；API 必须映射为 `task_version`。
    pub version: u64,
    /// 创建时间。
    pub created_at: u64,
    /// 最近持久化更新时间。
    pub updated_at: u64,
}

/// 待办列表筛选条件。
#[derive(Debug, Clone, Default)]
pub struct WorkItemFilter {
    /// 允许的任务类型集合；为空时不筛选。
    pub work_item_types: Vec<WorkItemType>,
    /// 允许的状态集合；为空时不筛选。
    pub statuses: Vec<WorkItemStatus>,
    /// 允许的责任角色集合；为空时不筛选。
    pub owner_roles: Vec<String>,
    /// 允许的责任组织集合；为空时不筛选。
    pub owner_organization_ids: Vec<String>,
    /// 角色与组织的已授权组合；组织为 `None` 表示该角色公司级授权。
    pub responsibility_scopes: Vec<(String, Option<String>)>,
    /// 允许的注册任务类型与权威业务对象类型组合。
    pub object_access_shapes: Option<Vec<(WorkItemType, String)>>,
    /// 当前个人责任人；为空时不筛选。
    pub owner_user_id: Option<String>,
    /// 是否只查询尚无个人责任人的责任池任务。
    pub unassigned_only: bool,
    /// 历史参与人；匹配曾负责、完成人或关闭人之一。
    pub history_actor_id: Option<String>,
    /// 具备组织级历史查看权的组织集合；`Some(空)` 表示公司级。
    pub history_managed_organization_ids: Option<Vec<String>>,
    /// 到期时间下界（包含）。
    pub due_from: Option<Instant>,
    /// 到期时间上界（不包含）。
    pub due_before: Option<Instant>,
    /// 允许的优先级集合；为空时不筛选。
    pub priorities: Vec<WorkItemPriority>,
    /// 权限过滤范围内的安全字面量检索。
    pub query: Option<String>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段白名单值。
    pub sort_by: Option<String>,
    /// 是否升序。
    pub sort_ascending: bool,
}

impl QueryFilter for WorkItemFilter {
    /// 构造与责任队列索引一致的 MongoDB 查询条件。
    ///
    /// # 返回
    /// 返回包含软删除约束的查询文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_enum_filter(&mut filter, "status", &self.statuses, WorkItemStatus::as_str);
        insert_enum_filter(
            &mut filter,
            "work_item_type",
            &self.work_item_types,
            WorkItemType::as_str,
        );
        insert_enum_filter(
            &mut filter,
            "priority",
            &self.priorities,
            WorkItemPriority::as_str,
        );
        insert_string_filter(&mut filter, "owner_role", &self.owner_roles);
        insert_string_filter(&mut filter, "owner_organization_id", &self.owner_organization_ids);
        if self.unassigned_only {
            filter.insert("owner_user_id", Bson::Null);
        } else if let Some(owner_user_id) = &self.owner_user_id {
            filter.insert("owner_user_id", owner_user_id);
        }
        let mut conjunctions = Vec::new();
        if let Some(shapes) = &self.object_access_shapes {
            conjunctions.push(object_access_shape_filter(shapes));
        }
        if !self.responsibility_scopes.is_empty() {
            conjunctions.push(responsibility_scope_filter(&self.responsibility_scopes));
        }
        if self.history_actor_id.is_some() || self.history_managed_organization_ids.is_some() {
            conjunctions.push(history_scope_filter(
                self.history_actor_id.as_deref(),
                self.history_managed_organization_ids.as_deref(),
            ));
        }
        if let Some(query) = self.query.as_deref() {
            conjunctions.push(literal_query_filter(query));
        }
        if !conjunctions.is_empty() {
            filter.insert("$and", conjunctions);
        }
        insert_due_range(&mut filter, self.due_from, self.due_before);
        filter
    }
}

impl Pagination for WorkItemFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)`。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 责任池原子开始处理的持久化结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartProcessingOutcome {
    /// 本次请求成功形成个人责任。
    Started(WorkItem),
    /// 同一用户的重试；返回当前事实，且不再次递增版本。
    AlreadyOwned(WorkItem),
    /// 任务已由其他用户负责。
    OwnershipConflict(WorkItem),
    /// 任务仍可开始处理，但请求携带的版本或责任范围已陈旧。
    VersionConflict(WorkItem),
    /// 任务不存在、非开放、非责任池或已不具备开始处理形态。
    NotStartable(Option<WorkItem>),
}

/// 开始处理 CAS 必须固定的服务端资格快照。
///
/// Service 在更新前校验角色、组织、对象参与权与岗位分离，并把校验时看到的
/// 责任范围装入本类型；Repository 将角色和组织一并放入原子过滤条件，避免
/// 资格校验后任务责任范围被并发替换。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartProcessingEligibility<'a> {
    /// 资格校验时看到的责任角色。
    pub owner_role: &'a str,
    /// 资格校验时看到的责任组织。
    pub owner_organization_id: &'a str,
}

impl<'a> Repository<'a, WorkItem> {
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
    pub async fn scan_work_item_batch(
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

    /// 在与队列完全相同的授权筛选内查找焦点任务。
    ///
    /// # 返回
    /// 任务同时满足 ID 与全部 scope/角色/组织/业务筛选时返回完整实体；否则
    /// 返回 `None`，调用方不得退回无过滤的 ID 查询。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn find_visible_by_id(
        &self,
        id: &str,
        filter: &WorkItemFilter,
        executor: &mut dyn Executor,
    ) -> Result<Option<WorkItem>> {
        let mut document = filter.to_doc();
        document.insert("id", id);
        mongo_ops::find_one(&self.collection(), document, executor).await
    }

    /// 以单文档 CAS 原子开始处理责任池任务。
    ///
    /// 过滤条件同时固定任务版本、开放状态、`POOL` 模式、尚无个人责任以及资格
    /// 校验时看到的角色和组织；更新管道以 `$ifNull` 保留首次分派/处理时间。
    /// CAS 未命中后只读取当前事实以区分同人幂等、他人冲突和版本冲突。
    ///
    /// # 错误
    /// 元数据越界、MongoDB 更新或反序列化失败时返回错误。
    pub async fn start_processing(
        &self,
        id: &str,
        expected_task_version: u64,
        eligibility: StartProcessingEligibility<'_>,
        actor_id: &str,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> Result<StartProcessingOutcome> {
        let expected_version =
            i64::try_from(expected_task_version).map_err(|_| Error::EntityMetadataOutOfRange("version"))?;
        let updated = mongo_ops::find_one_and_update_pipeline(
            &self.collection(),
            start_processing_filter(
                id,
                expected_version,
                eligibility.owner_role,
                eligibility.owner_organization_id,
            ),
            start_processing_pipeline(actor_id, at),
            executor,
        )
        .await?;
        if let Some(item) = updated {
            return Ok(StartProcessingOutcome::Started(item));
        }
        let current = self.find_by_id(id, executor).await?;
        Ok(classify_start_processing_miss(
            current,
            expected_task_version,
            actor_id,
        ))
    }

    /// 以 `id + OPEN + expected_task_version + approval_node_execution_id` 关闭审批任务。
    ///
    /// 改派和人员恢复不得更新旧 `CLOSED` 任务，只能为新执行插入新任务。
    ///
    /// # 错误
    /// 元数据越界或 MongoDB 更新失败时返回错误。
    pub async fn close_approval_task(
        &self,
        item: &WorkItem,
        expected_task_version: u64,
        approval_node_execution_id: &ApprovalNodeExecutionId,
        executor: &mut dyn Executor,
    ) -> Result<CasWriteOutcome<WorkItem>> {
        self.persist_open_approval_task(item, expected_task_version, approval_node_execution_id, executor)
            .await
    }

    /// 查询业务对象当前全部开放任务。
    ///
    /// 该查询供强类型服务核对当前责任事实；同一对象与任务类型的开放唯一性由
    /// `uk_work_items_open_object_type` 部分唯一索引保证。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_active_by_object(
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

    async fn persist_open_approval_task(
        &self,
        item: &WorkItem,
        expected_task_version: u64,
        approval_node_execution_id: &ApprovalNodeExecutionId,
        executor: &mut dyn Executor,
    ) -> Result<CasWriteOutcome<WorkItem>> {
        let next_version = next_task_version(expected_task_version)?;
        let mut set_doc = to_document(item)?;
        set_doc.insert("version", next_version);
        let matched = mongo_ops::update_one(
            &self.collection(),
            approval_task_cas_filter(&item.base.id, expected_task_version, approval_node_execution_id)?,
            doc! { "$set": set_doc },
            false,
            executor,
        )
        .await?
        .matched_count;
        if matched > 0 {
            let mut applied = item.clone();
            applied.base_mut().version = expected_task_version.saturating_add(1);
            return Ok(CasWriteOutcome::Applied(applied));
        }
        let current = self.find_by_id(&item.base.id, executor).await?;
        let expected_execution = approval_node_execution_id.clone();
        Ok(classify_cas_miss(current, expected_task_version, move |row| {
            approval_task_still_open(row, &expected_execution)
        }))
    }
}

fn next_task_version(expected_task_version: u64) -> Result<i64> {
    let next = expected_task_version
        .checked_add(1)
        .ok_or(Error::EntityMetadataOutOfRange("version"))?;
    i64::try_from(next).map_err(|_| Error::EntityMetadataOutOfRange("version"))
}

fn approval_task_still_open(item: &WorkItem, execution_id: &ApprovalNodeExecutionId) -> bool {
    item.status == WorkItemStatus::Open && item.approval_node_execution_id.as_ref() == Some(execution_id)
}

fn start_processing_filter(
    id: &str,
    expected_version: i64,
    owner_role: &str,
    owner_organization_id: &str,
) -> Document {
    doc! {
        "id": id,
        "version": expected_version,
        "status": WorkItemStatus::Open.as_str(),
        "owner_role": owner_role,
        "owner_organization_id": owner_organization_id,
        "owner_user_id": Bson::Null,
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    }
}

fn start_processing_pipeline(actor_id: &str, at: Instant) -> Vec<Document> {
    let timestamp = at.unix_secs();
    vec![doc! {
        "$set": {
            "owner_user_id": actor_id,
            "responsibility_actor_ids": {
                "$cond": [
                    { "$in": [actor_id, "$responsibility_actor_ids"] },
                    "$responsibility_actor_ids",
                    { "$concatArrays": ["$responsibility_actor_ids", [actor_id]] },
                ]
            },
            "assignment_source": AssignmentSource::SystemRule.as_str(),
            "assigned_at": { "$ifNull": ["$assigned_at", timestamp] },
            "started_at": { "$ifNull": ["$started_at", timestamp] },
            "current_assignment_at": timestamp,
            "last_activity_at": timestamp,
            "version": { "$add": ["$version", 1_i64] },
            "updated_at": timestamp,
        }
    }]
}

fn classify_start_processing_miss(
    current: Option<WorkItem>,
    expected_task_version: u64,
    actor_id: &str,
) -> StartProcessingOutcome {
    let Some(item) = current else {
        return StartProcessingOutcome::NotStartable(None);
    };
    if item.status != WorkItemStatus::Open {
        return StartProcessingOutcome::NotStartable(Some(item));
    }
    if item.owner_user_id.as_deref() == Some(actor_id) {
        return StartProcessingOutcome::AlreadyOwned(item);
    }
    if item.owner_user_id.is_some() {
        return StartProcessingOutcome::OwnershipConflict(item);
    }
    if item.base().version != expected_task_version {
        return StartProcessingOutcome::VersionConflict(item);
    }
    StartProcessingOutcome::NotStartable(Some(item))
}

fn insert_enum_filter<T: Copy>(
    filter: &mut Document,
    field: &str,
    values: &[T],
    code: impl Fn(T) -> &'static str,
) {
    match values {
        [] => {}
        [value] => {
            filter.insert(field, code(*value));
        }
        values => {
            filter.insert(
                field,
                doc! { "$in": values.iter().copied().map(code).collect::<Vec<_>>() },
            );
        }
    }
}

fn insert_string_filter(filter: &mut Document, field: &str, values: &[String]) {
    match values {
        [] => {}
        [value] => {
            filter.insert(field, value);
        }
        values => {
            filter.insert(field, doc! { "$in": values.to_vec() });
        }
    }
}

fn insert_due_range(filter: &mut Document, due_from: Option<Instant>, due_before: Option<Instant>) {
    let mut range = Document::new();
    if let Some(due_from) = due_from {
        range.insert("$gte", due_from.unix_secs());
    }
    if let Some(due_before) = due_before {
        range.insert("$lt", due_before.unix_secs());
    }
    if !range.is_empty() {
        filter.insert("due_at", range);
    }
}

fn literal_query_filter(query: &str) -> Document {
    let literal = regex::escape(query.trim());
    doc! {
        "$or": [
            { "business_object_id": { "$regex": &literal, "$options": "i" } },
            { "business_object_type": { "$regex": &literal, "$options": "i" } },
            { "reason_code": { "$regex": &literal, "$options": "i" } },
            { "impact_summary": { "$regex": &literal, "$options": "i" } },
        ]
    }
}

fn responsibility_scope_filter(scopes: &[(String, Option<String>)]) -> Document {
    let alternatives = scopes
        .iter()
        .map(|(role, organization_id)| {
            let mut filter = doc! { "owner_role": role };
            if let Some(organization_id) = organization_id {
                filter.insert("owner_organization_id", organization_id);
            }
            filter
        })
        .collect::<Vec<_>>();
    doc! { "$or": alternatives }
}

fn object_access_shape_filter(shapes: &[(WorkItemType, String)]) -> Document {
    if shapes.is_empty() {
        return doc! { "id": { "$exists": false } };
    }
    let alternatives = shapes
        .iter()
        .map(|(work_item_type, business_object_type)| {
            doc! {
                "work_item_type": work_item_type.as_str(),
                "business_object_type": business_object_type,
            }
        })
        .collect::<Vec<_>>();
    doc! { "$or": alternatives }
}

fn history_scope_filter(actor_id: Option<&str>, managed_organization_ids: Option<&[String]>) -> Document {
    let mut alternatives = Vec::new();
    if let Some(actor_id) = actor_id {
        alternatives.extend([
            doc! { "responsibility_actor_ids": actor_id },
            doc! { "completed_by": actor_id },
            doc! { "closed_by": actor_id },
        ]);
    }
    if let Some(organization_ids) = managed_organization_ids {
        if organization_ids.is_empty() {
            alternatives.push(Document::new());
        } else {
            alternatives.push(doc! { "owner_organization_id": { "$in": organization_ids.to_vec() } });
        }
    }
    doc! { "$or": alternatives }
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
    doc! { field: direction }
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
    use entity_core::HasBaseModel;
    use mongodb::bson::{doc, Bson};

    use super::{
        approval_task_still_open, classify_start_processing_miss, sort_doc, start_processing_filter,
        start_processing_pipeline, work_item_projection, QueryFilter, StartProcessingOutcome, WorkItemFilter,
        WorkItemRow,
    };
    use crate::repository::bpm::{approval_task_cas_filter, classify_cas_miss, CasWriteOutcome};
    use bpm::ApprovalNodeExecutionId;
    use entities::common::time::Instant;
    use entities::ids::WorkItemId;
    use entities::work_item::{
        AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemStatus, WorkItemType,
    };

    fn assigned_item() -> WorkItem {
        WorkItem::new_at(
            WorkItemId::new("wi-1"),
            WorkItemData {
                work_item_type: WorkItemType::ImportBusinessConfirmation,
                business_object_type: "LEGACY_IMPORT_BATCH".to_string(),
                business_object_id: "batch-1".to_string(),
                subject_version: "v1".to_string(),
                owner_role: "sales".to_string(),
                owner_organization_id: "org-1".to_string(),
                owner_user_id: Some("alice".to_string()),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: None,
                impact_summary: None,
            },
            Instant::from_unix_secs(100),
        )
        .unwrap()
    }

    #[test]
    fn scope_filter_supports_team_and_history_facts() {
        let team = WorkItemFilter {
            statuses: vec![WorkItemStatus::Open],
            work_item_types: vec![
                WorkItemType::ImportBusinessConfirmation,
                WorkItemType::BusinessException,
            ],
            owner_roles: vec!["sales".to_string(), "operations".to_string()],
            owner_organization_ids: vec!["org-1".to_string()],
            unassigned_only: true,
            priorities: vec![WorkItemPriority::High, WorkItemPriority::Urgent],
            due_from: Some(Instant::from_unix_secs(100)),
            due_before: Some(Instant::from_unix_secs(200)),
            page: 1,
            page_size: 20,
            ..WorkItemFilter::default()
        }
        .to_doc();
        assert_eq!(team.get_str("status").unwrap(), "OPEN");
        assert_eq!(team.get("owner_user_id"), Some(&Bson::Null));
        assert_eq!(
            team.get_document("owner_role").unwrap(),
            &doc! { "$in": ["sales", "operations"] }
        );
        assert_eq!(
            team.get_document("due_at").unwrap(),
            &doc! { "$gte": 100_i64, "$lt": 200_i64 }
        );
        assert_eq!(
            team.get_document("priority").unwrap(),
            &doc! { "$in": ["high", "urgent"] }
        );
        assert_eq!(
            team.get_document("work_item_type").unwrap(),
            &doc! { "$in": ["IMPORT_BUSINESS_CONFIRMATION", "BUSINESS_EXCEPTION"] }
        );

        let history = WorkItemFilter {
            statuses: vec![WorkItemStatus::Completed, WorkItemStatus::Closed],
            history_actor_id: Some("alice".to_string()),
            page: 1,
            page_size: 20,
            ..WorkItemFilter::default()
        }
        .to_doc();
        let history_or = history.get_array("$and").unwrap()[0]
            .as_document()
            .unwrap()
            .get_array("$or")
            .unwrap();
        assert_eq!(
            history_or,
            &vec![
                Bson::Document(doc! { "responsibility_actor_ids": "alice" }),
                Bson::Document(doc! { "completed_by": "alice" }),
                Bson::Document(doc! { "closed_by": "alice" }),
            ]
        );
    }

    #[test]
    fn text_query_is_literal_and_composes_with_history_scope() {
        let filter = WorkItemFilter {
            statuses: vec![WorkItemStatus::Completed, WorkItemStatus::Closed],
            history_actor_id: Some("alice".to_string()),
            query: Some("SO.[1]".to_string()),
            page: 1,
            page_size: 20,
            ..WorkItemFilter::default()
        }
        .to_doc();

        let conjunctions = filter.get_array("$and").unwrap();
        assert_eq!(conjunctions.len(), 2);
        let query = conjunctions[1].as_document().unwrap().get_array("$or").unwrap();
        let regex = query[0]
            .as_document()
            .unwrap()
            .get_document("business_object_id")
            .unwrap()
            .get_str("$regex")
            .unwrap();
        assert_eq!(regex, r"SO\.\[1\]");
    }

    #[test]
    fn responsibility_scopes_preserve_role_organization_pairs() {
        let filter = WorkItemFilter {
            responsibility_scopes: vec![
                ("role-sales".to_string(), Some("org-a".to_string())),
                ("role-procurement".to_string(), Some("org-b".to_string())),
            ],
            page: 1,
            page_size: 20,
            ..WorkItemFilter::default()
        }
        .to_doc();

        let scopes = filter.get_array("$and").unwrap()[0]
            .as_document()
            .unwrap()
            .get_array("$or")
            .unwrap();
        assert_eq!(
            scopes,
            &vec![
                Bson::Document(doc! { "owner_role": "role-sales", "owner_organization_id": "org-a" }),
                Bson::Document(doc! { "owner_role": "role-procurement", "owner_organization_id": "org-b" }),
            ]
        );
    }

    #[test]
    fn history_scope_unions_actor_and_managed_organizations() {
        let filter = WorkItemFilter {
            history_actor_id: Some("alice".to_string()),
            history_managed_organization_ids: Some(vec!["org-a".to_string()]),
            page: 1,
            page_size: 20,
            ..WorkItemFilter::default()
        }
        .to_doc();

        let history = filter.get_array("$and").unwrap()[0]
            .as_document()
            .unwrap()
            .get_array("$or")
            .unwrap();
        assert_eq!(history.len(), 4);
        assert_eq!(
            history[3],
            Bson::Document(doc! { "owner_organization_id": { "$in": ["org-a"] } })
        );
    }

    #[test]
    fn object_access_shapes_fail_closed_and_pair_type_with_object() {
        let denied = WorkItemFilter {
            object_access_shapes: Some(Vec::new()),
            page: 1,
            page_size: 20,
            ..WorkItemFilter::default()
        }
        .to_doc();
        assert_eq!(
            denied.get_array("$and").unwrap()[0],
            Bson::Document(doc! { "id": { "$exists": false } })
        );

        let allowed = WorkItemFilter {
            object_access_shapes: Some(vec![(
                WorkItemType::PurchaseOrderReview,
                "purchase_order".to_string(),
            )]),
            page: 1,
            page_size: 20,
            ..WorkItemFilter::default()
        }
        .to_doc();
        assert_eq!(
            allowed.get_array("$and").unwrap()[0],
            Bson::Document(doc! { "$or": [{
                "work_item_type": "PURCHASE_ORDER_REVIEW",
                "business_object_type": "purchase_order",
            }] })
        );
    }

    #[test]
    fn start_filter_fixes_version_responsibility_and_pool_shape() {
        let filter = start_processing_filter("wi-1", 7, "sales", "org-1");

        assert_eq!(filter.get_i64("version").unwrap(), 7);
        assert_eq!(filter.get_str("status").unwrap(), "OPEN");
        assert_eq!(filter.get_str("owner_role").unwrap(), "sales");
        assert_eq!(filter.get_str("owner_organization_id").unwrap(), "org-1");
        assert_eq!(filter.get("owner_user_id"), Some(&Bson::Null));
    }

    #[test]
    fn start_pipeline_uses_if_null_and_add() {
        let pipeline = start_processing_pipeline("alice", Instant::from_unix_secs(123));
        let set = pipeline[0].get_document("$set").unwrap();

        assert_eq!(set.get_str("owner_user_id").unwrap(), "alice");
        assert_eq!(
            set.get_document("responsibility_actor_ids").unwrap(),
            &doc! {
                "$cond": [
                    { "$in": ["alice", "$responsibility_actor_ids"] },
                    "$responsibility_actor_ids",
                    { "$concatArrays": ["$responsibility_actor_ids", ["alice"]] },
                ]
            }
        );
        assert_eq!(set.get_str("assignment_source").unwrap(), "SYSTEM_RULE");
        assert_eq!(
            set.get_document("assigned_at").unwrap(),
            &doc! { "$ifNull": ["$assigned_at", 123_i64] }
        );
        assert_eq!(
            set.get_document("started_at").unwrap(),
            &doc! { "$ifNull": ["$started_at", 123_i64] }
        );
        assert_eq!(
            set.get_document("version").unwrap(),
            &doc! { "$add": ["$version", 1_i64] }
        );
    }

    #[test]
    fn missed_start_distinguishes_idempotency_owner_and_version() {
        let same_actor = assigned_item();
        assert!(matches!(
            classify_start_processing_miss(Some(same_actor), 0, "alice"),
            StartProcessingOutcome::AlreadyOwned(_)
        ));

        let mut other_actor = assigned_item();
        other_actor.owner_user_id = Some("bob".to_string());
        assert!(matches!(
            classify_start_processing_miss(Some(other_actor), 0, "alice"),
            StartProcessingOutcome::OwnershipConflict(_)
        ));

        let mut stale = assigned_item();
        stale.owner_user_id = None;
        stale.base_mut().version = 2;
        assert!(matches!(
            classify_start_processing_miss(Some(stale), 1, "alice"),
            StartProcessingOutcome::VersionConflict(_)
        ));
    }

    #[test]
    fn sort_doc_is_whitelisted() {
        assert_eq!(sort_doc(None, false), doc! { "created_at": -1 });
        assert_eq!(
            sort_doc(Some("last_activity_at"), true),
            doc! { "last_activity_at": 1 }
        );
        assert_eq!(sort_doc(Some("assigned_at"), false), doc! { "assigned_at": -1 });
        assert_eq!(
            sort_doc(Some("business_object_id"), false),
            doc! { "created_at": -1 }
        );
    }

    #[test]
    fn approval_task_cas_miss_classifies_closed_and_version() {
        let execution = ApprovalNodeExecutionId::new("exec-1");
        let filter = approval_task_cas_filter("wi-1", 3, &execution).unwrap();
        assert_eq!(filter.get_str("status").unwrap(), "OPEN");
        assert_eq!(filter.get_str("approval_node_execution_id").unwrap(), "exec-1");

        let mut closed = assigned_item();
        closed.status = WorkItemStatus::Closed;
        closed.approval_node_execution_id = Some(execution.clone());
        assert!(!approval_task_still_open(&closed, &execution));
        let closed_version = closed.base().version;
        assert!(matches!(
            classify_cas_miss(Some(closed), closed_version, |item| {
                approval_task_still_open(item, &execution)
            }),
            CasWriteOutcome::StatusChanged(_)
        ));

        let mut stale = assigned_item();
        stale.approval_node_execution_id = Some(execution.clone());
        stale.base_mut().version = 4;
        assert!(matches!(
            classify_cas_miss(Some(stale), 3, |item| approval_task_still_open(item, &execution)),
            CasWriteOutcome::VersionConflict(_)
        ));

        let mut open_wrong = assigned_item();
        open_wrong.approval_node_execution_id = Some(ApprovalNodeExecutionId::new("exec-2"));
        let open_version = open_wrong.base().version;
        assert!(!approval_task_still_open(&open_wrong, &execution));
        assert!(matches!(
            classify_cas_miss(Some(open_wrong), open_version, |item| {
                approval_task_still_open(item, &execution)
            }),
            CasWriteOutcome::StatusChanged(_)
        ));
        assert!(matches!(
            classify_cas_miss::<WorkItem>(None, 1, |item| approval_task_still_open(item, &execution)),
            CasWriteOutcome::NotFound
        ));
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
        let decoded: WorkItemRow = mongodb::bson::from_document(doc! {
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
