//! 域 D06 `access_control` 仓储：audit_event 审计留痕查询。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{
    Executor, PageResult, Pagination, QueryFilter, Repository, Result, insert_literal_regex_filter, mongo_ops,
};
use serde::{Deserialize, Serialize};

use super::{audit_event_projection, sort_doc};
use crate::entity::access_control::paging::PageParams;
use crate::entity::access_control::{AuditEvent, AuditEventResult};

/// 审计列表的最小投影，不返回事件整文档。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuditEventRow {
    /// 审计事件的稳定 ID。
    pub id: String,
    /// 触发操作的账号 ID。
    pub actor_id: String,
    /// 事件发生时记录的操作者显示名。
    pub actor_label: String,
    /// 事件发生时记录的责任角色。
    pub actor_role: String,
    /// 审计目录中的动作代码。
    pub action_type: String,
    /// 被操作的业务对象类型代码。
    pub object_type: String,
    /// 被操作的业务记录 ID；无对象时为空。
    pub object_id: Option<String>,
    /// 可安全展示的业务对象标题。
    pub object_label: Option<String>,
    /// 关联请求的追踪号，用于串联同一请求的事件。
    pub request_id: Option<String>,
    /// 此次操作的最终结果。
    pub result: AuditEventResult,
    /// 只记录哪些字段发生变化，不保留字段值。
    pub changed_field_names: Vec<String>,
    /// 记录操作来源的 IP 地址（若有）。
    pub source_ip: Option<String>,
    /// 事件发生时间，以 Unix 秒记录。
    pub created_at: u64,
}

/// 审计列表的精确条件、关键词及分页选项。
#[derive(Debug, Clone, Default)]
pub struct AuditEventFilter {
    /// 用字面量关键词匹配操作者、动作、对象或追踪字段。
    pub q: Option<String>,
    /// 逗号分隔的界面动作标签命中代码，仅并入关键词的 OR 条件。
    pub keyword_actions: Option<String>,
    /// 按审计事件的稳定 ID 精确筛选。
    pub event_id: Option<String>,
    /// 精确匹配链路追踪号或请求号之一。
    pub trace_id: Option<String>,
    /// 事件发生时间下界（含，Unix 秒）。
    pub created_from: Option<u64>,
    /// 事件发生时间上界（不含，Unix 秒）。
    pub created_before: Option<u64>,

    /// 操作者 ID 的不区分大小写字面量匹配；未指定则不限制。
    pub actor_id: Option<String>,
    /// 动作代码的不区分大小写字面量匹配；未指定则不限制。
    pub action_type: Option<String>,
    /// 精确匹配业务对象类型代码；未指定则不限制。
    pub object_type: Option<String>,
    /// 精确匹配业务记录 ID；未指定则不限制。
    pub object_id: Option<String>,
    /// 按最终结果精确筛选；未指定则不限制。
    pub result: Option<AuditEventResult>,
    /// 上层已校验的分页和排序参数。
    pub paging: PageParams,
}

impl QueryFilter for AuditEventFilter {
    // 将列表条件下推到 MongoDB，并固定排除已删除记录。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(&mut filter, "actor_id", self.actor_id.as_deref());
        insert_literal_regex_filter(&mut filter, "action_type", self.action_type.as_deref());
        insert_exact_match(&mut filter, "object_type", self.object_type.as_deref());
        insert_exact_match(&mut filter, "object_id", self.object_id.as_deref());
        if let Some(result) = self.result {
            filter.insert("result", result.as_str());
        }
        insert_exact_match(&mut filter, "id", self.event_id.as_deref());
        if let Some(id) = &self.trace_id {
            filter.insert("$or", vec![doc! { "trace_id": id }, doc! { "request_id": id }]);
        }

        insert_time_range(&mut filter, self.created_from, self.created_before);
        insert_keyword_clauses(&mut filter, self.q.as_deref(), self.keyword_actions.as_deref());
        filter
    }
}

// 仅在指定值时追加字段的精确匹配条件。
fn insert_exact_match(filter: &mut Document, field: &str, value: Option<&str>) {
    if let Some(value) = value {
        filter.insert(field, value);
    }
}

// 为事件时间添加可选的半开区间，秒值超出 BSON 整数范围时取 `i64::MAX`。
fn insert_time_range(filter: &mut Document, from: Option<u64>, before: Option<u64>) {
    let mut time = Document::new();
    if let Some(from) = from {
        time.insert("$gte", i64::try_from(from).unwrap_or(i64::MAX));
    }
    if let Some(before) = before {
        time.insert("$lt", i64::try_from(before).unwrap_or(i64::MAX));
    }
    if !time.is_empty() {
        filter.insert("created_at", time);
    }
}

// 有关键词时追加多字段字面量或动作标签的并集，再与原有结构化筛选求交。
fn insert_keyword_clauses(filter: &mut Document, q: Option<&str>, keyword_actions: Option<&str>) {
    let Some(q) = q else {
        return;
    };
    let mut clauses = [
        "actor_id",
        "actor_label",
        "action_type",
        "object_type",
        "object_id",
        "object_label",
        "trace_id",
        "request_id",
    ]
    .into_iter()
    .map(|field| {
        let mut clause = Document::new();
        insert_literal_regex_filter(&mut clause, field, Some(q));
        clause
    })
    .collect::<Vec<_>>();
    if let Some(actions) = keyword_actions {
        clauses.push(doc! { "action_type": { "$in": actions.split(',').filter(|s| !s.is_empty()).collect::<Vec<_>>() } });
    }
    filter.insert("$and", vec![doc! { "$or": clauses }]);
}

impl Pagination for AuditEventFilter {
    // 将已校验的页码和页大小交给通用分页逻辑。
    fn page_and_size(&self) -> (u64, u64) {
        (self.paging.page, u64::from(self.paging.page_size))
    }
}

/// 为追加式审计事件集合提供列表查询，不提供软删除或恢复。
#[allow(async_fn_in_trait)]
pub trait AuditEventRepositoryExt {
    /// 用调用方执行器读取当前页投影行及同条件总数，不加载完整审计文档。
    /// 操作者和动作按忽略大小写的字面量匹配；对象和结果精确匹配，使用 `idx_audit_events_object_created` 对应字段。查询、游标读取或计数失败直接返回错误。
    async fn search_audit_events(
        &self,
        filter: &AuditEventFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<AuditEventRow>>;
}

impl AuditEventRepositoryExt for Repository<'_, AuditEvent> {
    // 用同一筛选和执行器分别读取最小字段投影和总数，确保分页不改变计数口径。
    async fn search_audit_events(
        &self,
        filter: &AuditEventFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<AuditEventRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(&filter.paging))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(audit_event_projection())
            .build();

        let collection = self.collection().clone_with_type::<AuditEventRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult { items, total: total as i64 })
    }
}
