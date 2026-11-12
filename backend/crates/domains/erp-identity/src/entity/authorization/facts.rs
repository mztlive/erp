use std::collections::BTreeSet;

use super::{AccessFact, ObjectFacts};
use crate::{Error, Result};

/// 业务事实是否已读取；已知空值与未读取分别表示。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum FactValue<T> {
    #[default]
    Missing,
    Known(T),
}

impl<T> FactValue<T> {
    /// 取得已经读取的事实。
    /// # 参数
    /// 无。
    /// # 返回
    /// 已知值；值本身可以为 None 或空集合。
    /// # 错误
    /// 未读取事实返回接入错误，不解释为条件不匹配。
    pub fn get(&self) -> Result<&T> {
        match self {
            Self::Known(value) => Ok(value),
            Self::Missing => Err(Error::ValidationError("缺少必要授权事实".into())),
        }
    }

    /// 判断事实是否已读取。
    fn loaded(&self) -> bool {
        matches!(self, Self::Known(_))
    }
}

impl FactValue<Option<String>> {
    /// 单值与多值表示必须二选一，避免不完整事实被另一种表示掩盖。
    fn loaded_with(&self, values: &FactValue<BTreeSet<String>>) -> Result<bool> {
        if self.loaded() && values.loaded() {
            return Err(Error::ValidationError("单值和多值授权事实不能同时提供".into()));
        }
        Ok(self.loaded() || values.loaded())
    }

    /// 在完整单值或多值事实中判断任一成员是否匹配。
    pub(crate) fn any(
        &self,
        values: &FactValue<BTreeSet<String>>,
        matches: impl Fn(&str) -> bool,
    ) -> Result<bool> {
        if !self.loaded_with(values)? {
            return Err(Error::ValidationError("缺少必要授权事实".into()));
        }
        match self {
            Self::Known(value) => Ok(value.as_deref().is_some_and(matches)),
            Self::Missing => Ok(values.get()?.iter().any(|value| matches(value))),
        }
    }
}

/// 可登记的事实维度；取值由对象端口或 Mapping 提供。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FactKind {
    Owner,
    Collaborators,
    Organization,
    SettlementParty,
    Warehouse,
    HistoryParticipants,
    Assignee,
    Participants,
}

impl AccessFact {
    /// 返回事实节点要求的维度。
    pub(crate) fn kind(&self) -> FactKind {
        match self {
            Self::Owned => FactKind::Owner,
            Self::Collaborating => FactKind::Collaborators,
            Self::OrganizationIn(_) => FactKind::Organization,
            Self::SettlementPartyIn(_) => FactKind::SettlementParty,
            Self::WarehouseIn(_) => FactKind::Warehouse,
            Self::HistoryParticipant => FactKind::HistoryParticipants,
            Self::AssignedTo => FactKind::Assignee,
            Self::Participant => FactKind::Participants,
        }
    }
}

impl ObjectFacts {
    /// 必需事实先于与或判定验证，不能被其他放行分支掩盖。
    pub(crate) fn require_facts(&self, required: &BTreeSet<FactKind>) -> Result<()> {
        for kind in required {
            let loaded = match kind {
                FactKind::Owner => self.owner_id.loaded_with(&self.owner_ids)?,
                FactKind::Collaborators => self.collaborators.loaded(),
                FactKind::Organization => self.org_unit_id.loaded_with(&self.org_unit_ids)?,
                FactKind::SettlementParty => self.settlement_party_id.loaded(),
                FactKind::Warehouse => self.warehouse_id.loaded(),
                FactKind::HistoryParticipants => self.history_participants.loaded(),
                FactKind::Assignee => self.assignee_id.loaded(),
                FactKind::Participants => self.participants.loaded(),
            };
            if !loaded {
                return Err(Error::ValidationError(format!("缺少必要授权事实：{kind:?}")));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::entity::authorization::AccessRule;
    use crate::entity::resource_access::ResourceKind;

    #[test]
    fn missing_fact_is_not_known_empty_and_cannot_be_hidden_by_a_grant() {
        let mut object = ObjectFacts { resource: "sales_order".into(), id: "s".into(), ..Default::default() };
        let grants = [(ResourceKind::SalesOrder, "s".into())].into();
        let rule = AccessRule::any(vec![
            AccessRule::Fact(AccessFact::Owned),
            AccessRule::RecordReadGrant(ResourceKind::SalesOrder),
        ]);
        assert!(matches!(rule.allows_object("u", &object, &grants), Err(Error::ValidationError(_))));
        object.owner_id = FactValue::Known(None);
        assert!(rule.allows_object("u", &object, &grants).unwrap());
        object.collaborators = FactValue::Known(BTreeSet::new());
        assert!(!AccessRule::Fact(AccessFact::Collaborating).allows_object("u", &object, &grants).unwrap());
    }

    #[test]
    fn explicit_required_fact_survives_unconditional_union() {
        let required = AccessRule::Allow.requiring(&[FactKind::Warehouse].into());
        let rule = AccessRule::any(vec![AccessRule::Allow, required]);
        let mut object = ObjectFacts::default();
        assert!(rule.allows_object("u", &object, &BTreeSet::new()).is_err());
        object.warehouse_id = FactValue::Known(None);
        assert!(rule.allows_object("u", &object, &BTreeSet::new()).unwrap());
    }

    #[test]
    fn ordinary_facts_cannot_be_erased_by_allow_or_deny_normalization() {
        let missing = ObjectFacts::default();
        for rule in [
            AccessRule::any(vec![AccessRule::Allow, AccessRule::Fact(AccessFact::Owned)]),
            AccessRule::all(vec![AccessRule::Deny, AccessRule::Fact(AccessFact::Owned)]),
        ] {
            assert!(matches!(
                rule.allows_object("u", &missing, &BTreeSet::new()),
                Err(Error::ValidationError(_))
            ));
        }
    }

    #[test]
    fn multiple_owners_and_organizations_match_any_complete_member() {
        let object = ObjectFacts {
            owner_ids: FactValue::Known(["other".into(), "u".into()].into()),
            org_unit_ids: FactValue::Known(["org-a".into(), "org-b".into()].into()),
            ..Default::default()
        };
        let rule = AccessRule::all(vec![
            AccessRule::Fact(AccessFact::Owned),
            AccessRule::Fact(AccessFact::OrganizationIn(["org-b".into()].into())),
        ])
        .requiring(&[FactKind::Owner, FactKind::Organization].into());
        assert!(rule.allows_object("u", &object, &BTreeSet::new()).unwrap());
        let denied =
            ObjectFacts { org_unit_ids: FactValue::Known(["org-c".into()].into()), ..object.clone() };
        assert!(!rule.allows_object("u", &denied, &BTreeSet::new()).unwrap());
        let non_owner = ObjectFacts { owner_ids: FactValue::Known(["other".into()].into()), ..object };
        assert!(!rule.allows_object("u", &non_owner, &BTreeSet::new()).unwrap());
    }

    #[test]
    fn empty_multi_value_facts_are_known_but_ambiguous_representations_fail() {
        let required = [FactKind::Owner, FactKind::Organization].into();
        let mut object = ObjectFacts {
            owner_ids: FactValue::Known(BTreeSet::new()),
            org_unit_ids: FactValue::Known(BTreeSet::new()),
            ..Default::default()
        };
        assert!(object.require_facts(&required).is_ok());
        assert!(!AccessRule::Fact(AccessFact::Owned).allows_object("u", &object, &BTreeSet::new()).unwrap());
        object.owner_id = FactValue::Known(None);
        assert!(matches!(object.require_facts(&required), Err(Error::ValidationError(_))));
        object.owner_id = FactValue::Missing;
        object.org_unit_id = FactValue::Known(None);
        assert!(matches!(object.require_facts(&required), Err(Error::ValidationError(_))));
        object.org_unit_id = FactValue::Missing;
        object.org_unit_ids = FactValue::Missing;
        assert!(matches!(object.require_facts(&required), Err(Error::ValidationError(_))));
    }
}
