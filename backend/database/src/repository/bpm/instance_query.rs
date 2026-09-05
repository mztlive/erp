use bpm::ids::ApprovalProcessInstanceId;
use bpm::model::types::ApprovalProcessInstanceStatus;
use bpm::model::ApprovalProcessInstance;
use bpm::SubjectRef;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;

use super::{
    clamp_limit, find_limited, ApprovalInstanceListCursor, ApprovalInstanceListFilter,
    ApprovalInstanceListView, ApprovalInstanceSummary, ApprovalInstanceTextQuery, BpmWorkflowRepository,
    INSTANCES, MAX_INSTANCE_PAGE,
};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

impl<'a> BpmWorkflowRepository<'a> {
    /// 按主键读取审批流程实例。
    ///
    /// # 参数
    /// * `instance_id` - 审批流程实例 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配且未软删除的实例；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    ///
    /// # 关键业务约束
    /// 本方法只读取实例身份，不放宽状态或主体约束。
    pub async fn find_instance_by_id(
        &self,
        instance_id: &ApprovalProcessInstanceId,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalProcessInstance>> {
        self.instances().find_by_id(instance_id.as_ref(), executor).await
    }

    /// 判断业务主体是否已经启动过审批实例。
    ///
    /// # 参数
    /// * `subject` - 业务对象稳定引用
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 任意未删除审批实例命中主体时返回 `true`。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    ///
    /// # 关键业务约束
    /// 终态实例仍证明单据已经启动过审批，因此本查询不附加状态条件。
    pub async fn has_started_instance_for_subject(
        &self,
        subject: &SubjectRef,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        mongo_ops::exists(
            &self.db.collection::<Document>(INSTANCES),
            doc! {
                "subject.subject_kind": subject.subject_kind(),
                "subject.subject_id": subject.subject_id(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            executor,
        )
        .await
    }

    /// 查询同一主体与提交版本的非终态实例。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn find_non_terminal_by_subject(
        &self,
        subject: &SubjectRef,
        subject_version: u32,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalProcessInstance>> {
        self.instances()
            .find_one(non_terminal_subject_filter(subject, subject_version), executor)
            .await
    }

    /// 读取同一主体与提交版本的可取消实例候选。
    ///
    /// 本查询按同一主体与提交版本读取最近实例，不预先过滤状态；实例是否可取消以及
    /// 开放任务数量是否一致，统一由 BPM 模型在全部事实加载后判定。
    ///
    /// # 参数
    /// * `subject` - 业务对象引用
    /// * `subject_version` - 冻结提交版本
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回可供取消规则校验的实例候选；没有时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn cancellation_instance_by_subject(
        &self,
        subject: &SubjectRef,
        subject_version: u32,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalProcessInstance>> {
        let mut rows = find_limited(
            &self.db.collection(INSTANCES),
            cancellation_subject_filter(subject, subject_version),
            doc! { "started_at": -1, "id": -1 },
            1,
            executor,
        )
        .await?;
        Ok(rows.pop())
    }

    /// 按主体读取最近一条审批实例，含已终态，供单据详情投影。
    ///
    /// 命中 `idx_approval_process_instances_subject_history`。撤回命令应使用
    /// `cancellation_instance_by_subject`，不得用本方法替代提交版本与状态校验。
    ///
    /// # 参数
    /// * `subject` - 业务对象引用
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 按 `started_at desc, id desc` 的最新实例；没有时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn find_latest_by_subject(
        &self,
        subject: &SubjectRef,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalProcessInstance>> {
        let mut rows = find_limited(
            &self.db.collection(INSTANCES),
            latest_subject_filter(subject),
            doc! { "started_at": -1, "id": -1 },
            1,
            executor,
        )
        .await?;
        Ok(rows.pop())
    }

    /// 按视图、状态和 DataScope 过滤分页读取实例摘要。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_instance_summaries(
        &self,
        filter: &ApprovalInstanceListFilter,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ApprovalInstanceSummary>> {
        if instance_list_scope_empty(filter) {
            return Ok(Vec::new());
        }
        let options = FindOptions::builder()
            .sort(instance_list_sort(filter))
            .limit(instance_list_limit(filter.limit))
            .projection(instance_summary_projection())
            .build();
        mongo_ops::find_many(
            &self.db.collection::<ApprovalInstanceSummary>(INSTANCES),
            instance_list_filter_doc(filter),
            options,
            executor,
        )
        .await
    }

    /// 统计当前授权过滤条件下的审批实例总数。
    ///
    /// 游标只限定当前页起点，不得缩小总数口径，因此统计时固定移除游标。
    ///
    /// # 参数
    /// * `filter` - 与列表相同的视图、状态、启动人和对象范围
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回当前过滤条件下的完整实例数量。
    ///
    /// # 错误
    /// MongoDB 统计失败时返回错误。
    pub async fn count_instance_summaries(
        &self,
        filter: &ApprovalInstanceListFilter,
        executor: &mut dyn Executor,
    ) -> Result<u64> {
        if instance_list_scope_empty(filter) {
            return Ok(0);
        }
        let mut total_filter = filter.clone();
        total_filter.cursor = None;
        mongo_ops::count_documents(
            &self.db.collection::<ApprovalInstanceSummary>(INSTANCES),
            instance_list_filter_doc(&total_filter),
            executor,
        )
        .await
    }

    /// 按实例主键批量读取审批运行的有界列表投影。
    ///
    /// # 参数
    /// * `instance_ids` - 已由工作项节点执行解析出的审批实例 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前节点、当前审批人、最近驳回摘要和实例状态；不扫描执行历史。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    ///
    /// # 关键业务约束
    /// 只允许使用有界实例投影补充工作台判断事实，不得为列表逐实例查询历史。
    pub async fn list_instance_summaries_by_ids(
        &self,
        instance_ids: &[ApprovalProcessInstanceId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ApprovalInstanceSummary>> {
        if instance_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids = instance_ids.iter().map(ToString::to_string).collect::<Vec<_>>();
        let options = FindOptions::builder()
            .projection(instance_summary_projection())
            .build();
        mongo_ops::find_many(
            &self.db.collection::<ApprovalInstanceSummary>(INSTANCES),
            doc! {
                "id": { "$in": ids },
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            options,
            executor,
        )
        .await
    }
}

/// 构造取消候选的主体与提交版本过滤条件，不预先判断实例状态。
///
/// # 参数
/// * `subject` - 业务对象引用
/// * `subject_version` - 冻结提交版本
///
/// # 返回
/// 返回主体、提交版本和软删除约束，供 BPM 模型统一判定是否可取消。
///
/// # 错误
/// 无。
pub(super) fn cancellation_subject_filter(subject: &SubjectRef, subject_version: u32) -> Document {
    doc! {
        "subject.subject_kind": subject.subject_kind(),
        "subject.subject_id": subject.subject_id(),
        "subject_version": i64::from(subject_version),
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    }
}

pub(super) fn non_terminal_subject_filter(subject: &SubjectRef, subject_version: u32) -> Document {
    doc! {
        "subject.subject_kind": subject.subject_kind(),
        "subject.subject_id": subject.subject_id(),
        "subject_version": i64::from(subject_version),
        "status": {
            "$in": [
                ApprovalProcessInstanceStatus::Running.as_str(),
                ApprovalProcessInstanceStatus::Blocked.as_str(),
            ]
        },
    }
}

/// 构造按主体读取历史实例的过滤条件，不含状态限制。
///
/// # 参数
/// * `subject` - 业务对象引用
///
/// # 返回
/// 返回含主体键与软删除约束的查询文档。
///
/// # 错误
/// 无。
pub(super) fn latest_subject_filter(subject: &SubjectRef) -> Document {
    doc! {
        "subject.subject_kind": subject.subject_kind(),
        "subject.subject_id": subject.subject_id(),
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    }
}

pub(crate) fn instance_list_scope_empty(filter: &ApprovalInstanceListFilter) -> bool {
    if filter.subject_ids.as_ref().is_some_and(Vec::is_empty) {
        return true;
    }
    filter.view == ApprovalInstanceListView::Started && !started_by_present(filter)
}

/// 判断 `Started` 视图是否带有非空发起人前缀。
///
/// # 参数
/// * `filter` - 实例列表过滤条件
///
/// # 返回
/// `started_by` 为非空字符串时返回 `true`。
fn started_by_present(filter: &ApprovalInstanceListFilter) -> bool {
    filter
        .started_by
        .as_ref()
        .is_some_and(|started_by| !started_by.is_empty())
}

/// 把列表过滤条件编译为 MongoDB 文档。
///
/// # 参数
/// * `filter` - Service 已计算的视图、启动人、对象范围、游标与字面量检索
///
/// # 返回
/// 返回可直接交给 `find` / `count_documents` 的过滤文档。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 字面量检索与游标都使用 `$or` 时必须用 `$and` 组合，不得互相覆盖。
pub(crate) fn instance_list_filter_doc(filter: &ApprovalInstanceListFilter) -> Document {
    let mut document = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
    if let Some(process_kind) = filter.process_kind {
        document.insert("process_kind", process_kind.as_str());
    }
    insert_instance_status(&mut document, filter);
    if let Some(started_by) = &filter.started_by {
        document.insert("started_by", started_by);
    }
    if let Some(subject_kind) = &filter.subject_kind {
        document.insert("subject.subject_kind", subject_kind);
    }
    if let Some(subject_ids) = &filter.subject_ids {
        document.insert("subject.subject_id", doc! { "$in": subject_ids.clone() });
    }
    insert_list_disjunctions(&mut document, filter);
    document
}

/// 把游标与字面量检索的 `$or` 组合成互不覆盖的过滤条件。
///
/// # 参数
/// * `document` - 已写入等值条件的过滤文档
/// * `filter` - 列表过滤条件
///
/// # 返回
/// 无；就地写入 `$or` 或 `$and`。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 顶层只能有一个 `$or`。游标与检索同时存在时必须用 `$and` 连接两组 `$or`，不得互相覆盖。
fn insert_list_disjunctions(document: &mut Document, filter: &ApprovalInstanceListFilter) {
    let mut groups = Vec::new();
    if let Some(cursor) = &filter.cursor {
        groups.push(doc! { "$or": instance_cursor_or(filter.view, cursor) });
    }
    if let Some(text_query) = &filter.text_query {
        groups.push(doc! { "$or": instance_text_query_or(text_query) });
    }
    match groups.len() {
        0 => {}
        1 => {
            if let Some(or) = groups[0].get("$or").cloned() {
                document.insert("$or", or);
            }
        }
        _ => {
            document.insert("$and", groups);
        }
    }
}

/// 构造字面量检索的 `$or` 分支。
///
/// # 参数
/// * `text_query` - 已规范化检索串
///
/// # 返回
/// 返回对象 ID、当前节点名和当前审批人的字面量匹配分支。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 检索串按字面量转义，调用方不得传入正则。快照编号只允许由联合快照聚合
/// 匹配，不得把未经三元组校验的实例 ID 注入本查询。
pub(super) fn instance_text_query_or(text_query: &ApprovalInstanceTextQuery) -> Vec<Document> {
    let literal = regex::escape(text_query.query.trim());
    let regex = doc! { "$regex": &literal, "$options": "i" };
    vec![
        doc! { "subject.subject_id": regex.clone() },
        doc! { "current_assignee_name": regex.clone() },
        doc! { "current_node_name": regex },
    ]
}

fn insert_instance_status(document: &mut Document, filter: &ApprovalInstanceListFilter) {
    if filter.view == ApprovalInstanceListView::Blocked {
        document.insert("status", ApprovalProcessInstanceStatus::Blocked.as_str());
        return;
    }
    if let Some(status) = filter.status {
        document.insert("status", status.as_str());
    }
}

pub(crate) fn instance_list_sort(filter: &ApprovalInstanceListFilter) -> Document {
    match filter.view {
        ApprovalInstanceListView::Started => doc! { "started_at": -1, "id": -1 },
        ApprovalInstanceListView::Blocked => doc! { "blocked_at": -1, "id": -1 },
        ApprovalInstanceListView::Managed if filter.status.is_some() => {
            doc! { "status": 1, "updated_at": -1, "id": -1 }
        }
        ApprovalInstanceListView::Managed => doc! { "updated_at": -1, "id": -1 },
    }
}

pub(crate) fn instance_cursor_or(
    view: ApprovalInstanceListView,
    cursor: &ApprovalInstanceListCursor,
) -> Vec<Document> {
    let field = match view {
        ApprovalInstanceListView::Started => "started_at",
        ApprovalInstanceListView::Blocked => "blocked_at",
        ApprovalInstanceListView::Managed => "updated_at",
    };
    vec![
        doc! { field: { "$lt": cursor.sort_time } },
        doc! { field: cursor.sort_time, "id": { "$lt": cursor.id.as_str() } },
    ]
}

pub(crate) fn instance_summary_projection() -> Document {
    doc! {
        "id": 1,
        "process_kind": 1,
        "process_definition_id": 1,
        "definition_version": 1,
        "subject": 1,
        "subject_version": 1,
        "status": 1,
        "current_round_no": 1,
        "current_node_execution_id": 1,
        "current_node_key": 1,
        "current_node_name": 1,
        "current_assignee_participant_id": 1,
        "current_assignee_name": 1,
        "latest_rejected_execution_id": 1,
        "latest_rejection_summary": 1,
        "last_status_changed_at": 1,
        "started_by": 1,
        "started_at": 1,
        "blocked_at": 1,
        "version": 1,
        "updated_at": 1,
    }
}

/// 返回实例列表统一页大小上限。
pub(crate) fn instance_list_limit(limit: u32) -> i64 {
    clamp_limit(limit, MAX_INSTANCE_PAGE)
}
