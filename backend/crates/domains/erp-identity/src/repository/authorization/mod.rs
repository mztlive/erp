//! 将权限语义编译为数据库内条件；根记录只在完整表达式完成后过滤。
pub(crate) mod binding;
#[cfg(test)]
pub(crate) mod fixtures;
mod fragment;
#[cfg(test)]
mod phase5_tests;
mod query;
#[cfg(test)]
pub(crate) mod testing;
#[cfg(feature = "test-support")]
pub(crate) mod unavailable;
use std::collections::BTreeMap;

use binding::FrozenMapping;
use erp_core::common::time::Instant;
use fragment::{Fragment, combine, exists};
use mongodb::bson::{Bson, Document, doc};
use persistence_core::Executor;
pub use query::CompiledAccess;
pub(crate) use query::{
    AccessProof, AccessValidity, QueryQualifications, Replay, ResolvedAccess, ResolvedFilter, ResolvedRules,
    ScopeMetadata, WholeRule,
};
use query::{FilterPlan, SourcePlan};

use crate::entity::authorization::{AccessContext, AccessRule, QueryOutput, RelationKey, RelationMatch};
use crate::entity::fingerprint::Fingerprint;
use crate::entity::resource_access::ResourceKind;
use crate::repository::fingerprint::BsonFingerprint;
use crate::repository::resource_grants::grant_lookup;
use crate::{Error, Permission, Result};

// 汇总本次授权编译所遇到的最早有效期。
struct Compiler {
    // 身份和各静态映射条件的最早失效时点。
    valid_until: Option<Instant>,
}

/// 用服务端签发的授权证明编译固定模型的根过滤、受限投影和已登记的份额、整账或管理计划。沿用原执行边界，聚合有效期和版本，仅在非事务路径保留已解析规则用于交付复验。
pub(crate) fn compile(
    proof: AccessProof,
    qualifications: QueryQualifications,
    mapping: &FrozenMapping,
    executor: &mut dyn Executor,
) -> Result<CompiledAccess> {
    let AccessProof { context, resolved, mut validity, scope } = proof;
    validity.ensure_current(executor)?;
    let model = mapping.model();

    let mut compiler = Compiler { valid_until: validity.valid_until };
    let (mut stages, _) = compiler.model(&resolved.rules.root, mapping, &context, 0)?;
    let whole = match &resolved.rules.whole {
        Some(WholeRule::Root) if model.output == QueryOutput::Object => Some(FilterPlan::Root),
        Some(WholeRule::Root) => return Err(Error::ValidationError("受限查询需要独立整账过滤".into())),
        Some(WholeRule::Separate(rule)) => {
            Some(FilterPlan::Separate(compiler.model(rule, mapping, &context, 0)?.0))
        },
        None => None,
    };
    let sources = if whole.is_some() {
        compiler.source_plans(&resolved.rules.root, &context, mapping)?
    } else {
        BTreeMap::new()
    };
    let management = match &resolved.rules.management {
        Some(ResolvedFilter::Root) => Some(FilterPlan::Root),
        Some(ResolvedFilter::Separate { permission, rule }) => {
            let management_context = AccessContext { permission: permission.clone(), ..context.clone() };
            Some(FilterPlan::Separate(compiler.model(rule, mapping, &management_context, 0)?.0))
        },
        None => None,
    };
    if let QueryOutput::Share { fields } | QueryOutput::Existence { fields } = &model.output {
        let mut projection = doc! { "_id": 0 };
        for field in fields {
            projection.insert(field, 1);
        }
        stages.push(doc! { "$project": projection });
    }

    validity.valid_until = compiler.valid_until;
    let mut version = Fingerprint::new("authorization-query-v4");
    version.text(&resolved.version);
    version.model(model);
    // 保留 v4 中原动态映射依赖的空序列，避免静态查询的分页版本变化。
    version.sequence(std::iter::empty::<&str>(), Fingerprint::text);
    version.sequence(&stages, Fingerprint::document);
    let management_stages = match &management {
        Some(FilterPlan::Separate(stages)) => Some(stages),
        _ => None,
    };
    version.optional(management_stages, |hash, stages| {
        hash.sequence(stages, Fingerprint::document);
    });

    let revalidation = (!validity.boundary.is_snapshot()).then(|| Box::new(resolved));
    let compiled = CompiledAccess {
        context,
        qualifications,
        validity,
        revalidation,
        scope,
        model: model.clone(),
        stages,
        version: version.finish(),
        sources,
        whole,
        management,
    };
    compiled.ensure_current(executor)?;
    Ok(compiled)
}

type SourceRules<'a> = BTreeMap<&'a RelationKey, Vec<(Option<&'a Permission>, &'a AccessRule)>>;

// 为份额子计划借用根层来源规则及独立入口；重复分支保留首次出现的位置。
fn source_rules<'a>(rule: &'a AccessRule, permission: Option<&'a Permission>, output: &mut SourceRules<'a>) {
    match rule {
        AccessRule::Related { relation, rule, .. } => {
            let rules = output.entry(relation).or_default();
            let branch = (permission, rule.as_ref());
            if !rules.contains(&branch) {
                rules.push(branch);
            }
        },
        AccessRule::All(rules) | AccessRule::Any(rules) => {
            for rule in rules {
                source_rules(rule, permission, output);
            }
        },
        AccessRule::Required { rule, .. } => source_rules(rule, permission, output),
        AccessRule::Independent { permission, rule } => source_rules(rule, Some(permission), output),
        _ => {},
    }
}

impl Compiler {
    // 为已登记的根层来源分别编译可见份额过滤，保留各分支入口上下文。
    fn source_plans(
        &mut self,
        rule: &AccessRule,
        root: &AccessContext,
        mapping: &FrozenMapping,
    ) -> Result<BTreeMap<RelationKey, SourcePlan>> {
        let mut rules = BTreeMap::new();
        source_rules(rule, None, &mut rules);
        let mut sources = BTreeMap::new();
        for (key, rules) in rules {
            let child =
                mapping.child(key).ok_or_else(|| Error::ValidationError("缺少来源 Mapping".into()))?;
            let mut fragment = Fragment::with_stages(child.stages());
            let mut conditions = Vec::new();
            for (entry, rule) in rules {
                let context = AccessContext {
                    resource: child.model().resource.clone(),
                    model: Some(child.model().key.clone()),
                    permission: entry.unwrap_or(&root.permission).clone(),
                    ..root.clone()
                };
                conditions.push(self.condition(rule, child, &context, &mut fragment, 0)?.0);
            }

            let stages = fragment.finish(combine("$or", conditions));
            sources.insert(key.clone(), SourcePlan { model: child.model().clone(), stages });
        }
        Ok(sources)
    }

    // 为根或来源模型开启独立关联作用域，结合固定阶段、有效记录条件和权限谓词，返回模型键表达式。
    fn model(
        &mut self,
        rule: &AccessRule,
        mapping: &FrozenMapping,
        context: &AccessContext,
        depth: usize,
    ) -> Result<(Vec<Document>, Bson)> {
        let mut fragment = Fragment::with_stages(mapping.stages());
        let (filter, key) = self.condition(rule, mapping, context, &mut fragment, depth)?;
        Ok((fragment.finish(filter), key))
    }

    // 在调用方给定的关联作用域内用当前上下文求交有效记录与授权条件，返回谓词和主键表达式。
    fn condition(
        &mut self,
        rule: &AccessRule,
        mapping: &FrozenMapping,
        context: &AccessContext,
        fragment: &mut Fragment,
        depth: usize,
    ) -> Result<(Document, Bson)> {
        self.check_mapping(mapping, context)?;
        let key = mapping.key(context)?;
        let records = fragment.fact(mapping.records(context)?)?;
        let access = self.node(rule, mapping, context, fragment, depth)?;
        if records.is_empty() {
            return Ok((access, key));
        }
        Ok((combine("$and", vec![records, access]), key))
    }

    // 模型在冻结时已验证；编译只核对本次上下文和静态期限。
    fn check_mapping(&mut self, mapping: &FrozenMapping, context: &AccessContext) -> Result<()> {
        if let Some(until) = mapping.valid_until(context) {
            if Instant::now() >= until {
                return Err(Error::ConflictError("DATA_SCOPE_CHANGED：静态时间边界已到期".into()));
            }
            self.valid_until = Some(self.valid_until.map_or(until, |old| old.min(until)));
        }
        if context.resource != mapping.model().resource
            || context.model.as_ref() != Some(&mapping.model().key)
        {
            return Err(Error::ValidationError("业务事实映射与授权资源不匹配".into()));
        }

        Ok(())
    }

    // 递归将授权节点转成当前模型的 MongoDB 谓词，校验必需维度与嵌套上限，遇到缺失或超限时拒绝而不截断。
    fn node(
        &mut self,
        rule: &AccessRule,
        mapping: &FrozenMapping,
        context: &AccessContext,
        fragment: &mut Fragment,
        depth: usize,
    ) -> Result<Document> {
        if depth > 16 {
            return Err(Error::ValidationError("授权查询嵌套超限".into()));
        }
        match rule {
            AccessRule::Allow => Ok(doc! { "$expr": true }),
            AccessRule::Deny => Ok(doc! { "$expr": false }),
            AccessRule::All(rules) | AccessRule::Any(rules) => {
                let operator = if matches!(rule, AccessRule::All(_)) { "$and" } else { "$or" };
                let mut children = Vec::new();
                for child in rules {
                    children.push(self.node(child, mapping, context, fragment, depth + 1)?);
                }
                Ok(combine(operator, children))
            },
            AccessRule::Required { facts: required, rule } => {
                if !required.is_subset(&mapping.model().facts) {
                    return Err(Error::ValidationError("Mapping 缺少登记的必需事实".into()));
                }
                self.node(rule, mapping, context, fragment, depth + 1)
            },
            AccessRule::Independent { permission, rule } => {
                let context = AccessContext { permission: permission.clone(), ..context.clone() };
                Ok(self.condition(rule, mapping, &context, fragment, depth + 1)?.0)
            },
            AccessRule::Fact(fact) => {
                if !mapping.model().facts.contains(&fact.kind()) {
                    return Err(Error::ValidationError("Mapping 未声明所需事实维度".into()));
                }
                fragment.fact(mapping.predicate(fact, context)?)
            },
            AccessRule::RecordReadGrant(resource) => self.read_grant(*resource, mapping, context, fragment),
            AccessRule::RelationPresence { relation, present } => {
                relation_presence(mapping, relation, *present, context)
            },
            AccessRule::Related { relation, resource, mode, rule } => {
                self.related((relation, resource, *mode, rule), mapping, context, fragment, depth)
            },
        }
    }

    // 以当前模型的授权主键查找有效记录读取来源，作为补充分支，不把授权行用作根记录。
    fn read_grant(
        &mut self,
        resource: ResourceKind,
        mapping: &FrozenMapping,
        context: &AccessContext,
        fragment: &mut Fragment,
    ) -> Result<Document> {
        if mapping.model().resource != resource.code() {
            return Err(Error::ValidationError("读取授权资源与查询记录不匹配".into()));
        }
        let alias = fragment.lookup(grant_lookup(context.actor.id(), resource, mapping.key(context)?))?;
        Ok(exists(&alias))
    }

    // 按完整原始引用编译来源存在性及范围；需全部命中时比较原引用与通过的来源集合，不让缺失或无权引用消失。
    fn related(
        &mut self,
        relation: (&RelationKey, &str, RelationMatch, &AccessRule),
        mapping: &FrozenMapping,
        context: &AccessContext,
        fragment: &mut Fragment,
        depth: usize,
    ) -> Result<Document> {
        let (key, resource, mode, rule) = relation;
        let (expected_ids, child_mapping) = mapping.relation(key, context)?;
        if child_mapping.model().resource != resource || child_mapping.model().output != QueryOutput::Object {
            return Err(Error::ValidationError("来源 Mapping 与登记资源不一致".into()));
        }
        if mode == RelationMatch::Any && mapping.model().output == QueryOutput::Object {
            return Err(Error::ValidationError("任一来源不能用于完整对象查询".into()));
        }

        let child_context = AccessContext {
            resource: resource.into(),
            model: Some(child_mapping.model().key.clone()),
            ..context.clone()
        };
        let (child, key_expression) = self.model(rule, child_mapping, &child_context, depth + 1)?;

        let expected = doc! { "$setUnion": [{ "$ifNull": [expected_ids, []] }, []] };
        let mut pipeline =
            vec![doc! { "$match": { "$expr": { "$in": [&key_expression, "$$identity_expected"] } } }];
        pipeline.extend(child);
        pipeline.push(doc! { "$project": { "_id": 0, "__identity_id": key_expression } });
        let alias = fragment.lookup(doc! { "from": &child_mapping.model().collection,
        "let": { "identity_expected": expected.clone() }, "pipeline": pipeline })?;

        let mut result = exists(&alias);
        if matches!(mode, RelationMatch::AllNonEmpty | RelationMatch::ExactlyOne) {
            result = doc! { "$expr": { "$and": [
                { "$gt": [{ "$size": expected.clone() }, 0] },
                { "$setEquals": [expected.clone(), format!("${alias}.__identity_id")] }
            ] } };
            if mode == RelationMatch::ExactlyOne {
                result = doc! { "$and": [result,
                    { "$expr": { "$eq": [{ "$size": expected }, 1] } }
                ] };
            }
        } else if matches!(mapping.model().output, QueryOutput::Share { .. }) {
            result = doc! { "$and": [
                result,
                { "$expr": { "$eq": [{ "$size": expected }, 1] } }
            ] };
        }
        Ok(result)
    }
}

// 按去重后的原始引用是否为空生成分区谓词，不在关联准备时剔除根候选。
fn relation_presence(
    mapping: &FrozenMapping,
    relation: &RelationKey,
    present: bool,
    context: &AccessContext,
) -> Result<Document> {
    let (expected_ids, _) = mapping.relation(relation, context)?;
    let size = doc! { "$size": { "$setUnion": [{ "$ifNull": [expected_ids, []] }, []] } };
    Ok(doc! { "$expr": { if present { "$gt" } else { "$eq" }: [size, 0] } })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use application_core::AuditActor;
    use erp_core::AccountKind;
    use erp_permissions::{
        policy_permission, purchase_order, receivable, sales_inquiry, sales_order, supplier_settlement,
    };
    use mongodb::bson::Bson;
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::fixtures::execute;
    use super::*;
    use crate::entity::access_control::{ResolvedScope, ScopeClause};
    use crate::entity::authorization::{
        AccessFact, FactKind, FactValue, ObjectFacts, QueryModel, QueryModelKey, RelatedObjects,
    };
    use crate::entity::resource_access::ResourceKind;
    use crate::ports::{AccessMapping, QueryFragment, RelationMapping};
    use crate::service::authorization::test_rules::{Output, data_condition, source_condition};
    use crate::{Permission, QualifiedPermissions};

    #[test]
    fn source_rules_deduplicate_by_entry_and_rule_in_first_seen_order() {
        let key = RelationKey::parse("sale").unwrap();
        let related = |rule| AccessRule::Related {
            relation: key.clone(),
            resource: "sales_order".into(),
            mode: RelationMatch::ExactlyOne,
            rule: Box::new(rule),
        };
        let independent =
            |permission, rule| AccessRule::Independent { permission, rule: Box::new(related(rule)) };
        let owned = AccessRule::Fact(AccessFact::Owned);
        let rule = AccessRule::Any(vec![
            independent(sales_order::DETAIL, owned.clone()),
            related(AccessRule::Allow),
            independent(sales_order::DETAIL, owned.clone()),
            independent(sales_order::LIST, owned.clone()),
            related(owned.clone()),
        ]);
        let mut grouped = BTreeMap::new();
        source_rules(&rule, None, &mut grouped);
        assert_eq!(
            grouped.values().next().unwrap().as_slice(),
            &[
                (Some(&sales_order::DETAIL), &owned),
                (None, &AccessRule::Allow),
                (Some(&sales_order::LIST), &owned),
                (None, &owned),
            ]
        );
    }

    // 可配置的编译器测试映射，用于模拟事实、来源和动作敏感过滤。
    struct Mapping {
        // 当前样本的模型、资源、集合及支持的事实维度。
        model: QueryModel,
        // 未按关系单独登记时使用的默认来源映射。
        child: Option<Arc<dyn AccessMapping>>,
        // 按关系名称覆盖的来源映射实例。
        sources: BTreeMap<String, Arc<dyn AccessMapping>>,
        // 是否为样本启用未删除记录筛选。
        live_only: bool,
        // 覆盖默认 ID 字段的主键表达式，用于测试计算键。
        key_expression: Option<Bson>,
        // 是否让事实、记录过滤及有效期依赖当前入口动作。
        action_sensitive: bool,
    }

    impl AccessMapping for Mapping {
        // 暴露样本登记的查询模型。
        fn model(&self) -> &QueryModel {
            &self.model
        }
        // 优先返回测试指定的计算键，未指定时引用模型 ID 字段。
        fn key(&self, _: &AccessContext) -> Result<Bson> {
            Ok(self.key_expression.clone().unwrap_or_else(|| format!("${}", self.model.id_field).into()))
        }
        // 独立详情动作的时限早于根动作，供编译器聚合期限。
        fn valid_until(&self, context: &AccessContext) -> Option<Instant> {
            self.action_sensitive.then(|| {
                let ttl = if context.permission == sales_order::DETAIL { 3600 } else { 7200 };
                Instant::from_unix_secs(context.as_of.unix_secs() + ttl)
            })
        }
        // 声明运行时解析的来源及别名。
        fn relations(&self) -> BTreeMap<RelationKey, Arc<dyn AccessMapping>> {
            ["sales", "sales_source", "purchase_source", "settlement_source"]
                .into_iter()
                .filter_map(|key| {
                    let child = self.sources.get(key).or(self.child.as_ref())?;
                    Some((RelationKey::parse(key).unwrap(), child.clone()))
                })
                .collect()
        }
        // 根据测试配置按当前动作或未删除状态限定有效样本记录。
        fn records(&self, context: &AccessContext) -> Result<QueryFragment> {
            Ok(QueryFragment::Match(if self.action_sensitive {
                doc! { "record_action": context.permission.action() }
            } else if self.live_only {
                doc! { "deleted_at": 0 }
            } else {
                doc! {}
            }))
        }
        // 将所有者、组织或历史参与事实转换为测试字段谓词或存在性关联，拒绝其他事实。
        fn predicate(&self, fact: &AccessFact, context: &AccessContext) -> Result<QueryFragment> {
            match fact {
                AccessFact::Owned => {
                    let mut filter = doc! { "sales_owner_user_id": context.actor.id() };
                    if self.action_sensitive {
                        filter.insert("owner_action", context.permission.action());
                    }
                    Ok(QueryFragment::Match(filter))
                },
                AccessFact::OrganizationIn(ids) => Ok(QueryFragment::Match(doc! {
                    "org_unit_id": { "$in": ids.iter().cloned().collect::<Vec<_>>() }
                })),
                AccessFact::HistoryParticipant => Ok(QueryFragment::Exists {
                    collection: "participants".into(),
                    bindings: doc! { "target": format!("${}", self.model.id_field) },
                    pipeline: vec![
                        doc! { "$match": { "user_id": context.actor.id(), "$expr": { "$eq": ["$order", "$$target"] } } },
                    ],
                }),
                _ => Err(Error::ValidationError("unsupported fact".into())),
            }
        }
        // 从测试关系解析实际引用字段，并选用该关系专属映射或默认来源。
        fn relation(&self, key: &RelationKey, _: &AccessContext) -> Result<RelationMapping> {
            let field = match key.as_str() {
                "sales" | "sales_source" => "$sales_ids",
                "purchase_source" => "$purchase_ids",
                "settlement_source" => "$settlement_ids",
                _ => return Err(Error::ValidationError("unknown relation".into())),
            };
            Ok(RelationMapping {
                expected_ids: field.into(),
                mapping: self
                    .sources
                    .get(key.as_str())
                    .cloned()
                    .or_else(|| self.child.clone())
                    .ok_or_else(|| Error::ValidationError("missing relation".into()))?,
            })
        }
    }

    // 构造共享样本集合的完整对象映射，可提供默认来源。
    fn mapping(resource: &str, child: Option<Arc<dyn AccessMapping>>) -> Arc<dyn AccessMapping> {
        Arc::new(Mapping {
            model: QueryModel {
                key: QueryModelKey::parse(resource).unwrap(),
                resource: resource.into(),
                collection: "access_rows".into(),
                id_field: "business_id".into(),
                output: QueryOutput::Object,
                facts: [FactKind::Owner, FactKind::Organization, FactKind::HistoryParticipants].into(),
            },
            child,
            sources: BTreeMap::new(),
            live_only: false,
            key_expression: None,
            action_sensitive: false,
        })
    }

    // 一次测试查询的证明与映射配对，用于在指定执行边界编译。
    struct Query {
        // 测试账号、固定规则及执行边界组成的原证明。
        proof: AccessProof,
        // 与证明模型匹配、供真实编译器调用的测试映射。
        mapping: Arc<FrozenMapping>,
    }

    impl Query {
        // 克隆样本证明并以空角色授权集调用真实查询编译器。
        fn compile(&self, executor: &mut dyn Executor) -> Result<CompiledAccess> {
            let qualifications = QueryQualifications { policy_version: 1, ..Default::default() };
            compile(self.proof.clone(), qualifications, &self.mapping, executor)
        }
    }

    // 将规则绑定到测试账号、映射模型及当前执行器边界，生成可复用测试输入。
    fn query(rule: AccessRule, mapping: Arc<dyn AccessMapping>, executor: &dyn Executor) -> Query {
        let model = mapping.model().clone();
        Query {
            proof: AccessProof {
                context: AccessContext {
                    actor: AuditActor::new("u".into(), "user".into(), AccountKind::Admin),
                    permission: Permission::parse(format!("{}:list", model.resource)).unwrap(),
                    resource: model.resource.clone(),
                    model: Some(model.key.clone()),
                    as_of: Instant::now(),
                },
                resolved: ResolvedAccess {
                    rules: ResolvedRules { root: rule, whole: None, management: None },
                    replay: Replay::Target,
                    version: "identity-v1".into(),
                },
                scope: ScopeMetadata { has_role_scope: false, organization_version: 11 },
                validity: AccessValidity {
                    valid_until: None,
                    boundary: executor.boundary().unwrap(),
                    service: Arc::new(()),
                },
            },
            mapping: FrozenMapping::freeze(mapping, &mut Vec::new(), 0).unwrap(),
        }
    }

    // 验证编译凭证保留原授权证明的入口、账号和组织修订等元数据。
    #[test]
    fn compiled_metadata_comes_from_the_original_authorization_proof() {
        let mut executor = ReadExecutor::default();
        let mapping = mapping("sales_order", None);
        let query = query(AccessRule::Allow, mapping.clone(), &executor);
        let as_of = query.proof.context.as_of;
        let compiled = query.compile(&mut executor).unwrap();
        assert_eq!(compiled.policy_version(), 1);
        assert_eq!(compiled.organization_version(), 11);
        assert_eq!(compiled.as_of(), as_of);
    }

    // 验证并集分支准备来源关联时不会提前丢弃可由其他分支放行的记录。
    #[test]
    fn union_prepares_relations_without_discarding_other_branches() {
        let mut executor = ReadExecutor::default();
        let mapping = mapping("sales_order", None);
        let rule = AccessRule::any(vec![
            AccessRule::Fact(AccessFact::Owned),
            AccessRule::Fact(AccessFact::HistoryParticipant),
        ]);
        let plan = query(rule, mapping.clone(), &executor).compile(&mut executor).unwrap();
        assert_eq!(plan.stages.len(), 3);
        assert!(plan.stages[0].contains_key("$lookup"));
        assert_eq!(
            plan.stages[1],
            doc! { "$match": { "$or": [
                { "sales_owner_user_id": "u" }, { "__identity_fact_0.0": { "$exists": true } }
            ] } }
        );
        assert_eq!(plan.stages[2], doc! { "$unset": ["__identity_fact_0"] });

        let rows = vec![
            doc! { "business_id": "owner", "sales_owner_user_id": "u" },
            doc! { "business_id": "participant", "sales_owner_user_id": "other" },
            doc! { "business_id": "denied", "sales_owner_user_id": "other" },
        ];
        let collections =
            [("participants".into(), vec![doc! { "user_id": "u", "order": "participant" }])].into();
        let result = execute(rows, plan.stages(), &collections, &doc! {});
        assert_eq!(
            result.iter().map(|row| row.get_str("business_id").unwrap()).collect::<Vec<_>>(),
            ["owner", "participant"]
        );
    }

    // 验证读取补充使用映射主键，仍须满足独立范围约束。
    #[test]
    fn grants_use_mapped_key_and_remain_inside_independent_constraints() {
        let mut executor = ReadExecutor::default();
        let mapping = mapping("sales_order", None);
        let rule = AccessRule::all(vec![
            AccessRule::Fact(AccessFact::Owned),
            AccessRule::any(vec![
                AccessRule::Fact(AccessFact::HistoryParticipant),
                AccessRule::RecordReadGrant(ResourceKind::SalesOrder),
            ]),
        ]);
        let plan = query(rule, mapping.clone(), &executor).compile(&mut executor).unwrap();
        assert_eq!(
            plan.stages[1].get_document("$lookup").unwrap().get_document("let").unwrap(),
            &doc! { "identity_key": "$business_id" }
        );

        let conditions = plan.stages[2].get_document("$match").unwrap().get_array("$and").unwrap();
        assert_eq!(conditions[0], doc! { "sales_owner_user_id": "u" }.into());
        assert_eq!(
            conditions[1],
            doc! { "$or": [
                { "__identity_fact_0.0": { "$exists": true } }, { "__identity_fact_1.0": { "$exists": true } }
            ] }
            .into()
        );
    }

    // 验证全部来源规则生成非空检查，并比较原始引用与通过的完整集合。
    #[test]
    fn all_sources_compiles_nonempty_complete_set_comparison() {
        let mut executor = ReadExecutor::default();
        let child = mapping("sales_order", None);
        let root = mapping("invoice", Some(child));
        let rule = AccessRule::Related {
            relation: RelationKey::parse("sales").unwrap(),
            resource: "sales_order".into(),
            mode: RelationMatch::AllNonEmpty,
            rule: Box::new(AccessRule::Fact(AccessFact::Owned)),
        };
        let plan = query(rule, root.clone(), &executor).compile(&mut executor).unwrap();
        let lookup = plan.stages[0].get_document("$lookup").unwrap();
        assert_eq!(lookup.get_str("from").unwrap(), "access_rows");
        let pipeline = lookup.get_array("pipeline").unwrap();
        assert_eq!(
            pipeline[0],
            doc! { "$match": { "$expr": { "$in": ["$business_id", "$$identity_expected"] } } }.into()
        );
        assert_eq!(pipeline[1], doc! { "$match": { "sales_owner_user_id": "u" } }.into());

        let conditions = plan.stages[1]
            .get_document("$match")
            .unwrap()
            .get_document("$expr")
            .unwrap()
            .get_array("$and")
            .unwrap();
        assert!(conditions[0].as_document().unwrap().contains_key("$gt"));
        assert!(conditions[1].as_document().unwrap().contains_key("$setEquals"));
    }

    // 验证事实维度缺失、模型错配或授权资源不符时拒绝编译。
    #[test]
    fn missing_dimensions_wrong_mapping_and_grant_resources_fail() {
        let mut executor = ReadExecutor::default();
        let sales = mapping("sales_order", None);
        let unknown =
            query(AccessRule::Fact(AccessFact::WarehouseIn(["w".into()].into())), sales.clone(), &executor);
        assert!(unknown.compile(&mut executor).is_err());

        let wrong_resource =
            query(AccessRule::RecordReadGrant(ResourceKind::PurchaseOrder), sales.clone(), &executor);
        assert!(wrong_resource.compile(&mut executor).is_err());
    }

    // 验证拒绝规则生成显式为假的谓词，而不是省略查询过滤。
    #[test]
    fn denied_query_compiles_false_instead_of_missing_filter() {
        let mut executor = ReadExecutor::default();
        let mapping = mapping("sales_order", None);
        let plan = query(AccessRule::Deny, mapping.clone(), &executor).compile(&mut executor).unwrap();
        assert_eq!(plan.stages, vec![doc! { "$match": { "$expr": false } }]);
    }

    // 验证受限视图仅投影登记字段，且每行份额仅能对应一个来源。
    #[test]
    fn partial_query_projects_only_registered_fields_and_share_has_one_source() {
        let mut executor = ReadExecutor::default();
        let rule = AccessRule::Related {
            relation: RelationKey::parse("sales").unwrap(),
            resource: "sales_order".into(),
            mode: RelationMatch::Any,
            rule: Box::new(AccessRule::Fact(AccessFact::Owned)),
        };
        let child = mapping("sales_order", None);
        let root = mapping("invoice", Some(child.clone()));
        assert!(query(rule.clone(), root.clone(), &executor).compile(&mut executor).is_err());

        let mut model = root.model().clone();
        model.output = QueryOutput::Share { fields: ["business_id".into(), "share_amount".into()].into() };
        let share: Arc<dyn AccessMapping> = Arc::new(Mapping {
            model,
            child: Some(child),
            sources: BTreeMap::new(),
            live_only: false,
            key_expression: None,
            action_sensitive: false,
        });
        let plan = query(rule, share.clone(), &executor).compile(&mut executor).unwrap();
        assert_eq!(
            plan.stages.last().unwrap(),
            &doc! { "$project": { "_id": 0, "business_id": 1, "share_amount": 1 } }
        );

        let conditions = plan.stages[1].get_document("$match").unwrap().get_array("$and").unwrap();
        assert_eq!(
            conditions[1],
            doc! { "$expr": { "$eq": [
                { "$size": { "$setUnion": [{ "$ifNull": ["$sales_ids", []] }, []] } }, 1
            ] } }
            .into()
        );
    }

    // 验证证明到期或执行器进入新读取阶段后，不能据此编译凭证。
    #[test]
    fn expired_proof_or_wrong_session_cannot_compile() {
        let mut executor = ReadExecutor::default();
        let mapping = mapping("sales_order", None);
        let mut query = query(AccessRule::Allow, mapping.clone(), &executor);
        query.proof.validity.valid_until = Some(Instant::from_unix_secs(1));
        assert!(matches!(query.compile(&mut executor), Err(Error::ConflictError(_))));

        query.proof.validity.valid_until = None;
        executor.next_read();
        assert!(matches!(query.compile(&mut executor), Err(Error::ValidationError(_))));
    }

    #[test]
    fn expired_mapping_is_rejected_before_a_context_mismatch() {
        let mut executor = ReadExecutor::default();
        let mapping = Arc::new(Mapping {
            model: mapping("sales_order", None).model().clone(),
            child: None,
            sources: BTreeMap::new(),
            live_only: false,
            key_expression: None,
            action_sensitive: true,
        });
        let mut query = query(AccessRule::Allow, mapping, &executor);
        query.proof.context.resource = "purchase_order".into();
        assert!(matches!(
            query.compile(&mut executor),
            Err(Error::ValidationError(message)) if message == "业务事实映射与授权资源不匹配"
        ));
        query.proof.context.as_of = Instant::from_unix_secs(1);
        assert!(matches!(
            query.compile(&mut executor),
            Err(Error::ConflictError(message)) if message == "DATA_SCOPE_CHANGED：静态时间边界已到期"
        ));
    }

    // 验证独立范围按自身动作转换，且更早的截止时点限制整份凭证。
    #[test]
    fn independent_scope_compiles_its_own_action_context_and_deadline() {
        let mut executor = ReadExecutor::default();
        let mapping = Arc::new(Mapping {
            model: mapping("sales_order", None).model().clone(),
            child: None,
            sources: BTreeMap::new(),
            live_only: false,
            key_expression: None,
            action_sensitive: true,
        });
        let rule = AccessRule::Independent {
            permission: Permission::parse("sales_order:detail").unwrap(),
            rule: Box::new(AccessRule::Fact(AccessFact::Owned)),
        };
        let query = query(rule, mapping.clone(), &executor);
        let plan = query.compile(&mut executor).unwrap();
        assert_eq!(
            plan.validity.valid_until,
            Some(Instant::from_unix_secs(query.proof.context.as_of.unix_secs() + 3600))
        );
        assert_eq!(
            plan.stages,
            vec![doc! { "$match": { "$and": [
                { "record_action": "list" },
                { "$and": [
                    { "record_action": "detail" },
                    { "sales_owner_user_id": "u", "owner_action": "detail" }
                ] }
            ] } }]
        );
    }

    // 验证辅助过滤的入口上下文及有效期参与编译，过滤变化影响摘要。
    #[test]
    fn auxiliary_filter_uses_the_resolved_context_and_deadline() {
        let mut executor = ReadExecutor::default();
        let mapping = Arc::new(Mapping {
            model: mapping("sales_order", None).model().clone(),
            child: None,
            sources: BTreeMap::new(),
            live_only: false,
            key_expression: None,
            action_sensitive: true,
        });
        let mut query = query(AccessRule::Allow, mapping.clone(), &executor);
        let as_of = query.proof.context.as_of;
        query.proof.resolved.rules.management = Some(ResolvedFilter::Separate {
            permission: sales_order::DETAIL,
            rule: AccessRule::Fact(AccessFact::Owned),
        });

        let plan = query.compile(&mut executor).unwrap();
        assert_eq!(plan.validity.valid_until, Some(Instant::from_unix_secs(as_of.unix_secs() + 3600)));
        assert_eq!(
            plan.management_stages().unwrap(),
            &[doc! { "$match": { "$and": [
                { "record_action": "detail" },
                { "sales_owner_user_id": "u", "owner_action": "detail" }
            ] } }]
        );
        query.proof.resolved.rules.management =
            Some(ResolvedFilter::Separate { permission: sales_order::DETAIL, rule: AccessRule::Deny });
        let denied = query.compile(&mut executor).unwrap();
        assert_eq!(plan.stages(), denied.stages());
        assert_ne!(plan.version(), denied.version());

        query.proof.resolved.rules.management = Some(ResolvedFilter::Root);
        let root = query.compile(&mut executor).unwrap();
        assert_eq!(root.management_stages().unwrap(), root.stages());
    }

    // 验证来源范围即使允许全部，引用对象仍须存在且未删除。
    #[test]
    fn related_shared_scope_still_requires_live_source_record() {
        let mut executor = ReadExecutor::default();
        let child: Arc<dyn AccessMapping> = Arc::new(Mapping {
            model: mapping("purchase_order", None).model().clone(),
            child: None,
            sources: BTreeMap::new(),
            live_only: true,
            key_expression: None,
            action_sensitive: false,
        });
        let root = mapping("invoice", Some(child));
        let rule = AccessRule::Related {
            relation: RelationKey::parse("sales").unwrap(),
            resource: "purchase_order".into(),
            mode: RelationMatch::AllNonEmpty,
            rule: Box::new(AccessRule::Allow),
        };
        let plan = query(rule, root.clone(), &executor).compile(&mut executor).unwrap();
        let pipeline = plan.stages[0].get_document("$lookup").unwrap().get_array("pipeline").unwrap();
        assert_eq!(
            pipeline[1],
            doc! { "$match": { "$and": [{ "deleted_at": 0 }, { "$expr": true }] } }.into()
        );
    }
    // 验证记录读取授权和来源匹配都使用映射提供的计算主键。
    #[test]
    fn calculated_keys_are_used_by_grants_and_related_record_matching() {
        let mut executor = ReadExecutor::default();
        let expression = doc! { "$toString": "$_id" };
        let calculated: Arc<dyn AccessMapping> = Arc::new(Mapping {
            model: mapping("sales_order", None).model().clone(),
            child: None,
            sources: BTreeMap::new(),
            live_only: false,
            key_expression: Some(expression.clone().into()),
            action_sensitive: false,
        });

        let grant =
            query(AccessRule::RecordReadGrant(ResourceKind::SalesOrder), calculated.clone(), &executor);
        let compiled = grant.compile(&mut executor).unwrap();
        assert_eq!(
            compiled.stages[0].get_document("$lookup").unwrap().get_document("let").unwrap(),
            &doc! { "identity_key": expression.clone() }
        );

        let root = mapping("invoice", Some(calculated));
        let rule = AccessRule::Related {
            relation: RelationKey::parse("sales").unwrap(),
            resource: "sales_order".into(),
            mode: RelationMatch::AllNonEmpty,
            rule: Box::new(AccessRule::Allow),
        };
        let compiled = query(rule, root.clone(), &executor).compile(&mut executor).unwrap();
        let pipeline = compiled.stages[0].get_document("$lookup").unwrap().get_array("pipeline").unwrap();
        assert_eq!(
            pipeline[0],
            doc! { "$match": { "$expr": { "$in": [expression.clone(), "$$identity_expected"] } } }.into()
        );
        assert_eq!(
            pipeline.last().unwrap(),
            &doc! { "$project": { "_id": 0, "__identity_id": expression } }.into()
        );
    }

    // 验证即便授权分支允许全部记录，仍须校验映射声明的必需事实。
    #[test]
    fn required_fact_dimensions_are_checked_even_when_rule_allows_every_record() {
        let mut executor = ReadExecutor::default();
        let mapping = mapping("sales_order", None);
        let rule = AccessRule::Allow.requiring(&[FactKind::Warehouse].into());
        assert!(matches!(
            query(rule, mapping.clone(), &executor).compile(&mut executor),
            Err(Error::ValidationError(_))
        ));
        let rule = AccessRule::Allow.requiring(&[FactKind::Owner].into());
        assert!(query(rule, mapping.clone(), &executor).compile(&mut executor).is_ok());
    }

    // 验证询价列表和对象条件的授权判定一致。
    #[test]
    fn inquiry_query_matches_the_object_condition() {
        let mut executor = ReadExecutor::default();
        let mapping = Arc::new(Mapping {
            model: mapping("sales_inquiry", None).model().clone(),
            child: None,
            sources: BTreeMap::new(),
            live_only: true,
            key_expression: None,
            action_sensitive: false,
        });
        let scope = ResolvedScope {
            role_clauses: vec![ScopeClause {
                self_owned: true,
                org_unit_ids: ["team".into()].into(),
                ..Default::default()
            }],
            user_limit: None,
        };
        let rule = data_condition(&sales_inquiry::QUERY_SALES_LIST, Some(&scope)).unwrap();
        let query = query(rule.clone(), mapping.clone(), &executor);
        let plan = query.compile(&mut executor).unwrap();

        let samples = [("own", "u", "other"), ("team", "other", "team"), ("denied", "other", "other")];
        let rows = samples
            .iter()
            .map(|(id, owner, org)| {
                doc! {
                    "business_id": *id, "sales_owner_user_id": *owner, "org_unit_id": *org, "deleted_at": 0
                }
            })
            .collect();
        let result = execute(rows, plan.stages(), &Default::default(), &doc! {});
        let ids = result.iter().map(|row| row.get_str("business_id").unwrap()).collect::<Vec<_>>();
        assert_eq!(ids, ["own", "team"]);

        for (id, owner, org) in samples {
            let object = ObjectFacts {
                resource: "sales_inquiry".into(),
                id: id.into(),
                version: "v1".into(),
                owner_id: FactValue::Known(Some(owner.into())),
                org_unit_id: FactValue::Known(Some(org.into())),
                ..Default::default()
            };
            assert_eq!(
                rule.allows_object(query.proof.context.actor.id(), &object, &Default::default()).unwrap(),
                ids.contains(&id)
            );
        }
    }

    // 对比完整、缺失、无权及已删除来源样本的列表与对象授权成员。
    #[test]
    fn source_query_and_object_membership_agree_for_complete_missing_denied_and_deleted_sources() {
        let mut executor = ReadExecutor::default();
        let child = Arc::new(Mapping {
            model: QueryModel {
                collection: "access_sources".into(),
                ..mapping("sales_order", None).model().clone()
            },
            child: None,
            sources: BTreeMap::new(),
            live_only: true,
            key_expression: None,
            action_sensitive: false,
        });
        let root = Arc::new(Mapping {
            model: mapping("receivable_account", None).model().clone(),
            child: Some(child),
            sources: BTreeMap::new(),
            live_only: true,
            key_expression: None,
            action_sensitive: false,
        });

        let permission = receivable::ACCOUNT_DETAIL;
        let qualified = QualifiedPermissions::test_grants(&[("finance", std::slice::from_ref(&permission))]);
        let scope = ResolvedScope {
            role_clauses: vec![ScopeClause { self_owned: true, ..Default::default() }],
            user_limit: None,
        };
        let rule = source_condition(&permission, &qualified, &[(sales_order::DETAIL, scope)], Output::Object)
            .unwrap();
        let mut query = query(rule.clone(), root.clone(), &executor);
        query.proof.context.permission = permission;
        let plan = query.compile(&mut executor).unwrap();

        let sources = [("s1", "u", 0), ("s2", "u", 0), ("denied", "other", 0), ("deleted", "u", 1)];
        let collections = [("access_sources".into(), sources.iter().map(|(id, owner, deleted)| doc! {
            "business_id": *id, "sales_owner_user_id": *owner, "org_unit_id": "team", "deleted_at": *deleted
        }).collect())].into();
        let samples: [(&str, &[&str]); 5] = [
            ("all", &["s1", "s2"]),
            ("mixed", &["s1", "denied"]),
            ("missing", &["s1", "missing"]),
            ("deleted", &["deleted"]),
            ("empty", &[]),
        ];
        let rows = samples
            .iter()
            .map(|(id, refs)| doc! { "business_id": *id, "sales_ids": refs.to_vec(), "deleted_at": 0 })
            .collect();
        let result = execute(rows, plan.stages(), &collections, &doc! {});
        let ids = result.iter().map(|row| row.get_str("business_id").unwrap()).collect::<Vec<_>>();
        assert_eq!(ids, ["all"]);

        for (id, refs) in samples {
            let children = sources
                .iter()
                .filter(|(id, _, deleted)| refs.contains(id) && *deleted == 0)
                .map(|(id, owner, _)| ObjectFacts {
                    resource: "sales_order".into(),
                    id: (*id).into(),
                    version: "v1".into(),
                    owner_id: FactValue::Known(Some((*owner).into())),
                    org_unit_id: FactValue::Known(Some("team".into())),
                    ..Default::default()
                })
                .collect();
            let object = ObjectFacts {
                resource: "receivable_account".into(),
                id: id.into(),
                version: "v1".into(),
                relations: [(
                    RelationKey::parse("sales_source").unwrap(),
                    RelatedObjects {
                        expected_ids: refs.iter().map(|id| (*id).to_owned()).collect(),
                        objects: children,
                    },
                )]
                .into(),
                ..Default::default()
            };
            assert_eq!(
                rule.allows_object(query.proof.context.actor.id(), &object, &Default::default()).unwrap(),
                ids.contains(&id)
            );
        }
    }

    // 验证多来源查询对每个相同来源只关联一次，且不改变授权成员。
    #[test]
    fn multi_source_query_joins_each_source_once_without_changing_membership() {
        for mut executor in [
            Box::new(ReadExecutor::default()) as Box<dyn Executor>,
            Box::new(SnapshotExecutor::default()) as Box<dyn Executor>,
        ] {
            let source = |resource: &str| Mapping {
                model: QueryModel {
                    collection: format!("sources_{resource}"),
                    ..mapping(resource, None).model().clone()
                },
                child: None,
                sources: BTreeMap::new(),
                live_only: true,
                key_expression: None,
                action_sensitive: false,
            };
            let sales = Arc::new(source("sales_order"));
            let purchase = Arc::new(source("purchase_order"));
            let settlement = Arc::new(source("supplier_settlement_statement"));
            let root = Arc::new(Mapping {
                sources: [
                    ("sales_source".into(), sales.clone() as Arc<dyn AccessMapping>),
                    ("purchase_source".into(), purchase.clone() as Arc<dyn AccessMapping>),
                    ("settlement_source".into(), settlement.clone() as Arc<dyn AccessMapping>),
                ]
                .into(),
                ..source("customer_receipt")
            });

            let permission = receivable::CUSTOMER_RECEIPT_DETAIL;
            let qualified = QualifiedPermissions::test_grants(&[(
                "finance",
                &[permission.clone(), policy_permission::FINANCE_LEDGER_READ],
            )]);
            let scope = ResolvedScope {
                role_clauses: vec![ScopeClause { self_owned: true, ..Default::default() }],
                user_limit: None,
            };
            let scopes = [
                (sales_order::DETAIL, scope.clone()),
                (purchase_order::DETAIL, scope.clone()),
                (supplier_settlement::STATEMENT_LIST, scope),
            ];
            let rule = source_condition(&permission, &qualified, &scopes, Output::Object).unwrap();
            let mut query = query(rule, root.clone(), executor.as_ref());
            query.proof.context.permission = permission;
            let plan = query.compile(executor.as_mut()).unwrap();
            assert_eq!(plan.stages().iter().filter(|stage| stage.contains_key("$lookup")).count(), 3);

            let make_row =
                |id: &str, sales_ids: Vec<&str>, purchase_ids: Vec<&str>, settlement_ids: Vec<&str>| {
                    doc! {
                        "business_id": id, "sales_ids": sales_ids, "purchase_ids": purchase_ids,
                        "settlement_ids": settlement_ids, "deleted_at": 0
                    }
                };
            let allowed = (0..8)
                .map(|mask| {
                    make_row(
                        &format!("partition_{mask}"),
                        if mask & 1 != 0 { vec!["s1", "s1"] } else { vec![] },
                        if mask & 2 != 0 { vec!["p1"] } else { vec![] },
                        if mask & 4 != 0 { vec!["t1"] } else { vec![] },
                    )
                })
                .collect::<Vec<_>>();
            let mut rows = allowed.clone();
            rows.extend([
                make_row("denied_sales", vec!["denied"], vec!["p1"], vec!["t1"]),
                make_row("denied_purchase", vec!["s1"], vec!["denied"], vec!["t1"]),
                make_row("denied_settlement", vec!["s1"], vec!["p1"], vec!["denied"]),
                make_row("missing", vec!["s1", "missing"], vec![], vec![]),
            ]);
            let row = |id: &str, owner: &str| {
                doc! {
                    "business_id": id, "sales_owner_user_id": owner, "org_unit_id": "team", "deleted_at": 0
                }
            };
            let collections = [
                (sales.model.collection.clone(), vec![row("s1", "u"), row("denied", "other")]),
                (purchase.model.collection.clone(), vec![row("p1", "u"), row("denied", "other")]),
                (settlement.model.collection.clone(), vec![row("t1", "u"), row("denied", "other")]),
            ]
            .into();
            let result = execute(rows, plan.stages(), &collections, &doc! {});
            assert_eq!(result, allowed);

            let snapshot = executor.boundary().unwrap().is_snapshot();
            assert_eq!(plan.revalidation.is_none(), snapshot);
        }
    }

    // 验证记录授权关联使用合法变量名，且只接受接收人、目标和版本相符的有效来源。
    #[test]
    fn record_grant_query_uses_valid_variables_and_only_accepts_live_matching_sources() {
        let mut executor = ReadExecutor::default();
        let mapping = mapping("sales_order", None);
        let query = query(AccessRule::RecordReadGrant(ResourceKind::SalesOrder), mapping.clone(), &executor);
        let plan = query.compile(&mut executor).unwrap();

        let grants = [
            ("allowed", "allowed"),
            ("allowed", "second-source"),
            ("stale", "stale"),
            ("wrong_user", "wrong_user"),
            ("wrong_target", "wrong_target"),
        ]
        .into_iter()
        .map(|(id, source)| {
            doc! {
                "recipient": "u", "resource": "sales_order", "action": "read", "active": true,
                "deleted_at": 0, "resource_id": id, "source_id": source, "source_version": 1
            }
        })
        .collect();
        let source = |id: &str, user: &str, version: i32, target: &str| {
            doc! {
                "id": id, "key": { "recipient": user }, "terminated": false, "deleted_at": 0,
                "version": version, "targets": [{ "resource": "sales_order", "id": target }]
            }
        };
        let collections = [
            ("resource_grants".into(), grants),
            (
                "resource_grant_sources".into(),
                vec![
                    source("allowed", "u", 1, "allowed"),
                    source("second-source", "u", 1, "allowed"),
                    source("stale", "u", 2, "stale"),
                    source("wrong_user", "other", 1, "wrong_user"),
                    source("wrong_target", "u", 1, "other"),
                ],
            ),
        ]
        .into();
        let rows = ["allowed", "stale", "wrong_user", "wrong_target", "missing"]
            .iter()
            .map(|id| doc! { "business_id": *id })
            .collect();

        let result = execute(rows, plan.stages(), &collections, &doc! {});
        assert_eq!(result, vec![doc! { "business_id": "allowed" }]);
    }

    // 验证快照凭证不保存复验规则但保留到期限制，不能跨写入或事务复用。
    #[test]
    fn snapshot_mapping_keeps_only_expiry_and_rejects_cross_write_or_transaction_reuse() {
        let mut executor = SnapshotExecutor::default();
        let mapping = Arc::new(Mapping {
            model: mapping("sales_order", None).model().clone(),
            child: None,
            sources: BTreeMap::new(),
            live_only: false,
            key_expression: None,
            action_sensitive: true,
        });
        let query = query(AccessRule::Allow, mapping.clone(), &executor);
        let mut plan = query.compile(&mut executor).unwrap();
        assert!(plan.revalidation.is_none());
        assert!(plan.validity.valid_until.is_some());
        plan.ensure_current(&mut executor).unwrap();

        executor.next_read();
        assert!(matches!(plan.ensure_current(&mut executor), Err(Error::ValidationError(_))));
        executor.restart();
        assert!(matches!(plan.ensure_current(&mut executor), Err(Error::ValidationError(_))));

        plan.validity.boundary = executor.boundary().unwrap();
        plan.validity.valid_until = Some(Instant::from_unix_secs(1));
        assert!(matches!(plan.ensure_current(&mut executor), Err(Error::ConflictError(_))));
    }

    // 验证即使原快照不需重读，交付时也必须先核对服务实例和执行边界。
    #[test]
    fn revalidation_mode_checks_service_and_boundary_before_skipping_snapshot_reads() {
        use persistence_core::NoTransaction;

        let mut snapshot = SnapshotExecutor::default();
        let mapping = mapping("sales_order", None);
        let snapshot_query = query(AccessRule::Allow, mapping.clone(), &snapshot);
        assert!(
            !snapshot_query
                .proof
                .validity
                .needs_revalidation(&snapshot_query.proof.validity.service, &mut snapshot)
                .unwrap()
        );
        assert!(matches!(
            snapshot_query.proof.validity.needs_revalidation(&Arc::new(()), &mut snapshot),
            Err(Error::ValidationError(_))
        ));
        assert!(matches!(snapshot_query.compile(&mut NoTransaction), Err(Error::ValidationError(_))));
        snapshot.session();
        assert!(matches!(
            snapshot_query
                .proof
                .validity
                .needs_revalidation(&snapshot_query.proof.validity.service, &mut snapshot),
            Err(Error::ValidationError(_))
        ));

        let mut executor = ReadExecutor::default();
        let proof = query(AccessRule::Allow, mapping, &executor).proof;
        assert!(proof.validity.needs_revalidation(&proof.validity.service, &mut executor).unwrap());
        executor.next_read();
        assert!(matches!(
            proof.validity.needs_revalidation(&proof.validity.service, &mut executor),
            Err(Error::ValidationError(_))
        ));
    }
}
