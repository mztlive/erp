use super::Fingerprint;
use crate::entity::authorization::{
    AccessFact, AccessRule, FactKind, MappingFacts, QueryModel, QueryOutput, RelationMatch,
};

impl Fingerprint {
    /// 将条件树绑定到授权版本：固定节点标记和字段顺序，交集与并集各自忽略分支排列。
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
            AccessRule::Independent { permission, rule } => {
                self.text("independent");
                self.text(permission.resource());
                self.text(permission.action());
                self.rule(rule);
            },
            AccessRule::RecordReadGrant(resource) => {
                self.text("record_read_grant");
                self.text(resource.code());
            },
            AccessRule::RelationPresence { relation, present } => {
                self.text("relation_presence");
                self.text(relation.as_str());
                self.boolean(*present);
            },
            AccessRule::Related { relation, resource, mode, rule } => {
                self.text("related");
                self.text(relation.as_str());
                self.text(resource);
                self.text(match mode {
                    RelationMatch::Any => "any",
                    RelationMatch::AllNonEmpty => "all_non_empty",
                    RelationMatch::ExactlyOne => "exactly_one",
                });
                self.rule(rule);
            },
        }
    }

    // 先写事实维度，再按条件写入无序 ID 集合或期望的任务完成标记。
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
            AccessFact::Completed(completed) => self.boolean(*completed),
        }
    }

    // 使用固定协议文本标记事实维度，使枚举重命名或重排不改变编码。
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
            FactKind::Completed => "completed",
        });
    }

    /// 将模型标识、资源、集合、行主键、输出粒度及投影字段和支持的事实维度绑定到查询版本。
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

    /// 写入动态映射的修订号与到期时刻以绑定依赖，时间保留秒和纳秒；准备数据不直接编码。
    pub(crate) fn facts(&mut self, facts: &MappingFacts) {
        let MappingFacts { version, valid_until, .. } = facts;
        self.optional(version.as_deref(), Self::text);
        self.optional(*valid_until, |hash, until| {
            hash.text(&until.unix_secs().to_string());
            hash.number(u64::from(until.as_utc().timestamp_subsec_nanos()));
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

    // 验证样例节点的编码可区分并锁定固定摘要，交集重排不变且与并集有别。
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
    }

    // 验证任务完成、未完成和任务参与分别产生独立版本。
    #[test]
    fn task_completion_values_have_distinct_fingerprints() {
        let facts = [AccessFact::Completed(false), AccessFact::Completed(true), AccessFact::Participant];
        let hashes = facts
            .into_iter()
            .map(|fact| {
                let mut hash = Fingerprint::new("rule-test-v2");
                hash.rule(&AccessRule::Fact(fact));
                hash.finish()
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(hashes.len(), 3);
    }

    // 锁定模型与动态依赖的固定摘要，并验证模型字段、修订号和到期变化均影响版本。
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
            data: Default::default(),
            version: Some("revision".into()),
            valid_until: Some(Instant::from_unix_secs(100)),
        };
        let hash = |model: &QueryModel, facts: &MappingFacts| {
            let mut hash = Fingerprint::new("mapping-test-v3");
            hash.model(model);
            hash.facts(facts);
            hash.finish()
        };

        let original = hash(&model, &facts);
        assert_eq!(original, "1d4065d4218375da9e6d5b8c6eeda8a4");

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
        ];
        for changed in changed_facts {
            assert_ne!(original, hash(&model, &changed));
        }
    }

    // 验证即使内层数据条件相同，独立入口权限变化也会改变版本。
    #[test]
    fn independent_permission_is_bound_even_when_resolved_scope_is_identical() {
        let version = |action| {
            let mut hash = Fingerprint::new("independent-test-v1");
            hash.rule(&AccessRule::Independent {
                permission: crate::Permission::parse(format!("sales_order:{action}")).unwrap(),
                rule: Box::new(AccessRule::Allow),
            });
            hash.finish()
        };
        assert_ne!(version("detail"), version("list"));
    }
}
