//! 域 D06 `access_control` 仓储：audit_event 审计留痕查询。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{
    Executor, PageResult, Pagination, QueryFilter, Repository, Result, insert_literal_regex_filter, mongo_ops,
};
use serde::{Deserialize, Serialize};

use super::{audit_event_projection, sort_doc};
use crate::entity::access_control::{AuditEvent, AuditEventResult};

/// 审计事件列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuditEventRow {
    /// 实体主键。
    pub id: String,
    /// 操作者 ID。
    pub actor_id: String,
    /// 操作者名称快照。
    pub actor_label: String,
    /// 责任角色快照。
    pub actor_role: String,
    /// 动作代码。
    pub action_type: String,
    /// 业务对象类型代码。
    pub object_type: String,
    /// 业务对象 ID。
    pub object_id: Option<String>,
    /// 业务对象安全标题。
    pub object_label: Option<String>,
    /// 请求追踪号。
    pub request_id: Option<String>,
    /// 最终结果。
    pub result: AuditEventResult,
    /// 变更字段名（只记录字段名和「已变更」）。
    pub changed_field_names: Vec<String>,
    /// 来源 IP。
    pub source_ip: Option<String>,
    /// 创建时间（秒级时间戳，即事件发生时间）。
    pub created_at: u64,
}

/// 审计事件列表筛选条件。
#[derive(Debug, Clone)]
pub struct AuditEventFilter {
    /// 操作者、动作、对象或追踪号字面量关键词。
    pub q: Option<String>,
    /// 界面动作标签匹配的动作代码（逗号分隔），只作为关键词 OR 条件。
    pub keyword_actions: Option<String>,
    /// 审计事件稳定身份。
    pub event_id: Option<String>,
    /// 链路追踪号或请求号精确筛选。
    pub trace_id: Option<String>,
    /// 创建时间下界（含，Unix 秒）。
    pub created_from: Option<u64>,
    /// 创建时间上界（不含，Unix 秒）。
    pub created_before: Option<u64>,

    /// 操作者 ID（忽略大小写字面量模糊匹配）；`None` 表示不筛选。
    pub actor_id: Option<String>,
    /// 动作代码（忽略大小写字面量模糊匹配）；`None` 表示不筛选。
    pub action_type: Option<String>,
    /// 业务对象类型代码；`None` 表示不筛选。
    pub object_type: Option<String>,
    /// 业务对象 ID；`None` 表示不筛选。
    pub object_id: Option<String>,
    /// 最终结果；`None` 表示不筛选。
    pub result: Option<AuditEventResult>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`created_at` / `updated_at`，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl Default for AuditEventFilter {
    /// 返回首页空筛选（`page: 1`，`page_size: 20`）。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回筛选为空、降序的首页过滤条件。
    ///
    /// # 错误
    /// 无。
    fn default() -> Self {
        Self {
            q: None,
            keyword_actions: None,
            event_id: None,
            trace_id: None,
            created_from: None,
            created_before: None,
            actor_id: None,
            action_type: None,
            object_type: None,
            object_id: None,
            result: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        }
    }
}

impl QueryFilter for AuditEventFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
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

/// 追加可选精确匹配条件（`None` 时不写入字段）。
///
/// # 参数
/// * `filter` - 待追加的查询条件文档
/// * `field` - 字段名
/// * `value` - 可选精确值；`None` 表示不筛选
fn insert_exact_match(filter: &mut Document, field: &str, value: Option<&str>) {
    if let Some(value) = value {
        filter.insert(field, value);
    }
}

/// 追加 `created_at` 时间范围条件（秒级时间戳，越界回落 `i64::MAX`）。
///
/// # 参数
/// * `filter` - 待追加的查询条件文档
/// * `from` - 区间下界（含）；`None` 表示不限制
/// * `before` - 区间上界（不含）；`None` 表示不限制
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

/// 追加关键词全文子句（多字段字面量 OR）与动作标签子句。
///
/// # 参数
/// * `filter` - 待追加的查询条件文档
/// * `q` - 操作者/动作/对象/追踪号字面量关键词；`None` 表示不筛选
/// * `keyword_actions` - 界面动作标签匹配的动作代码（逗号分隔）；`None` 表示不限制
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
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 审计事件集合仓储的域特有查询。
#[allow(async_fn_in_trait)]
pub trait AuditEventRepositoryExt {
    /// 分页检索审计事件（投影查询）。
    ///
    /// 只返回 [`AuditEventRow`] 所需的审计字段，不加载整文档；`actor_id` /
    /// `action_type` 按字面量忽略大小写模糊匹配（复用 `repository::regex_filter`），
    /// 对象/结果精确匹配覆盖 `idx_audit_events_object_created`。审计事件是
    /// 追加式留痕，本集合**不提供**软删除/恢复方法。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    async fn search_audit_events(
        &self,
        filter: &AuditEventFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<AuditEventRow>>;
}

impl AuditEventRepositoryExt for Repository<'_, AuditEvent> {
    async fn search_audit_events(
        &self,
        filter: &AuditEventFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<AuditEventRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
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
