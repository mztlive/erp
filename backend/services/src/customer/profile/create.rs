//! 客户资料创建用例与事务载荷。

use database::{AccessControlExt, CustomerExt, NoTransaction, PartyExt, Transactional};
use entities::{
    customer::{
        AssignmentRole, CustomerAccount, CustomerAccountData, CustomerAccountId, CustomerAccountStatus,
        CustomerAssignment, CustomerAssignmentData, CustomerAssignmentId, CustomerProfileCommand,
        CustomerProfileOperation,
    },
    ids::{PartyId, PartyRevisionId},
    party::{Party, PartyData, PartyKind, PartyRevision, PartyRevisionData, PartyStatus},
};
use id_generator::next_id;
use mongodb::Database;

use crate::{
    audit::AuditActor,
    errors::{Error, Result},
};

use super::super::{CustomerProfileMutationView, SaveCustomerProfileRequest};
use super::{
    facts::PartyFacts,
    idempotency::{
        command_view, profile_command, replay_command, request_fingerprint, ProfileCommandInput,
        TransactionResolutionContext,
    },
    numbering::business_no,
    CustomerProfileService,
};

impl CustomerProfileService {
    /// 原子创建 Party、客户角色、首条 OWNER 与首批资料事实。
    ///
    /// 首条 OWNER 固定为当前创建人；请求体中的 `owner_user_id` 即使提交也会被忽略。
    ///
    /// # 参数
    /// * `req` - 创建客户资料命令
    /// * `actor` - 已通过鉴权的审计操作人；其账号 ID 写入首条 OWNER 归属
    ///
    /// # 返回
    /// 返回创建命令的稳定结果视图。
    ///
    /// # 错误
    /// 输入非法、创建人账号不存在或已停用、身份重复、敏感值加密失败或事务失败时返回错误。
    pub async fn create(
        &self,
        req: SaveCustomerProfileRequest,
        actor: &AuditActor,
    ) -> Result<CustomerProfileMutationView> {
        req.validate_protocol()?;
        req.validate_structure(CustomerProfileOperation::Create)?;
        let fingerprint = request_fingerprint(&req)?;
        if let Some(command) = self.command_record(&req.idempotency_key).await? {
            return replay_command(command, "create", None, actor.id(), &fingerprint);
        }
        let owner_user_id = actor.id().to_string();
        self.ensure_user_exists(&owner_user_id).await?;
        let prepared = self.prepare_create(req, owner_user_id, fingerprint.clone(), actor)?;
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
                operation: "create",
                customer_id: None,
                initiated_by: actor.id(),
                fingerprint: &fingerprint,
            },
        )
        .await
    }

    /// 构造完整创建事务载荷。
    fn prepare_create(
        &self,
        req: SaveCustomerProfileRequest,
        owner_user_id: String,
        fingerprint: String,
        actor: &AuditActor,
    ) -> Result<PreparedCreate> {
        let party_id = PartyId::new(next_id());
        let customer_id = CustomerAccountId::new(next_id());
        let revision_id = PartyRevisionId::new(next_id());
        let (party, revision) = create_party(&req, &party_id, &revision_id, actor.id())?;
        let account = create_customer(&req, &customer_id, &party_id, actor.id())?;
        let assignment = create_owner(&req, &customer_id, owner_user_id)?;
        let facts = self.create_facts(&req, &party_id, actor.id())?;
        PreparedCreate::new(
            PreparedCreateParts {
                party,
                revision,
                account,
                assignment,
                facts,
            },
            req,
            fingerprint,
            actor,
        )
    }

    /// 校验负责人账号存在。
    async fn ensure_user_exists(&self, user_id: &str) -> Result<()> {
        let account = self
            .db
            .accounts()
            .find_account(user_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("负责销售账号不存在".to_string()))?;
        account
            .ensure_can_login()
            .map_err(|error| Error::BusinessLogicError(error.to_string()))
    }
}

/// 创建事务载荷。
struct PreparedCreateParts {
    party: Party,
    revision: PartyRevision,
    account: CustomerAccount,
    assignment: CustomerAssignment,
    facts: PartyFacts,
}

/// 创建事务载荷及其幂等、审计事实。
struct PreparedCreate {
    party: Party,
    revision: PartyRevision,
    account: CustomerAccount,
    assignment: CustomerAssignment,
    facts: PartyFacts,
    command: CustomerProfileCommand,
    audit: entities::AuditLog,
    result: CustomerProfileMutationView,
}

impl PreparedCreate {
    /// 构造命令、审计及稳定结果。
    fn new(
        parts: PreparedCreateParts,
        req: SaveCustomerProfileRequest,
        fingerprint: String,
        actor: &AuditActor,
    ) -> Result<Self> {
        let PreparedCreateParts {
            party,
            revision,
            account,
            assignment,
            facts,
        } = parts;
        let command = profile_command(
            &party,
            &revision,
            &account,
            ProfileCommandInput {
                operation: "create",
                req,
                fingerprint,
                initiated_by: actor.id(),
                customer_version: account.base.version,
                party_version: party.base.version,
            },
        )?;
        let result = command_view(command.clone());
        let audit = actor.clone().resource_log(
            "customer_profile.create",
            "customer_profile",
            account.base.id.clone(),
        )?;
        Ok(Self {
            party,
            revision,
            account,
            assignment,
            facts,
            command,
            audit,
            result,
        })
    }

    /// 将完整客户资料与幂等结果写入同一事务。
    async fn persist(self, db: &Database, session: &mut mongodb::ClientSession) -> Result<()> {
        db.party_revisions().create(&self.revision, session).await?;
        db.parties().create(&self.party, session).await?;
        db.customer_accounts().create(&self.account, session).await?;
        db.customer_assignments()
            .create(&self.assignment, session)
            .await?;
        self.facts.persist(db, session).await?;
        db.customer_profile_commands()
            .create(&self.command, session)
            .await?;
        db.audit_logs().create(&self.audit, session).await?;
        Ok(())
    }
}

/// 创建 Party 与首个名称修订。
fn create_party(
    req: &SaveCustomerProfileRequest,
    party_id: &PartyId,
    revision_id: &PartyRevisionId,
    actor: &str,
) -> Result<(Party, PartyRevision)> {
    let mut party = Party::new(
        party_id.clone(),
        PartyData {
            party_no: business_no("P"),
            party_kind: PartyKind::Enterprise,
            unified_credit_code: req.unified_credit_code.clone(),
            status: PartyStatus::Active,
        },
        actor,
    )?;
    party.stable.current_revision_id = Some(revision_id.to_string());
    let revision = PartyRevision::new(
        revision_id.clone(),
        PartyRevisionData {
            party_id: party_id.clone(),
            revision_no: 1,
            legal_name: req.legal_name.clone(),
            short_name: req.short_name.clone(),
            change_reason: req.change_reason.clone(),
        },
    )?;
    Ok((party, revision))
}

/// 创建客户角色。
fn create_customer(
    req: &SaveCustomerProfileRequest,
    customer_id: &CustomerAccountId,
    party_id: &PartyId,
    actor: &str,
) -> Result<CustomerAccount> {
    Ok(CustomerAccount::new(
        customer_id.clone(),
        CustomerAccountData {
            party_id: party_id.clone(),
            customer_no: business_no("KH"),
            default_payment_term_id: req.default_payment_term_id.clone(),
            status: req.status.unwrap_or(CustomerAccountStatus::Active),
        },
        actor,
    )?)
}

/// 创建首条 OWNER 归属。
fn create_owner(
    req: &SaveCustomerProfileRequest,
    customer_id: &CustomerAccountId,
    owner_user_id: String,
) -> Result<CustomerAssignment> {
    Ok(CustomerAssignment::new(
        CustomerAssignmentId::new(next_id()),
        CustomerAssignmentData {
            customer_id: customer_id.clone(),
            user_id: owner_user_id,
            assignment_role: AssignmentRole::Owner,
            valid_from: req.effective_from,
            valid_to: None,
            change_reason: req.change_reason.clone(),
        },
    )?)
}
