//! ERP 审批集成仓储：业务对象快照与通知 outbox。

use entities::approval_integration::{
    ApprovalNotificationDeliveryStatus, ApprovalNotificationOutbox, ApprovalSubjectSnapshot,
};
use entities::common::time::Instant;
use entities::document_registry::DocumentType;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Bson, Document};
use mongodb::options::ReturnDocument;
use mongodb::Collection;

use super::Repository;
use crate::executor::Executor;
use crate::{mongo_ops, Result};

const MAX_OUTBOX_BATCH: i64 = 50;

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

    /// 按业务对象身份查询快照。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn find_by_subject(
        &self,
        document_type: DocumentType,
        business_object_id: &str,
        subject_version: u32,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalSubjectSnapshot>> {
        self.find_one(
            snapshot_by_subject_filter(document_type, business_object_id, subject_version),
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

fn snapshot_by_subject_filter(
    document_type: DocumentType,
    business_object_id: &str,
    subject_version: u32,
) -> Document {
    doc! {
        "document_type": document_type.as_str(),
        "business_object_id": business_object_id,
        "subject_version": i64::from(subject_version),
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
        snapshot_by_process_instance_filter, snapshot_by_subject_filter, MAX_OUTBOX_BATCH,
    };
    use entities::approval_integration::ApprovalNotificationDeliveryStatus;
    use entities::common::time::Instant;
    use entities::document_registry::DocumentType;
    use mongodb::bson::{doc, Bson};

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

    #[test]
    fn snapshot_queries_are_instance_unique_and_writes_are_insert_only() {
        assert_eq!(
            <mongodb::Database as crate::repository::extensions::ApprovalIntegrationExt>::APPROVAL_SUBJECT_SNAPSHOTS,
            "approval_subject_snapshots"
        );
        assert_eq!(
            snapshot_by_process_instance_filter("inst-1"),
            doc! { "approval_process_instance_id": "inst-1" }
        );
        assert_eq!(
            snapshot_by_subject_filter(DocumentType::StockAdjustment, "adj-1", 2),
            doc! {
                "document_type": DocumentType::StockAdjustment.as_str(),
                "business_object_id": "adj-1",
                "subject_version": 2_i64,
            }
        );
        // 快照仅 insert_one，本模块无 update/replace API。
        // 收据同键异载荷比较由 bpm::ApprovalCommandReceipt::reconcile 承担，
        // 真实冲突/回读验收留给 P6。
    }
}
