use erp_core::field_update::FieldUpdate;
use id_generator::next_id;
use persistence_core::Executor;

use super::{ACTOR, BuiltinCompany, PartyService};
use crate::entity::party::company::CompanyProfile;
use crate::entity::party::{Party, PartyId, PartyRevision, PartyRevisionData, PartyRevisionId, PartyUpdate};
use crate::repository::PartyExt;
use crate::repository::prelude::*;
use crate::{Error, Result};

impl PartyService {
    async fn builtin_candidate(
        &self,
        seed: &BuiltinCompany,
        executor: &mut dyn Executor,
    ) -> Result<Option<Party>> {
        let mut candidates = Vec::new();
        if let Some(party) = self
            .db
            .parties()
            .find_by_unified_credit_code_including_deleted(&seed.credit_code, executor)
            .await?
        {
            candidates.push(party);
        }
        let names = self.db.party().exact_current_party_ids_by_name(&seed.legal_name, executor).await?;
        for id in names {
            if let Some(party) = self.db.parties().find_by_id(&id, executor).await? {
                candidates.push(party);
            }
        }
        seed.identity(candidates)
    }

    pub(super) async fn sync_company_identity(
        &self,
        seed: &BuiltinCompany,
        executor: &mut dyn Executor,
    ) -> Result<(Party, bool)> {
        let existing = self.builtin_candidate(seed, executor).await?;
        let is_new = existing.is_none();
        let mut party = match existing {
            Some(party) => party,
            None => seed.new_party(next_id())?,
        };
        let current = match &party.stable.current_revision_id {
            Some(id) => Some(
                self.db
                    .party_revisions()
                    .find_by_id(id, executor)
                    .await?
                    .ok_or_else(|| Error::ConflictError("内建公司当前名称修订缺失".into()))?,
            ),
            None => None,
        };
        if let Some(revision) = &current {
            party.current_revision(std::slice::from_ref(revision))?;
        }
        let Some((profile, name_changed)) = seed.change(&party, current.as_ref())? else {
            return Ok((party, false));
        };
        party.company_profile = Some(profile.clone());
        party.update(
            PartyUpdate { unified_credit_code: FieldUpdate::Set(seed.credit_code.clone()), status: None },
            ACTOR,
        )?;
        if name_changed {
            self.append_builtin_name(&mut party, profile, executor).await?;
        }
        if is_new {
            self.db.parties().create(&party, executor).await?;
        } else {
            self.db.parties().update(&mut party, executor).await?;
        }
        Ok((party, true))
    }
    async fn append_builtin_name(
        &self,
        party: &mut Party,
        profile: CompanyProfile,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let party_id = PartyId::new(&party.base.id);
        let revision = PartyRevision::new(
            PartyRevisionId::new(next_id()),
            PartyRevisionData {
                party_id: party_id.clone(),
                revision_no: self.db.party_revisions().next_revision_no(&party_id, executor).await?,
                legal_name: profile.legal_name,
                short_name: profile.short_name,
                change_reason: "同步内建公司资料".into(),
            },
        )?;
        self.db.party_revisions().create(&revision, executor).await?;
        party.stable.current_revision_id = Some(revision.base.id);
        Ok(())
    }
}
