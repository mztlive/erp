//! 域 D03 `work_item` 仓储：指定责任人的人工任务队列查询。

use crate::entity::work_item::{AssignmentSource, WorkItemPriority, WorkItemStatus, WorkItemType};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::common::time::Instant;
use mongodb::bson::{doc, Document};
use serde::{Deserialize, Serialize};

use persistence_core::{Pagination, QueryFilter};

mod approval;
mod finance;
mod fulfillment;
mod integration_task_binding;
mod mapping_task;
mod query;

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
    /// 允许的责任组织集合；为空时不筛选。
    pub owner_organization_ids: Vec<String>,
    /// 允许的注册任务类型与权威业务对象类型组合。
    pub object_access_shapes: Option<Vec<(WorkItemType, String)>>,
    /// 当前个人责任人；为空时不筛选。
    pub owner_user_id: Option<String>,
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
        insert_active_type_filter(&mut filter, &self.work_item_types);
        insert_enum_filter(
            &mut filter,
            "priority",
            &self.priorities,
            WorkItemPriority::as_str,
        );
        insert_string_filter(&mut filter, "owner_organization_id", &self.owner_organization_ids);
        if let Some(owner_user_id) = &self.owner_user_id {
            filter.insert("owner_user_id", owner_user_id);
        }
        let mut conjunctions = Vec::new();
        if let Some(shapes) = &self.object_access_shapes {
            conjunctions.push(object_access_shape_filter(shapes));
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

/// 过滤已退役的历史任务类型；显式仅选旧类型时返回空集，禁止扩大查询范围。
///
/// # 参数
/// * `filter` - 工作项查询文档
/// * `types` - 请求的类型；空集合表示全部当前任务
///
/// # 返回
/// 原地写入类型约束，不访问存储。
fn insert_active_type_filter(filter: &mut Document, types: &[WorkItemType]) {
    let active_types = types
        .iter()
        .copied()
        .filter(|kind| {
            !matches!(
                kind,
                WorkItemType::CardFundsReview | WorkItemType::CardFundsDeltaReview
            )
        })
        .collect::<Vec<_>>();
    if types.is_empty() {
        filter.insert(
            "work_item_type",
            doc! { "$nin": ["CARD_FUNDS_REVIEW", "CARD_FUNDS_DELTA_REVIEW"] },
        );
    } else if active_types.is_empty() {
        filter.insert("work_item_type", doc! { "$in": Vec::<String>::new() });
    } else {
        insert_enum_filter(filter, "work_item_type", &active_types, WorkItemType::as_str);
    }
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
            { "responsibility_key": { "$regex": &literal, "$options": "i" } },
            { "reason_code": { "$regex": &literal, "$options": "i" } },
            { "impact_summary": { "$regex": &literal, "$options": "i" } },
        ]
    }
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

#[cfg(test)]
mod tests {
    use mongodb::bson::{doc, Bson};

    use super::WorkItemFilter;
    use crate::entity::work_item::{WorkItemPriority, WorkItemStatus, WorkItemType};
    use erp_core::common::time::Instant;
    use persistence_core::QueryFilter;

    #[test]
    fn scope_filter_supports_direct_owner_and_history_facts() {
        let mine = WorkItemFilter {
            statuses: vec![WorkItemStatus::Open],
            work_item_types: vec![
                WorkItemType::ImportBusinessConfirmation,
                WorkItemType::BusinessException,
            ],
            owner_user_id: Some("alice".to_string()),
            owner_organization_ids: vec!["org-1".to_string()],
            priorities: vec![WorkItemPriority::High, WorkItemPriority::Urgent],
            due_from: Some(Instant::from_unix_secs(100)),
            due_before: Some(Instant::from_unix_secs(200)),
            page: 1,
            page_size: 20,
            ..WorkItemFilter::default()
        }
        .to_doc();
        assert_eq!(mine.get_str("status").unwrap(), "OPEN");
        assert_eq!(mine.get_str("owner_user_id").unwrap(), "alice");
        assert_eq!(
            mine.get_document("due_at").unwrap(),
            &doc! { "$gte": 100_i64, "$lt": 200_i64 }
        );
        assert_eq!(
            mine.get_document("priority").unwrap(),
            &doc! { "$in": ["high", "urgent"] }
        );
        assert_eq!(
            mine.get_document("work_item_type").unwrap(),
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
}

#[cfg(test)]
mod retired_review_tests {
    use super::*;

    /// 默认队列和显式类型筛选均不得重新暴露已退役任务。
    #[test]
    fn retired_review_types_are_excluded_from_queues() {
        let default = WorkItemFilter::default().to_doc();
        assert_eq!(
            default
                .get_document("work_item_type")
                .unwrap()
                .get_array("$nin")
                .unwrap()
                .len(),
            2
        );
        let retired = WorkItemFilter {
            work_item_types: vec![WorkItemType::CardFundsReview],
            ..Default::default()
        }
        .to_doc();
        assert!(retired
            .get_document("work_item_type")
            .unwrap()
            .get_array("$in")
            .unwrap()
            .is_empty());
        let mixed = WorkItemFilter {
            work_item_types: vec![WorkItemType::CardFundsReview, WorkItemType::SalesInvoiceExecution],
            ..Default::default()
        }
        .to_doc();
        assert_eq!(
            mixed.get_str("work_item_type").unwrap(),
            "SALES_INVOICE_EXECUTION"
        );
    }
}
