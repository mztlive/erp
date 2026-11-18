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

/// 授权事件队列；不承担读取授权判定。
#[derive(Clone)]
pub struct GrantEventRepository {
    db: Database,
}

impl GrantEventRepository {
    /// 用身份库创建授权事件队列仓储，供事件消费者领取和回写使用；构造不启动后台循环。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 原子领取到期事件或回收过期租约。
    /// 最多一个事件。
    ///
    /// 数据库失败或令牌为空时拒绝。
    ///
    /// owner 为每次领取唯一令牌，now 为秒级时间。
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
    /// 发布授权前，在原写事务读取最新事件并核对领取令牌。允许幂等返回已完成事件；事件缺失或租约已被接管时拒绝。
    pub async fn claimed_event(&self, claim: &GrantEvent, executor: &mut dyn Executor) -> Result<GrantEvent> {
        let current = resource_grants::events(&self.db)
            .find_by_id(&claim.base.id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("授权事件不存在".into()))?;
        current.ensure_claim(claim)?;
        Ok(current)
    }
    /// 在保存授权结果的同一事务内将事件标记为已处理；已完成重放不再写入。无事务、版本冲突或持久化失败时拒绝。
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
    /// 发布失败后只回写仍由原领取令牌持有的事件。完成或已被接管时不写；数据库失败原样返回，租约到期后仍可被重新领取。
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
    /// 读取单一来源状态，供发布时排除永久终止及旧事件。
    /// 已存在的来源。
    ///
    /// 身份非法或读取失败时拒绝。
    ///
    /// key 为可信事件来源，executor 为发布事务。
    pub async fn source(&self, key: &SourceKey, executor: &mut dyn Executor) -> Result<Option<GrantSource>> {
        Ok(resource_grants::sources(&self.db).find_by_id(&key.id()?, executor).await?)
    }
}

/// 队列状态不包含账号或业务证据。
#[derive(Debug)]
pub struct GrantQueueHealth {
    pub backlog: u64,
    pub oldest_age_secs: u64,
    pub failures: u64,
    pub latest_fact_version: Option<u64>,
}
impl GrantEventRepository {
    /// 读取待处理量、最老未完成时长及失败量。
    /// 当前队列快照。
    ///
    /// 数据库失败时返回错误。
    ///
    /// now 为当前秒级时间。
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
    // 对正式事件按主键有界重算，禁止从当前责任人猜测历史授权。
    // 本页末尾游标；空页表示一轮结束。每次最多 100 条。
    //
    // 查询或更新失败返回错误；已重新排队事件可安全重放。
    //
    // filter 为类型化选择条件，after 为主键游标，now 为重新可领取时间。
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
    Source(SourceKey),
    Resource(ResourceKind),
    Rule(GrantRule),
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
    /// 读取同事务正式事件，校验完整证据。
    /// 当前事件状态，缺失返回空值。
    ///
    /// 同键异载荷或读取失败返回错误。
    ///
    /// expected 为业务侧提供的正式事件，executor 为调用方执行器。
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
    /// 发布记录授权需要定位正式业务事件时，按精确来源身份走索引查找，不扫描该账号的事件全集。
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
