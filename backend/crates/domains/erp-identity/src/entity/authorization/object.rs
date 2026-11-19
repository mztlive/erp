use std::collections::BTreeSet;

use erp_core::common::time::Instant;

use super::{AccessFact, AccessRule, ObjectFacts, RelatedObjects, RelationMatch, ResourceTargets};
use crate::entity::resource_access::ResourceKind;
use crate::{Error, Result};

// 当前目标及来源已查证的记录级读取授权，按资源与记录 ID 匹配。
type Grants = BTreeSet<(ResourceKind, String)>;

impl AccessRule {
    /// 用与查询编译共用的规则判断指定对象；逐支传播事实错误，记录授权只使用本次目标取证。
    pub(crate) fn allows_object(&self, actor: &str, object: &ObjectFacts, grants: &Grants) -> Result<bool> {
        match self {
            Self::Allow => Ok(true),
            Self::Deny => Ok(false),
            Self::All(rules) | Self::Any(rules) => {
                let all = matches!(self, Self::All(_));
                let mut matches = all && !rules.is_empty();
                for rule in rules {
                    let allowed = rule.allows_object(actor, object, grants)?;
                    matches = if all { matches && allowed } else { matches || allowed };
                }
                Ok(matches)
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

    // 拒绝重复、额外或资源错配的已读来源，再按匹配方式核对有权来源对全部引用的覆盖。
    fn allows_related(
        &self,
        actor: &str,
        related: &RelatedObjects,
        resource: &str,
        mode: RelationMatch,
        grants: &Grants,
    ) -> Result<bool> {
        let mut seen = BTreeSet::new();
        let mut allowed = 0;
        for child in &related.objects {
            if child.resource != resource
                || !related.expected_ids.contains(&child.id)
                || !seen.insert(&child.id)
            {
                return Err(Error::ValidationError("来源身份错配或重复".into()));
            }
            if self.allows_object(actor, child, grants)? {
                allowed += 1;
            }
        }

        // 每个通过的来源都是唯一的预期引用，计数即可判断覆盖。
        let expected = related.expected_ids.len();
        Ok(match mode {
            RelationMatch::Any => allowed > 0,
            RelationMatch::AllNonEmpty => allowed > 0 && allowed == expected,
            RelationMatch::ExactlyOne => expected == 1 && allowed == 1,
        })
    }

    /// 递归收集规则实际使用的记录授权资源，供本次对象取证定向读取。
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
    // 基于本次账号与对象事实评估单个条件，取证错误交给组合规则处理。
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
    /// 为复验准备可比较的事实：校验对象树和展开上限后，递归按资源和 ID 排列根对象及来源。
    pub(crate) fn canonicalize(objects: &mut [Self]) -> Result<()> {
        let mut flattened = Vec::new();
        for object in objects.iter() {
            object.flatten(&mut flattened, 0)?;
        }

        Self::sort(objects);
        Ok(())
    }

    // 按资源和 ID 重排各层对象，保留原有成员、重复项和来源引用。
    fn sort(objects: &mut [Self]) {
        objects.sort_unstable_by(|a, b| (&a.resource, &a.id).cmp(&(&b.resource, &b.id)));
        for object in objects {
            for relation in object.relations.values_mut() {
                Self::sort(&mut relation.objects);
            }
        }
    }

    /// 为定向取证展开根对象及来源；最多累计 1000 个对象，根深度为 0，最大深度为 8。
    /// 身份或版本缺失、到期、来源引用非法或超限时返回错误。
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
    /// 校验来源引用格式，拒绝额外及重复的已读对象；未读到的引用留给覆盖规则判断。
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

    // 构造带负责人、已知无仓库且未指派的销售单来源。
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

    // 构造按指定匹配方式检查销售单来源负责人的规则。
    fn rule(mode: RelationMatch) -> AccessRule {
        AccessRule::Related {
            relation: RelationKey::parse("sales").unwrap(),
            resource: "sales_order".into(),
            mode,
            rule: Box::new(AccessRule::Fact(AccessFact::Owned)),
        }
    }

    // 构造固定引用两张销售单的发票，并注入本次实际读到的来源。
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

    // 验证完整对象要求所有引用均存在且有权，任一命中仅满足宽松的来源匹配。
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

    // 验证即使只要求一个来源命中，也拒绝重复、额外和资源错配的取证结果。
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
    fn related_evaluation_keeps_checking_after_a_match_and_reports_the_first_error() {
        let rule = rule(RelationMatch::Any);
        let grants = BTreeSet::new();
        let missing = ObjectFacts { owner_id: FactValue::Missing, ..source("s1", "u") };
        for (first, expected_error) in
            [(source("s1", "u"), "来源身份错配或重复"), (missing, "缺少必要授权事实")]
        {
            let object = root(vec![first, source("extra", "u")]);
            assert!(matches!(
                rule.allows_object("u", &object, &grants),
                Err(Error::ValidationError(message)) if message == expected_error
            ));
        }
    }

    #[test]
    fn exactly_one_requires_one_reference_and_one_allowed_source() {
        let rule = rule(RelationMatch::ExactlyOne);
        let grants = BTreeSet::new();
        let mut object = root(vec![source("s1", "u")]);
        assert!(!rule.allows_object("u", &object, &grants).unwrap());
        object.relations.values_mut().next().unwrap().expected_ids.remove("s2");
        assert!(rule.allows_object("u", &object, &grants).unwrap());
        object.relations.values_mut().next().unwrap().objects.clear();
        assert!(!rule.allows_object("u", &object, &grants).unwrap());
    }

    // 验证空引用无法满足全部来源规则，未装载关系和已到期事实均报错。
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

    // 验证参与和历史参与不能视作任务指派，处理人须按当前事实单独匹配。
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

    // 验证记录读取授权只匹配指定资源与 ID，且仍受独立仓库范围约束。
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
