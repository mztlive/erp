//! 集成列表投影行、筛选条件与查询文档组装。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::common::time::Instant;
use mongodb::bson::{Document, doc};
use persistence_core::{Pagination, QueryFilter, insert_literal_regex_filter};
use serde::{Deserialize, Serialize};

use crate::entity::integration_ops::{
    ErrorClass, ErrorTaskStatus, InboxMessageId, InboxMessageStatus, MessageType, ResolutionAction,
    ResolutionType, ResultingStatus, SourceSystemId,
};

/// 入站消息列表投影行（列表接口只取必要字段，禁止返回整文档；
/// 内容引用 `payload_reference` 不进入列表投影）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InboxMessageRow {
    /// 实体主键。
    pub id: String,
    /// 来源系统 ID。
    pub source_system_id: SourceSystemId,
    /// 来源事件 ID。
    pub source_event_id: String,
    /// 消息类型。
    pub message_type: MessageType,
    /// 业务事实键（幂等键）。
    pub business_fact_key: String,
    /// 来源契约版本。
    pub payload_schema_version: String,
    /// 消息处理状态。
    pub status: InboxMessageStatus,
    /// 来源系统发送时间。
    pub source_sent_at: Option<Instant>,
    /// ERP 接收时间。
    pub received_at: Instant,
    /// 处理完成时间。
    pub processed_at: Option<Instant>,
    /// 乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 入站消息列表筛选条件。
#[derive(Debug, Clone)]
pub struct InboxMessageFilter {
    /// 来源系统 ID；`None` 表示不筛选。
    pub source_system_id: Option<SourceSystemId>,
    /// 消息类型；`None` 表示不筛选。
    pub message_type: Option<MessageType>,
    /// 消息处理状态；`None` 表示不筛选。
    pub status: Option<InboxMessageStatus>,
    /// 来源事件 ID 模糊匹配（字面量、忽略大小写）；`None` 表示不筛选。
    pub source_event_id: Option<String>,
    /// 接收时间下界（Unix 秒，含）；`None` 表示不筛选。
    pub received_at_from: Option<i64>,
    /// 接收时间上界（Unix 秒，含）；`None` 表示不筛选。
    pub received_at_to: Option<i64>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl Default for InboxMessageFilter {
    fn default() -> Self {
        Self {
            source_system_id: None,
            message_type: None,
            status: None,
            source_event_id: None,
            received_at_from: None,
            received_at_to: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        }
    }
}

impl QueryFilter for InboxMessageFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = undeleted_base();
        if let Some(source_system_id) = &self.source_system_id {
            filter.insert("source_system_id", source_system_id.to_string());
        }
        if let Some(message_type) = self.message_type {
            filter.insert("message_type", message_type.as_str());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        insert_literal_regex_filter(&mut filter, "source_event_id", self.source_event_id.as_deref());
        insert_time_range(&mut filter, "received_at", self.received_at_from, self.received_at_to);
        filter
    }
}

impl Pagination for InboxMessageFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 集成错误任务列表投影行（列表接口只取必要字段；解决证据文本
/// `resolution` 不进入列表投影）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IntegrationErrorTaskRow {
    /// 实体主键。
    pub id: String,
    /// 关联的消息。
    pub message_id: Option<InboxMessageId>,
    /// 关联的业务对象。
    pub business_object_id: Option<String>,
    /// 错误分类。
    pub error_class: ErrorClass,
    /// 任务状态。
    pub status: ErrorTaskStatus,
    /// 责任角色。
    pub owner_role: Option<String>,
    /// 责任人。
    pub owner_user_id: Option<String>,
    /// 当前处理人有效内部组织。
    pub owner_org_unit_id: Option<String>,
    /// 已办处理人。
    pub completed_by: Option<String>,
    /// 重试次数。
    pub attempt_count: u32,
    /// 最近尝试时间。
    pub last_attempt_at: Option<Instant>,
    /// 最近尝试结果（脱敏）。
    pub last_attempt_summary: Option<String>,
    /// 解决方式。
    pub resolution_type: Option<ResolutionType>,
    /// 完成时间。
    pub resolved_at: Option<Instant>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 集成错误任务列表筛选条件。
#[derive(Debug, Clone)]
pub struct IntegrationErrorTaskFilter {
    /// 字面量关键词；分页和计数共同使用。
    pub q: Option<String>,
    /// 关联的消息；`None` 表示不筛选。
    pub message_id: Option<InboxMessageId>,
    /// 关联的业务对象；`None` 表示不筛选。
    pub business_object_id: Option<String>,
    /// 错误分类；`None` 表示不筛选。
    pub error_class: Option<ErrorClass>,
    /// 任务状态；`None` 表示不筛选。
    pub status: Option<ErrorTaskStatus>,
    /// 责任角色；`None` 表示不筛选。
    pub owner_role: Option<String>,
    /// 当前处理人精确 ID 列表；空表示不按人员收窄。
    pub handler_user_ids: Vec<String>,
    /// 历史处理人精确 ID 列表；空表示不按已办收窄。
    pub operator_user_ids: Vec<String>,
    /// 请求组织收窄后的处理人组织；空表示不按组织收窄。
    pub owner_org_unit_ids: Vec<String>,
    /// 已解析授权条件；`None` 表示公司范围不加额外条件。
    pub scope_document: Option<Document>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl Default for IntegrationErrorTaskFilter {
    fn default() -> Self {
        Self {
            q: None,
            message_id: None,
            business_object_id: None,
            error_class: None,
            status: None,
            owner_role: None,
            handler_user_ids: Vec::new(),
            operator_user_ids: Vec::new(),
            owner_org_unit_ids: Vec::new(),
            scope_document: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        }
    }
}

impl QueryFilter for IntegrationErrorTaskFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = undeleted_base();
        if let Some(message_id) = &self.message_id {
            filter.insert("message_id", message_id.to_string());
        }
        if let Some(business_object_id) = &self.business_object_id {
            filter.insert("business_object_id", business_object_id);
        }
        if let Some(error_class) = self.error_class {
            filter.insert("error_class", error_class.as_str());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        if let Some(owner_role) = &self.owner_role {
            filter.insert("owner_role", owner_role);
        }
        insert_id_in(&mut filter, "owner_user_id", &self.handler_user_ids);
        insert_id_in(&mut filter, "completed_by", &self.operator_user_ids);
        insert_id_in(&mut filter, "owner_org_unit_id", &self.owner_org_unit_ids);
        keyword_filter(
            &mut filter,
            self.q.as_deref(),
            &["id", "business_object_id", "message_id", "last_attempt_summary", "error_class"],
        );
        error_label_filter(&mut filter, self.q.as_deref());
        and_scope(filter, self.scope_document.as_ref())
    }
}

impl Pagination for IntegrationErrorTaskFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 对账差异列表投影行（正式差异事实，只读）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReconciliationDifferenceRow {
    /// 实体主键。
    pub id: String,
    /// 差异对象类型。
    pub business_object_type: String,
    /// 差异对象 ID。
    pub business_object_id: String,
    /// 差异分类。
    pub difference_type: String,
    /// 左侧不可变证据引用。
    pub left_fact_reference: Option<String>,
    /// 右侧不可变证据引用。
    pub right_fact_reference: Option<String>,
    /// 当前处理人。
    pub owner_user_id: Option<String>,
    /// 当前处理人有效内部组织。
    pub owner_org_unit_id: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 差异发现时间（秒级时间戳）。
    pub created_at: u64,
}

/// 对账差异列表筛选条件。
#[derive(Debug, Clone)]
pub struct ReconciliationDifferenceFilter {
    /// 字面量关键词；分页和计数共同使用。
    pub q: Option<String>,
    /// 差异对象类型；`None` 表示不筛选。
    pub business_object_type: Option<String>,
    /// 差异对象 ID；`None` 表示不筛选。
    pub business_object_id: Option<String>,
    /// 差异分类；`None` 表示不筛选。
    pub difference_type: Option<String>,
    /// 发现时间下界（Unix 秒，含）；`None` 表示不筛选。
    pub created_at_from: Option<i64>,
    /// 发现时间上界（Unix 秒，含）；`None` 表示不筛选。
    pub created_at_to: Option<i64>,
    /// 当前处理人精确 ID 列表；空表示不按人员收窄。
    pub handler_user_ids: Vec<String>,
    /// 历史处理人命中的差异 ID；`None` 表示不按已办收窄。
    pub operator_difference_ids: Option<Vec<String>>,
    /// 请求组织收窄后的处理人组织；空表示不按组织收窄。
    pub owner_org_unit_ids: Vec<String>,
    /// 已解析授权条件；`None` 表示公司范围不加额外条件。
    pub scope_document: Option<Document>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl Default for ReconciliationDifferenceFilter {
    fn default() -> Self {
        Self {
            q: None,
            business_object_type: None,
            business_object_id: None,
            difference_type: None,
            created_at_from: None,
            created_at_to: None,
            handler_user_ids: Vec::new(),
            operator_difference_ids: None,
            owner_org_unit_ids: Vec::new(),
            scope_document: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        }
    }
}

impl QueryFilter for ReconciliationDifferenceFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = undeleted_base();
        if let Some(business_object_type) = &self.business_object_type {
            filter.insert("business_object_type", business_object_type);
        }
        if let Some(business_object_id) = &self.business_object_id {
            filter.insert("business_object_id", business_object_id);
        }
        if let Some(difference_type) = &self.difference_type {
            filter.insert("difference_type", difference_type);
        }
        insert_time_range(&mut filter, "created_at", self.created_at_from, self.created_at_to);
        insert_id_in(&mut filter, "owner_user_id", &self.handler_user_ids);
        insert_id_in(&mut filter, "owner_org_unit_id", &self.owner_org_unit_ids);
        if let Some(ids) = &self.operator_difference_ids {
            filter.insert("id", doc! { "$in": ids });
        }
        keyword_filter(
            &mut filter,
            self.q.as_deref(),
            &[
                "id",
                "business_object_id",
                "business_object_type",
                "difference_type",
                "left_fact_reference",
                "right_fact_reference",
            ],
        );
        if self.q.as_deref().is_some_and(|q| "对账差异".contains(q)) {
            filter.remove("$or");
        }
        and_scope(filter, self.scope_document.as_ref())
    }
}

impl Pagination for ReconciliationDifferenceFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 差异解决记录历史投影行（不可变追加记录，只读）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolutionHistoryRow {
    /// 实体主键。
    pub id: String,
    /// 递增处理序号。
    pub resolution_no: u32,
    /// 解决动作。
    pub resolution_action: ResolutionAction,
    /// 动作后的派生状态。
    pub resulting_status: ResultingStatus,
    /// 终态证据引用。
    pub evidence_reference: Option<String>,
    /// 处理人。
    pub handled_by: String,
    /// 处理时间。
    pub handled_at: Instant,
}

/// 未删除基底文档（三类列表过滤与批量查询共用；修订表外全部列表查询自动追加）。
///
/// # 返回
/// 返回仅含 `deleted_at` 未删除标记的查询文档。
pub(crate) fn undeleted_base() -> Document {
    doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON }
}

/// 向查询条件追加秒级时间戳闭区间范围（BSON Int64 形态，与 `Instant`/`created_at`
/// 持久化形态一致；区间两端可选，任一端缺失表示不设界）。
///
/// # 参数
/// * `filter` - 待追加的查询条件
/// * `field` - 时间字段名
/// * `from` - 下界（含）；`None` 表示不设下界
/// * `to` - 上界（含）；`None` 表示不设上界
///
/// # 返回
/// 无返回值；直接修改传入的查询文档。
pub(crate) fn insert_time_range(filter: &mut Document, field: &str, from: Option<i64>, to: Option<i64>) {
    let mut range = Document::new();
    if let Some(from) = from {
        range.insert("$gte", from);
    }
    if let Some(to) = to {
        range.insert("$lte", to);
    }
    if !range.is_empty() {
        filter.insert(field, range);
    }
}
/// 向查询条件追加精确 ID 列表 `$in` 条件（空集合不加条件，避免误伤）。
///
/// # 参数
/// * `filter` - 待追加的查询条件
/// * `field` - ID 字段名
/// * `ids` - 精确 ID 列表；空表示不收窄
///
/// # 返回
/// 无返回值；直接修改传入的查询文档。
pub(crate) fn insert_id_in(filter: &mut Document, field: &str, ids: &[String]) {
    if !ids.is_empty() {
        filter.insert(field, doc! { "$in": ids });
    }
}

/// 把已解析授权条件与业务筛选求交（空授权文档不加条件，公司范围不收窄）。
///
/// # 参数
/// * `filter` - 业务筛选文档
/// * `scope` - 已解析授权条件；`None` 或空文档表示公司范围
///
/// # 返回
/// 返回求交后的查询文档。
pub(crate) fn and_scope(filter: Document, scope: Option<&Document>) -> Document {
    match scope {
        Some(scope) if !scope.is_empty() => doc! { "$and": [filter, scope.clone()] },
        _ => filter,
    }
}

/// 向查询条件追加字面量多字段 OR 匹配（结构化范围保留为 AND 条件）。
///
/// # 参数
/// * `filter` - 待追加的查询条件
/// * `q` - 字面量关键词；`None` 表示不筛选
/// * `fields` - 待匹配字段列表
///
/// # 返回
/// 无返回值；直接修改传入的查询文档。
pub(crate) fn keyword_filter(filter: &mut Document, q: Option<&str>, fields: &[&str]) {
    let Some(q) = q else {
        return;
    };
    let clauses = fields
        .iter()
        .map(|field| {
            let mut clause = Document::new();
            insert_literal_regex_filter(&mut clause, field, Some(q));
            clause
        })
        .collect::<Vec<_>>();
    filter.insert("$or", clauses);
}

/// 保留队列现有中文错误类别搜索，别名仅扩展类别 OR 分支。
fn error_label_filter(filter: &mut Document, q: Option<&str>) {
    let Some(q) = q else {
        return;
    };
    let aliases = [
        (ErrorClass::CapabilityGap, "能力不足"),
        (ErrorClass::MappingError, "参数/映射错误"),
        (ErrorClass::BusinessRejected, "供应商业务拒绝"),
        (ErrorClass::TransientFailure, "临时故障"),
        (ErrorClass::ResultUnknown, "结果未知"),
        (ErrorClass::AuthSignature, "鉴权/签名失败"),
        (ErrorClass::RateLimited, "调用次数受限"),
        (ErrorClass::OutOfOrder, "通知顺序异常"),
    ];
    let codes = aliases
        .iter()
        .filter(|(class, label)| label.contains(q) || class.label().contains(q))
        .map(|(class, _)| class.as_str())
        .collect::<Vec<_>>();
    if let Ok(clauses) = filter.get_array_mut("$or") {
        clauses.push(doc! { "error_class": { "$in": codes } }.into());
    }
}

#[cfg(test)]
mod tests {
    use persistence_core::QueryFilter;

    use super::{InboxMessageFilter, IntegrationErrorTaskFilter, ReconciliationDifferenceFilter};
    use crate::entity::integration_ops::{
        ErrorClass, ErrorTaskStatus, InboxMessageStatus, MessageType, SourceSystemId,
    };

    #[test]
    fn inbox_filter_applies_optional_fields_and_time_range() {
        let filter = InboxMessageFilter {
            source_system_id: Some(SourceSystemId::new("sys-mall-1")),
            message_type: Some(MessageType::PaymentSucceeded),
            status: Some(InboxMessageStatus::Received),
            source_event_id: Some("SO-1.".to_string()),
            received_at_from: Some(1_700_000_000),
            received_at_to: Some(1_700_000_100),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("source_system_id").unwrap(), "sys-mall-1");
        assert_eq!(document.get_str("message_type").unwrap(), "PAYMENT_SUCCEEDED");
        assert_eq!(document.get_str("status").unwrap(), "received");
        assert_eq!(document.get_document("source_event_id").unwrap().get_str("$regex").unwrap(), r"SO\-1\.");
        let range = document.get_document("received_at").unwrap();
        assert_eq!(range.get_i64("$gte").unwrap(), 1_700_000_000);
        assert_eq!(range.get_i64("$lte").unwrap(), 1_700_000_100);
    }

    #[test]
    fn error_task_filter_maps_enums_to_stable_codes() {
        let filter = IntegrationErrorTaskFilter {
            q: None,
            message_id: None,
            business_object_id: Some("so-1".to_string()),
            error_class: Some(ErrorClass::TransientFailure),
            status: Some(ErrorTaskStatus::AutoRetrying),
            owner_role: Some("ops".to_string()),
            handler_user_ids: vec!["u-1".to_string()],
            operator_user_ids: Vec::new(),
            owner_org_unit_ids: Vec::new(),
            scope_document: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_str("error_class").unwrap(), "transient_failure");
        assert_eq!(document.get_str("status").unwrap(), "auto_retrying");
        assert_eq!(document.get_str("owner_role").unwrap(), "ops");
        assert_eq!(
            document.get_document("owner_user_id").unwrap().get_array("$in").unwrap()[0].as_str(),
            Some("u-1")
        );
    }

    #[test]
    fn difference_filter_applies_object_key_and_time_range() {
        let filter = ReconciliationDifferenceFilter {
            q: None,
            business_object_type: Some("mall_order".to_string()),
            business_object_id: Some("MO-1".to_string()),
            difference_type: Some("amount_mismatch".to_string()),
            created_at_from: Some(1_700_000_000),
            created_at_to: Some(1_700_000_100),
            handler_user_ids: Vec::new(),
            operator_difference_ids: None,
            owner_org_unit_ids: Vec::new(),
            scope_document: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_str("business_object_type").unwrap(), "mall_order");
        assert_eq!(document.get_str("business_object_id").unwrap(), "MO-1");
        assert_eq!(document.get_str("difference_type").unwrap(), "amount_mismatch");
        let range = document.get_document("created_at").unwrap();
        assert_eq!(range.get_i64("$gte").unwrap(), 1_700_000_000);
        assert_eq!(range.get_i64("$lte").unwrap(), 1_700_000_100);
    }
}

#[cfg(test)]
mod keyword_tests {
    use super::*;

    /// 关键词按字面量 OR 匹配且不覆盖状态、归属与软删除条件。
    #[test]
    fn literal_keyword_keeps_structured_constraints() {
        let filter = IntegrationErrorTaskFilter {
            q: Some("event.[x]".into()),
            message_id: None,
            business_object_id: None,
            error_class: None,
            status: Some(ErrorTaskStatus::Pending),
            owner_role: None,
            handler_user_ids: vec!["alice".into()],
            operator_user_ids: Vec::new(),
            owner_org_unit_ids: Vec::new(),
            scope_document: None,
            page: 3,
            page_size: 50,
            sort_by: None,
            sort_ascending: false,
        };
        let document = filter.to_doc();
        assert_eq!(document.get_str("status").unwrap(), "pending");
        assert!(document.contains_key("deleted_at"));
        assert!(document.contains_key("owner_user_id"));
        let clauses = document.get_array("$or").unwrap();
        assert!(
            clauses.iter().any(|clause| clause.as_document().unwrap().contains_key("last_attempt_summary"))
        );
        assert_eq!(
            clauses[0].as_document().unwrap().get_document("id").unwrap().get_str("$regex").unwrap(),
            r"event\.\[x\]"
        );
    }

    #[test]
    fn difference_filter_projects_current_handler_ids() {
        let filter = ReconciliationDifferenceFilter {
            q: None,
            business_object_type: None,
            business_object_id: None,
            difference_type: None,
            created_at_from: None,
            created_at_to: None,
            handler_user_ids: vec!["handler-1".into()],
            operator_difference_ids: Some(vec!["diff-9".into()]),
            owner_org_unit_ids: vec!["org-a".into()],
            scope_document: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };
        let document = filter.to_doc();
        assert_eq!(
            document.get_document("owner_user_id").unwrap().get_array("$in").unwrap()[0].as_str(),
            Some("handler-1")
        );
        assert_eq!(
            document.get_document("id").unwrap().get_array("$in").unwrap()[0].as_str(),
            Some("diff-9")
        );
        assert_eq!(
            document.get_document("owner_org_unit_id").unwrap().get_array("$in").unwrap()[0].as_str(),
            Some("org-a")
        );
    }
}
