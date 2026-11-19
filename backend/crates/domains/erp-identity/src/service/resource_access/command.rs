use application_core::AuditActor;
use erp_permissions::access_control;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::ResourceGrantService;
use crate::dto::GrantSourcePage;
use crate::entity::resource_access::event::{GrantEvent, GrantEventEvidence};
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
                    this.apply(update, actor.id(), executor).await
                })
            })
            .await
    }
    /// 自动处理器发布规则计算结果时，沿用外层授权事务，重验触发事实、正式分配及完整结果后保存，返回当前来源及版本。
    /// 缺少快照事务、传入人工来源、事实校验失败或来源冲突时拒绝发布。
    pub async fn apply_source(
        &self,
        update: SourceUpdate,
        executor: &mut dyn Executor,
    ) -> Result<GrantSource> {
        require_transaction(executor)?;
        if update.key.rule == "manual" {
            return Err(Error::Forbidden("自动处理器不能修改人工来源".into()));
        }
        self.facts.validate_source(&update, executor).await?;
        self.apply(update, "resource-grant-worker", executor).await
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
        operator: &str,
        executor: &mut dyn Executor,
    ) -> Result<GrantSource> {
        let repo = resource_grants::sources(&self.db);
        let existing = repo.find_by_id(&update.key.id()?, executor).await?;
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

    use async_trait::async_trait;
    use erp_core::AccountKind;
    use mongodb::Client;
    use persistence_core::testing::SnapshotExecutor;
    use persistence_core::{NoTransaction, ReadExecutor};

    use super::*;
    use crate::entity::access_control::ResolvedScope;
    use crate::entity::resource_access::GrantTarget;
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

        // 来源分页若误触发自动来源校验，立即使测试失败。
        async fn validate_source(&self, _: &SourceUpdate, _: &mut dyn Executor) -> Result<()> {
            panic!("来源读取不得调用来源发布校验")
        }
    }

    // 验证发布入口的事务检查拒绝无事务执行器。
    #[test]
    fn publishing_requires_an_existing_transaction() {
        assert!(matches!(require_transaction(&mut NoTransaction), Err(Error::Internal(_))));
    }

    // 验证来源分页在查询前拒绝缺少快照或管理资格的调用，并区分执行边界错误和权限拒绝。
    #[tokio::test]
    async fn source_page_rejects_missing_snapshot_or_management_qualification_before_reading_sources() {
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
        let service = ResourceGrantService::new(db, authorization, Arc::new(UnusedGrantFacts));
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
