use std::collections::BTreeSet;

use erp_core::common::time::Instant;

use super::{AccessContext, AccessFact, AccessRule, ObjectFacts, RelationMatch};
use crate::entity::resource_access::ResourceKind;
use crate::{Error, Result};

type Grants = BTreeSet<(ResourceKind, String)>;

impl AccessRule {
    /// 对象与查询共用表达式，读取授权集合只包含本次目标。
    pub(crate) fn allows_object(
        &self,
        context: &AccessContext,
        object: &ObjectFacts,
        grants: &Grants,
    ) -> Result<bool> {
        match self {
            Self::Allow => Ok(true),
            Self::Deny => Ok(false),
            Self::All(rules) | Self::Any(rules) => {
                let mut results = Vec::new();
                for rule in rules {
                    results.push(rule.allows_object(context, object, grants)?);
                }
                Ok(!results.is_empty()
                    && if matches!(self, Self::All(_)) {
                        results.iter().all(|v| *v)
                    } else {
                        results.iter().any(|v| *v)
                    })
            },
            Self::Fact(fact) => fact.matches_object(context.actor.id(), object),
            Self::Required { facts, rule } => {
                object.require_facts(facts)?;
                rule.allows_object(context, object, grants)
            },
            Self::RecordReadGrant(resource) => {
                Ok(object.resource == resource.code() && grants.contains(&(*resource, object.id.clone())))
            },
            Self::Related { relation, resource, mode, rule } => {
                let related = object
                    .relations
                    .get(relation)
                    .ok_or_else(|| Error::ValidationError("缺少必要来源关系".into()))?;
                let mut seen = BTreeSet::new();
                let mut allowed = BTreeSet::new();
                for child in &related.objects {
                    if child.resource != *resource
                        || !related.expected_ids.contains(&child.id)
                        || !seen.insert(child.id.clone())
                    {
                        return Err(Error::ValidationError("来源身份错配或重复".into()));
                    }
                    if rule.allows_object(context, child, grants)? {
                        allowed.insert(child.id.clone());
                    }
                }
                Ok(match mode {
                    RelationMatch::Any => !allowed.is_empty(),
                    RelationMatch::AllNonEmpty => {
                        !related.expected_ids.is_empty() && allowed == related.expected_ids
                    },
                })
            },
        }
    }

    /// 只查询规则实际使用的读取授权资源。
    pub(crate) fn grant_resources(&self, resources: &mut BTreeSet<ResourceKind>) {
        match self {
            Self::RecordReadGrant(resource) => {
                resources.insert(*resource);
            },
            Self::All(rules) | Self::Any(rules) => {
                for rule in rules {
                    rule.grant_resources(resources);
                }
            },
            Self::Related { rule, .. } | Self::Required { rule, .. } => rule.grant_resources(resources),
            _ => {},
        }
    }
}

impl AccessFact {
    /// 归属和参与事实均相对于已冻结账号解释。
    fn matches_object(&self, actor: &str, object: &ObjectFacts) -> Result<bool> {
        Ok(match self {
            Self::Owned => object.owner_id.get()?.as_deref() == Some(actor),
            Self::Collaborating => object.collaborators.get()?.contains(actor),
            Self::OrganizationIn(ids) => {
                object.org_unit_id.get()?.as_ref().is_some_and(|id| ids.contains(id))
            },
            Self::SettlementPartyIn(ids) => {
                object.settlement_party_id.get()?.as_ref().is_some_and(|id| ids.contains(id))
            },
            Self::WarehouseIn(ids) => object.warehouse_id.get()?.as_ref().is_some_and(|id| ids.contains(id)),
            Self::HistoryParticipant => object.history_participants.get()?.contains(actor),
            Self::AssignedTo => object.assignee_id.get()?.as_deref() == Some(actor),
            Self::Participant => object.participants.get()?.contains(actor),
        })
    }
}

impl ObjectFacts {
    /// 有界展开事实，防止递归关系或遗漏身份造成错误授权。
    pub(crate) fn flatten<'a>(&'a self, output: &mut Vec<&'a Self>, depth: usize) -> Result<()> {
        if depth > 8 || output.len() >= 1000 || self.resource.is_empty() || self.version.is_empty() {
            return Err(Error::ValidationError("授权对象事实缺失或规模超限".into()));
        }
        if self.valid_until.is_some_and(|until| Instant::now() >= until) {
            return Err(Error::ConflictError("DATA_SCOPE_CHANGED：对象责任关系已到期".into()));
        }
        output.push(self);
        for related in self.relations.values() {
            for child in &related.objects {
                child.flatten(output, depth + 1)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use application_core::AuditActor;
    use erp_core::AccountKind;

    use super::*;
    use crate::Permission;
    use crate::entity::authorization::{FactValue, QueryModelKey, RelatedObjects, RelationKey};

    fn context() -> AccessContext {
        AccessContext {
            actor: AuditActor::new("u".into(), "user".into(), AccountKind::Admin),
            permission: Permission::parse("invoice:detail").unwrap(),
            resource: "invoice".into(),
            model: Some(QueryModelKey::parse("invoice").unwrap()),
            as_of: Instant::from_unix_secs(1),
        }
    }

    fn source(id: &str, owner: &str) -> ObjectFacts {
        ObjectFacts {
            resource: "sales_order".into(),
            id: id.into(),
            version: "v1".into(),
            owner_id: FactValue::Known(Some(owner.into())),
            warehouse_id: FactValue::Known(None),
            assignee_id: FactValue::Known(None),
            ..Default::default()
        }
    }

    fn rule(mode: RelationMatch) -> AccessRule {
        AccessRule::Related {
            relation: RelationKey::parse("sales").unwrap(),
            resource: "sales_order".into(),
            mode,
            rule: Box::new(AccessRule::Fact(AccessFact::Owned)),
        }
    }

    fn root(children: Vec<ObjectFacts>) -> ObjectFacts {
        ObjectFacts {
            resource: "invoice".into(),
            id: "invoice1".into(),
            version: "v1".into(),
            relations: [(
                RelationKey::parse("sales").unwrap(),
                RelatedObjects {
                    expected_ids: BTreeSet::from(["s1".into(), "s2".into()]),
                    objects: children,
                },
            )]
            .into(),
            ..Default::default()
        }
    }

    #[test]
    fn all_sources_requires_every_actual_reference_and_any_does_not_authorize_whole_object() {
        let context = context();
        let grants = BTreeSet::new();
        let all = rule(RelationMatch::AllNonEmpty);
        let any = rule(RelationMatch::Any);
        let missing = root(vec![source("s1", "u")]);
        assert!(!all.allows_object(&context, &missing, &grants).unwrap());
        assert!(any.allows_object(&context, &missing, &grants).unwrap());
        let denied = root(vec![source("s1", "u"), source("s2", "other")]);
        assert!(!all.allows_object(&context, &denied, &grants).unwrap());
        let complete = root(vec![source("s1", "u"), source("s2", "u")]);
        assert!(all.allows_object(&context, &complete, &grants).unwrap());
        assert!(!all.allows_object(&context, &root(vec![]), &grants).unwrap());
    }

    #[test]
    fn duplicate_extra_and_wrong_resource_sources_are_errors() {
        for children in [
            vec![source("s1", "u"), source("s1", "u")],
            vec![source("extra", "u")],
            vec![ObjectFacts { resource: "customer".into(), ..source("s1", "u") }],
        ] {
            assert!(
                rule(RelationMatch::Any)
                    .allows_object(&context(), &root(children), &BTreeSet::new())
                    .is_err()
            );
        }
    }

    #[test]
    fn empty_relations_missing_relations_and_expired_facts_never_pass() {
        let mut object = root(vec![]);
        object.relations.values_mut().next().unwrap().expected_ids.clear();
        assert!(
            !rule(RelationMatch::AllNonEmpty).allows_object(&context(), &object, &BTreeSet::new()).unwrap()
        );
        object.relations.clear();
        assert!(rule(RelationMatch::Any).allows_object(&context(), &object, &BTreeSet::new()).is_err());
        object.valid_until = Some(Instant::from_unix_secs(1));
        assert!(object.flatten(&mut Vec::new(), 0).is_err());
    }

    #[test]
    fn task_assignment_is_distinct_from_participation_and_history() {
        let mut object = source("s1", "u");
        object.history_participants = FactValue::Known(["u".into()].into());
        object.participants = FactValue::Known(["u".into()].into());
        let grants = BTreeSet::new();
        let assigned = AccessRule::Fact(AccessFact::AssignedTo);
        assert!(!assigned.allows_object(&context(), &object, &grants).unwrap());
        object.assignee_id = FactValue::Known(Some("u".into()));
        assert!(assigned.allows_object(&context(), &object, &grants).unwrap());
        assert!(
            AccessRule::Fact(AccessFact::HistoryParticipant)
                .allows_object(&context(), &object, &grants)
                .unwrap()
        );
    }

    #[test]
    fn exact_record_grant_cannot_bypass_independent_scope() {
        let mut object = source("s1", "other");
        let read = AccessRule::any(vec![
            AccessRule::Fact(AccessFact::Owned),
            AccessRule::RecordReadGrant(ResourceKind::SalesOrder),
        ]);
        let grants = BTreeSet::from([(ResourceKind::SalesOrder, "s1".into())]);
        assert!(read.allows_object(&context(), &object, &grants).unwrap());
        assert!(!read.allows_object(&context(), &object, &BTreeSet::new()).unwrap());
        let limited =
            AccessRule::all(vec![read, AccessRule::Fact(AccessFact::WarehouseIn(["w".into()].into()))]);
        assert!(!limited.allows_object(&context(), &object, &grants).unwrap());
        object.warehouse_id = FactValue::Known(Some("w".into()));
        assert!(limited.allows_object(&context(), &object, &grants).unwrap());
        object.id = "s2".into();
        assert!(!limited.allows_object(&context(), &object, &grants).unwrap());
        object.id = "s1".into();
        object.resource = "purchase_order".into();
        assert!(!limited.allows_object(&context(), &object, &grants).unwrap());
    }
}
