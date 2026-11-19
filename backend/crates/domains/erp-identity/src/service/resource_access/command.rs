use application_core::AuditActor;
use erp_permissions::access_control;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::{GrantEventService, ResourceGrantService};
use crate::dto::GrantSourcePage;
use crate::entity::resource_access::event::{GrantEvent, GrantEventEvidence, GrantEventStatus};
use crate::entity::resource_access::{GrantSource, SourceKey, SourceUpdate};
use crate::repository::resource_grants;
use crate::{AccessControlExt, AccessTarget, Error, Result};

impl ResourceGrantService {
    /// 管理员手工授予指定记录的读取权时，为稳定来源创建或更新授权，返回当前来源及版本。
    /// `rule` 和 `business_type` 均须为 `manual`，`business_id` 使用稳定命令号；同版本同目标重放不写库。
    /// 管理资格不足、接收账号或目标无效、来源终止或版本冲突时，整个事务失败。
    pub async fn grant(&self, actor: &AuditActor, update: SourceUpdate) -> Result<GrantSource> {
        let this = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    this.authorize_manager(&actor, executor).await?;
                    if update.key.rule != "manual" || update.key.business_type != "manual" {
                        return Err(Error::ValidationError("管理员只能创建 manual 来源".into()));
                    }
                    let existing =
                        resource_grants::sources(&this.db).find_by_id(&update.key.id()?, executor).await?;
                    this.apply(update, existing, actor.id(), executor).await
                })
            })
            .await
    }
    /// 在调用方的发布事务内复核事件领取，从可信业务事实计算并应用自动授权，同事务完成事件。
    /// 已完成事件不写入；终止来源不读取事实，旧事实不覆盖新版本。领取失效或事实校验失败时不完成事件。
    pub async fn apply_source(&self, claim: &GrantEvent, executor: &mut dyn Executor) -> Result<()> {
        require_transaction(executor)?;
        let queue = GrantEventService::new(self.db.clone());
        let event = queue.claimed_event(claim, executor).await?;
        if event.status == GrantEventStatus::Applied {
            return Ok(());
        }

        let existing = queue.source(&event.source, executor).await?;
        self.publish_source(&event, existing, executor).await?;
        queue.complete_event(event, executor).await
    }

    // 终止来源不再计算；过期判断使用当前事实版本，而非事件触发时的版本。
    async fn publish_source(
        &self,
        event: &GrantEvent,
        existing: Option<GrantSource>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if existing.as_ref().is_some_and(|source| source.terminated) {
            return Ok(());
        }
        if event.source.rule == "manual" {
            return Err(Error::Forbidden("自动处理器不能修改人工来源".into()));
        }

        let update = self.facts.compute_source(event, executor).await?;
        if update.key != event.source {
            return Err(Error::ConflictError("自动授权业务关联或正式分配事实不一致".into()));
        }
        if !existing.as_ref().is_some_and(|source| source.fact_version > update.fact_version) {
            self.apply(update, existing, "resource-grant-worker", executor).await?;
        }
        Ok(())
    }

    /// 管理员撤销记录读取权时，按 `key` 和先前读取的版本永久终止单个来源，记录原因并返回终止后的来源。
    /// 事务提交后，新快照中的读取授权判定立即排除此来源；管理资格不足、来源缺失或版本冲突时拒绝撤销。
    pub async fn revoke(
        &self,
        actor: &AuditActor,
        key: SourceKey,
        expected_version: u64,
        reason: String,
    ) -> Result<GrantSource> {
        let this = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    this.authorize_manager(&actor, executor).await?;

                    let repo = resource_grants::sources(&this.db);
                    let mut source = repo
                        .find_by_id(&key.id()?, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("授权来源不存在".into()))?;
                    if source.base.version != expected_version {
                        return Err(Error::ConflictError("授权来源版本已变化".into()));
                    }

                    if source.revoke(actor.id(), &reason)? {
                        repo.update(&mut source, executor).await?;
                        resource_grants::advance(
                            &this.db,
                            &source.key.recipient,
                            &source.resources(),
                            executor,
                        )
                        .await?;
                        resource_grants::audit_source(&this.db, &source, executor).await?;
                    }
                    Ok(source)
                })
            })
            .await
    }
    /// 管理员查看指定接收账号的授权来源时，在同一快照内按 `after` 续页。
    /// 每页最多 100 条，包含终止记录及原因，并返回下一页游标；管理资格不足、读取阶段变化或数据库失败时返回错误。
    pub async fn sources(
        &self,
        actor: &AuditActor,
        recipient: &str,
        after: Option<&str>,
    ) -> Result<GrantSourcePage> {
        let service = self.clone();
        let actor = actor.clone();
        let recipient = recipient.to_owned();
        let after = after.map(str::to_owned);
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(
                    async move { service.source_page(&actor, &recipient, after.as_deref(), executor).await },
                )
            })
            .await
    }

    // 在来源分页事务中先检查管理资格，再读取本页，并在返回前确认仍处于原读取阶段。
    async fn source_page(
        &self,
        actor: &AuditActor,
        recipient: &str,
        after: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<GrantSourcePage> {
        self.authorize_manager(actor, executor).await?;
        let boundary =
            executor.boundary().ok_or_else(|| Error::ValidationError("授权来源读取缺少执行边界".into()))?;
        let page = resource_grants::for_recipient(&self.db, recipient, after, executor).await?;
        if !boundary.is_current(executor) {
            return Err(Error::ValidationError("授权来源读取跨越执行阶段".into()));
        }
        Ok(page)
    }

    /// 在维护事务中检查当前账号的记录读取授权管理资格，要求同一启用角色具备统一范围维护的全部资格及全公司组织范围。
    /// 身份失效、资格不足或数据库读取失败时返回错误。
    pub async fn authorize_manager(&self, actor: &AuditActor, executor: &mut dyn Executor) -> Result<()> {
        self.authorization
            .require(actor, &access_control::RESOURCE_GRANT_MANAGE, AccessTarget::Operation, executor)
            .await
    }
    // 人工或自动入口完成各自校验后，在原事务内应用来源结果；有变更时校验接收账号和目标，同步授权行、修订号及审计。
    async fn apply(
        &self,
        update: SourceUpdate,
        existing: Option<GrantSource>,
        operator: &str,
        executor: &mut dyn Executor,
    ) -> Result<GrantSource> {
        let repo = resource_grants::sources(&self.db);
        let is_new = existing.is_none();
        let mut source = match existing {
            Some(source) => source,
            None => GrantSource::new(update.key)?,
        };
        let mut affected = source.resources();
        if !source.apply(
            update.rule_version,
            update.fact_version,
            update.targets,
            operator,
            &update.reason,
        )? {
            return Ok(source);
        }

        self.db
            .accounts()
            .find_by_id(&source.key.recipient, executor)
            .await?
            .filter(|account| account.is_active_backoffice())
            .ok_or_else(|| Error::ValidationError("接收账号不可用".into()))?;
        self.facts.validate_targets(&source.targets, executor).await?;

        if is_new {
            repo.create(&source, executor).await?;
        } else {
            repo.update(&mut source, executor).await?;
        }
        resource_grants::sync_rows(&self.db, &source, executor).await?;
        affected.extend(source.resources());
        resource_grants::advance(&self.db, &source.key.recipient, &affected, executor).await?;

        resource_grants::audit_source(&self.db, &source, executor).await?;
        Ok(source)
    }
}
// 发布来源或追加事件前，要求调用方已提供快照事务执行器。
fn require_transaction(executor: &mut dyn Executor) -> Result<()> {
    if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
        return Err(Error::Internal("授权发布必须加入调用方事务".into()));
    }
    Ok(())
}

/// 写入正式分配等业务事实时，在同一业务事务内向身份库追加授权事件；历史回填须使用已核验的证据。
/// 追加成功只表示入队，须由后台发布来源后才可能生效。
/// 新事件返回 `true`，同证据重放返回 `false`；缺少快照事务、证据非法、幂等冲突或持久化失败时，调用方须终止事务。
pub async fn append_event(
    db: &Database,
    evidence: GrantEventEvidence,
    executor: &mut dyn Executor,
) -> Result<bool> {
    require_transaction(executor)?;
    let event = GrantEvent::new(evidence)?;
    let repo = resource_grants::events(db);
    if let Some(old) = resource_grants::event_by_key(db, &event.dedup_key, executor).await? {
        old.ensure_replay(&event)?;
        return Ok(false);
    }

    // 唯一索引兜底并发去重；插入失败必须退出原业务事务，后续重试再读取已有事件。
    repo.create(&event, executor).await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;
    use erp_core::AccountKind;
    use mongodb::Client;
    use persistence_core::testing::SnapshotExecutor;
    use persistence_core::{ExecutionBoundary, NoTransaction, ReadExecutor};

    use super::*;
    use crate::entity::access_control::ResolvedScope;
    use crate::entity::resource_access::event::GrantEventPayload;
    use crate::entity::resource_access::{GrantTarget, ResourceKind};
    use crate::ports::ResourceGrantFacts;
    use crate::service::authorization::fixed;

    // 来源分页测试使用的事实端口替身，任何发布校验调用都会使测试失败。
    struct UnusedGrantFacts;

    #[async_trait]
    impl ResourceGrantFacts for UnusedGrantFacts {
        // 来源分页若误触发写入目标校验，立即使测试失败。
        async fn validate_targets(&self, _: &[GrantTarget], _: &mut dyn Executor) -> Result<()> {
            panic!("来源读取不得调用写入目标校验")
        }

        // 来源分页若误触发自动来源计算，立即使测试失败。
        async fn compute_source(&self, _: &GrantEvent, _: &mut dyn Executor) -> Result<SourceUpdate> {
            panic!("来源读取不得调用来源发布校验")
        }
    }

    // 记录自动事实计算次数，并确认端口沿原快照读取同一事件。
    struct ComputedGrantFacts {
        // 发布入口收到的事件记录。
        event_id: String,
        // 发布事务的执行边界。
        boundary: ExecutionBoundary,
        // 实际调用次数。
        calls: AtomicUsize,
    }

    #[async_trait]
    impl ResourceGrantFacts for ComputedGrantFacts {
        // 过期或相同的版本不触发目标校验或写入。
        async fn validate_targets(&self, _: &[GrantTarget], _: &mut dyn Executor) -> Result<()> {
            panic!("过期或重放的授权不得校验目标")
        }

        async fn compute_source(
            &self,
            event: &GrantEvent,
            executor: &mut dyn Executor,
        ) -> Result<SourceUpdate> {
            assert_eq!(event.base.id, self.event_id);
            assert!(self.boundary.is_current(executor));
            self.calls.fetch_add(1, Ordering::Relaxed);
            Ok(SourceUpdate {
                key: event.source.clone(),
                rule_version: 1,
                fact_version: 2,
                targets: vec![GrantTarget { resource: ResourceKind::SalesOrder, id: "sales-order".into() }],
                reason: "正式业务分配".into(),
            })
        }
    }

    // 构造在访问持久化前即可验证的身份服务。
    async fn service(facts: Arc<dyn ResourceGrantFacts>) -> ResourceGrantService {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("resource_sources_unit");
        let authorization = fixed::authorization(
            &[],
            ResolvedScope { role_clauses: Vec::new(), user_limit: None },
            Vec::new(),
            Vec::new(),
        )
        .unwrap();
        ResourceGrantService::new(db, authorization, facts)
    }

    // 构造审批分配事件用于自动来源发布测试。
    fn assignment_event() -> GrantEvent {
        GrantEvent::new(GrantEventEvidence {
            payload: GrantEventPayload::SalesOrderApprovalAssigned {
                sales_order_id: "sales-order".into(),
                approval_instance_id: "approval".into(),
                node_id: "node".into(),
                assignment_id: "assignment".into(),
            },
            recipient: "recipient".into(),
            fact_version: 1,
            occurred_at: 1,
        })
        .unwrap()
    }

    // 无快照、来源已终止或自动处理人工来源时均不调用事实端口。
    #[tokio::test]
    async fn publishing_skips_facts_without_snapshot_or_for_terminated_and_manual_sources() {
        let service = service(Arc::new(UnusedGrantFacts)).await;
        let mut event = assignment_event();
        assert!(matches!(service.apply_source(&event, &mut NoTransaction).await, Err(Error::Internal(_))));

        let mut executor = SnapshotExecutor::default();
        let mut source = GrantSource::new(event.source.clone()).unwrap();
        source.revoke("operator", "撤销").unwrap();
        service.publish_source(&event, Some(source), &mut executor).await.unwrap();

        event.source.rule = "manual".into();
        assert!(matches!(
            service.publish_source(&event, None, &mut executor).await,
            Err(Error::Forbidden(_))
        ));
    }

    // 按计算所得而非事件原始版本跳过旧结果，同版本重放不写入但不同目标仍然冲突。
    #[tokio::test]
    async fn publishing_computes_once_and_compares_the_computed_version() {
        let event = assignment_event();
        let mut executor = SnapshotExecutor::default();
        let facts = Arc::new(ComputedGrantFacts {
            event_id: event.base.id.clone(),
            boundary: executor.boundary().unwrap(),
            calls: AtomicUsize::new(0),
        });
        let service = service(facts.clone()).await;
        for (index, (fact_version, target, conflicts)) in
            [(3, "other-order", false), (2, "sales-order", false), (2, "other-order", true)]
                .into_iter()
                .enumerate()
        {
            let mut source = GrantSource::new(event.source.clone()).unwrap();
            let targets = vec![GrantTarget { resource: ResourceKind::SalesOrder, id: target.into() }];
            source.apply(1, fact_version, targets, "operator", "正式业务分配").unwrap();

            let result = service.publish_source(&event, Some(source), &mut executor).await;
            if conflicts {
                assert!(
                    matches!(result, Err(Error::ConflictError(message)) if message == "同版本授权结果不一致")
                );
            } else {
                result.unwrap();
            }
            assert_eq!(facts.calls.load(Ordering::Relaxed), index + 1);
        }
    }

    // 验证来源分页在查询前拒绝缺少快照或管理资格的调用，并区分执行边界错误和权限拒绝。
    #[tokio::test]
    async fn source_page_rejects_missing_snapshot_or_management_qualification_before_reading_sources() {
        let service = service(Arc::new(UnusedGrantFacts)).await;
        let actor = AuditActor::new("operator".into(), "operator".into(), AccountKind::Admin);
        for executor in [&mut NoTransaction as &mut dyn Executor, &mut ReadExecutor::default()] {
            assert!(matches!(
                service.source_page(&actor, "recipient", Some("cursor"), executor).await,
                Err(Error::ValidationError(_))
            ));
        }

        assert!(matches!(
            service.source_page(&actor, "recipient", Some("cursor"), &mut SnapshotExecutor::default()).await,
            Err(Error::Forbidden(_))
        ));
    }
}
