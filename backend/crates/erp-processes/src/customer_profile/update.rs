//! 客户资料修订用例与事务载荷。

use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_core::field_update::FieldUpdate;
use erp_core::ids::{PartyId, PartyRevisionId};
use erp_customer::{
    CustomerAccount, CustomerAccountUpdate, CustomerExt, CustomerProfileCommand,
    CustomerProfileCommandResultData, CustomerProfileMutationView, CustomerProfileOperation,
    CustomerProfileReplayContext, SaveCustomerProfileRequest,
};
use erp_party::{Party, PartyExt, PartyRevision, PartyRevisionData, PartyUpdate};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{NoTransaction, Transactional};

use super::CustomerProfileService;
use super::facts::PartyFactChanges;
use super::idempotency::{checked_command_view, command_view};
use crate::adapters::customer_access;
use crate::{Error, Result};

impl CustomerProfileService {
    /// 原子修订 Party 身份、客户角色与显式提交的资料事实集合。
    ///
    /// # 参数
    /// * `customer_id` - 目标客户
    /// * `req` - 修订命令
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回修订命令的稳定结果视图。
    ///
    /// # 错误
    /// 输入非法、无更新范围、乐观锁冲突、既有事实不属于当前客户、幂等键冲突或事务失败时返回错误。
    ///
    /// # 关键业务约束
    /// 写入事务内必须调用 `CustomerAccess.require_with`；handler 事前检查不能代替。
    pub async fn update(
        &self,
        customer_id: &str,
        req: SaveCustomerProfileRequest,
        actor: &AuditActor,
    ) -> Result<CustomerProfileMutationView> {
        req.validate_protocol()?;
        req.validate_structure(CustomerProfileOperation::Update)?;
        let replay = req.replay_context(CustomerProfileOperation::Update, Some(customer_id), actor.id())?;
        if let Some(command) = self.command_record(replay.idempotency_key()).await? {
            return checked_command_view(command, &replay);
        }
        let prepared = self.prepare_update(customer_id, req, replay.clone(), actor).await?;
        let intended = prepared.result.clone();
        let transaction = self.commit_update(customer_id, prepared, actor).await;
        self.resolve_transaction(transaction, intended, &replay).await
    }

    /// 在同一写入事务内重验对象范围并持久化修订。
    ///
    /// # 参数
    /// * `customer_id` - 目标客户
    /// * `prepared` - 已构造的修订事务载荷
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 事务成功时返回 `Ok`；失败时返回原事务错误供幂等恢复。
    ///
    /// # 错误
    /// 账号、动作或对象资格失效、乐观锁冲突或写入失败时拒绝。
    ///
    /// # 关键业务约束
    /// 必须在原领域事务调用 `require_with`，handler 事前检查不能代替。
    async fn commit_update(
        &self,
        customer_id: &str,
        prepared: PreparedUpdate,
        actor: &AuditActor,
    ) -> Result<()> {
        let rbac = self.require_rbac()?.clone();
        let actor = actor.clone();
        let customer_id = customer_id.to_string();
        let db = self.db.clone();
        db.client()
            .clone()
            .with_transaction(move |session| {
                Box::pin(async move {
                    customer_access(db.clone(), rbac)
                        .require_with(actor, "update", &customer_id, session)
                        .await?;
                    prepared.persist(&db, session).await
                })
            })
            .await
    }

    /// 装载并校验修订载荷；不写入、不重验范围。
    ///
    /// # 参数
    /// * `customer_id` - 目标客户
    /// * `req` - 修订命令
    /// * `replay` - 幂等上下文
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回可在事务内持久化的修订载荷。
    ///
    /// # 错误
    /// 客户不存在、版本冲突或事实校验失败时拒绝。
    ///
    /// # 关键业务约束
    /// 范围重验必须留在 `commit_update` 的同一写入事务内。
    async fn prepare_update(
        &self,
        customer_id: &str,
        req: SaveCustomerProfileRequest,
        replay: CustomerProfileReplayContext,
        actor: &AuditActor,
    ) -> Result<PreparedUpdate> {
        let mut account = self.load_customer(customer_id).await?;
        account
            .ensure_version(req.expected_customer_version.unwrap_or_default())
            .map_err(|error| Error::ConflictError(error.to_string()))?;
        let mut party = self.load_party(&account.party_id).await?;
        party
            .ensure_version(req.expected_party_version.unwrap_or_default())
            .map_err(|error| Error::ConflictError(error.to_string()))?;
        let revision_no =
            self.db.party_revisions().next_revision_no(&account.party_id, &mut NoTransaction).await?;
        let revision = update_roots(&mut party, &mut account, &req, revision_no, actor.id())?;
        let facts = self.prepare_fact_changes(&account.party_id, &req, actor.id()).await?;
        PreparedUpdate::new(party, revision, account, facts, req, replay, actor)
    }
}

/// 修订事务载荷。
struct PreparedUpdate {
    party: Party,
    revision: PartyRevision,
    account: CustomerAccount,
    facts: PartyFactChanges,
    command: CustomerProfileCommand,
    audit: erp_audit::AuditLog,
    result: CustomerProfileMutationView,
}

impl PreparedUpdate {
    /// 构造命令、审计及保存后的稳定版本结果。
    fn new(
        party: Party,
        revision: PartyRevision,
        account: CustomerAccount,
        facts: PartyFactChanges,
        req: SaveCustomerProfileRequest,
        replay: CustomerProfileReplayContext,
        actor: &AuditActor,
    ) -> Result<Self> {
        let command = CustomerProfileCommand::record_success(
            next_id(),
            &replay,
            CustomerProfileCommandResultData {
                customer_id: account.base.id.clone(),
                customer_no: account.customer_no.clone(),
                party_id: party.base.id.clone(),
                revision_id: revision.base.id.clone(),
                revision_no: revision.revision.revision_no,
                customer_version: account.base.version + 1,
                party_version: party.base.version + 1,
                effective_from: req.effective_from,
                change_reason: req.change_reason,
            },
        )?;
        let result = command_view(command.clone());
        let audit = actor.clone().resource_log(
            "customer_profile.update",
            "customer_profile",
            account.base.id.clone(),
        )?;
        Ok(Self { party, revision, account, facts, command, audit, result })
    }

    /// 将根修订、事实差异、幂等结果与审计写入同一事务。
    async fn persist(mut self, db: &Database, session: &mut mongodb::ClientSession) -> Result<()> {
        db.party_revisions().create(&self.revision, session).await?;
        db.parties().update(&mut self.party, session).await?;
        db.customer_accounts().update(&mut self.account, session).await?;
        self.facts.persist(db, session).await?;
        db.customer_profile_commands().create(&self.command, session).await?;
        db.audit_logs().create(&self.audit, session).await?;
        Ok(())
    }
}

fn update_roots(
    party: &mut Party,
    account: &mut CustomerAccount,
    req: &SaveCustomerProfileRequest,
    revision_no: u32,
    actor: &str,
) -> Result<PartyRevision> {
    let revision_id = PartyRevisionId::new(next_id());
    party.update(
        PartyUpdate {
            unified_credit_code: FieldUpdate::from_optional_text(req.unified_credit_code.clone()),
            status: None,
        },
        actor,
    )?;
    party.stable.current_revision_id = Some(revision_id.to_string());
    let status = req.status.filter(|status| *status != account.stable.status);
    account.update(
        CustomerAccountUpdate {
            default_payment_term_id: FieldUpdate::from_optional_text(req.default_payment_term_id.clone()),
            status,
        },
        actor,
    )?;
    Ok(PartyRevision::new(
        revision_id,
        PartyRevisionData {
            party_id: party_id(party),
            revision_no,
            legal_name: req.legal_name.clone(),
            short_name: req.short_name.clone(),
            change_reason: req.change_reason.clone(),
        },
    )?)
}

/// 返回 Party ID newtype。
fn party_id(party: &Party) -> PartyId {
    PartyId::new(party.base.id.clone())
}
