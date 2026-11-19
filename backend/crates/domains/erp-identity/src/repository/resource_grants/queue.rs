//! 授权事件领取、租约隔离和事务结果保存。
use mongodb::Database;
use mongodb::bson::{Document, doc};
use mongodb::options::{FindOptions, ReturnDocument};
use persistence_core::{Executor, mongo_ops};
use serde::{Deserialize, Serialize};

use crate::entity::resource_access::event::{GrantEvent, GrantEventStatus};
use crate::entity::resource_access::{GrantSource, ResourceKind, SourceKey};
use crate::repository::resource_grants;
use crate::{Error, Result};

/// 授权事件消费者的 MongoDB 队列仓储，管理领取及发布回写，不判断记录读取资格。
#[derive(Clone)]
pub struct GrantEventRepository {
    // 存储正式事件、来源和队列统计所用的身份数据库。
    db: Database,
}

impl GrantEventRepository {
    /// 绑定身份数据库，供消费者领取或回写事件；不启动后台循环。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 按可领取时间及 ID 原子领取至多一个到期事件，或接手过期租约。每次领取使用唯一非空令牌，`now` 为 Unix 秒；时间溢出或数据库错误时拒绝。
    pub async fn claim_event(&self, owner: &str, now: u64) -> Result<Option<GrantEvent>> {
        if owner.is_empty() {
            return Err(Error::ValidationError("领取令牌不能为空".into()));
        }

        let now = i64::try_from(now).map_err(|_| Error::ValidationError("时间溢出".into()))?;
        let filter = doc! { "deleted_at": 0, "$or": [
            { "status": { "$in": ["pending", "failed"] }, "available_at": { "$lte": now } },
            { "status": "processing", "lease_until": { "$lte": now } }
        ] };

        Ok(self
            .db
            .collection::<GrantEvent>("resource_grant_events")
            .find_one_and_update(
                filter,
                doc! { "$set": { "status": "processing", "lease_owner": owner,
                "lease_until": now.saturating_add(300) }, "$inc": { "attempts": 1, "version": 1 } },
            )
            .sort(doc! { "available_at": 1, "id": 1 })
            .return_document(ReturnDocument::After)
            .await?)
    }
    /// 发布前在原写事务重新读取事件并核对领取令牌；已完成事件允许幂等返回，缺失或租约已被接管时拒绝。
    pub async fn claimed_event(&self, claim: &GrantEvent, executor: &mut dyn Executor) -> Result<GrantEvent> {
        let current = resource_grants::events(&self.db)
            .find_by_id(&claim.base.id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("授权事件不存在".into()))?;
        current.ensure_claim(claim)?;
        Ok(current)
    }
    /// 在授权结果的同一快照事务内标记事件完成并清除租约和失败信息。已完成重放不写；无事务、版本冲突或写入失败时拒绝。
    pub async fn complete_event(&self, mut event: GrantEvent, executor: &mut dyn Executor) -> Result<()> {
        if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
            return Err(Error::Internal("事件完成必须使用事务".into()));
        }
        if event.status == GrantEventStatus::Applied {
            return Ok(());
        }

        event.status = GrantEventStatus::Applied;
        event.lease_owner = None;
        event.lease_until = None;
        event.failure = None;
        resource_grants::events(&self.db).update(&mut event, executor).await?;
        Ok(())
    }
    /// 发布失败时只回写仍由原令牌持有的事件；已完成或被接管则不更新。数据库错误原样返回，租约过期后可重新领取。
    pub async fn fail_event(&self, event: &GrantEvent, now: u64, reason: &str) -> Result<()> {
        let delay = event.retry_delay();
        let next = i64::try_from(now.saturating_add(delay))
            .map_err(|_| Error::ValidationError("时间溢出".into()))?;

        self.db
            .collection::<Document>("resource_grant_events")
            .update_one(
                doc! { "id": &event.base.id, "status": "processing", "lease_owner": &event.lease_owner },
                doc! { "$set": { "status": "failed", "available_at": next, "failure": reason,
                "lease_owner": null, "lease_until": null }, "$inc": { "version": 1 } },
            )
            .await?;
        Ok(())
    }
    /// 在发布事务按可信来源键读取唯一来源状态，用于拒绝永久终止的来源及旧事件。无来源返回 `None`，身份非法或读取失败时拒绝。
    pub async fn source(&self, key: &SourceKey, executor: &mut dyn Executor) -> Result<Option<GrantSource>> {
        Ok(resource_grants::sources(&self.db).find_by_id(&key.id()?, executor).await?)
    }
}

/// 授权队列运行指标，不含账号身份或业务证据。
#[derive(Debug)]
pub struct GrantQueueHealth {
    /// 未完成事件数，包含待处理、处理中及失败。
    pub backlog: u64,
    /// 最老未完成事件距发生时的秒数；没有积压时为 0。
    pub oldest_age_secs: u64,
    /// 当前尚处于失败状态的未删除事件数。
    pub failures: u64,
    /// 最近更新的选定自动授权来源所记录的业务事实版本；没有来源时为空。
    pub latest_fact_version: Option<u64>,
}
impl GrantEventRepository {
    /// 按当前 Unix 秒计算未完成事件积压、最老积压时长和失败数，并读取最近来源的事实版本；任一查询失败直接返回错误。
    pub async fn grant_health(&self, now: u64) -> Result<GrantQueueHealth> {
        let collection = self.db.collection::<GrantEvent>("resource_grant_events");
        let filter = doc! { "status": { "$in": ["pending", "processing", "failed"] }, "deleted_at": 0 };
        let backlog = collection.count_documents(filter.clone()).await?;
        let oldest = collection.find_one(filter).sort(doc! { "evidence.occurred_at": 1 }).await?;
        let failures = collection.count_documents(doc! { "status": "failed", "deleted_at": 0 }).await?;

        let latest = self
            .db
            .collection::<GrantSource>("resource_grant_sources")
            .find_one(doc! { "key.rule": { "$in": ["invoice_approved", "sales_order_approval_assigned", "purchase_order_approval_assigned"] } })
            .sort(doc! { "updated_at": -1, "id": 1 })
            .await
            ?;

        Ok(GrantQueueHealth {
            latest_fact_version: latest.map(|s| s.fact_version),
            backlog,
            failures,
            oldest_age_secs: oldest.map_or(0, |e| now.saturating_sub(e.evidence.occurred_at)),
        })
    }
    /// 按类型化条件和主键游标读取至多 100 条正式事件，将本页仍为已处理且版本匹配的事件重新排队，不按当前责任人推断历史。返回本页末尾游标，空页表示结束；读取或写入失败返回错误，重排后可安全重放。
    pub(crate) async fn requeue_with_executor(
        &self,
        filter: &GrantRecompute,
        after: Option<&str>,
        now: u64,
        executor: &mut dyn Executor,
    ) -> Result<Option<String>> {
        let mut query = filter.query()?;
        query.insert("deleted_at", 0);
        if let Some(after) = after {
            query.insert("id", doc! { "$gt": after });
        }
        let collection = self.db.collection::<GrantEvent>("resource_grant_events");
        let events = mongo_ops::find_many(
            &collection,
            query,
            FindOptions::builder().sort(doc! { "id": 1 }).limit(100).build(),
            executor,
        )
        .await?;

        let now = i64::try_from(now).map_err(|_| Error::ValidationError("时间溢出".into()))?;
        for event in &events {
            mongo_ops::update_one(&collection, doc! { "id": &event.base.id, "status": "applied", "version": i64::try_from(event.base.version).map_err(|_| Error::Internal("版本溢出".into()))? },
                doc! { "$set": { "status": "pending", "available_at": now }, "$inc": { "version": 1 } }, false, executor)
                .await?;
        }
        Ok(events.last().map(|e| e.base.id.clone()))
    }
}
/// 维护入口仅允许固定规则、资源类型或稳定来源。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case", deny_unknown_fields)]
pub enum GrantRecompute {
    All,
    Source(
        /// 仅重排具有此稳定来源身份的正式事件。
        SourceKey
    ),
    Resource(
        /// 重排能为此资源类型生成读取授权的规则事件。
        ResourceKind
    ),
    Rule(
        /// 仅重排由此固定规则产生的正式事件。
        GrantRule
    ),
}
/// 已实现的规则版本固定为 1；新增规则必须增加类型化变体。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantRule {
    InvoiceTaskReassigned,
    InvoiceApproved,
    SalesOrderApprovalAssigned,
    PurchaseOrderApprovalAssigned,
}
impl GrantRule {
    // 返回正式事件使用的稳定规则代码，供按规则重算筛选。
    fn code(&self) -> &'static str {
        match self {
            Self::InvoiceApproved => "invoice_approved",
            Self::InvoiceTaskReassigned => "invoice_task_reassigned",
            Self::SalesOrderApprovalAssigned => "sales_order_approval_assigned",
            Self::PurchaseOrderApprovalAssigned => "purchase_order_approval_assigned",
        }
    }
}
impl GrantRecompute {
    // 将维护范围转为正式事件筛选；支持全部、已验证的精确来源、资源或规则。
    fn query(&self) -> Result<Document> {
        Ok(match self {
            Self::All => doc! {},
            Self::Source(key) => {
                key.id()?;
                doc! { "source.rule": &key.rule, "source.business_type": &key.business_type, "source.business_id": &key.business_id, "source.recipient": &key.recipient }
            },
            Self::Rule(rule) => doc! { "source.rule": rule.code() },
            Self::Resource(ResourceKind::SalesOrder) => {
                doc! { "source.rule": { "$in": ["invoice_approved", "invoice_task_reassigned", "sales_order_approval_assigned"] } }
            },
            Self::Resource(ResourceKind::PurchaseOrder) => {
                doc! { "source.rule": "purchase_order_approval_assigned" }
            },
        })
    }
}

impl GrantEventRepository {
    /// 在同一事务按幂等键读取正式事件并校验完整证据；缺失返回 `None`，同键异载荷或读取失败返回错误。
    pub async fn event(
        &self,
        expected: &GrantEvent,
        executor: &mut dyn Executor,
    ) -> Result<Option<GrantEvent>> {
        let event = resource_grants::event_by_key(&self.db, &expected.dedup_key, executor).await?;
        if let Some(event) = &event {
            event.ensure_replay(expected)?;
        }
        Ok(event)
    }
}

impl GrantEventRepository {
    /// 发布记录授权时按精确来源身份走索引定位正式业务事件，不扫描接收人的事件全集。
    pub async fn source_event(
        &self,
        source: &SourceKey,
        executor: &mut dyn Executor,
    ) -> Result<Option<GrantEvent>> {
        source.id()?;
        Ok(resource_grants::events(&self.db)
            .find_one(
                doc! {
                    "source.rule": &source.rule, "source.business_type": &source.business_type,
                    "source.business_id": &source.business_id, "source.recipient": &source.recipient,
                },
                executor,
            )
            .await?)
    }
}
