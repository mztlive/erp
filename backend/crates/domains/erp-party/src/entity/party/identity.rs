//! 已有企业身份复用规则；复用不得改写名称、信用代码或从属资料。

use erp_core::{Error, Result};

use super::{Party, PartyId, PartyRevision};

impl Party {
    /// 校验已有主体能否按输入身份复用。
    ///
    /// # 参数
    /// * `revision` - 当前名称修订。
    /// * `name` / `credit` - 已规范化的请求名称及可选信用代码。
    /// # 返回
    /// 身份完整、有效且一致时成功；未提供信用代码时保留已有代码。
    /// # 错误
    /// 停用、删除、我方主体、修订错配、同名异码或同码异名时拒绝。
    pub fn ensure_reusable_identity(
        &self,
        revision: &PartyRevision,
        name: &str,
        credit: Option<&str>,
    ) -> Result<()> {
        if self.base.is_deleted() || !self.is_active() || self.company_profile.is_some() {
            return Err(Error::from("已有主体不可复用，请先核对其启停状态和主体类型"));
        }
        if revision.base.is_deleted()
            || revision.party_id != PartyId::new(&self.base.id)
            || self.stable.current_revision_id.as_deref() != Some(revision.base.id.as_str())
        {
            return Err(Error::from("已有主体的当前资料不完整，请先修复主体资料"));
        }
        if revision.legal_name != name
            || credit.is_some_and(|value| self.unified_credit_code.as_deref() != Some(value))
        {
            return Err(Error::from("名称或信用代码与已有主体不一致，请核对后再复用"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PartyData, PartyKind, PartyRevisionData, PartyRevisionId, PartyStatus};

    fn identity() -> (Party, PartyRevision) {
        let mut party = Party::new(
            PartyId::new("supplier-party"),
            PartyData {
                party_no: "P-1".into(),
                party_kind: PartyKind::Enterprise,
                unified_credit_code: Some("91310000123456789A".into()),
                status: PartyStatus::Active,
            },
            "admin",
        )
        .unwrap();
        party.stable.current_revision_id = Some("revision-1".into());
        let revision = PartyRevision::new(
            PartyRevisionId::new("revision-1"),
            PartyRevisionData {
                party_id: PartyId::new("supplier-party"),
                revision_no: 3,
                legal_name: "共享企业".into(),
                short_name: Some("共享".into()),
                change_reason: "已有供应商".into(),
            },
        )
        .unwrap();
        (party, revision)
    }

    #[test]
    fn reuse_preserves_identity_and_accepts_omitted_credit() {
        let (party, revision) = identity();
        let before = (party.clone(), revision.clone());
        assert!(party.ensure_reusable_identity(&revision, "共享企业", None).is_ok());
        assert!(party.ensure_reusable_identity(&revision, "共享企业", Some("91310000123456789A")).is_ok());
        assert_eq!((party, revision), before);
    }

    #[test]
    fn reuse_rejects_name_credit_and_current_revision_mismatch() {
        let (party, mut revision) = identity();
        assert!(party.ensure_reusable_identity(&revision, "其他企业", None).is_err());
        assert!(party.ensure_reusable_identity(&revision, "共享企业", Some("91310000123456789B")).is_err());
        revision.party_id = PartyId::new("other-party");
        assert!(party.ensure_reusable_identity(&revision, "共享企业", None).is_err());
        revision.party_id = PartyId::new(&party.base.id);
        revision.base.id = "historical-revision".into();
        assert!(party.ensure_reusable_identity(&revision, "共享企业", None).is_err());
    }

    #[test]
    fn reuse_rejects_disabled_or_deleted_identity() {
        let (mut party, mut revision) = identity();
        party.stable.status = PartyStatus::Disabled;
        assert!(party.ensure_reusable_identity(&revision, "共享企业", None).is_err());
        party.stable.status = PartyStatus::Active;
        party.base.deleted_at = 1;
        assert!(party.ensure_reusable_identity(&revision, "共享企业", None).is_err());
        party.base.deleted_at = 0;
        revision.base.deleted_at = 1;
        assert!(party.ensure_reusable_identity(&revision, "共享企业", None).is_err());
    }
}
