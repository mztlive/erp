use std::collections::BTreeSet;

use erp_core::common::time::Instant;

use super::{AccessFact, AccessRule, ObjectFacts, RelatedObjects, RelationMatch, ResourceTargets};
use crate::entity::resource_access::ResourceKind;
use crate::{Error, Result};

type Grants = BTreeSet<(ResourceKind, String)>;

impl AccessRule {
    // 以和查询编译一致的条件判断精确对象；记录读取补充只从本次目标取得，不接受账号历史授权全集。
    pub(crate) fn allows_object(&self, actor: &str, object: &ObjectFacts, grants: &Grants) -> Result<bool> {
        match self {
            Self::Allow => Ok(true),
            Self::Deny => Ok(false),
            Self::All(rules) | Self::Any(rules) => {
                let mut results = Vec::new();
                for rule in rules {
                    results.push(rule.allows_object(actor, object, grants)?);
                }
                Ok(!results.is_empty()
                    && if matches!(self, Self::All(_)) {
                        results.iter().all(|v| *v)
                    } else {
                        results.iter().any(|v| *v)
                    })
            },
            Self::Fact(fact) => fact.matches_object(actor, object),
            Self::Required { facts, rule } => {
                object.require_facts(facts)?;
                rule.allows_object(actor, object, grants)
            },
            Self::Independent { rule, .. } => rule.allows_object(actor, object, grants),
            Self::RelationPresence { relation, present } => {
                let related = object
                    .relations
                    .get(relation)
                    .ok_or_else(|| Error::ValidationError("缺少必要来源关系".into()))?;
                related.validate()?;
                Ok(*present != related.expected_ids.is_empty())
            },
            Self::RecordReadGrant(resource) => {
                Ok(object.resource == resource.code() && grants.contains(&(*resource, object.id.clone())))
            },
            Self::Related { relation, resource, mode, rule } => {
                let related = object
                    .relations
                    .get(relation)
                    .ok_or_else(|| Error::ValidationError("缺少必要来源关系".into()))?;
                rule.allows_related(actor, related, resource, *mode, grants)
            },
        }
    }

    // 在判断来源覆盖前核对完整引用与已读对象身份，确保缺失或无权来源不能在匹配前被丢弃。
    fn allows_related(
        &self,
        actor: &str,
        related: &RelatedObjects,
        resource: &str,
        mode: RelationMatch,
        grants: &Grants,
    ) -> Result<bool> {
        let mut seen = BTreeSet::new();
        let mut allowed = BTreeSet::new();
        for child in &related.objects {
            if child.resource != resource
                || !related.expected_ids.contains(&child.id)
                || !seen.insert(child.id.clone())
            {
                return Err(Error::ValidationError("来源身份错配或重复".into()));
            }
            if self.allows_object(actor, child, grants)? {
                allowed.insert(child.id.clone());
            }
        }

        Ok(match mode {
            RelationMatch::Any => !allowed.is_empty(),
            RelationMatch::AllNonEmpty => !related.expected_ids.is_empty() && allowed == related.expected_ids,
            RelationMatch::ExactlyOne => related.expected_ids.len() == 1 && allowed == related.expected_ids,
        })
    }

    // 收集当前规则实际使用的记录授权资源，供对象取证定向查询，不读取未使用的补充授权。
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
            Self::Related { rule, .. } | Self::Required { rule, .. } | Self::Independent { rule, .. } => {
                rule.grant_resources(resources)
            },
            _ => {},
        }
    }
}

impl AccessFact {
    // 以本次冻结的账号解释对象归属和参与事实，供上层组合规则逐项判定。
    fn matches_object(&self, actor: &str, object: &ObjectFacts) -> Result<bool> {
        Ok(match self {
            Self::Owned => object.owner_id.any(&object.owner_ids, |id| id == actor)?,
            Self::Collaborating => object.collaborators.get()?.contains(actor),
            Self::OrganizationIn(ids) => {
                object.org_unit_id.any(&object.org_unit_ids, |id| ids.contains(id))?
            },
            Self::SettlementPartyIn(ids) => {
                object.settlement_party_id.get()?.as_ref().is_some_and(|id| ids.contains(id))
            },
            Self::WarehouseIn(ids) => object.warehouse_id.get()?.as_ref().is_some_and(|id| ids.contains(id)),
            Self::HistoryParticipant => object.history_participants.get()?.contains(actor),
            Self::AssignedTo => object.assignee_id.get()?.as_deref() == Some(actor),
            Self::Participant => object.participants.get()?.contains(actor),
            Self::Completed(completed) => object.completed.get()? == completed,
        })
    }
}

impl ObjectFacts {
    // 为对象复验整理可比较的事实集合：先核对完整性及展开上限，再按资源和身份稳定排序根对象和嵌套来源。
    pub(crate) fn canonicalize(objects: &mut [Self]) -> Result<()> {
        let mut flattened = Vec::new();
        for object in objects.iter() {
            object.flatten(&mut flattened, 0)?;
        }

        Self::sort(objects);
        Ok(())
    }

    // 为对象复验统一已校验事实的排列；只重排成员，不去重或改写实际来源引用。
    fn sort(objects: &mut [Self]) {
        objects.sort_unstable_by(|a, b| (&a.resource, &a.id).cmp(&(&b.resource, &b.id)));
        for object in objects {
            for relation in object.relations.values_mut() {
                Self::sort(&mut relation.objects);
            }
        }
    }

    // 为定向读取记录授权有界展开根对象及来源；超限或身份不完整时不能截断后继续放行。
    pub(crate) fn flatten<'a>(&'a self, output: &mut Vec<&'a Self>, depth: usize) -> Result<()> {
        if depth > 8 || output.len() >= 1000 || self.resource.is_empty() || self.version.is_empty() {
            return Err(Error::ValidationError("授权对象事实缺失或规模超限".into()));
        }
        ResourceTargets::new(&[&self.id])?;
        if self.valid_until.is_some_and(|until| Instant::now() >= until) {
            return Err(Error::ConflictError("DATA_SCOPE_CHANGED：对象责任关系已到期".into()));
        }

        output.push(self);
        for related in self.relations.values() {
            related.validate()?;
            for child in &related.objects {
                child.flatten(output, depth + 1)?;
            }
        }
        Ok(())
    }
}

impl RelatedObjects {
    // 对照实际来源对象核验原始引用身份，供覆盖规则据完整集合作判定。未读取到的引用须保留，不能当作不存在关系。
    pub(crate) fn validate(&self) -> Result<()> {
        let ids = self.expected_ids.iter().map(String::as_str).collect::<Vec<_>>();
        ResourceTargets::new(&ids)?;
        let mut seen = BTreeSet::new();
        for object in &self.objects {
            if !self.expected_ids.contains(&object.id) || !seen.insert(&object.id) {
                return Err(Error::ValidationError("来源身份错配或重复".into()));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::entity::authorization::{FactValue, RelatedObjects, RelationKey};

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
        let grants = BTreeSet::new();
        let all = rule(RelationMatch::AllNonEmpty);
        let any = rule(RelationMatch::Any);

        let missing = root(vec![source("s1", "u")]);
        assert!(!all.allows_object("u", &missing, &grants).unwrap());
        assert!(any.allows_object("u", &missing, &grants).unwrap());

        let denied = root(vec![source("s1", "u"), source("s2", "other")]);
        assert!(!all.allows_object("u", &denied, &grants).unwrap());

        let complete = root(vec![source("s1", "u"), source("s2", "u")]);
        assert!(all.allows_object("u", &complete, &grants).unwrap());
        assert!(!all.allows_object("u", &root(vec![]), &grants).unwrap());
    }

    #[test]
    fn duplicate_extra_and_wrong_resource_sources_are_errors() {
        for children in [
            vec![source("s1", "u"), source("s1", "u")],
            vec![source("extra", "u")],
            vec![ObjectFacts { resource: "customer".into(), ..source("s1", "u") }],
        ] {
            assert!(rule(RelationMatch::Any).allows_object("u", &root(children), &BTreeSet::new()).is_err());
        }
    }

    #[test]
    fn empty_relations_missing_relations_and_expired_facts_never_pass() {
        let mut object = root(vec![]);
        object.relations.values_mut().next().unwrap().expected_ids.clear();
        assert!(!rule(RelationMatch::AllNonEmpty).allows_object("u", &object, &BTreeSet::new()).unwrap());

        object.relations.clear();
        assert!(rule(RelationMatch::Any).allows_object("u", &object, &BTreeSet::new()).is_err());

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
        assert!(!assigned.allows_object("u", &object, &grants).unwrap());
        object.assignee_id = FactValue::Known(Some("u".into()));
        assert!(assigned.allows_object("u", &object, &grants).unwrap());
        assert!(
            AccessRule::Fact(AccessFact::HistoryParticipant).allows_object("u", &object, &grants).unwrap()
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
        assert!(read.allows_object("u", &object, &grants).unwrap());
        assert!(!read.allows_object("u", &object, &BTreeSet::new()).unwrap());

        let limited =
            AccessRule::all(vec![read, AccessRule::Fact(AccessFact::WarehouseIn(["w".into()].into()))]);
        assert!(!limited.allows_object("u", &object, &grants).unwrap());
        object.warehouse_id = FactValue::Known(Some("w".into()));
        assert!(limited.allows_object("u", &object, &grants).unwrap());

        object.id = "s2".into();
        assert!(!limited.allows_object("u", &object, &grants).unwrap());
        object.id = "s1".into();
        object.resource = "purchase_order".into();
        assert!(!limited.allows_object("u", &object, &grants).unwrap());
    }
}
