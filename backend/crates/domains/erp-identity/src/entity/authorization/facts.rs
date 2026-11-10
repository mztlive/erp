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
                FactKind::Owner => self.owner_id.loaded(),
                FactKind::Collaborators => self.collaborators.loaded(),
                FactKind::Organization => self.org_unit_id.loaded(),
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
    use application_core::AuditActor;
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;

    use super::*;
    use crate::Permission;
    use crate::entity::authorization::{AccessContext, AccessRule};
    use crate::entity::resource_access::ResourceKind;

    fn context() -> AccessContext {
        AccessContext {
            actor: AuditActor::new("u".into(), "user".into(), AccountKind::Admin),
            permission: Permission::parse("sales_order:detail").unwrap(),
            resource: "sales_order".into(),
            model: None,
            as_of: Instant::from_unix_secs(1),
        }
    }

    #[test]
    fn missing_fact_is_not_known_empty_and_cannot_be_hidden_by_a_grant() {
        let mut object = ObjectFacts { resource: "sales_order".into(), id: "s".into(), ..Default::default() };
        let grants = [(ResourceKind::SalesOrder, "s".into())].into();
        let rule = AccessRule::any(vec![
            AccessRule::Fact(AccessFact::Owned),
            AccessRule::RecordReadGrant(ResourceKind::SalesOrder),
        ]);
        assert!(matches!(rule.allows_object(&context(), &object, &grants), Err(Error::ValidationError(_))));
        object.owner_id = FactValue::Known(None);
        assert!(rule.allows_object(&context(), &object, &grants).unwrap());
        object.collaborators = FactValue::Known(BTreeSet::new());
        assert!(
            !AccessRule::Fact(AccessFact::Collaborating).allows_object(&context(), &object, &grants).unwrap()
        );
    }

    #[test]
    fn explicit_required_fact_survives_unconditional_union_and_create_conversion() {
        let required = AccessRule::Allow.requiring(&[FactKind::Warehouse].into());
        let rule = AccessRule::any(vec![AccessRule::Allow, required]);
        let mut object = ObjectFacts::default();
        assert!(rule.allows_object(&context(), &object, &BTreeSet::new()).is_err());
        assert!(rule.without_reads().allows_object(&context(), &object, &BTreeSet::new()).is_err());
        object.warehouse_id = FactValue::Known(None);
        assert!(rule.allows_object(&context(), &object, &BTreeSet::new()).unwrap());
    }
}
