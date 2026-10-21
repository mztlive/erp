//! 客户资料创建用例与事务载荷。

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_core::ids::{PartyId, PartyRevisionId};
use erp_customer::{
    AssignmentRole, CustomerAccount, CustomerAccountData, CustomerAccountId, CustomerAccountStatus,
    CustomerAssignment, CustomerAssignmentData, CustomerAssignmentId, CustomerExt, CustomerProfileCommand,
    CustomerProfileCommandResultData, CustomerProfileMutationView, CustomerProfileOperation,
    CustomerProfileReplayContext, SaveCustomerProfileRequest,
};
use erp_identity::AccessControlExt;
use erp_identity::repository::prelude::*;
use erp_party::{Party, PartyData, PartyExt, PartyKind, PartyRevision, PartyRevisionData, PartyStatus};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::CustomerProfileService;
use super::facts::PartyFacts;
use super::idempotency::{checked_command_view, command_view};
use super::identity::CreationIdentity;
use super::numbering::business_no;
use crate::adapters::customer_access;
use crate::audit::persist_log;
use crate::{Error, Result};

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
    /// 输入非法、无创建范围、创建人账号不存在或已停用、身份重复、敏感值加密失败或事务失败时返回错误。
    ///
    /// # 关键业务约束
    /// 写入事务内必须调用 `CustomerAccess.require_create`；handler 事前检查不能代替。
    pub async fn create(
        &self,
        req: SaveCustomerProfileRequest,
        actor: &AuditActor,
    ) -> Result<CustomerProfileMutationView> {
        req.validate_protocol()?;
        req.validate_structure(CustomerProfileOperation::Create)?;
        let replay = req.replay_context(CustomerProfileOperation::Create, None, actor.id())?;
        if let Some(command) = self.command_record(replay.idempotency_key()).await? {
            return checked_command_view(command, &replay);
        }
        let transaction = self.commit_create(req, replay.clone(), actor).await;
        self.resolve_transaction(transaction, &replay).await
    }

    /// 在同一写入事务内重验创建范围并持久化资料。
    ///
    /// # 参数
    /// * `req` / `replay` - 创建请求及幂等上下文
    /// * `actor` - 将写入首条主责的操作人
    ///
    /// # 返回
    /// 事务成功时返回 `Ok`；失败时返回原事务错误供幂等恢复。
    ///
    /// # 错误
    /// 无创建资格、范围为空或写入冲突时拒绝。
    ///
    /// # 关键业务约束
    /// 必须在原领域事务调用 `require_create`，handler 事前检查不能代替。
    async fn commit_create(
        &self,
        req: SaveCustomerProfileRequest,
        replay: CustomerProfileReplayContext,
        actor: &AuditActor,
    ) -> Result<CustomerProfileMutationView> {
        let rbac = self.require_rbac()?.clone();
        let actor = actor.clone();
        let this = self.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    customer_access(this.db.clone(), rbac).require_create(&actor, executor).await?;
                    this.ensure_user_exists(actor.id(), executor).await?;
                    let identity = this.creation_identity(&req, &actor, executor).await?;
                    let prepared = this.prepare_create(req, identity, replay, &actor)?;
                    let result = prepared.result.clone();
                    prepared.persist(&this.db, executor).await?;
                    Ok(result)
                })
            })
            .await
    }

    /// 构造完整创建事务载荷；不写入、不重验范围。
    ///
    /// # 参数
    /// * `req` - 创建命令
    /// * `identity` - 已在当前事务内查重的新建或复用主体
    /// * `replay` - 幂等上下文
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回可在事务内持久化的创建载荷。
    ///
    /// # 错误
    /// 身份、客户或归属构造失败时拒绝。
    ///
    /// # 关键业务约束
    /// 范围重验必须留在 `commit_create` 的同一写入事务内。
    fn prepare_create(
        &self,
        req: SaveCustomerProfileRequest,
        identity: CreationIdentity,
        replay: CustomerProfileReplayContext,
        actor: &AuditActor,
    ) -> Result<PreparedCreate> {
        let CreationIdentity { party, revision, reused } = identity;
        let party_id = PartyId::new(&party.base.id);
        let customer_id = CustomerAccountId::new(next_id());
        let account = create_customer(&req, &customer_id, &party_id, actor.id())?;
        let assignment = create_owner(&req, &customer_id, actor.id().to_string())?;
        let facts = if reused { None } else { Some(self.create_facts(&req, &party_id, actor.id())?) };
        PreparedCreate::new(
            PreparedCreateParts { party, revision, account, assignment, facts, reused },
            req,
            replay,
            actor,
        )
    }

    /// 校验负责人账号存在。
    async fn ensure_user_exists(&self, user_id: &str, executor: &mut dyn Executor) -> Result<()> {
        let account = self
            .db
            .accounts()
            .find_account(user_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("负责销售账号不存在".to_string()))?;
        account.ensure_can_login().map_err(|error| Error::BusinessLogicError(error.to_string()))
    }
}

/// 创建事务载荷。
struct PreparedCreateParts {
    party: Party,
    revision: PartyRevision,
    account: CustomerAccount,
    assignment: CustomerAssignment,
    facts: Option<PartyFacts>,
    reused: bool,
}

/// 创建事务载荷及其幂等、审计事实。
struct PreparedCreate {
    party: Party,
    revision: PartyRevision,
    account: CustomerAccount,
    assignment: CustomerAssignment,
    facts: Option<PartyFacts>,
    reused: bool,
    command: CustomerProfileCommand,
    audit: erp_audit::AuditLog,
    result: CustomerProfileMutationView,
}

impl PreparedCreate {
    /// 构造命令、审计及稳定结果。
    fn new(
        parts: PreparedCreateParts,
        req: SaveCustomerProfileRequest,
        replay: CustomerProfileReplayContext,
        actor: &AuditActor,
    ) -> Result<Self> {
        let PreparedCreateParts { party, revision, account, assignment, facts, reused } = parts;
        let command = CustomerProfileCommand::record_success(
            next_id(),
            &replay,
            CustomerProfileCommandResultData {
                customer_id: account.base.id.clone(),
                customer_no: account.customer_no.clone(),
                party_id: party.base.id.clone(),
                revision_id: revision.base.id.clone(),
                revision_no: revision.revision.revision_no,
                customer_version: account.base.version,
                party_version: party.base.version + u64::from(reused),
                effective_from: req.effective_from,
                change_reason: req.change_reason,
            },
        )?;
        let result = command_view(command.clone());
        let audit = actor.clone().resource_log(
            "customer_profile.create",
            "customer_profile",
            account.base.id.clone(),
        )?;
        Ok(Self { party, revision, account, assignment, facts, reused, command, audit, result })
    }

    /// 将完整客户资料与幂等结果写入同一事务。
    async fn persist(mut self, db: &Database, executor: &mut dyn Executor) -> Result<()> {
        if self.reused {
            // 校验被复用主体仍是当前版本，防止并发停用或改名后继续按旧身份建档。
            db.parties().update(&mut self.party, executor).await?;
        } else {
            db.party_revisions().create(&self.revision, executor).await?;
            db.parties().create(&self.party, executor).await?;
        }
        db.customer_accounts().create(&self.account, executor).await?;
        db.customer_assignments().create(&self.assignment, executor).await?;
        if let Some(facts) = self.facts {
            facts.persist(db, executor).await?;
        }
        db.customer_profile_commands().create(&self.command, executor).await?;
        persist_log(db, &self.audit, executor).await?;
        Ok(())
    }
}

/// 创建 Party 与首个名称修订。
/// # 参数
/// * `req` / `party_id` / `revision_id` / `actor` - 请求、新主体和修订 ID 及操作人。
/// # 返回
/// 返回尚未写入的新主体和首个名称修订。
/// # 错误
/// 名称、信用代码等字段非法时返回领域错误。
pub(super) fn create_party(
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use erp_core::AccountKind;
    use erp_party::SensitiveDataCodec;
    use mongodb::Client;
    use serde_json::json;

    use super::*;

    /// 仅调用纯载荷准备，不执行数据库读写。
    #[tokio::test]
    async fn reuse_creates_only_customer_role_and_preserves_existing_identity() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap().database("unit_test");
        let service =
            CustomerProfileService::new(db, Arc::new(SensitiveDataCodec::from_secret(b"reuse-unit-test")));
        let actor = AuditActor::new("sales-a".into(), "sales".into(), AccountKind::Admin);
        let mut req: SaveCustomerProfileRequest = serde_json::from_value(json!({
            "idempotency_key": "reuse-supplier-party", "reuse_existing_party": true,
            "legal_name": "已有供应商", "short_name": "本次输入的简称",
            "default_payment_term_id": "POSTPAY_NET30",
            "contacts": [{"contact_name": "本次输入的联系人", "mobile": "13800000000", "is_default": true}],
            "effective_from": "2026-10-21", "change_reason": "建立客户角色"
        }))
        .unwrap();
        let (mut party, mut revision) = create_party(
            &req,
            &PartyId::new("supplier-party"),
            &PartyRevisionId::new("existing-revision"),
            "buyer",
        )
        .unwrap();
        party.base.version = 7;
        party.unified_credit_code = Some("91310000123456789A".into());
        revision.revision.revision_no = 4;
        revision.short_name = Some("原有简称".into());
        let replay = req.replay_context(CustomerProfileOperation::Create, None, actor.id()).unwrap();
        let prepared = service
            .prepare_create(
                req.clone(),
                CreationIdentity { party: party.clone(), revision: revision.clone(), reused: true },
                replay,
                &actor,
            )
            .unwrap();
        assert_eq!(prepared.party, party);
        assert_eq!(prepared.revision, revision);
        assert!(prepared.reused);
        assert!(prepared.facts.is_none());
        assert_eq!(prepared.account.party_id, PartyId::new("supplier-party"));
        assert_eq!(prepared.account.default_payment_term_id.as_deref(), Some("POSTPAY_NET30"));
        assert_eq!(prepared.assignment.user_id, "sales-a");
        assert_eq!(prepared.result.party_version, 8);
        assert_eq!(prepared.result.revision_no, 4);
        assert_eq!(prepared.result.revision_id, "existing-revision");

        req.reuse_existing_party = false;
        let replay = req.replay_context(CustomerProfileOperation::Create, None, actor.id()).unwrap();
        let created = service
            .prepare_create(req, CreationIdentity { party, revision, reused: false }, replay, &actor)
            .unwrap();
        assert!(created.facts.is_some());
        assert_eq!(created.result.party_version, 7);
    }
}
