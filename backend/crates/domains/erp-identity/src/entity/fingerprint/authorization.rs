use super::Fingerprint;
use crate::entity::authorization::{
    AccessFact, AccessRule, AccessSubject, FactKind, MappingFacts, QueryModel, QueryOutput, RelationMatch,
};

impl Fingerprint {
    /// 主体编码：固定种类标记，随后为来源或视图标识。
    pub(crate) fn subject(&mut self, subject: &AccessSubject) {
        match subject {
            AccessSubject::Direct => self.text("direct"),
            AccessSubject::Source(key) => {
                self.text("source");
                self.text(key.as_str());
            },
            AccessSubject::View(key) => {
                self.text("view");
                self.text(key.as_str());
            },
        }
    }

    /// 规则编码：固定节点标记及声明顺序的字段；All/Any 分支按无序集合编码。
    pub(crate) fn rule(&mut self, rule: &AccessRule) {
        match rule {
            AccessRule::Allow => self.text("allow"),
            AccessRule::Deny => self.text("deny"),
            AccessRule::All(rules) | AccessRule::Any(rules) => {
                self.text(if matches!(rule, AccessRule::All(_)) { "all" } else { "any" });
                self.unordered(rules, Self::rule);
            },
            AccessRule::Fact(fact) => {
                self.text("fact");
                self.fact(fact);
            },
            AccessRule::Required { facts, rule } => {
                self.text("required");
                self.unordered(facts, Self::kind);
                self.rule(rule);
            },
            AccessRule::RecordReadGrant(resource) => {
                self.text("record_read_grant");
                self.text(resource.code());
            },
            AccessRule::Related { relation, resource, mode, rule } => {
                self.text("related");
                self.text(relation.as_str());
                self.text(resource);
                self.text(match mode {
                    RelationMatch::Any => "any",
                    RelationMatch::AllNonEmpty => "all_non_empty",
                });
                self.rule(rule);
            },
        }
    }

    /// 事实节点以固定维度标记及可选 ID 集合编码。
    fn fact(&mut self, fact: &AccessFact) {
        self.kind(&fact.kind());
        match fact {
            AccessFact::OrganizationIn(ids)
            | AccessFact::SettlementPartyIn(ids)
            | AccessFact::WarehouseIn(ids) => {
                self.unordered(ids, |hash, id| hash.text(id));
            },
            AccessFact::Owned
            | AccessFact::Collaborating
            | AccessFact::HistoryParticipant
            | AccessFact::AssignedTo
            | AccessFact::Participant => {},
        }
    }

    /// 不依赖枚举名称、声明顺序或 Debug 的事实维度代码。
    fn kind(&mut self, kind: &FactKind) {
        self.text(match kind {
            FactKind::Owner => "owner",
            FactKind::Collaborators => "collaborators",
            FactKind::Organization => "organization",
            FactKind::SettlementParty => "settlement_party",
            FactKind::Warehouse => "warehouse",
            FactKind::HistoryParticipants => "history_participants",
            FactKind::Assignee => "assignee",
            FactKind::Participants => "participants",
        });
    }

    /// 模型字段顺序：标识、资源、集合、主键、输出种类及字段、事实维度。
    pub(crate) fn model(&mut self, model: &QueryModel) {
        let QueryModel { key, resource, collection, id_field, output, facts } = model;
        self.text(key.as_str());
        self.text(resource);
        self.text(collection);
        self.text(id_field);
        match output {
            QueryOutput::Object => self.text("object"),
            QueryOutput::Share { fields } | QueryOutput::Existence { fields } => {
                self.text(if matches!(output, QueryOutput::Share { .. }) { "share" } else { "existence" });
                self.unordered(fields, |hash, field| hash.text(field));
            },
        }
        self.unordered(facts, Self::kind);
    }

    /// 映射事实字段顺序：修订号、到期时刻、命名集合；时刻保留秒及纳秒。
    pub(crate) fn facts(&mut self, facts: &MappingFacts) {
        let MappingFacts { version, valid_until, sets } = facts;
        self.optional(version.as_deref(), Self::text);
        self.optional(*valid_until, |hash, until| {
            hash.text(&until.unix_secs().to_string());
            hash.number(u64::from(until.as_utc().timestamp_subsec_nanos()));
        });
        self.unordered(sets, |hash, (name, values)| {
            hash.text(name);
            hash.unordered(values, |hash, value| hash.text(value));
        });
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use erp_core::common::time::Instant;

    use super::*;
    use crate::entity::authorization::{QueryModelKey, RelationKey};
    use crate::entity::resource_access::ResourceKind;

    /// 所有节点、事实维度和主体种类均具有独立编码。
    #[test]
    fn authorization_tags_and_branch_order_are_explicit() {
        let rules = vec![
            AccessRule::Allow,
            AccessRule::Deny,
            AccessRule::All(vec![AccessRule::Allow]),
            AccessRule::Any(vec![AccessRule::Allow]),
            AccessRule::Fact(AccessFact::Owned),
            AccessRule::Fact(AccessFact::Collaborating),
            AccessRule::Fact(AccessFact::OrganizationIn(["a".into()].into())),
            AccessRule::Fact(AccessFact::SettlementPartyIn(["a".into()].into())),
            AccessRule::Fact(AccessFact::WarehouseIn(["a".into()].into())),
            AccessRule::Fact(AccessFact::HistoryParticipant),
            AccessRule::Fact(AccessFact::AssignedTo),
            AccessRule::Fact(AccessFact::Participant),
            AccessRule::Required { facts: [FactKind::Owner].into(), rule: Box::new(AccessRule::Allow) },
            AccessRule::RecordReadGrant(ResourceKind::SalesOrder),
            AccessRule::RecordReadGrant(ResourceKind::PurchaseOrder),
            AccessRule::Related {
                relation: RelationKey::parse("source").unwrap(),
                resource: "invoice".into(),
                mode: RelationMatch::Any,
                rule: Box::new(AccessRule::Allow),
            },
            AccessRule::Related {
                relation: RelationKey::parse("source").unwrap(),
                resource: "invoice".into(),
                mode: RelationMatch::AllNonEmpty,
                rule: Box::new(AccessRule::Allow),
            },
        ];
        let hash = |rule: &AccessRule| {
            let mut hash = Fingerprint::new("rule-test-v2");
            hash.rule(rule);
            hash.finish()
        };
        assert_eq!(rules.iter().map(hash).collect::<BTreeSet<_>>().len(), rules.len());
        let mut golden = Fingerprint::new("rule-test-v2");
        golden.sequence(&rules, Fingerprint::rule);
        assert_eq!(golden.finish(), "3f0505d708daeb3790d15a777d654016");
        let branches = vec![rules[4].clone(), rules[5].clone()];
        let reversed = branches.iter().rev().cloned().collect();
        assert_eq!(hash(&AccessRule::All(branches.clone())), hash(&AccessRule::All(reversed)));
        assert_ne!(hash(&AccessRule::All(branches.clone())), hash(&AccessRule::Any(branches)));
        let subjects = [
            AccessSubject::Direct,
            AccessSubject::Source(RelationKey::parse("same").unwrap()),
            AccessSubject::View(QueryModelKey::parse("same").unwrap()),
        ];
        let versions: BTreeSet<_> = subjects
            .iter()
            .map(|subject| {
                let mut hash = Fingerprint::new("subject-test-v2");
                hash.subject(subject);
                hash.finish()
            })
            .collect();
        assert_eq!(versions.len(), subjects.len());
    }

    /// 模型身份、集合、输出范围和事实内容均影响分页版本。
    #[test]
    fn model_and_mapping_facts_bind_every_field() {
        let model = QueryModel {
            key: QueryModelKey::parse("orders").unwrap(),
            resource: "sales_order".into(),
            collection: "orders".into(),
            id_field: "id".into(),
            output: QueryOutput::Object,
            facts: [FactKind::Owner].into(),
        };
        let facts = MappingFacts {
            version: Some("revision".into()),
            valid_until: Some(Instant::from_unix_secs(100)),
            sets: [("owners".into(), ["u1".into()].into())].into(),
        };
        let hash = |model: &QueryModel, facts: &MappingFacts| {
            let mut hash = Fingerprint::new("mapping-test-v2");
            hash.model(model);
            hash.facts(facts);
            hash.finish()
        };
        let original = hash(&model, &facts);
        assert_eq!(original, "487280d5206a5462cf4b2cdccf8dfad3");
        let changed_models = [
            QueryModel { key: QueryModelKey::parse("other").unwrap(), ..model.clone() },
            QueryModel { resource: "other".into(), ..model.clone() },
            QueryModel { collection: "other".into(), ..model.clone() },
            QueryModel { id_field: "other".into(), ..model.clone() },
            QueryModel { output: QueryOutput::Share { fields: ["id".into()].into() }, ..model.clone() },
            QueryModel { output: QueryOutput::Existence { fields: ["id".into()].into() }, ..model.clone() },
            QueryModel { facts: [FactKind::Warehouse].into(), ..model.clone() },
        ];
        for changed in changed_models {
            assert_ne!(original, hash(&changed, &facts));
        }
        let changed_facts = [
            MappingFacts { version: None, ..facts.clone() },
            MappingFacts { version: Some("other".into()), ..facts.clone() },
            MappingFacts { valid_until: None, ..facts.clone() },
            MappingFacts { valid_until: Some(Instant::from_unix_secs(101)), ..facts.clone() },
            MappingFacts { sets: [("other".into(), ["u1".into()].into())].into(), ..facts.clone() },
            MappingFacts { sets: [("owners".into(), ["u2".into()].into())].into(), ..facts.clone() },
        ];
        for changed in changed_facts {
            assert_ne!(original, hash(&model, &changed));
        }
    }
}
