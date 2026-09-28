//! 已生成的占位名称改成名录里的名称。再次生成时只改仍带「演示」字样的资料。

use application_core::AuditActor;
use erp_party::{PartyExt, UpdatePartyRequest};
use persistence_core::NoTransaction;

use super::DemoMasterDataService;
use crate::Result;
use crate::adapters::party_service;

impl DemoMasterDataService {
    pub(super) async fn rename_party_if_placeholder(
        &self,
        actor: &AuditActor,
        party_id: &str,
        legal_name: &str,
        short_name: &str,
    ) -> Result<()> {
        let Some(party) =
            self.db.parties().find_by_id_including_deleted(party_id, &mut NoTransaction).await?
        else {
            return Ok(());
        };
        let Some(revision_id) = party.stable.current_revision_id else {
            return Ok(());
        };
        let Some(revision) =
            self.db.party_revisions().find_by_id_including_deleted(&revision_id, &mut NoTransaction).await?
        else {
            return Ok(());
        };
        if !revision.legal_name.contains('演') {
            return Ok(());
        }
        party_service(self.db.clone())
            .update_party(
                party_id,
                UpdatePartyRequest {
                    version: party.base.version,
                    status: None,
                    unified_credit_code: None,
                    legal_name: legal_name.to_string(),
                    short_name: Some(short_name.to_string()),
                    change_reason: "更新资料名称".to_string(),
                },
                actor,
            )
            .await?;
        Ok(())
    }
}
