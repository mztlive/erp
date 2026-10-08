//! 内建我方主体清单与确定性合并规则。
use serde::Deserialize;

use super::company::CompanyProfile;
use super::{Party, PartyData, PartyId, PartyKind, PartyRevision, PartyStatus};
use crate::{Error, Result};

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BuiltinCompany {
    pub legal_name: String,
    pub credit_code: String,
    pub address: String,
    pub telephone: String,
    pub bank_name: String,
    pub bank_account: Option<String>,
}

impl BuiltinCompany {
    /// 构造新企业；稳定编号由清单中的税号生成。
    pub(crate) fn new_party(&self, id: String) -> Result<Party> {
        Ok(Party::new(
            PartyId::new(id),
            PartyData {
                party_no: format!("COMPANY-{}", self.credit_code),
                party_kind: PartyKind::Enterprise,
                unified_credit_code: Some(self.credit_code.clone()),
                status: PartyStatus::Active,
            },
            "system:company-bootstrap",
        )?)
    }

    /// 读取编译时嵌入的权威清单。
    pub(crate) fn all() -> Result<Vec<Self>> {
        serde_json::from_str(include_str!("builtin_companies.json"))
            .map_err(|_| Error::ValidationError("内建公司清单格式错误".into()))
    }

    /// 保留简称和别名，将明确提供的名称与开户行合并到公司角色。
    pub(crate) fn profile(&self, party: &Party, revision: Option<&PartyRevision>) -> Result<CompanyProfile> {
        let short_name = party
            .company_profile
            .as_ref()
            .and_then(|p| p.short_name.clone())
            .or_else(|| revision.and_then(|r| r.short_name.clone()));
        let aliases = party.company_profile.as_ref().map(|p| p.aliases.clone()).unwrap_or_default();
        let mut profile = CompanyProfile::new(self.legal_name.clone(), short_name, aliases)?;
        profile.bank_name = Some(self.bank_name.clone());
        Ok(profile)
    }

    /// 计算公司资料变更；相同资料返回 None，不推进数据库版本。
    pub(crate) fn change(
        &self,
        party: &Party,
        current: Option<&PartyRevision>,
    ) -> Result<Option<(CompanyProfile, bool)>> {
        let profile = self.profile(party, current)?;
        let name_changed =
            current.is_none_or(|r| r.legal_name != profile.legal_name || r.short_name != profile.short_name);
        let changed = name_changed
            || party.company_profile.as_ref() != Some(&profile)
            || party.unified_credit_code.as_deref() != Some(self.credit_code.as_str());
        Ok(changed.then_some((profile, name_changed)))
    }

    /// 税号与完整名称必须收敛到同一可维护企业。
    pub(crate) fn identity(&self, mut candidates: Vec<Party>) -> Result<Option<Party>> {
        candidates.sort_by(|a, b| a.base.id.cmp(&b.base.id));
        candidates.dedup_by(|a, b| a.base.id == b.base.id);
        if candidates.len() > 1 {
            return Err(Error::ConflictError(format!(
                "内建公司 {} 的税号与名称指向不同主体",
                self.legal_name
            )));
        }
        let candidate = candidates.pop();
        if let Some(party) = &candidate
            && (party.base.deleted_at != 0 || party.party_kind != PartyKind::Enterprise)
        {
            return Err(Error::ConflictError(format!("内建公司 {} 命中已删除或非企业主体", self.legal_name)));
        }
        Ok(candidate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::party::{PartyData, PartyId, PartyRevisionData, PartyRevisionId, PartyStatus};

    fn party(id: &str) -> Party {
        Party::new(
            PartyId::new(id),
            PartyData {
                party_no: id.into(),
                party_kind: PartyKind::Enterprise,
                unified_credit_code: None,
                status: PartyStatus::Active,
            },
            "test",
        )
        .unwrap()
    }

    #[test]
    fn manifest_has_six_unique_companies_and_only_research_account_is_missing() {
        let companies = BuiltinCompany::all().unwrap();
        assert_eq!(companies.len(), 6);
        let mut codes = companies.iter().map(|c| &c.credit_code).collect::<Vec<_>>();
        codes.sort();
        codes.dedup();
        assert_eq!(codes.len(), 6);
        let missing = companies.iter().filter(|c| c.bank_account.is_none()).collect::<Vec<_>>();
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].legal_name, "广州福尚云科技研发有限公司");
        for company in &companies {
            let p = party("existing");
            assert_eq!(company.profile(&p, None).unwrap().legal_name, company.legal_name);
        }
    }

    #[test]
    fn identity_reuses_id_and_rejects_ambiguous_or_deleted_records() {
        let seed = &BuiltinCompany::all().unwrap()[0];
        assert!(seed.identity(vec![]).unwrap().is_none());
        let p = party("existing");
        assert_eq!(seed.identity(vec![p.clone(), p.clone()]).unwrap().unwrap().base.id, "existing");
        assert!(seed.identity(vec![p.clone(), party("other")]).is_err());
        let mut deleted = p;
        deleted.base.deleted_at = 1;
        assert!(seed.identity(vec![deleted]).is_err());
    }

    #[test]
    fn profile_update_preserves_short_name_aliases_and_is_repeatable() {
        let seed = &BuiltinCompany::all().unwrap()[0];
        let mut p = party("existing");
        p.company_profile =
            Some(CompanyProfile::new("旧名称".into(), Some("简称".into()), vec!["别名".into()]).unwrap());
        let profile = seed.profile(&p, None).unwrap();
        assert_eq!(profile.short_name.as_deref(), Some("简称"));
        assert_eq!(profile.aliases, vec!["别名"]);
        assert_eq!(profile.bank_name.as_deref(), Some(seed.bank_name.as_str()));
        p.company_profile = Some(profile.clone());
        assert_eq!(seed.profile(&p, None).unwrap(), profile);
    }
    #[test]
    fn company_change_is_noop_on_replay_and_does_not_rename_for_bank_only_updates() {
        let mut seed = BuiltinCompany::all().unwrap().remove(0);
        let mut p = seed.new_party("same-id".into()).unwrap();
        let (profile, name_changed) = seed.change(&p, None).unwrap().unwrap();
        assert!(name_changed);
        p.company_profile = Some(profile.clone());
        let revision = PartyRevision::new(
            PartyRevisionId::new("revision"),
            PartyRevisionData {
                party_id: PartyId::new(&p.base.id),
                revision_no: 1,
                legal_name: profile.legal_name,
                short_name: profile.short_name,
                change_reason: "初始".into(),
            },
        )
        .unwrap();
        assert!(seed.change(&p, Some(&revision)).unwrap().is_none());
        seed.bank_name = "新开户行".into();
        let (profile, name_changed) = seed.change(&p, Some(&revision)).unwrap().unwrap();
        assert!(!name_changed);
        assert_eq!(profile.bank_name.as_deref(), Some("新开户行"));
        p.company_profile = Some(profile);
        assert!(seed.change(&p, Some(&revision)).unwrap().is_none());
        seed.legal_name = "新公司名称".into();
        assert!(seed.change(&p, Some(&revision)).unwrap().unwrap().1);
        assert_eq!(p.base.id, "same-id");
    }
}
