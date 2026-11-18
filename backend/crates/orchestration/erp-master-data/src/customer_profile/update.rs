//! 客户资料修订用例与事务载荷。

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_core::field_update::FieldUpdate;
use erp_core::ids::{PartyId, PartyRevisionId};
use erp_customer::{
    CustomerAccount, CustomerAccountUpdate, CustomerExt, CustomerProfileCommand,
    CustomerProfileCommandResultData, CustomerProfileMutationView, CustomerProfileOperation,
    CustomerProfileReplayContext, SaveCustomerProfileRequest,
};
use erp_party::repository::prelude::*;
use erp_party::service::party::identity::ensure_identity_available;
use erp_party::{Party, PartyExt, PartyRevision, PartyRevisionData, PartyUpdate};
use erp_permissions::customer as customer_permissions;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::facts::PartyFactChanges;
use super::idempotency::command_view;
use super::{CustomerProfileProcess, field_changes};
use crate::{Error, Result, command_insert_error};

impl CustomerProfileProcess {
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
        let fields = field_changes::permissions(&req, CustomerProfileOperation::Update);

        let fields_owned = fields.clone();
        let this = self.clone();
        let actor_owned = actor.clone();
        let customer_id = customer_id.to_owned();
        let replay_owned = replay.clone();
        let transaction = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    this.access
                        .require_with(
                            actor_owned.clone(),
                            &customer_permissions::UPDATE,
                            &customer_id,
                            executor,
                        )
                        .await?;
                    this.require_fields(&actor_owned, &fields_owned, executor).await?;

                    if let Some(view) = this.replay_command(&replay_owned, &actor_owned, executor).await? {
                        return Ok(view);
                    }

                    let prepared =
                        this.prepare_update(&customer_id, req, replay_owned, &actor_owned, executor).await?;
                    let result = prepared.result.clone();
                    prepared.persist(&this.db, executor).await?;
                    Ok(result)
                })
            })
            .await;

        self.resolve_transaction(transaction, &replay, actor, &fields).await
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
    /// 授权、载荷读取和落库必须使用原写入事务执行器。
    async fn prepare_update(
        &self,
        customer_id: &str,
        req: SaveCustomerProfileRequest,
        replay: CustomerProfileReplayContext,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<PreparedUpdate> {
        let mut account = self.load_customer(customer_id, executor).await?;
        account
            .ensure_version(req.expected_customer_version.unwrap_or_default())
            .map_err(|error| Error::ConflictError(error.to_string()))?;
        let mut party = self.load_party(&account.party_id, executor).await?;
        party
            .ensure_version(req.expected_party_version.unwrap_or_default())
            .map_err(|error| Error::ConflictError(error.to_string()))?;

        let revision_no = self.db.party_revisions().next_revision_no(&account.party_id, executor).await?;
        let revision = update_roots(&mut party, &mut account, &req, revision_no, actor.id())?;

        let facts = self.prepare_fact_changes(&account.party_id, &req, actor.id(), executor).await?;
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
    async fn persist(mut self, db: &Database, executor: &mut dyn Executor) -> Result<()> {
        ensure_identity_available(db, &self.party, &self.revision, executor).await?;

        db.party_revisions().create(&self.revision, executor).await?;
        db.parties().update(&mut self.party, executor).await?;
        db.customer_accounts().update(&mut self.account, executor).await?;
        self.facts.persist(db, executor).await?;

        db.customer_profile_commands().create(&self.command, executor).await.map_err(command_insert_error)?;
        persist_log(db, &self.audit, executor).await?;
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
