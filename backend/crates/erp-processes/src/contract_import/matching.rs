//! 归档事务内精确匹配当前主数据并冻结身份快照。
use application_core::AuditActor;
use erp_contract::ContractExt;
use erp_contract::entity::recognition::{
    ContractField, ContractImport, ContractValues, MatchedIdentity, RecognitionProof, match_identity,
};
use erp_core::ids::PartyId;
use erp_party::PartyExt;
use erp_party::repository::exact_identity::{ContractIdentityCandidate, exact_parties};
use erp_party::repository::prelude::*;
use persistence_core::Executor;

use super::ContractImportProcess;
use crate::{Error, Result};

impl ContractImportProcess {
    /// 按已确认身份匹配或建档，固定合同默认结算为我方主体。
    /// # 参数
    /// * `task` / `values` / `allow_create` - 本人任务、确认内容及建档选择。
    /// * `actor` / `executor` - 当前操作人及归档事务。
    /// # 返回
    /// 包含原识别依据与当前主数据版本的归档证明。
    /// # 错误
    /// 主体冲突、客户不一致、权限或数据库失败。
    pub(super) async fn match_all(
        &self,
        task: &ContractImport,
        values: &ContractValues,
        allow_create: bool,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<RecognitionProof> {
        let company = self.company(values, executor).await?;
        let settlement = self.settlement(task, &company, executor).await?;
        let (customer, customer_party) = self.customer(task, values, allow_create, actor, executor).await?;
        if task.command.expected_customer_id.as_ref().is_some_and(|id| id != &customer.base.id) {
            return Err(Error::BusinessLogicError(
                "合同对方主体与当前销售单客户不一致，请上传同一客户的合同".into(),
            ));
        }
        Ok(RecognitionProof {
            confirmed_fields: None,
            import_id: task.base.id.clone(),
            source_sha256: task.source.sha256.clone(),
            customer_id: customer.base.id,
            customer_version: customer.base.version,
            customer_party,
            company,
            settlement,
            extraction: task
                .extraction
                .clone()
                .ok_or_else(|| Error::ValidationError("缺少识别结果".into()))?,
        })
    }

    async fn company(&self, values: &ContractValues, executor: &mut dyn Executor) -> Result<MatchedIdentity> {
        let name = values
            .required(ContractField::CompanyName)
            .map_err(|error| Error::ValidationError(error.message))?;
        let credit = values
            .fields
            .get(&ContractField::CompanyCreditCode)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(str::to_ascii_uppercase);
        let candidates = exact_parties(&self.db, name, true, credit.as_deref(), executor)
            .await?
            .into_iter()
            .map(|party| MatchedIdentity {
                id: party.base.id,
                version: party.base.version,
                revision_id: party.stable.current_revision_id,
                legal_name: party.company_profile.map(|profile| profile.legal_name).unwrap_or_default(),
                credit_code: party.unified_credit_code,
            })
            .collect();
        match_identity(name, credit.as_deref(), candidates).map_err(|error| {
            let message = if error.code == "MASTER_NOT_FOUND" {
                "未匹配到启用的我方公司，请选择已维护的我方签约主体".into()
            } else {
                format!("我方签约主体：{}", error.message)
            };
            Error::BusinessLogicError(message)
        })
    }

    async fn settlement(
        &self,
        task: &ContractImport,
        company: &MatchedIdentity,
        executor: &mut dyn Executor,
    ) -> Result<MatchedIdentity> {
        let Some(target) = &task.command.revision_target else {
            return Ok(company.clone());
        };
        let original = self
            .db
            .contracts()
            .find_by_id(&target.contract_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("原合同不存在".into()))?;
        if original.settlement_party_id.as_ref() == company.id {
            return Ok(company.clone());
        }
        self.party_identity(&original.settlement_party_id, executor).await
    }

    async fn party_identity(
        &self,
        party_id: &PartyId,
        executor: &mut dyn Executor,
    ) -> Result<MatchedIdentity> {
        let party = self
            .db
            .parties()
            .find_party(party_id, executor)
            .await?
            .filter(|party| party.is_active() && !party.base.is_deleted())
            .ok_or_else(|| Error::BusinessLogicError("原合同结算主体不可用，请先维护主体".into()))?;
        let revision_id = party
            .stable
            .current_revision_id
            .as_ref()
            .ok_or_else(|| Error::BusinessLogicError("原合同结算主体缺少当前资料".into()))?;
        let revision = self
            .db
            .party_revisions()
            .find_by_id(revision_id, executor)
            .await?
            .filter(|revision| {
                revision.party_id == PartyId::new(party.base.id.clone()) && !revision.base.is_deleted()
            })
            .ok_or_else(|| Error::BusinessLogicError("原合同结算主体资料不可用".into()))?;
        Ok(MatchedIdentity {
            id: party.base.id,
            version: party.base.version,
            revision_id: Some(revision.base.id),
            legal_name: revision.legal_name,
            credit_code: party.unified_credit_code,
        })
    }
}

/// 将精确候选转换为当前身份；保留停用、删除和资料损坏的拒绝语义。
/// # 参数
/// * `name` / `credit` / `candidates` - 确认身份及当前候选。
/// # 返回
/// 唯一可用身份；只有零候选返回 `None`。
/// # 错误
/// 身份冲突、歧义或当前主数据不可用。
pub(super) fn identity_from_candidates(
    name: &str,
    credit: Option<&str>,
    candidates: &[ContractIdentityCandidate],
) -> Result<Option<MatchedIdentity>> {
    if candidates.is_empty() {
        return Ok(None);
    }
    if candidates.iter().any(|candidate| candidate.legal_name.is_none()) {
        return Err(Error::BusinessLogicError("已有主体缺少当前身份资料，请先修复，不能重复建档".into()));
    }
    let matches = candidates
        .iter()
        .map(|candidate| MatchedIdentity {
            id: candidate.party.base.id.clone(),
            version: candidate.party.base.version,
            revision_id: candidate.party.stable.current_revision_id.clone(),
            legal_name: candidate.legal_name.clone().unwrap_or_default(),
            credit_code: candidate.party.unified_credit_code.clone(),
        })
        .collect();
    let identity = match_identity(name, credit, matches)
        .map_err(|error| Error::BusinessLogicError(format!("对方签约主体：{}", error.message)))?;
    if candidates.iter().any(|candidate| {
        candidate.party.base.id == identity.id
            && (!candidate.party.is_active() || candidate.party.base.is_deleted())
    }) {
        return Err(Error::BusinessLogicError("已有主体已停用或删除，请先恢复，不能重复建档".into()));
    }
    Ok(Some(identity))
}

#[cfg(test)]
mod tests {
    use erp_party::{Party, PartyData, PartyKind, PartyStatus};

    use super::*;

    fn candidate(id: &str, name: Option<&str>, credit: &str) -> ContractIdentityCandidate {
        ContractIdentityCandidate {
            party: Party::new(
                PartyId::new(id),
                PartyData {
                    party_no: format!("P-{id}"),
                    party_kind: PartyKind::Enterprise,
                    unified_credit_code: Some(credit.into()),
                    status: PartyStatus::Active,
                },
                "owner",
            )
            .unwrap(),
            legal_name: name.map(str::to_owned),
        }
    }

    #[test]
    fn identity_resolves_credit_among_same_names_and_preserves_actual_identity() {
        let candidates = [
            candidate("a", Some("客户公司"), "91310000ABCDEF1234"),
            candidate("b", Some("客户公司"), "91310000ABCDEF5678"),
        ];
        let resolved =
            identity_from_candidates("客户公司", Some("91310000abcdef5678"), &candidates).unwrap().unwrap();
        assert_eq!(resolved.id, "b");
        assert!(identity_from_candidates("客户公司", None, &candidates).is_err());
        assert!(identity_from_candidates("另一个公司", Some("91310000ABCDEF5678"), &candidates).is_err());
        assert!(identity_from_candidates("客户公司", None, &[]).unwrap().is_none());
    }

    #[test]
    fn unavailable_or_broken_existing_identity_never_becomes_missing() {
        for deleted in [false, true] {
            let mut existing = candidate("a", Some("客户公司"), "91310000ABCDEF1234");
            if deleted {
                existing.party.base.deleted_at = 1;
            } else {
                existing.party.stable.status = PartyStatus::Disabled;
            }
            assert!(identity_from_candidates("客户公司", None, &[existing]).is_err());
        }
        let broken = candidate("a", None, "91310000ABCDEF1234");
        assert!(identity_from_candidates("客户公司", Some("91310000ABCDEF1234"), &[broken]).is_err());
    }
}
