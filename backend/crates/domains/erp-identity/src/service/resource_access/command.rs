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
    /// 管理员按稳定来源授予指定记录；重放不写库。
    /// 来源当前版本。
    ///
    /// 管理资格不足、接收账号无效、目标无效、来源终止或版本冲突时拒绝整个事务。
    ///
    /// actor 为管理员；update 的 rule 必须为 manual，business_id 为稳定命令号。
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
    /// 自动处理器发布前强制重验业务触发事实、正式分配及完整结果，不开启嵌套事务。
    /// 当前来源及版本。
    ///
    /// 无事务、人工来源、可信事实校验失败或来源冲突时拒绝。
    ///
    /// update 为规则计算结果；executor 必须属于外层授权事务。
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
    /// 永久撤销一个来源；事务提交后所有读取立即排除此来源。
    /// 终止后的来源。
    ///
    /// 管理资格不足、来源缺失或版本冲突拒绝。
    ///
    /// actor 为管理员；key 为来源身份；expected_version 为读取版本；reason 为撤销原因。
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
    /// 管理员在同一快照查看指定接收账号的来源及终止原因。
    /// 最多 100 个来源及下一页游标，包含终止记录。
    ///
    /// 管理资格不足、读取阶段变化或数据库失败时拒绝。
    ///
    /// actor 为管理员；recipient 为接收账号；after 为上一页游标。
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

    // 已通过读取授权管理入口后，沿同一快照分页读取来源及终止原因，不交付跨阶段结果。
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

    /// 校验读取授权的维护资格。
    /// 同角色具备统一范围维护的全部资格和全公司组织范围时成功。
    ///
    /// 身份失效、资格不足或数据库失败时拒绝。
    ///
    /// actor 为当前管理员，executor 为维护事务。
    pub async fn authorize_manager(&self, actor: &AuditActor, executor: &mut dyn Executor) -> Result<()> {
        self.authorization
            .require(actor, &access_control::RESOURCE_GRANT_MANAGE, AccessTarget::Operation, executor)
            .await
    }
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
fn require_transaction(executor: &mut dyn Executor) -> Result<()> {
    if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
        return Err(Error::Internal("授权发布必须加入调用方事务".into()));
    }
    Ok(())
}

/// 在业务原事务追加已提交事实对应的幂等授权事件。
/// 新事件返回 true，同证据重放返回 false。
///
/// 无事务、证据非法、幂等冲突或持久化失败拒绝。
///
/// db 为身份集合所在数据库；evidence 为正式分配证据；executor 为业务事务。
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

    struct UnusedGrantFacts;

    #[async_trait]
    impl ResourceGrantFacts for UnusedGrantFacts {
        async fn validate_targets(&self, _: &[GrantTarget], _: &mut dyn Executor) -> Result<()> {
            panic!("来源读取不得调用写入目标校验")
        }

        async fn validate_source(&self, _: &SourceUpdate, _: &mut dyn Executor) -> Result<()> {
            panic!("来源读取不得调用来源发布校验")
        }
    }

    #[test]
    fn publishing_requires_an_existing_transaction() {
        assert!(matches!(require_transaction(&mut NoTransaction), Err(Error::Internal(_))));
    }

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
