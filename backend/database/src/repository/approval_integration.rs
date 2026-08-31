//! ERP 审批集成仓储：业务对象快照与通知 outbox。

use bpm::ProcessKind;
use entities::approval_integration::{
    ApprovalNotificationDeliveryStatus, ApprovalNotificationOutbox, ApprovalSubjectSnapshot,
};
use entities::common::time::Instant;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use futures_util::TryStreamExt;
use mongodb::bson::{doc, Bson, Document};
use mongodb::options::ReturnDocument;
use mongodb::{Collection, Database};
use serde::Deserialize;

use super::bpm::{
    instance_cursor_or, instance_list_filter_doc, instance_list_limit, instance_list_scope_empty,
    instance_list_sort, instance_summary_projection, ApprovalInstanceListFilter, ApprovalInstanceSummary,
};
use super::extensions::{ApprovalIntegrationExt, BpmExt};
use super::Repository;
use crate::executor::Executor;
use crate::{mongo_ops, Error, Result};

const MAX_OUTBOX_BATCH: i64 = 50;
const INSTANCES: &str = <Database as BpmExt>::APPROVAL_PROCESS_INSTANCES;
const SNAPSHOTS: &str = <Database as ApprovalIntegrationExt>::APPROVAL_SUBJECT_SNAPSHOTS;

/// Service 已证明的单一流程种类读取范围。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalRuntimeReadTypeScope {
    /// 已证明可进入当前视图的流程种类。
    pub process_kind: ProcessKind,
    /// 管理视图中由对象读取权限对应角色得出的组织范围；`None` 表示不需要
    /// 具体组织证明（本人发起或公司级范围）。
    pub organization_ids: Option<Vec<String>>,
}

/// Service 已证明的运行实例读取范围。
///
/// Repository 只接收发起人事实，或逐流程种类绑定的组织范围，不读取或解释
/// RBAC、角色和 DataScope。枚举形态禁止把 Started 错接到管理 DataScope，也
/// 禁止把管理视图伪装成发起人旁路。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalRuntimeReadScope {
    /// 本人发起的普通读取；无需从展示快照证明组织。
    Started {
        /// 允许进入审批运行时的流程种类。
        process_kinds: Vec<ProcessKind>,
        /// 必须与实例 `started_by` 过滤完全一致的当前账号。
        submitted_by: String,
    },
    /// Managed/Blocked 管理读取；每类流程绑定自己的对象权限 DataScope。
    Managed {
        /// 已证明类型级运行管理、对象读取权限及其组织范围的流程种类。
        type_scopes: Vec<ApprovalRuntimeReadTypeScope>,
    },
}

/// 经授权范围过滤后的审批实例列表行。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ApprovalRuntimeReadRow {
    /// BPM 有界列表投影。
    #[serde(flatten)]
    pub instance: ApprovalInstanceSummary,
    /// 与实例精确三元组匹配的唯一冻结快照；缺失或漂移时为空。
    pub snapshot: Option<ApprovalSubjectSnapshot>,
}

/// 经不可变快照范围过滤后的审批实例页。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalRuntimeReadPage {
    /// 当前稳定游标页。
    pub items: Vec<ApprovalRuntimeReadRow>,
    /// 不含游标的完整授权范围总数。
    pub total: u64,
}

/// 审批运行实例与不可变业务快照的联合只读仓储。
pub struct ApprovalRuntimeReadRepository<'a> {
    db: &'a Database,
}

#[derive(Debug, Deserialize)]
struct ApprovalRuntimeReadFacet {
    #[serde(default)]
    items: Vec<ApprovalRuntimeReadRow>,
    #[serde(default)]
    total: Vec<ApprovalRuntimeReadCount>,
}

#[derive(Debug, Deserialize)]
struct ApprovalRuntimeReadCount {
    count: i64,
}

impl<'a> ApprovalRuntimeReadRepository<'a> {
    /// 创建审批运行联合只读仓储。
    ///
    /// # 参数
    /// * `db` - 当前 MongoDB 数据库
    ///
    /// # 返回
    /// 返回不自行开启事务的只读仓储。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 在 MongoDB 内完成快照授权范围、精确三元组、检索、计数与分页。
    ///
    /// # 参数
    /// * `filter` - 视图、状态、字面量检索、游标与页大小
    /// * `scope` - Service 已证明的流程种类、责任组织和可选提交人
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回当前稳定页与不含游标的完整授权总数。
    ///
    /// # 错误
    /// MongoDB 聚合、反序列化或计数越界时返回错误。
    ///
    /// # 关键业务约束
    /// 空类型、显式空组织和无发起人的 Started 范围必须在访问数据库前返回空页；
    /// Repository 不得解释 RBAC，且不得在分页后过滤授权事实。
    pub async fn search(
        &self,
        filter: &ApprovalInstanceListFilter,
        scope: &ApprovalRuntimeReadScope,
        executor: &mut dyn Executor,
    ) -> Result<ApprovalRuntimeReadPage> {
        if runtime_read_scope_empty(filter, scope) {
            return Ok(ApprovalRuntimeReadPage {
                items: Vec::new(),
                total: 0,
            });
        }
        let rows = aggregate_runtime_read(
            &self.db.collection::<Document>(INSTANCES),
            runtime_read_pipeline(filter, scope),
            executor,
        )
        .await?;
        runtime_read_page(rows.into_iter().next())
    }
}

async fn aggregate_runtime_read(
    collection: &Collection<Document>,
    pipeline: Vec<Document>,
    executor: &mut dyn Executor,
) -> Result<Vec<ApprovalRuntimeReadFacet>> {
    match executor.session() {
        Some(session) => Ok(collection
            .aggregate(pipeline)
            .with_type::<ApprovalRuntimeReadFacet>()
            .session(&mut *session)
            .await?
            .stream(session)
            .try_collect::<Vec<_>>()
            .await?),
        None => Ok(collection
            .aggregate(pipeline)
            .with_type::<ApprovalRuntimeReadFacet>()
            .await?
            .try_collect::<Vec<_>>()
            .await?),
    }
}

fn runtime_read_page(facet: Option<ApprovalRuntimeReadFacet>) -> Result<ApprovalRuntimeReadPage> {
    let Some(facet) = facet else {
        return Ok(ApprovalRuntimeReadPage {
            items: Vec::new(),
            total: 0,
        });
    };
    let total = facet.total.first().map_or(Ok(0), |row| {
        u64::try_from(row.count).map_err(|_| Error::EntityMetadataOutOfRange("approval_runtime_total"))
    })?;
    Ok(ApprovalRuntimeReadPage {
        items: facet.items,
        total,
    })
}

fn runtime_read_scope_empty(filter: &ApprovalInstanceListFilter, scope: &ApprovalRuntimeReadScope) -> bool {
    let process_kinds = runtime_read_process_kinds(scope);
    instance_list_scope_empty(filter)
        || process_kinds.is_empty()
        || filter
            .process_kind
            .is_some_and(|requested| !process_kinds.contains(&requested))
        || match scope {
            ApprovalRuntimeReadScope::Started { submitted_by, .. } => {
                filter.view != super::bpm::ApprovalInstanceListView::Started
                    || submitted_by.is_empty()
                    || filter.started_by.as_deref() != Some(submitted_by)
            }
            ApprovalRuntimeReadScope::Managed { .. } => {
                filter.view == super::bpm::ApprovalInstanceListView::Started
            }
        }
}

/// 构造实例列表与唯一快照的授权聚合。
///
/// 游标只进入 `items` facet；`total` 在同一授权、检索与状态条件上独立计数。
fn runtime_read_pipeline(
    filter: &ApprovalInstanceListFilter,
    scope: &ApprovalRuntimeReadScope,
) -> Vec<Document> {
    let mut base_filter = filter.clone();
    base_filter.cursor = None;
    base_filter.text_query = None;
    let mut instance_match = instance_list_filter_doc(&base_filter);
    if base_filter.process_kind.is_none() {
        instance_match.insert(
            "process_kind",
            doc! {
                "$in": runtime_read_process_kinds(scope)
                    .into_iter()
                    .map(ProcessKind::as_str)
                    .collect::<Vec<_>>()
            },
        );
    }
    instance_match.insert(
        "$expr",
        doc! { "$eq": ["$process_kind", "$subject.subject_kind"] },
    );
    let mut pipeline = vec![
        doc! { "$match": instance_match },
        doc! { "$sort": instance_list_sort(filter) },
        doc! {
            "$lookup": {
                "from": SNAPSHOTS,
                "localField": "id",
                "foreignField": "approval_process_instance_id",
                "as": "_runtime_snapshots",
            }
        },
        doc! {
            "$set": {
                "_runtime_live_snapshots": {
                    "$filter": {
                        "input": "$_runtime_snapshots",
                        "as": "snapshot",
                        "cond": { "$eq": ["$$snapshot.deleted_at", NOT_DELETED_TIMESTAMP_BSON] },
                    }
                }
            }
        },
        doc! {
            "$set": {
                "_runtime_snapshot": { "$arrayElemAt": ["$_runtime_live_snapshots", 0] }
            }
        },
        doc! { "$set": { "_runtime_snapshot_exact": runtime_snapshot_exact_expr() } },
    ];
    if let Some(scope_match) = runtime_snapshot_scope_match(scope) {
        pipeline.push(doc! { "$match": scope_match });
    }
    if let Some(text_query) = &filter.text_query {
        pipeline.push(doc! { "$match": runtime_text_match(&text_query.query) });
    }
    pipeline.push(doc! { "$facet": runtime_read_facets(filter) });
    pipeline
}

fn runtime_snapshot_exact_expr() -> Document {
    doc! {
        "$and": [
            { "$eq": [{ "$size": "$_runtime_live_snapshots" }, 1] },
            { "$eq": ["$_runtime_snapshot.approval_process_instance_id", "$id"] },
            { "$eq": ["$_runtime_snapshot.document_type", "$process_kind"] },
            { "$eq": ["$_runtime_snapshot.document_type", "$subject.subject_kind"] },
            { "$eq": ["$_runtime_snapshot.business_object_id", "$subject.subject_id"] },
            { "$eq": ["$_runtime_snapshot.subject_version", "$subject_version"] },
        ]
    }
}

/// 组织范围不是公司级时，必须由精确快照证明责任组织。
///
/// 缺失或漂移快照不得被用于组织授权；本人发起或公司级范围无需从快照推断
/// 组织，因而可保留实例并把快照投影为空。
fn runtime_snapshot_scope_match(scope: &ApprovalRuntimeReadScope) -> Option<Document> {
    let ApprovalRuntimeReadScope::Managed { type_scopes } = scope else {
        return None;
    };
    let branches = type_scopes
        .iter()
        .filter(|type_scope| !type_scope.organization_ids.as_ref().is_some_and(Vec::is_empty))
        .map(|type_scope| match &type_scope.organization_ids {
            None => doc! { "process_kind": type_scope.process_kind.as_str() },
            Some(organization_ids) => doc! {
                "process_kind": type_scope.process_kind.as_str(),
                "$expr": {
                    "$and": [
                        "$_runtime_snapshot_exact",
                        {
                            "$in": [
                                "$_runtime_snapshot.payload.responsible_org_id",
                                organization_ids.clone(),
                            ]
                        },
                    ]
                },
            },
        })
        .collect::<Vec<_>>();
    (!branches.is_empty()).then(|| doc! { "$or": branches })
}

fn runtime_read_process_kinds(scope: &ApprovalRuntimeReadScope) -> Vec<ProcessKind> {
    let mut process_kinds = match scope {
        ApprovalRuntimeReadScope::Started { process_kinds, .. } => process_kinds.clone(),
        ApprovalRuntimeReadScope::Managed { type_scopes } => type_scopes
            .iter()
            .filter(|type_scope| !type_scope.organization_ids.as_ref().is_some_and(Vec::is_empty))
            .map(|type_scope| type_scope.process_kind)
            .collect::<Vec<_>>(),
    };
    process_kinds.sort_by_key(|process_kind| process_kind.as_str());
    process_kinds.dedup();
    process_kinds
}

fn runtime_text_match(query: &str) -> Document {
    let literal = regex::escape(query.trim());
    let regex = doc! { "$regex": literal, "$options": "i" };
    doc! {
        "$or": [
            { "subject.subject_id": regex.clone() },
            { "current_assignee_name": regex.clone() },
            { "current_node_name": regex.clone() },
            {
                "$and": [
                    { "_runtime_snapshot_exact": true },
                    { "_runtime_snapshot.payload.document_no": regex },
                ]
            },
        ]
    }
}

fn runtime_read_facets(filter: &ApprovalInstanceListFilter) -> Document {
    let mut items = Vec::new();
    if let Some(cursor) = &filter.cursor {
        items.push(doc! { "$match": { "$or": instance_cursor_or(filter.view, cursor) } });
    }
    items.push(doc! { "$limit": instance_list_limit(filter.limit) });
    let mut projection = instance_summary_projection();
    projection.insert(
        "snapshot",
        doc! {
            "$cond": ["$_runtime_snapshot_exact", "$_runtime_snapshot", Bson::Null]
        },
    );
    projection.insert("_id", 0);
    items.push(doc! { "$project": projection });
    doc! {
        "items": items,
        "total": [{ "$count": "count" }],
    }
}

/// 写入与实例一一对应的不可变业务对象快照。
impl<'a> Repository<'a, ApprovalSubjectSnapshot> {
    /// 插入启动时冻结的业务对象快照；写后不得再更新。
    ///
    /// # 错误
    /// 同一实例已有快照或 MongoDB 写入失败时返回错误。
    pub async fn create_immutable_snapshot(
        &self,
        snapshot: &ApprovalSubjectSnapshot,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(&self.collection(), snapshot, executor).await
    }

    /// 按审批实例读取唯一快照。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn find_by_process_instance_id(
        &self,
        approval_process_instance_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalSubjectSnapshot>> {
        self.find_one(
            snapshot_by_process_instance_filter(approval_process_instance_id),
            executor,
        )
        .await
    }

    /// 按审批实例批量读取不可变业务对象快照。
    ///
    /// # 参数
    /// * `approval_process_instance_ids` - 当前列表页内的审批实例 ID
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回全部匹配且未删除的快照；调用方按实例 ID 关联。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn find_by_process_instance_ids(
        &self,
        approval_process_instance_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ApprovalSubjectSnapshot>> {
        if approval_process_instance_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(
            snapshot_by_process_instances_filter(approval_process_instance_ids),
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, ApprovalNotificationOutbox> {
    /// 追加一条通知 outbox 记录。
    ///
    /// # 错误
    /// 去重键冲突或 MongoDB 写入失败时返回错误。
    pub async fn enqueue_outbox(
        &self,
        item: &ApprovalNotificationOutbox,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(&self.collection(), item, executor).await
    }

    /// 以原子条件更新领取一批可投递消息；两个 worker 不得同时取得同一条。
    ///
    /// # 错误
    /// 元数据越界或 MongoDB 更新失败时返回错误。
    pub async fn lease_outbox_batch(
        &self,
        worker_id: &str,
        now: Instant,
        lease_until: Instant,
        limit: u32,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ApprovalNotificationOutbox>> {
        let mut leased = Vec::new();
        let take = clamp_outbox_limit(limit);
        for _ in 0..take {
            let Some(item) =
                lease_one_outbox(&self.collection(), worker_id, now, lease_until, executor).await?
            else {
                break;
            };
            leased.push(item);
        }
        Ok(leased)
    }

    /// 以当前租约持有者为条件标记投递成功。
    ///
    /// # 错误
    /// MongoDB 更新或反序列化失败时返回错误。
    pub async fn mark_outbox_delivered(
        &self,
        outbox_id: &str,
        expected_lease_owner: &str,
        delivered_at: Instant,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalNotificationOutbox>> {
        find_one_and_update_pipeline(
            &self.collection(),
            lease_owner_filter(outbox_id, expected_lease_owner),
            mark_outbox_delivered_pipeline(delivered_at),
            executor,
        )
        .await
    }

    /// 以当前租约持有者为条件重排下次尝试。
    ///
    /// # 错误
    /// MongoDB 更新或反序列化失败时返回错误。
    pub async fn reschedule_outbox(
        &self,
        outbox_id: &str,
        expected_lease_owner: &str,
        next_attempt_at: Instant,
        error_kind: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalNotificationOutbox>> {
        find_one_and_update_pipeline(
            &self.collection(),
            lease_owner_filter(outbox_id, expected_lease_owner),
            reschedule_outbox_pipeline(next_attempt_at, error_kind),
            executor,
        )
        .await
    }

    /// 以当前租约持有者为条件将消息转入死信。
    ///
    /// # 错误
    /// MongoDB 更新或反序列化失败时返回错误。
    pub async fn dead_letter_outbox(
        &self,
        outbox_id: &str,
        expected_lease_owner: &str,
        error_kind: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalNotificationOutbox>> {
        find_one_and_update_pipeline(
            &self.collection(),
            lease_owner_filter(outbox_id, expected_lease_owner),
            dead_letter_outbox_pipeline(error_kind),
            executor,
        )
        .await
    }
}

/// 原子领取一条到期或租约过期的 outbox 消息。
async fn lease_one_outbox(
    collection: &Collection<ApprovalNotificationOutbox>,
    worker_id: &str,
    now: Instant,
    lease_until: Instant,
    executor: &mut dyn Executor,
) -> Result<Option<ApprovalNotificationOutbox>> {
    find_one_and_update_sorted(
        collection,
        outbox_lease_filter(now),
        lease_take_pipeline(worker_id, now, lease_until),
        lease_take_sort(),
        executor,
    )
    .await
}

/// 构造竞争取租约成功时的 `$set` 管道。
///
/// 将消息标为 `IN_FLIGHT`，写入 `lease_owner`/`lease_until`，并递增版本。
///
/// # 参数
/// * `worker_id` - 取得租约的 worker
/// * `now` - 本次领取时间，写入 `updated_at`
/// * `lease_until` - 租约到期时间
///
/// # 返回
/// 返回单步 `$set` 管道。
///
/// # 错误
/// 无。
fn lease_take_pipeline(worker_id: &str, now: Instant, lease_until: Instant) -> Vec<Document> {
    vec![doc! {
        "$set": {
            "delivery_status": ApprovalNotificationDeliveryStatus::InFlight.as_str(),
            "lease_owner": worker_id,
            "lease_until": lease_until.unix_secs(),
            "version": { "$add": ["$version", 1_i64] },
            "updated_at": now.unix_secs(),
        }
    }]
}

/// 返回取租约扫描顺序：先到期者优先，同时间按 `id` 升序。
///
/// # 返回
/// 返回 `{ next_attempt_at: 1, id: 1 }`。
///
/// # 错误
/// 无。
fn lease_take_sort() -> Document {
    doc! { "next_attempt_at": 1, "id": 1 }
}

fn snapshot_by_process_instance_filter(approval_process_instance_id: &str) -> Document {
    doc! { "approval_process_instance_id": approval_process_instance_id }
}

fn snapshot_by_process_instances_filter(approval_process_instance_ids: &[String]) -> Document {
    doc! {
        "approval_process_instance_id": {
            "$in": approval_process_instance_ids.to_vec(),
        }
    }
}

fn outbox_lease_filter(now: Instant) -> Document {
    let now = now.unix_secs();
    doc! {
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        "$or": [
            {
                "delivery_status": ApprovalNotificationDeliveryStatus::Pending.as_str(),
                "next_attempt_at": { "$lte": now },
            },
            {
                "delivery_status": ApprovalNotificationDeliveryStatus::InFlight.as_str(),
                "lease_until": { "$lte": now },
            },
        ],
    }
}

fn lease_owner_filter(outbox_id: &str, expected_lease_owner: &str) -> Document {
    doc! {
        "id": outbox_id,
        "lease_owner": expected_lease_owner,
        "delivery_status": ApprovalNotificationDeliveryStatus::InFlight.as_str(),
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    }
}

/// 构造投递成功的 `$set` 管道：写 `Delivered` 并清空租约。
///
/// # 参数
/// * `delivered_at` - 投递完成时间
///
/// # 返回
/// 返回单步 `$set` 管道。
fn mark_outbox_delivered_pipeline(delivered_at: Instant) -> Vec<Document> {
    vec![doc! {
        "$set": {
            "delivery_status": ApprovalNotificationDeliveryStatus::Delivered.as_str(),
            "lease_owner": Bson::Null,
            "lease_until": Bson::Null,
            "delivered_at": delivered_at.unix_secs(),
            "version": { "$add": ["$version", 1_i64] },
            "updated_at": delivered_at.unix_secs(),
        }
    }]
}

/// 构造重试回 `Pending` 的 `$set` 管道，并递增 `attempt_count`。
///
/// # 参数
/// * `next_attempt_at` - 下次尝试时间
/// * `error_kind` - 本次失败分类
///
/// # 返回
/// 返回单步 `$set` 管道。
fn reschedule_outbox_pipeline(next_attempt_at: Instant, error_kind: &str) -> Vec<Document> {
    vec![doc! {
        "$set": {
            "delivery_status": ApprovalNotificationDeliveryStatus::Pending.as_str(),
            "next_attempt_at": next_attempt_at.unix_secs(),
            "last_error_class": error_kind,
            "lease_owner": Bson::Null,
            "lease_until": Bson::Null,
            "attempt_count": { "$add": ["$attempt_count", 1_i64] },
            "version": { "$add": ["$version", 1_i64] },
            "updated_at": next_attempt_at.unix_secs(),
        }
    }]
}

/// 构造转入死信的 `$set` 管道。
///
/// # 参数
/// * `error_kind` - 死信失败分类
///
/// # 返回
/// 返回单步 `$set` 管道；`dead_lettered_at` 取当前租约到期或下次尝试时间。
fn dead_letter_outbox_pipeline(error_kind: &str) -> Vec<Document> {
    vec![doc! {
        "$set": {
            "delivery_status": ApprovalNotificationDeliveryStatus::DeadLetter.as_str(),
            "last_error_class": error_kind,
            "lease_owner": Bson::Null,
            "lease_until": Bson::Null,
            "dead_lettered_at": { "$ifNull": ["$lease_until", "$next_attempt_at"] },
            "version": { "$add": ["$version", 1_i64] },
        }
    }]
}

fn clamp_outbox_limit(limit: u32) -> i64 {
    if limit == 0 {
        return 1;
    }
    i64::from(limit).min(MAX_OUTBOX_BATCH)
}

/// 带排序的单文档更新管道，供竞争取租约使用。
async fn find_one_and_update_sorted<T>(
    collection: &Collection<T>,
    filter: Document,
    pipeline: Vec<Document>,
    sort: Document,
    executor: &mut dyn Executor,
) -> Result<Option<T>>
where
    T: serde::de::DeserializeOwned + Send + Sync,
{
    let operation = collection
        .find_one_and_update(filter, pipeline)
        .sort(sort)
        .return_document(ReturnDocument::After);
    let document = match executor.session() {
        Some(session) => operation.session(session).await?,
        None => operation.await?,
    };
    Ok(document)
}

async fn find_one_and_update_pipeline<T>(
    collection: &Collection<T>,
    filter: Document,
    pipeline: Vec<Document>,
    executor: &mut dyn Executor,
) -> Result<Option<T>>
where
    T: serde::de::DeserializeOwned + Send + Sync,
{
    mongo_ops::find_one_and_update_pipeline(collection, filter, pipeline, executor).await
}

#[cfg(test)]
mod tests {
    use super::{
        clamp_outbox_limit, dead_letter_outbox_pipeline, lease_owner_filter, lease_take_pipeline,
        lease_take_sort, mark_outbox_delivered_pipeline, outbox_lease_filter, reschedule_outbox_pipeline,
        runtime_read_pipeline, runtime_read_scope_empty, runtime_snapshot_scope_match,
        snapshot_by_process_instances_filter, ApprovalRuntimeReadScope, ApprovalRuntimeReadTypeScope,
        MAX_OUTBOX_BATCH,
    };
    use crate::repository::bpm::{
        ApprovalInstanceListCursor, ApprovalInstanceListFilter, ApprovalInstanceListView,
        ApprovalInstanceTextQuery,
    };
    use bpm::model::types::ApprovalProcessInstanceStatus;
    use bpm::ProcessKind;
    use entities::approval_integration::ApprovalNotificationDeliveryStatus;
    use entities::common::time::Instant;

    use mongodb::bson::{doc, Bson};

    fn runtime_filter() -> ApprovalInstanceListFilter {
        ApprovalInstanceListFilter {
            view: ApprovalInstanceListView::Blocked,
            process_kind: None,
            status: Some(ApprovalProcessInstanceStatus::Blocked),
            started_by: None,
            subject_kind: None,
            subject_ids: None,
            text_query: Some(ApprovalInstanceTextQuery {
                query: "ADJ.[1]".to_string(),
            }),
            cursor: Some(ApprovalInstanceListCursor {
                sort_time: 20,
                id: "inst-2".to_string(),
            }),
            limit: 21,
        }
    }

    fn runtime_type_scopes() -> Vec<ApprovalRuntimeReadTypeScope> {
        vec![
            ApprovalRuntimeReadTypeScope {
                process_kind: ProcessKind::StockAdjustment,
                organization_ids: Some(vec!["org-1".to_string()]),
            },
            ApprovalRuntimeReadTypeScope {
                process_kind: ProcessKind::SalesOrder,
                organization_ids: Some(vec!["org-1".to_string()]),
            },
        ]
    }

    fn runtime_scope() -> ApprovalRuntimeReadScope {
        ApprovalRuntimeReadScope::Managed {
            type_scopes: runtime_type_scopes(),
        }
    }

    #[test]
    fn snapshot_batch_filter_uses_only_requested_instances() {
        let filter = snapshot_by_process_instances_filter(&["inst-1".to_string(), "inst-2".to_string()]);
        assert_eq!(
            filter,
            doc! {
                "approval_process_instance_id": {
                    "$in": ["inst-1", "inst-2"],
                }
            }
        );
    }

    #[test]
    fn runtime_pipeline_filters_scope_and_exact_snapshot_before_facet() {
        let pipeline = runtime_read_pipeline(&runtime_filter(), &runtime_scope());
        let instance_match = pipeline[0].get_document("$match").unwrap();
        assert_eq!(
            instance_match.get_document("process_kind").unwrap(),
            &doc! { "$in": ["sales_order", "stock_adjustment"] }
        );
        assert_eq!(
            instance_match.get_document("$expr").unwrap(),
            &doc! { "$eq": ["$process_kind", "$subject.subject_kind"] }
        );
        assert!(!instance_match.contains_key("$or"));

        assert_eq!(
            pipeline[1].get_document("$sort").unwrap(),
            &doc! { "blocked_at": -1, "id": -1 }
        );
        let lookup = pipeline[2].get_document("$lookup").unwrap();
        assert_eq!(lookup.get_str("localField").unwrap(), "id");
        assert_eq!(
            lookup.get_str("foreignField").unwrap(),
            "approval_process_instance_id"
        );
        let exact = pipeline[5]
            .get_document("$set")
            .unwrap()
            .get_document("_runtime_snapshot_exact")
            .unwrap()
            .to_string();
        for field in [
            "approval_process_instance_id",
            "document_type",
            "business_object_id",
            "subject_version",
        ] {
            assert!(exact.contains(field));
        }
        let scope_match = pipeline[6].get_document("$match").unwrap().to_string();
        assert!(scope_match.contains("_runtime_snapshot_exact"));
        assert!(scope_match.contains("responsible_org_id"));
        assert!(scope_match.contains("org-1"));

        let text = pipeline[7].get_document("$match").unwrap().to_string();
        assert!(text.contains("_runtime_snapshot.payload.document_no"));
        assert!(text.contains("_runtime_snapshot_exact"));
        assert!(text.contains(r"ADJ\.\[1\]"));
        let facet = pipeline[8].get_document("$facet").unwrap();
        let items = facet.get_array("items").unwrap();
        assert!(items[0].as_document().unwrap().contains_key("$match"));
        assert!(items.iter().any(|stage| {
            stage
                .as_document()
                .is_some_and(|document| document.contains_key("$limit"))
        }));
        assert_eq!(
            facet.get_array("total").unwrap(),
            &vec![Bson::Document(doc! { "$count": "count" })]
        );
        assert!(!facet.get_array("total").unwrap()[0]
            .as_document()
            .unwrap()
            .contains_key("$match"));
    }

    #[test]
    fn runtime_company_scope_keeps_missing_snapshot_before_facet() {
        let scope = ApprovalRuntimeReadScope::Managed {
            type_scopes: runtime_type_scopes()
                .into_iter()
                .map(|type_scope| ApprovalRuntimeReadTypeScope {
                    organization_ids: None,
                    ..type_scope
                })
                .collect(),
        };
        let pipeline = runtime_read_pipeline(&runtime_filter(), &scope);
        assert!(pipeline.iter().all(|stage| {
            stage
                .get_document("$match")
                .map_or(true, |filter| !filter.to_string().contains("responsible_org_id"))
        }));
        let projection = pipeline
            .last()
            .unwrap()
            .get_document("$facet")
            .unwrap()
            .get_array("items")
            .unwrap()
            .last()
            .unwrap()
            .as_document()
            .unwrap()
            .get_document("$project")
            .unwrap();
        assert!(projection.get_document("snapshot").unwrap().contains_key("$cond"));
    }

    #[test]
    fn runtime_scope_rejects_empty_and_requested_type_outside_proven_types() {
        let filter = runtime_filter();
        let empty = ApprovalRuntimeReadScope::Managed {
            type_scopes: Vec::new(),
        };
        assert!(runtime_read_scope_empty(&filter, &empty));
        let empty_org = ApprovalRuntimeReadScope::Managed {
            type_scopes: runtime_type_scopes()
                .into_iter()
                .map(|type_scope| ApprovalRuntimeReadTypeScope {
                    organization_ids: Some(Vec::new()),
                    ..type_scope
                })
                .collect(),
        };
        assert!(runtime_read_scope_empty(&filter, &empty_org));

        let requested = ApprovalInstanceListFilter {
            process_kind: Some(ProcessKind::PurchaseOrder),
            ..filter
        };
        assert!(runtime_read_scope_empty(&requested, &runtime_scope()));
    }

    #[test]
    fn started_scope_requires_matching_view_and_actor_without_snapshot_scope() {
        let mut filter = runtime_filter();
        filter.view = ApprovalInstanceListView::Started;
        filter.started_by = Some("starter".to_string());
        let matching = ApprovalRuntimeReadScope::Started {
            process_kinds: vec![ProcessKind::StockAdjustment],
            submitted_by: "starter".to_string(),
        };
        assert!(!runtime_read_scope_empty(&filter, &matching));
        assert!(runtime_snapshot_scope_match(&matching).is_none());
        assert!(runtime_read_scope_empty(
            &filter,
            &ApprovalRuntimeReadScope::Started {
                process_kinds: vec![ProcessKind::StockAdjustment],
                submitted_by: "other".to_string(),
            }
        ));
        assert!(runtime_read_scope_empty(&runtime_filter(), &matching));
    }

    #[test]
    fn lease_filter_allows_pending_due_and_expired_inflight() {
        let filter = outbox_lease_filter(Instant::from_unix_secs(1_000));
        let alternatives = filter.get_array("$or").unwrap();
        assert_eq!(alternatives.len(), 2);
        let pending = alternatives[0].as_document().unwrap();
        assert_eq!(
            pending.get_str("delivery_status").unwrap(),
            ApprovalNotificationDeliveryStatus::Pending.as_str()
        );
        assert_eq!(
            pending.get_document("next_attempt_at").unwrap(),
            &doc! { "$lte": 1_000_i64 }
        );
        let inflight = alternatives[1].as_document().unwrap();
        assert_eq!(
            inflight.get_str("delivery_status").unwrap(),
            ApprovalNotificationDeliveryStatus::InFlight.as_str()
        );
        assert_eq!(
            inflight.get_document("lease_until").unwrap(),
            &doc! { "$lte": 1_000_i64 }
        );
        let serialized = filter.to_string();
        assert!(!serialized.contains("DELIVERED"));
        assert!(!serialized.contains("DEAD_LETTER"));
    }

    #[test]
    fn clamp_outbox_limit_and_lease_owner_reject_non_inflight() {
        assert_eq!(clamp_outbox_limit(0), 1);
        assert_eq!(clamp_outbox_limit(1), 1);
        assert_eq!(clamp_outbox_limit(50), MAX_OUTBOX_BATCH);
        assert_eq!(clamp_outbox_limit(51), MAX_OUTBOX_BATCH);
        assert_eq!(clamp_outbox_limit(u32::MAX), MAX_OUTBOX_BATCH);

        let filter = lease_owner_filter("ob-1", "worker-a");
        assert_eq!(filter.get_str("id").unwrap(), "ob-1");
        assert_eq!(filter.get_str("lease_owner").unwrap(), "worker-a");
        assert_eq!(
            filter.get_str("delivery_status").unwrap(),
            ApprovalNotificationDeliveryStatus::InFlight.as_str()
        );
        let serialized = filter.to_string();
        assert!(!serialized.contains("PENDING"));
        assert!(!serialized.contains("DELIVERED"));
        assert!(!serialized.contains("DEAD_LETTER"));
    }

    #[test]
    fn lease_take_pipeline_sets_inflight_owner_and_sorts_by_due_id() {
        let now = Instant::from_unix_secs(1_000);
        let lease_until = Instant::from_unix_secs(1_500);
        let pipeline = lease_take_pipeline("worker-a", now, lease_until);
        assert_eq!(pipeline.len(), 1);
        let set = pipeline[0].get_document("$set").unwrap();
        assert_eq!(
            set.get_str("delivery_status").unwrap(),
            ApprovalNotificationDeliveryStatus::InFlight.as_str()
        );
        assert_eq!(set.get_str("lease_owner").unwrap(), "worker-a");
        assert_eq!(set.get_i64("lease_until").unwrap(), 1_500);
        assert_eq!(set.get_i64("updated_at").unwrap(), 1_000);
        assert_eq!(
            set.get_document("version").unwrap(),
            &doc! { "$add": ["$version", 1_i64] }
        );
        assert_eq!(lease_take_sort(), doc! { "next_attempt_at": 1, "id": 1 });
    }

    #[test]
    fn delivery_updates_require_current_lease_owner() {
        let filter = lease_owner_filter("ob-1", "worker-a");
        assert_eq!(filter.get_str("id").unwrap(), "ob-1");
        assert_eq!(filter.get_str("lease_owner").unwrap(), "worker-a");
        assert_eq!(
            filter.get_str("delivery_status").unwrap(),
            ApprovalNotificationDeliveryStatus::InFlight.as_str()
        );
    }

    #[test]
    fn outbox_success_pipelines_clear_lease_and_contrast_lease_filter() {
        let delivered_at = Instant::from_unix_secs(2_000);
        let delivered = mark_outbox_delivered_pipeline(delivered_at);
        assert_eq!(delivered.len(), 1);
        let delivered_set = delivered[0].get_document("$set").unwrap();
        assert_eq!(
            delivered_set.get_str("delivery_status").unwrap(),
            ApprovalNotificationDeliveryStatus::Delivered.as_str()
        );
        assert_eq!(delivered_set.get("lease_owner").unwrap(), &Bson::Null);
        assert_eq!(delivered_set.get("lease_until").unwrap(), &Bson::Null);
        assert_eq!(delivered_set.get_i64("delivered_at").unwrap(), 2_000);
        assert_eq!(
            delivered_set.get_document("version").unwrap(),
            &doc! { "$add": ["$version", 1_i64] }
        );
        assert_eq!(delivered_set.get_i64("updated_at").unwrap(), 2_000);

        let next_attempt_at = Instant::from_unix_secs(3_000);
        let rescheduled = reschedule_outbox_pipeline(next_attempt_at, "timeout");
        assert_eq!(rescheduled.len(), 1);
        let rescheduled_set = rescheduled[0].get_document("$set").unwrap();
        assert_eq!(
            rescheduled_set.get_str("delivery_status").unwrap(),
            ApprovalNotificationDeliveryStatus::Pending.as_str()
        );
        assert_eq!(rescheduled_set.get_i64("next_attempt_at").unwrap(), 3_000);
        assert_eq!(rescheduled_set.get_str("last_error_class").unwrap(), "timeout");
        assert_eq!(rescheduled_set.get("lease_owner").unwrap(), &Bson::Null);
        assert_eq!(rescheduled_set.get("lease_until").unwrap(), &Bson::Null);
        assert_eq!(
            rescheduled_set.get_document("attempt_count").unwrap(),
            &doc! { "$add": ["$attempt_count", 1_i64] }
        );

        let dead_lettered = dead_letter_outbox_pipeline("exhausted");
        assert_eq!(dead_lettered.len(), 1);
        let dead_set = dead_lettered[0].get_document("$set").unwrap();
        assert_eq!(
            dead_set.get_str("delivery_status").unwrap(),
            ApprovalNotificationDeliveryStatus::DeadLetter.as_str()
        );
        assert_eq!(dead_set.get_str("last_error_class").unwrap(), "exhausted");
        assert_eq!(dead_set.get("lease_owner").unwrap(), &Bson::Null);
        assert_eq!(dead_set.get("lease_until").unwrap(), &Bson::Null);
        assert_eq!(
            dead_set.get_document("dead_lettered_at").unwrap(),
            &doc! { "$ifNull": ["$lease_until", "$next_attempt_at"] }
        );
        assert!(!dead_set.contains_key("delivered_at"));

        let lease_filter = outbox_lease_filter(Instant::from_unix_secs(1_000)).to_string();
        assert!(!lease_filter.contains("DELIVERED"));
        assert!(!lease_filter.contains("DEAD_LETTER"));
        assert!(lease_filter.contains("PENDING"));
        assert!(lease_filter.contains("IN_FLIGHT"));
    }
}
