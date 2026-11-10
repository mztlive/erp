use application_core::AuditActor;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::ResourceGrantService;
use crate::entity::resource_access::event::{GrantEvent, GrantEventEvidence};
use crate::entity::resource_access::query::GrantSourcePage;
use crate::entity::resource_access::{GrantSource, SourceKey, SourceUpdate};
use crate::repository::resource_grants;
use crate::service::access_control::resolve::DataScopeService;
use crate::{AccessControlExt, Error, Permission, Result};

impl ResourceGrantService {
    /// 管理员按稳定来源授予指定记录；重放不写库。
    /// # 参数
    /// actor 为管理员；update 的 rule 必须为 manual，business_id 为稳定命令号。
    /// # 返回
    /// 来源当前版本。
    /// # 错误
    /// 管理资格不足、接收账号无效、目标无效、来源终止或版本冲突时拒绝整个事务。
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
    /// # 参数
    /// update 为规则计算结果；executor 必须属于外层授权事务。
    /// # 返回
    /// 当前来源及版本。
    /// # 错误
    /// 无事务、人工来源、可信事实校验失败或来源冲突时拒绝。
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
    /// # 参数
    /// actor 为管理员；key 为来源身份；expected_version 为读取版本；reason 为撤销原因。
    /// # 返回
    /// 终止后的来源。
    /// # 错误
    /// 管理资格不足、来源缺失或版本冲突拒绝。
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
    /// 管理员查看指定接收账号的来源及终止原因。
    /// # 参数
    /// actor 为管理员；recipient 为接收账号；after 为上一页游标；executor 为调用执行器。
    /// # 返回
    /// 最多 100 个来源及下一页游标，包含终止记录。
    /// # 错误
    /// 管理资格不足或数据库失败时拒绝。
    pub async fn sources(
        &self,
        actor: &AuditActor,
        recipient: &str,
        after: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<GrantSourcePage> {
        self.authorize_manager(actor, executor).await?;
        resource_grants::for_recipient(&self.db, recipient, after, executor).await
    }
    /// 校验读取授权的维护资格。
    /// # 参数
    /// actor 为当前管理员，executor 为维护事务。
    /// # 返回
    /// 同角色具备范围维护及全公司组织管理资格时成功。
    /// # 错误
    /// 身份失效、资格不足或数据库失败时拒绝。
    pub async fn authorize_manager(&self, actor: &AuditActor, executor: &mut dyn Executor) -> Result<()> {
        let permissions = [Permission::parse("data_scope:create")?];
        let access = DataScopeService::new(self.db.clone(), self.rbac.clone())
            .resolve_permissions(actor, &Permission::parse("org_unit:manage")?, &permissions, executor)
            .await?;
        if access.scope.user_limit.is_some() || !access.scope.role_clauses.iter().any(|c| c.company) {
            return Err(Error::Forbidden("指定记录授权管理需要全公司组织管理资格".into()));
        }
        Ok(())
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
    if executor.session().is_none() {
        return Err(Error::Internal("授权发布必须加入调用方事务".into()));
    }
    Ok(())
}

/// 在业务原事务追加已提交事实对应的幂等授权事件。
/// # 参数
/// db 为身份集合所在数据库；evidence 为正式分配证据；executor 为业务事务。
/// # 返回
/// 新事件返回 true，同证据重放返回 false。
/// # 错误
/// 无事务、证据非法、幂等冲突或持久化失败拒绝。
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
    use persistence_core::NoTransaction;

    use super::*;
    #[test]
    fn publishing_requires_an_existing_transaction() {
        assert!(matches!(require_transaction(&mut NoTransaction), Err(Error::Internal(_))));
    }
}
