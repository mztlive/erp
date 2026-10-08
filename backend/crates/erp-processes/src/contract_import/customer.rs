//! 用户确认后在合同归档事务内补建客户；已有身份和资料不得覆盖。
use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_contract::entity::recognition::{
    ContractField, ContractImport, ContractValues, ImportFailure, MatchedIdentity,
};
use erp_core::common::time::BusinessDate;
use erp_core::ids::{CustomerAccountId, CustomerAssignmentId, PartyId, PartyRevisionId};
use erp_customer::repository::prelude::*;
use erp_customer::{
    AssignmentRole, CustomerAccount, CustomerAccountData, CustomerAccountStatus, CustomerAssignment,
    CustomerAssignmentData, CustomerExt,
};
use erp_identity::AccessControlExt;
use erp_identity::repository::prelude::*;
use erp_party::repository::exact_identity::contract_identity_candidates;
use erp_party::{Party, PartyData, PartyExt, PartyKind, PartyRevision, PartyRevisionData, PartyStatus};
use id_generator::next_id;
use persistence_core::Executor;

use super::ContractImportProcess;
use super::matching::identity_from_candidates;
use crate::adapters::customer_access;
use crate::audit::persist_log;
use crate::{Error, Result};

impl ContractImportProcess {
    /// 在归档事务内取得客户身份；缺失时只按明确确认建立身份。
    /// # 参数
    /// * `task` / `values` / `allow_create` - 本人任务、确认身份和建档选择。
    /// * `actor` / `executor` - 当前操作人及归档事务。
    /// # 返回
    /// 当前客户角色及企业身份。
    /// # 错误
    /// 主体冲突、停用、缺失确认、权限不足或数据库失败。
    pub(super) async fn customer(
        &self,
        task: &ContractImport,
        values: &ContractValues,
        allow_create: bool,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(CustomerAccount, MatchedIdentity)> {
        let name = values.required(ContractField::CustomerName).map_err(invalid)?;
        let credit = values
            .fields
            .get(&ContractField::CustomerCreditCode)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(str::to_ascii_uppercase);
        let candidates = contract_identity_candidates(&self.db, name, credit.as_deref(), executor).await?;
        let identity = identity_from_candidates(name, credit.as_deref(), &candidates)?;
        if let Some(identity) = identity {
            let existing = self
                .db
                .customer_accounts()
                .find_by_party_including_deleted(&PartyId::new(identity.id.clone()), executor)
                .await?;
            if let Some(customer) = existing {
                if !customer.is_active() {
                    return Err(Error::BusinessLogicError("对应客户已停用，请先启用客户资料".into()));
                }
                if customer.base.is_deleted() {
                    return Err(Error::BusinessLogicError("对应客户已删除，请先恢复客户资料".into()));
                }
                return Ok((customer, identity));
            }
            self.require_customer_creation(task, allow_create, actor, executor).await?;
            let customer = self.create_customer(&identity.id, actor, executor).await?;
            return Ok((customer, identity));
        }
        self.require_customer_creation(task, allow_create, actor, executor).await?;
        self.new_customer(values, actor, executor).await
    }

    async fn new_customer(
        &self,
        values: &ContractValues,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(CustomerAccount, MatchedIdentity)> {
        let (party, revision) = new_party(values, actor.id())?;
        self.db.party_revisions().create(&revision, executor).await?;
        self.db.parties().create(&party, executor).await?;
        persist_log(
            &self.db,
            &actor.clone().resource_log("party.create", "party", party.base.id.clone())?,
            executor,
        )
        .await?;
        let customer = self.create_customer(&party.base.id, actor, executor).await?;
        Ok((
            customer,
            MatchedIdentity {
                id: party.base.id,
                version: party.base.version,
                revision_id: Some(revision.base.id),
                legal_name: revision.legal_name,
                credit_code: party.unified_credit_code,
            },
        ))
    }

    async fn require_customer_creation(
        &self,
        task: &ContractImport,
        allow_create: bool,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if !allow_create {
            return Err(Error::BusinessLogicError("未找到对应客户，请核对或确认创建客户后归档".into()));
        }
        if task.command.expected_customer_id.is_some() || task.command.revision_target.is_some() {
            return Err(Error::BusinessLogicError("当前合同必须关联原客户，不能创建其它客户".into()));
        }
        customer_access(self.db.clone(), self.rbac.clone()).require_create(actor, executor).await?;
        self.db
            .accounts()
            .find_account(actor.id(), executor)
            .await?
            .ok_or_else(|| Error::NotFound("客户负责人账号不存在".into()))?
            .ensure_can_login()
            .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
        Ok(())
    }

    async fn create_customer(
        &self,
        party_id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<CustomerAccount> {
        let (customer, owner) = customer_records(party_id, actor.id())?;
        self.db.customer_accounts().create(&customer, executor).await?;
        self.db.customer_assignments().create(&owner, executor).await?;
        persist_log(
            &self.db,
            &actor.clone().resource_log(
                "customer_profile.create",
                "customer_profile",
                customer.base.id.clone(),
            )?,
            executor,
        )
        .await?;
        Ok(customer)
    }
}

fn customer_records(party_id: &str, actor: &str) -> Result<(CustomerAccount, CustomerAssignment)> {
    let customer = CustomerAccount::new(
        CustomerAccountId::new(next_id()),
        CustomerAccountData {
            party_id: PartyId::new(party_id),
            customer_no: format!("KH-{}", next_id()),
            default_payment_term_id: None,
            status: CustomerAccountStatus::Active,
        },
        actor,
    )?;
    let owner = CustomerAssignment::new(
        CustomerAssignmentId::new(next_id()),
        CustomerAssignmentData {
            customer_id: CustomerAccountId::new(customer.base.id.clone()),
            user_id: actor.into(),
            assignment_role: AssignmentRole::Owner,
            valid_from: BusinessDate::today(),
            valid_to: None,
            change_reason: "合同导入确认建档".into(),
        },
    )?;
    Ok((customer, owner))
}

fn new_party(values: &ContractValues, actor: &str) -> Result<(Party, PartyRevision)> {
    let party_id = PartyId::new(next_id());
    let revision_id = PartyRevisionId::new(next_id());
    let mut party = Party::new(
        party_id.clone(),
        PartyData {
            party_no: format!("P-{}", next_id()),
            party_kind: PartyKind::Enterprise,
            unified_credit_code: Some(
                values
                    .required(ContractField::CustomerCreditCode)
                    .map_err(|_| {
                        Error::ValidationError("新建企业客户需确认统一社会信用代码，防止重复建档".into())
                    })?
                    .into(),
            ),
            status: PartyStatus::Active,
        },
        actor,
    )?;
    party.stable.current_revision_id = Some(revision_id.to_string());
    let revision = PartyRevision::new(
        revision_id,
        PartyRevisionData {
            party_id,
            revision_no: 1,
            legal_name: values.required(ContractField::CustomerName).map_err(invalid)?.into(),
            short_name: None,
            change_reason: "合同导入确认建档".into(),
        },
    )?;
    Ok((party, revision))
}

fn invalid(error: ImportFailure) -> Error {
    Error::ValidationError(error.message)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn customer_records_owns_customer_by_actor_without_contract_default_terms() {
        let (customer, owner) = customer_records("existing-party", "sales-owner").unwrap();
        assert_eq!(customer.party_id.as_ref(), "existing-party");
        assert!(customer.default_payment_term_id.is_none());
        assert!(customer.is_active());
        assert_eq!(owner.customer_id.to_string(), customer.base.id);
        assert_eq!(owner.user_id, "sales-owner");
        assert_eq!(owner.assignment_role, AssignmentRole::Owner);
        assert_eq!(owner.valid_from, BusinessDate::today());
        assert!(owner.valid_to.is_none());
    }

    #[test]
    fn new_party_uses_only_confirmed_identity_and_does_not_set_company_role() {
        let values = ContractValues {
            fields: BTreeMap::from([
                (ContractField::CustomerName, " 客户有限公司 ".into()),
                (ContractField::CustomerCreditCode, " 91310000abcdef1234 ".into()),
            ]),
        };
        let (party, revision) = new_party(&values, "sales-owner").unwrap();
        assert_eq!(party.unified_credit_code.as_deref(), Some("91310000ABCDEF1234"));
        assert_eq!(party.stable.created_by, "sales-owner");
        assert!(party.company_profile.is_none());
        assert_eq!(revision.legal_name, "客户有限公司");
        assert_eq!(revision.party_id.to_string(), party.base.id);
        assert_eq!(party.stable.current_revision_id.as_deref(), Some(revision.base.id.as_str()));
    }

    #[test]
    fn new_party_rejects_missing_or_invalid_credit_code() {
        let mut values = ContractValues {
            fields: BTreeMap::from([(ContractField::CustomerName, "客户有限公司".into())]),
        };
        assert!(new_party(&values, "sales-owner").is_err());
        values.fields.insert(ContractField::CustomerCreditCode, "123".into());
        assert!(new_party(&values, "sales-owner").is_err());
    }
}
