//! 客户资料修订用例与事务载荷。

use database::{AccessControlExt, CustomerExt, NoTransaction, PartyExt, Transactional};
use entities::{
    customer::{CustomerAccount, CustomerAccountUpdate, CustomerProfileCommand, CustomerProfileOperation},
    field_update::FieldUpdate,
    ids::{PartyId, PartyRevisionId},
    party::{Party, PartyRevision, PartyRevisionData, PartyUpdate},
};
use id_generator::next_id;
use mongodb::Database;

use crate::{
    audit::AuditActor,
    errors::{Error, Result},
};

use super::super::{CustomerProfileMutationView, SaveCustomerProfileRequest};
use super::{
    facts::PartyFactChanges,
    idempotency::{
        command_view, profile_command, replay_command, request_fingerprint, ProfileCommandInput,
        TransactionResolutionContext,
    },
    CustomerProfileService,
};

impl CustomerProfileService {
    /// 原子修订 Party 身份、客户角色与显式提交的资料事实集合。
    ///
    /// # Errors
    /// 输入非法、乐观锁冲突、既有事实不属于当前客户、幂等键冲突或事务失败时返回错误。
    pub async fn update(
        &self,
        customer_id: &str,
        req: SaveCustomerProfileRequest,
        actor: &AuditActor,
    ) -> Result<CustomerProfileMutationView> {
        req.validate_protocol()?;
        req.validate_structure(CustomerProfileOperation::Update)?;
        let fingerprint = request_fingerprint(&req)?;
        if let Some(command) = self.command_record(&req.idempotency_key).await? {
            return replay_command(command, "update", Some(customer_id), actor.id(), &fingerprint);
        }
        let prepared = self
            .prepare_update(customer_id, req, fingerprint.clone(), actor)
            .await?;
        let intended = prepared.result.clone();
        let idempotency_key = prepared.command.idempotency_key.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        let transaction = client
            .with_transaction(move |session| Box::pin(async move { prepared.persist(&db, session).await }))
            .await;
        self.resolve_transaction(
            transaction,
            intended,
            TransactionResolutionContext {
                idempotency_key: &idempotency_key,
                operation: "update",
                customer_id: Some(customer_id),
                initiated_by: actor.id(),
                fingerprint: &fingerprint,
            },
        )
        .await
    }

    async fn prepare_update(
        &self,
        customer_id: &str,
        req: SaveCustomerProfileRequest,
        fingerprint: String,
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
        let revision_no = self
            .db
            .party_revisions()
            .next_revision_no(&account.party_id, &mut NoTransaction)
            .await?;
        let revision = update_roots(&mut party, &mut account, &req, revision_no, actor.id())?;
        let facts = self
            .prepare_fact_changes(&account.party_id, &req, actor.id())
            .await?;
        PreparedUpdate::new(party, revision, account, facts, req, fingerprint, actor)
    }
}

/// 修订事务载荷。
struct PreparedUpdate {
    party: Party,
    revision: PartyRevision,
    account: CustomerAccount,
    facts: PartyFactChanges,
    command: CustomerProfileCommand,
    audit: entities::AuditLog,
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
        fingerprint: String,
        actor: &AuditActor,
    ) -> Result<Self> {
        let command = profile_command(
            &party,
            &revision,
            &account,
            ProfileCommandInput {
                operation: "update",
                req,
                fingerprint,
                initiated_by: actor.id(),
                customer_version: account.base.version + 1,
                party_version: party.base.version + 1,
            },
        )?;
        let result = command_view(command.clone());
        let audit = actor.clone().resource_log(
            "customer_profile.update",
            "customer_profile",
            account.base.id.clone(),
        )?;
        Ok(Self {
            party,
            revision,
            account,
            facts,
            command,
            audit,
            result,
        })
    }

    /// 将根修订、事实差异、幂等结果与审计写入同一事务。
    async fn persist(mut self, db: &Database, session: &mut mongodb::ClientSession) -> Result<()> {
        db.party_revisions().create(&self.revision, session).await?;
        db.parties().update(&mut self.party, session).await?;
        db.customer_accounts().update(&mut self.account, session).await?;
        self.facts.persist(db, session).await?;
        db.customer_profile_commands()
            .create(&self.command, session)
            .await?;
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
            unified_credit_code: string_update(req.unified_credit_code.clone()),
            status: None,
        },
        actor,
    )?;
    party.stable.current_revision_id = Some(revision_id.to_string());
    let status = req.status.filter(|status| *status != account.stable.status);
    account.update(
        CustomerAccountUpdate {
            default_payment_term_id: string_update(req.default_payment_term_id.clone()),
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

/// 将可选字符串映射为保留、清空或设置意图。
fn string_update(value: Option<String>) -> FieldUpdate<String> {
    match value {
        Some(value) if value.trim().is_empty() => FieldUpdate::Clear,
        Some(value) => FieldUpdate::Set(value),
        None => FieldUpdate::Unchanged,
    }
}

/// 返回 Party ID newtype。
fn party_id(party: &Party) -> PartyId {
    PartyId::new(party.base.id.clone())
}
