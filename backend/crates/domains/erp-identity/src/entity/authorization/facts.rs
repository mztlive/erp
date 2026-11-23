use std::collections::BTreeSet;

use super::{AccessFact, ObjectFacts};
use crate::{Error, Result};

/// 对象取证的读取状态，区分未取证、确认无值和已有值。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum FactValue<T> {
    /// 事实尚未读取；规则需要该维度时报告接入错误。
    #[default]
    Missing,
    /// 已确认的事实，允许空值或空集合。
    Known(
        /// 事实端口实际读取的值。
        T,
    ),
}

impl<T> FactValue<T> {
    /// 借用已确认的事实，原样保留 `None` 或空集合。
    /// 事实未读取时返回接入错误，调用方须据此中止判断。
    pub fn get(&self) -> Result<&T> {
        match self {
            Self::Known(value) => Ok(value),
            Self::Missing => Err(Error::ValidationError("缺少必要授权事实".into())),
        }
    }

    // 检查端口是否已确认该事实，已知空值也算完成取证。
    fn loaded(&self) -> bool {
        matches!(self, Self::Known(_))
    }
}

impl FactValue<Option<String>> {
    // 检查单值或多值事实的装载状态；均未读取返回 `false`，同时提供两种表示返回错误。
    fn loaded_with(&self, values: &FactValue<BTreeSet<String>>) -> Result<bool> {
        if self.loaded() && values.loaded() {
            return Err(Error::ValidationError("单值和多值授权事实不能同时提供".into()));
        }
        Ok(self.loaded() || values.loaded())
    }

    /// 在唯一已装载的表示中查找匹配成员；已知空值不命中，缺失或双重表示报错。
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

/// 规则可要求取证的维度，由对象事实端口或查询映射提供。
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
    Completed,
}

impl AccessFact {
    /// 提取条件所需的事实维度，供启动时校验映射能力及运行时核对取证。
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
            Self::Completed(_) => FactKind::Completed,
        }
    }
}

impl ObjectFacts {
    /// 在组合条件求值前校验所有必需维度；缺失事实即使存在其他放行分支也返回接入错误。
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
                FactKind::Completed => self.completed.loaded(),
            };
            if !loaded {
                tracing::debug!(?kind, "缺少必要授权事实");
                let fact = match kind {
                    FactKind::Owner => "负责人",
                    FactKind::Collaborators => "协作人员",
                    FactKind::Organization => "所属组织",
                    FactKind::SettlementParty => "结算主体",
                    FactKind::Warehouse => "所属仓库",
                    FactKind::HistoryParticipants => "历史参与人",
                    FactKind::Assignee => "处理人",
                    FactKind::Participants => "参与人",
                    FactKind::Completed => "完成状态",
                };
                return Err(Error::ValidationError(format!(
                    "缺少判断访问权限所需的{fact}信息，请联系管理员核对该记录"
                )));
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

    // 验证记录读取授权无法掩盖缺失的归属事实，确认无人负责后仍可由该授权放行。
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

    // 验证与无条件放行取并集后，仓库事实仍须先取证。
    #[test]
    fn explicit_required_fact_survives_unconditional_union() {
        let required = AccessRule::Allow.requiring(&[FactKind::Warehouse].into());
        let rule = AccessRule::any(vec![AccessRule::Allow, required]);
        let mut object = ObjectFacts::default();
        assert!(rule.allows_object("u", &object, &BTreeSet::new()).is_err());
        object.warehouse_id = FactValue::Known(None);
        assert!(rule.allows_object("u", &object, &BTreeSet::new()).unwrap());
    }

    // 验证归一化并集和交集时，无条件节点不会消除普通事实的取证错误。
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

    // 验证多负责人和多组织各自任一成员匹配即可满足本维度，两个维度仍须同时成立。
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

    // 验证空集合属于已知事实，且单值与多值并存或完全缺失均使取证校验失败。
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
