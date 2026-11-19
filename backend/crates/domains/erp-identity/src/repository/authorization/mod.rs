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
use std::sync::Arc;

use erp_core::common::time::Instant;
use fragment::{Fragment, combine, exists};
use futures_util::future::BoxFuture;
use mongodb::bson::{Bson, Document, doc};
use persistence_core::{ExecutionBoundary, Executor};
pub use query::CompiledAccess;
pub(crate) use query::{AccessProof, ApprovalSource, MappingDependency, ResolvedFilter};
use query::{ManagementPlan, SourcePlan};

use crate::entity::authorization::{
    AccessContext, AccessRule, MappingFacts, QueryModel, QueryOutput, RelationKey, RelationMatch,
};
use crate::entity::fingerprint::Fingerprint;
use crate::entity::resource_access::ResourceKind;
use crate::ports::AccessMapping;
use crate::repository::fingerprint::BsonFingerprint;
use crate::repository::resource_grants::grant_lookup;
use crate::{Error, Permission, Result};

/// 一次编译共享动态准备结果及复验依赖；关联别名由各集合的 Fragment 管理。
struct Compiler<'a> {
    executor: &'a mut dyn Executor,
    boundary: ExecutionBoundary,
    valid_until: Option<Instant>,
    prepared: Vec<MappingDependency>,
    versions: Vec<String>,
}

// 接收服务端生成的完整授权证明，编译固定模型的过滤与投影，同时保留交付时需要检查的依赖。
pub(crate) async fn compile(
    proof: &AccessProof,
    mapping: &Arc<dyn AccessMapping>,
    model: &QueryModel,
    executor: &mut dyn Executor,
) -> Result<CompiledAccess> {
    proof.ensure_current(executor)?;
    if mapping.model() != model {
        return Err(Error::ValidationError("Mapping 描述与启动绑定不一致".into()));
    }

    let mut compiler = Compiler {
        executor,
        boundary: proof.boundary.clone(),
        valid_until: proof.valid_until,
        prepared: vec![],
        versions: vec![],
    };
    let (fragment, _) = compiler.model(&proof.rule, mapping, &proof.context, 0).await?;
    let mut stages = fragment.finish();
    let (sources, whole) = compiler.plans(proof, mapping).await?;
    let management = match &proof.management_filter {
        Some(ResolvedFilter::Root) => Some(ManagementPlan::Root),
        Some(ResolvedFilter::Separate { context, rule }) => {
            Some(ManagementPlan::Separate(compiler.model(rule, mapping, context, 0).await?.0.finish()))
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

    let mut proof = proof.clone();
    proof.valid_until = compiler.valid_until;
    let mut version = Fingerprint::new("authorization-query-v4");
    version.text(&proof.version);
    version.model(model);
    version.unordered(&compiler.versions, |hash, value| hash.text(value));
    version.sequence(&stages, |hash, stage| hash.bson(&Bson::Document(stage.clone())));
    let management_stages = match &management {
        Some(ManagementPlan::Separate(stages)) => Some(stages),
        _ => None,
    };
    version.optional(management_stages, |hash, stages| {
        hash.sequence(stages, |hash, stage| hash.bson(&Bson::Document(stage.clone())));
    });

    let compiled = CompiledAccess {
        proof,
        model: model.clone(),
        stages,
        dependencies: if compiler.boundary.is_snapshot() { vec![] } else { compiler.prepared },
        version: version.finish(),
        sources,
        whole,
        management,
    };
    compiled.ensure_current(compiler.executor)?;
    Ok(compiled)
}

// 为份额计划收集根层来源关系的授权规则；嵌套关系仍由各自来源模型独立编译。
fn source_rules(
    rule: &AccessRule,
    permission: Option<&Permission>,
    output: &mut BTreeMap<RelationKey, Vec<(Option<Permission>, AccessRule)>>,
) {
    match rule {
        AccessRule::Related { relation, rule, .. } => {
            let rules = output.entry(relation.clone()).or_default();
            let branch = (permission.cloned(), rule.as_ref().clone());
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

impl Compiler<'_> {
    // 复合资金查询需要来源份额和整账时，使用同一次身份、范围及映射准备编译两类固定计划，沿用根执行边界。
    async fn plans(
        &mut self,
        proof: &AccessProof,
        mapping: &Arc<dyn AccessMapping>,
    ) -> Result<(BTreeMap<RelationKey, SourcePlan>, Option<Vec<Document>>)> {
        let Some(whole_rule) = &proof.whole_rule else {
            return Ok((BTreeMap::new(), None));
        };
        let whole = self.model(whole_rule, mapping, &proof.context, 0).await?.0.finish();

        let mut rules = BTreeMap::new();
        source_rules(&proof.rule, None, &mut rules);
        let mut sources = BTreeMap::new();
        for (key, rules) in rules {
            let child = mapping
                .relations()
                .remove(&key)
                .ok_or_else(|| Error::ValidationError("缺少来源 Mapping".into()))?;
            let mut fragment = Fragment::with_stages(child.stages());
            let mut conditions = Vec::new();
            for (entry, rule) in rules {
                let context = AccessContext {
                    resource: child.model().resource.clone(),
                    model: Some(child.model().key.clone()),
                    permission: entry.unwrap_or_else(|| proof.context.permission.clone()),
                    ..proof.context.clone()
                };
                conditions.push(self.condition(&rule, &child, &context, &mut fragment, 0).await?.0);
            }

            fragment.filter = combine("$or", conditions);
            let stages = fragment.finish();
            sources.insert(key, SourcePlan { model: child.model().clone(), stages });
        }
        Ok((sources, Some(whole)))
    }

    // 构造根或来源模型的条件片段；各模型都应用有效行约束，即便公司范围分支也不读取已删除记录。
    fn model<'a>(
        &'a mut self,
        rule: &'a AccessRule,
        mapping: &'a Arc<dyn AccessMapping>,
        context: &'a AccessContext,
        depth: usize,
    ) -> BoxFuture<'a, Result<(Fragment, Bson)>> {
        Box::pin(async move {
            let mut fragment = Fragment::with_stages(mapping.stages());
            let (filter, key) = self.condition(rule, mapping, context, &mut fragment, depth).await?;
            fragment.filter = filter;
            Ok((fragment, key))
        })
    }

    // 编译独立资格条件时复用所在集合的关联作用域；来源子查询使用自己的作用域，避免临时别名冲突。
    fn condition<'a>(
        &'a mut self,
        rule: &'a AccessRule,
        mapping: &'a Arc<dyn AccessMapping>,
        context: &'a AccessContext,
        fragment: &'a mut Fragment,
        depth: usize,
    ) -> BoxFuture<'a, Result<(Document, Bson)>> {
        Box::pin(async move {
            let facts = self.prepare(mapping, context).await?;
            let key = mapping.key(context, facts.as_ref())?;
            let records = fragment.fact(mapping.records(context, facts.as_ref())?)?;
            let access = self.node(rule, mapping, context, facts.as_ref(), fragment, depth).await?;
            if records.is_empty() {
                return Ok((access, key));
            }
            Ok((combine("$and", vec![records, access]), key))
        })
    }

    // 编译时按 Mapping 实例及完整授权上下文缓存实际准备事实，仅在本次读取阶段复用。静态映射不制造空事实；有效期和资源标识仍须核验。
    async fn prepare(
        &mut self,
        mapping: &Arc<dyn AccessMapping>,
        context: &AccessContext,
    ) -> Result<Option<MappingFacts>> {
        mapping.model().validate()?;
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

        let Some(preparation) = mapping.preparation() else {
            return Ok(None);
        };
        if let Some(cached) = self
            .prepared
            .iter()
            .find(|cached| Arc::ptr_eq(&cached.mapping, mapping) && cached.context == *context)
        {
            cached.facts.validate(self.boundary.is_snapshot(), Instant::now())?;
            return Ok(Some(cached.facts.clone()));
        }

        let facts = preparation.prepare(context, self.executor).await?;
        if !self.boundary.is_current(self.executor) {
            return Err(Error::ValidationError("Mapping 准备改变了授权读取阶段".into()));
        }
        facts.validate(self.boundary.is_snapshot(), Instant::now())?;
        if let Some(until) = facts.valid_until {
            self.valid_until = Some(self.valid_until.map_or(until, |old| old.min(until)));
        }

        let key = mapping.key(context, Some(&facts))?;
        let mut version = Fingerprint::new("mapping-preparation-v1");
        version.model(mapping.model());
        version.facts(&facts);
        version.json(&facts.data);
        version.bson(&key);
        self.versions.push(version.finish());
        self.prepared.push(MappingDependency {
            mapping: mapping.clone(),
            model: mapping.model().clone(),
            context: context.clone(),
            facts: facts.clone(),
            key,
        });
        Ok(Some(facts))
    }

    // 在当前查询模型内递归编译单个授权节点；规则登记过深时拒绝，不能截断后继续执行。
    fn node<'a>(
        &'a mut self,
        rule: &'a AccessRule,
        mapping: &'a Arc<dyn AccessMapping>,
        context: &'a AccessContext,
        facts: Option<&'a MappingFacts>,
        fragment: &'a mut Fragment,
        depth: usize,
    ) -> BoxFuture<'a, Result<Document>> {
        Box::pin(async move {
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
                        children.push(self.node(child, mapping, context, facts, fragment, depth + 1).await?);
                    }
                    Ok(combine(operator, children))
                },
                AccessRule::Required { facts: required, rule } => {
                    if !required.is_subset(&mapping.model().facts) {
                        return Err(Error::ValidationError("Mapping 缺少登记的必需事实".into()));
                    }
                    self.node(rule, mapping, context, facts, fragment, depth + 1).await
                },
                AccessRule::Independent { permission, rule } => {
                    let context = AccessContext { permission: permission.clone(), ..context.clone() };
                    Ok(self.condition(rule, mapping, &context, fragment, depth + 1).await?.0)
                },
                AccessRule::Fact(fact) => {
                    if !mapping.model().facts.contains(&fact.kind()) {
                        return Err(Error::ValidationError("Mapping 未声明所需事实维度".into()));
                    }
                    fragment.fact(mapping.predicate(fact, context, facts)?)
                },
                AccessRule::RecordReadGrant(resource) => {
                    self.read_grant(*resource, mapping.as_ref(), context, facts, fragment)
                },
                AccessRule::RelationPresence { relation, present } => {
                    relation_presence(mapping.as_ref(), relation, *present, context, facts)
                },
                AccessRule::Related { relation, resource, mode, rule } => {
                    self.related((relation, resource, *mode, rule), mapping, context, facts, fragment, depth)
                        .await
                },
            }
        })
    }

    // 编译记录读取补充分支时，以当前模型主键查找该账号可用的有效来源，不把授权表变成根查询。
    fn read_grant(
        &mut self,
        resource: ResourceKind,
        mapping: &dyn AccessMapping,
        context: &AccessContext,
        facts: Option<&MappingFacts>,
        fragment: &mut Fragment,
    ) -> Result<Document> {
        if mapping.model().resource != resource.code() {
            return Err(Error::ValidationError("读取授权资源与查询记录不匹配".into()));
        }
        let alias =
            fragment.lookup(grant_lookup(context.actor.id(), resource, mapping.key(context, facts)?))?;
        Ok(exists(&alias))
    }

    // 按真实引用编译来源条件；需要完整覆盖时比较全部原始引用与通过来源集合，不能先过滤掉缺失或无权引用。
    async fn related(
        &mut self,
        relation: (&RelationKey, &str, RelationMatch, &AccessRule),
        mapping: &Arc<dyn AccessMapping>,
        context: &AccessContext,
        facts: Option<&MappingFacts>,
        fragment: &mut Fragment,
        depth: usize,
    ) -> Result<Document> {
        let (key, resource, mode, rule) = relation;
        let related = mapping.relation(key, context, facts)?;
        if related.mapping.model().resource != resource
            || related.mapping.model().output != QueryOutput::Object
        {
            return Err(Error::ValidationError("来源 Mapping 与登记资源不一致".into()));
        }
        if mode == RelationMatch::Any && mapping.model().output == QueryOutput::Object {
            return Err(Error::ValidationError("任一来源不能用于完整对象查询".into()));
        }

        let child_context = AccessContext {
            resource: resource.into(),
            model: Some(related.mapping.model().key.clone()),
            ..context.clone()
        };
        let (child, key_expression) = self.model(rule, &related.mapping, &child_context, depth + 1).await?;

        let expected = doc! { "$setUnion": [{ "$ifNull": [related.expected_ids, []] }, []] };
        let mut pipeline =
            vec![doc! { "$match": { "$expr": { "$in": [&key_expression, "$$identity_expected"] } } }];
        pipeline.extend(child.finish());
        pipeline.push(doc! { "$project": { "_id": 0, "__identity_id": key_expression } });
        let alias = fragment.lookup(doc! { "from": &related.mapping.model().collection,
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

// 为来源分区计算完整引用是否存在的条件，仅形成谓词，不在关联准备时剔除根候选。
fn relation_presence(
    mapping: &dyn AccessMapping,
    relation: &RelationKey,
    present: bool,
    context: &AccessContext,
    facts: Option<&MappingFacts>,
) -> Result<Document> {
    let related = mapping.relation(relation, context, facts)?;
    let size = doc! { "$size": { "$setUnion": [{ "$ifNull": [related.expected_ids, []] }, []] } };
    Ok(doc! { "$expr": { if present { "$gt" } else { "$eq" }: [size, 0] } })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicU64, Ordering};

    use application_core::AuditActor;
    use async_trait::async_trait;
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
    use crate::ports::{MappingPreparation, QueryFragment, RelationMapping};
    use crate::service::authorization::rules::sources::{Output, data_condition as source_condition};
    use crate::service::authorization::rules::{self};
    use crate::{Permission, QualifiedPermissions};

    struct Mapping {
        model: QueryModel,
        child: Option<Arc<dyn AccessMapping>>,
        versioned: Option<bool>,
        revision: AtomicU64,
        prepare_calls: AtomicU64,
        sources: BTreeMap<String, Arc<dyn AccessMapping>>,
        live_only: bool,
        key_expression: Option<Bson>,
        action_sensitive: bool,
    }

    impl AccessMapping for Mapping {
        fn model(&self) -> &QueryModel {
            &self.model
        }
        fn key(&self, _: &AccessContext, _: Option<&MappingFacts>) -> Result<Bson> {
            Ok(self.key_expression.clone().unwrap_or_else(|| format!("${}", self.model.id_field).into()))
        }
        fn preparation(&self) -> Option<&dyn MappingPreparation> {
            self.versioned.map(|_| self as &dyn MappingPreparation)
        }
        fn records(&self, context: &AccessContext, _: Option<&MappingFacts>) -> Result<QueryFragment> {
            Ok(QueryFragment::Match(if self.action_sensitive {
                doc! { "record_action": context.permission.action() }
            } else if self.live_only {
                doc! { "deleted_at": 0 }
            } else {
                doc! {}
            }))
        }
        fn predicate(
            &self,
            fact: &AccessFact,
            context: &AccessContext,
            _: Option<&MappingFacts>,
        ) -> Result<QueryFragment> {
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
        fn relation(
            &self,
            key: &RelationKey,
            _: &AccessContext,
            _: Option<&MappingFacts>,
        ) -> Result<RelationMapping> {
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

    #[async_trait]
    impl MappingPreparation for Mapping {
        async fn prepare(&self, context: &AccessContext, _: &mut dyn Executor) -> Result<MappingFacts> {
            assert!(self.versioned.is_some(), "static mappings must never prepare");
            self.prepare_calls.fetch_add(1, Ordering::Relaxed);
            Ok(MappingFacts {
                data: Default::default(),
                version: self.versioned.unwrap_or(false).then(|| {
                    if self.action_sensitive {
                        format!(
                            "facts-{}-v{}",
                            context.permission.action(),
                            self.revision.load(Ordering::Relaxed)
                        )
                    } else {
                        format!("facts-v{}", self.revision.load(Ordering::Relaxed))
                    }
                }),
                valid_until: self
                    .action_sensitive
                    .then(|| Instant::from_unix_secs(context.as_of.unix_secs() + 3600)),
            })
        }
    }

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
            versioned: Some(true),
            revision: AtomicU64::new(1),
            prepare_calls: AtomicU64::new(0),
            sources: BTreeMap::new(),
            live_only: false,
            key_expression: None,
            action_sensitive: false,
        })
    }

    struct Query {
        proof: AccessProof,
        mapping: Arc<dyn AccessMapping>,
        model: QueryModel,
    }

    impl Query {
        async fn compile(&self, executor: &mut dyn Executor) -> Result<CompiledAccess> {
            compile(&self.proof, &self.mapping, &self.model, executor).await
        }
    }

    fn query(rule: AccessRule, mapping: Arc<dyn AccessMapping>, executor: &dyn Executor) -> Query {
        let model = mapping.model().clone();
        Query {
            proof: AccessProof {
                approval_source: None,
                context: AccessContext {
                    actor: AuditActor::new("u".into(), "user".into(), AccountKind::Admin),
                    permission: Permission::parse(format!("{}:list", model.resource)).unwrap(),
                    resource: model.resource.clone(),
                    model: Some(model.key.clone()),
                    as_of: Instant::now(),
                },
                qualified: crate::QualifiedPermissions::test_grants(&[]),
                task: false,
                whole_rule: None,
                management_filter: None,
                has_role_scope: false,
                rule,
                version: "identity-v1".into(),
                valid_until: None,
                policy_version: 7,
                organization_version: 11,
                boundary: executor.boundary().unwrap(),
                service: Arc::new(()),
            },
            mapping,
            model,
        }
    }

    #[tokio::test]
    async fn compiled_metadata_comes_from_the_original_authorization_proof() {
        let mut executor = ReadExecutor::default();
        let mapping = mapping("sales_order", None);
        let query = query(AccessRule::Allow, mapping.clone(), &executor);
        let as_of = query.proof.context.as_of;
        let compiled = query.compile(&mut executor).await.unwrap();
        assert_eq!(compiled.policy_version(), 7);
        assert_eq!(compiled.organization_version(), 11);
        assert_eq!(compiled.as_of(), as_of);
    }

    #[tokio::test]
    async fn union_prepares_relations_without_discarding_other_branches() {
        let mut executor = ReadExecutor::default();
        let mapping = mapping("sales_order", None);
        let rule = AccessRule::any(vec![
            AccessRule::Fact(AccessFact::Owned),
            AccessRule::Fact(AccessFact::HistoryParticipant),
        ]);
        let plan = query(rule, mapping.clone(), &executor).compile(&mut executor).await.unwrap();
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

    #[tokio::test]
    async fn grants_use_mapped_key_and_remain_inside_independent_constraints() {
        let mut executor = ReadExecutor::default();
        let mapping = mapping("sales_order", None);
        let rule = AccessRule::all(vec![
            AccessRule::Fact(AccessFact::Owned),
            AccessRule::any(vec![
                AccessRule::Fact(AccessFact::HistoryParticipant),
                AccessRule::RecordReadGrant(ResourceKind::SalesOrder),
            ]),
        ]);
        let plan = query(rule, mapping.clone(), &executor).compile(&mut executor).await.unwrap();
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

    #[tokio::test]
    async fn all_sources_compiles_nonempty_complete_set_comparison() {
        let mut executor = ReadExecutor::default();
        let child = mapping("sales_order", None);
        let root = mapping("invoice", Some(child));
        let rule = AccessRule::Related {
            relation: RelationKey::parse("sales").unwrap(),
            resource: "sales_order".into(),
            mode: RelationMatch::AllNonEmpty,
            rule: Box::new(AccessRule::Fact(AccessFact::Owned)),
        };
        let plan = query(rule, root.clone(), &executor).compile(&mut executor).await.unwrap();
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
        assert_eq!(plan.dependencies.len(), 2);
    }

    #[tokio::test]
    async fn missing_dimensions_wrong_mapping_and_grant_resources_fail() {
        let mut executor = ReadExecutor::default();
        let sales = mapping("sales_order", None);
        let unknown =
            query(AccessRule::Fact(AccessFact::WarehouseIn(["w".into()].into())), sales.clone(), &executor);
        assert!(unknown.compile(&mut executor).await.is_err());

        let wrong_resource =
            query(AccessRule::RecordReadGrant(ResourceKind::PurchaseOrder), sales.clone(), &executor);
        assert!(wrong_resource.compile(&mut executor).await.is_err());
    }

    #[tokio::test]
    async fn unversioned_nontransaction_facts_are_rejected() {
        let mut executor = ReadExecutor::default();
        let mapping: Arc<dyn AccessMapping> = Arc::new(Mapping {
            model: mapping("sales_order", None).model().clone(),
            child: None,
            versioned: Some(false),
            revision: AtomicU64::new(1),
            prepare_calls: AtomicU64::new(0),
            sources: BTreeMap::new(),
            live_only: false,
            key_expression: None,
            action_sensitive: false,
        });
        assert!(query(AccessRule::Allow, mapping.clone(), &executor).compile(&mut executor).await.is_err());
    }

    #[tokio::test]
    async fn denied_query_compiles_false_instead_of_missing_filter() {
        let mut executor = ReadExecutor::default();
        let mapping = mapping("sales_order", None);
        let plan = query(AccessRule::Deny, mapping.clone(), &executor).compile(&mut executor).await.unwrap();
        assert_eq!(plan.stages, vec![doc! { "$match": { "$expr": false } }]);
    }

    #[tokio::test]
    async fn partial_query_projects_only_registered_fields_and_share_has_one_source() {
        let mut executor = ReadExecutor::default();
        let rule = AccessRule::Related {
            relation: RelationKey::parse("sales").unwrap(),
            resource: "sales_order".into(),
            mode: RelationMatch::Any,
            rule: Box::new(AccessRule::Fact(AccessFact::Owned)),
        };
        let child = mapping("sales_order", None);
        let root = mapping("invoice", Some(child.clone()));
        assert!(query(rule.clone(), root.clone(), &executor).compile(&mut executor).await.is_err());

        let mut model = root.model().clone();
        model.output = QueryOutput::Share { fields: ["business_id".into(), "share_amount".into()].into() };
        let share: Arc<dyn AccessMapping> = Arc::new(Mapping {
            model,
            child: Some(child),
            versioned: Some(true),
            revision: AtomicU64::new(1),
            prepare_calls: AtomicU64::new(0),
            sources: BTreeMap::new(),
            live_only: false,
            key_expression: None,
            action_sensitive: false,
        });
        let plan = query(rule, share.clone(), &executor).compile(&mut executor).await.unwrap();
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

    #[tokio::test]
    async fn mapping_revision_changes_invalidate_dependencies_and_page_version() {
        let mut executor = ReadExecutor::default();
        let live = Arc::new(Mapping {
            model: mapping("sales_order", None).model().clone(),
            child: None,
            versioned: Some(true),
            revision: AtomicU64::new(1),
            prepare_calls: AtomicU64::new(0),
            sources: BTreeMap::new(),
            live_only: false,
            key_expression: None,
            action_sensitive: false,
        });
        let query = query(AccessRule::Allow, live.clone(), &executor);
        let plan = query.compile(&mut executor).await.unwrap();
        assert!(plan.dependencies[0].validate(&mut executor).await.is_ok());

        live.revision.store(2, Ordering::Relaxed);
        assert!(matches!(plan.dependencies[0].validate(&mut executor).await, Err(Error::ConflictError(_))));
        let updated = query.compile(&mut executor).await.unwrap();
        assert_ne!(plan.version(), updated.version());
    }

    #[tokio::test]
    async fn expired_proof_or_wrong_session_cannot_compile() {
        let mut executor = ReadExecutor::default();
        let mapping = mapping("sales_order", None);
        let mut query = query(AccessRule::Allow, mapping.clone(), &executor);
        query.proof.valid_until = Some(Instant::from_unix_secs(1));
        assert!(matches!(query.compile(&mut executor).await, Err(Error::ConflictError(_))));

        query.proof.valid_until = None;
        executor.next_read();
        assert!(matches!(query.compile(&mut executor).await, Err(Error::ValidationError(_))));
    }

    #[tokio::test]
    async fn independent_scope_compiles_and_revalidates_its_own_action_context() {
        let mut executor = ReadExecutor::default();
        let mapping = Arc::new(Mapping {
            model: mapping("sales_order", None).model().clone(),
            child: None,
            versioned: Some(true),
            revision: AtomicU64::new(1),
            prepare_calls: AtomicU64::new(0),
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
        let plan = query.compile(&mut executor).await.unwrap();
        assert_eq!(plan.dependencies.len(), 2);
        assert_eq!(plan.dependencies[0].context.permission.action(), "list");
        assert_eq!(plan.dependencies[1].context.permission.action(), "detail");
        assert_eq!(plan.dependencies[1].facts.version.as_deref(), Some("facts-detail-v1"));
        assert!(plan.dependencies[1].facts.valid_until.is_some());
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
        assert!(plan.dependencies[1].validate(&mut executor).await.is_ok());

        mapping.revision.store(2, Ordering::Relaxed);
        assert!(matches!(plan.dependencies[1].validate(&mut executor).await, Err(Error::ConflictError(_))));
    }

    #[tokio::test]
    async fn auxiliary_filter_uses_the_resolved_context_and_keeps_its_own_mapping_dependency() {
        let mut executor = ReadExecutor::default();
        let mapping = Arc::new(Mapping {
            model: mapping("sales_order", None).model().clone(),
            child: None,
            versioned: Some(true),
            revision: AtomicU64::new(1),
            prepare_calls: AtomicU64::new(0),
            sources: BTreeMap::new(),
            live_only: false,
            key_expression: None,
            action_sensitive: true,
        });
        let mut query = query(AccessRule::Allow, mapping.clone(), &executor);
        let context = AccessContext { permission: sales_order::DETAIL, ..query.proof.context.clone() };
        query.proof.management_filter = Some(ResolvedFilter::Separate {
            context: Box::new(context.clone()),
            rule: AccessRule::Fact(AccessFact::Owned),
        });

        let plan = query.compile(&mut executor).await.unwrap();
        assert_eq!(plan.dependencies.len(), 2);
        assert_eq!(plan.dependencies[0].context, query.proof.context);
        let dependency = &plan.dependencies[1];
        assert_eq!(dependency.context, context);
        assert_eq!(dependency.facts.version.as_deref(), Some("facts-detail-v1"));
        assert_eq!(plan.proof.valid_until, dependency.facts.valid_until);
        assert_eq!(
            plan.management_stages().unwrap(),
            &[doc! { "$match": { "$and": [
                { "record_action": "detail" },
                { "sales_owner_user_id": "u", "owner_action": "detail" }
            ] } }]
        );
        dependency.validate(&mut executor).await.unwrap();
        query.proof.management_filter =
            Some(ResolvedFilter::Separate { context: Box::new(context), rule: AccessRule::Deny });
        let denied = query.compile(&mut executor).await.unwrap();
        assert_eq!(plan.stages(), denied.stages());
        assert_ne!(plan.version(), denied.version());

        query.proof.management_filter = Some(ResolvedFilter::Root);
        let root = query.compile(&mut executor).await.unwrap();
        assert_eq!(root.management_stages().unwrap(), root.stages());
        assert_eq!(root.dependencies.len(), 1);

        mapping.revision.store(2, Ordering::Relaxed);
        assert!(matches!(dependency.validate(&mut executor).await, Err(Error::ConflictError(_))));
    }

    #[tokio::test]
    async fn related_shared_scope_still_requires_live_source_record() {
        let mut executor = ReadExecutor::default();
        let child: Arc<dyn AccessMapping> = Arc::new(Mapping {
            model: mapping("purchase_order", None).model().clone(),
            child: None,
            versioned: Some(true),
            revision: AtomicU64::new(1),
            prepare_calls: AtomicU64::new(0),
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
        let plan = query(rule, root.clone(), &executor).compile(&mut executor).await.unwrap();
        let pipeline = plan.stages[0].get_document("$lookup").unwrap().get_array("pipeline").unwrap();
        assert_eq!(
            pipeline[1],
            doc! { "$match": { "$and": [{ "deleted_at": 0 }, { "$expr": true }] } }.into()
        );
    }
    #[tokio::test]
    async fn calculated_keys_are_used_by_grants_and_related_record_matching() {
        let mut executor = ReadExecutor::default();
        let expression = doc! { "$toString": "$_id" };
        let calculated: Arc<dyn AccessMapping> = Arc::new(Mapping {
            model: mapping("sales_order", None).model().clone(),
            child: None,
            versioned: Some(true),
            revision: AtomicU64::new(1),
            prepare_calls: AtomicU64::new(0),
            sources: BTreeMap::new(),
            live_only: false,
            key_expression: Some(expression.clone().into()),
            action_sensitive: false,
        });

        let grant =
            query(AccessRule::RecordReadGrant(ResourceKind::SalesOrder), calculated.clone(), &executor);
        let compiled = grant.compile(&mut executor).await.unwrap();
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
        let compiled = query(rule, root.clone(), &executor).compile(&mut executor).await.unwrap();
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

    #[tokio::test]
    async fn required_mapping_facts_are_checked_even_when_rule_allows_every_record() {
        let mut executor = ReadExecutor::default();
        let mapping = mapping("sales_order", None);
        let rule = AccessRule::Allow.requiring(&[FactKind::Warehouse].into());
        assert!(matches!(
            query(rule, mapping.clone(), &executor).compile(&mut executor).await,
            Err(Error::ValidationError(_))
        ));
        let rule = AccessRule::Allow.requiring(&[FactKind::Owner].into());
        assert!(query(rule, mapping.clone(), &executor).compile(&mut executor).await.is_ok());
    }

    #[tokio::test]
    async fn static_inquiry_mapping_needs_no_preparation_and_matches_the_object_condition() {
        let mut executor = ReadExecutor::default();
        let mapping = Arc::new(Mapping {
            model: mapping("sales_inquiry", None).model().clone(),
            child: None,
            versioned: None,
            revision: AtomicU64::new(1),
            prepare_calls: AtomicU64::new(0),
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
        let rule = rules::data_condition(&sales_inquiry::QUERY_SALES_LIST, Some(&scope)).unwrap();
        let query = query(rule.clone(), mapping.clone(), &executor);
        let plan = query.compile(&mut executor).await.unwrap();
        assert!(plan.dependencies.is_empty());

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

    #[tokio::test]
    async fn source_query_and_object_membership_agree_for_complete_missing_denied_and_deleted_sources() {
        let mut executor = ReadExecutor::default();
        let child = Arc::new(Mapping {
            model: QueryModel {
                collection: "access_sources".into(),
                ..mapping("sales_order", None).model().clone()
            },
            child: None,
            versioned: None,
            revision: AtomicU64::new(1),
            prepare_calls: AtomicU64::new(0),
            sources: BTreeMap::new(),
            live_only: true,
            key_expression: None,
            action_sensitive: false,
        });
        let root = Arc::new(Mapping {
            model: mapping("receivable_account", None).model().clone(),
            child: Some(child),
            versioned: None,
            revision: AtomicU64::new(1),
            prepare_calls: AtomicU64::new(0),
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
        let plan = query.compile(&mut executor).await.unwrap();
        assert!(plan.dependencies.is_empty());

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

    #[tokio::test]
    async fn multi_source_query_prepares_and_joins_each_source_once_without_changing_membership() {
        for dynamic in [false, true] {
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
                    versioned: dynamic.then_some(true),
                    revision: AtomicU64::new(1),
                    prepare_calls: AtomicU64::new(0),
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
                let plan = query.compile(executor.as_mut()).await.unwrap();
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

                for source in [&sales, &purchase, &settlement] {
                    assert_eq!(source.prepare_calls.load(Ordering::Relaxed), u64::from(dynamic));
                }

                let snapshot = executor.boundary().unwrap().is_snapshot();
                assert_eq!(plan.dependencies.len(), if dynamic && !snapshot { 4 } else { 0 });
                for dependency in &plan.dependencies {
                    dependency.validate(executor.as_mut()).await.unwrap();
                }
                for source in [&sales, &purchase, &settlement] {
                    assert_eq!(
                        source.prepare_calls.load(Ordering::Relaxed),
                        if dynamic { if snapshot { 1 } else { 2 } } else { 0 }
                    );
                }
            }
        }
    }

    #[tokio::test]
    async fn preparation_cache_keeps_mapping_instances_and_contexts_separate() {
        let mut executor = ReadExecutor::default();
        let first = Arc::new(Mapping {
            model: mapping("sales_order", None).model().clone(),
            child: None,
            versioned: Some(true),
            revision: AtomicU64::new(1),
            prepare_calls: AtomicU64::new(0),
            sources: BTreeMap::new(),
            live_only: false,
            key_expression: None,
            action_sensitive: false,
        });
        let second = Arc::new(Mapping {
            model: first.model.clone(),
            child: None,
            versioned: Some(true),
            prepare_calls: AtomicU64::new(0),
            revision: AtomicU64::new(1),
            sources: BTreeMap::new(),
            live_only: false,
            key_expression: None,
            action_sensitive: false,
        });
        let proof = query(AccessRule::Allow, first.clone(), &executor).proof;
        let mut compiler = Compiler {
            executor: &mut executor,
            boundary: proof.boundary,
            valid_until: None,
            prepared: vec![],
            versions: vec![],
        };

        let first_mapping: Arc<dyn AccessMapping> = first.clone();
        let second_mapping: Arc<dyn AccessMapping> = second.clone();
        compiler.prepare(&first_mapping, &proof.context).await.unwrap();
        compiler.prepare(&first_mapping, &proof.context).await.unwrap();
        compiler.prepare(&second_mapping, &proof.context).await.unwrap();
        let detail = AccessContext { permission: sales_order::DETAIL, ..proof.context.clone() };
        compiler.prepare(&first_mapping, &detail).await.unwrap();
        let later = AccessContext {
            as_of: Instant::from_unix_secs(proof.context.as_of.unix_secs() + 1),
            ..proof.context.clone()
        };
        compiler.prepare(&first_mapping, &later).await.unwrap();
        let other = AccessContext {
            actor: AuditActor::new("other".into(), "other user".into(), AccountKind::Admin),
            ..proof.context.clone()
        };
        compiler.prepare(&first_mapping, &other).await.unwrap();

        assert_eq!(first.prepare_calls.load(Ordering::Relaxed), 4);
        assert_eq!(second.prepare_calls.load(Ordering::Relaxed), 1);
        assert_eq!(compiler.prepared.len(), 5);
    }

    #[tokio::test]
    async fn record_grant_query_uses_valid_variables_and_only_accepts_live_matching_sources() {
        let mut executor = ReadExecutor::default();
        let mapping = mapping("sales_order", None);
        let query = query(AccessRule::RecordReadGrant(ResourceKind::SalesOrder), mapping.clone(), &executor);
        let plan = query.compile(&mut executor).await.unwrap();

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

    #[tokio::test]
    async fn snapshot_mapping_keeps_only_expiry_and_rejects_cross_write_or_transaction_reuse() {
        let mut executor = SnapshotExecutor::default();
        let mapping = Arc::new(Mapping {
            model: mapping("sales_order", None).model().clone(),
            child: None,
            versioned: Some(false),
            revision: AtomicU64::new(1),
            prepare_calls: AtomicU64::new(0),
            sources: BTreeMap::new(),
            live_only: false,
            key_expression: None,
            action_sensitive: true,
        });
        let query = query(AccessRule::Allow, mapping.clone(), &executor);
        let mut plan = query.compile(&mut executor).await.unwrap();
        assert!(plan.dependencies.is_empty());
        assert!(plan.proof.valid_until.is_some());
        mapping.revision.store(2, Ordering::Relaxed);
        plan.ensure_current(&mut executor).unwrap();

        executor.next_read();
        assert!(matches!(plan.ensure_current(&mut executor), Err(Error::ValidationError(_))));
        executor.restart();
        assert!(matches!(plan.ensure_current(&mut executor), Err(Error::ValidationError(_))));

        plan.proof.boundary = executor.boundary().unwrap();
        plan.proof.valid_until = Some(Instant::from_unix_secs(1));
        assert!(matches!(plan.ensure_current(&mut executor), Err(Error::ConflictError(_))));
    }

    #[tokio::test]
    async fn revalidation_mode_checks_service_and_boundary_before_skipping_snapshot_reads() {
        use persistence_core::NoTransaction;

        let mut snapshot = SnapshotExecutor::default();
        let mapping = mapping("sales_order", None);
        let snapshot_query = query(AccessRule::Allow, mapping.clone(), &snapshot);
        assert!(
            !snapshot_query.proof.needs_revalidation(&snapshot_query.proof.service, &mut snapshot).unwrap()
        );
        assert!(matches!(
            snapshot_query.proof.needs_revalidation(&Arc::new(()), &mut snapshot),
            Err(Error::ValidationError(_))
        ));
        assert!(matches!(snapshot_query.compile(&mut NoTransaction).await, Err(Error::ValidationError(_))));
        snapshot.session();
        assert!(matches!(
            snapshot_query.proof.needs_revalidation(&snapshot_query.proof.service, &mut snapshot),
            Err(Error::ValidationError(_))
        ));

        let mut executor = ReadExecutor::default();
        let proof = query(AccessRule::Allow, mapping, &executor).proof;
        assert!(proof.needs_revalidation(&proof.service, &mut executor).unwrap());
        executor.next_read();
        assert!(matches!(
            proof.needs_revalidation(&proof.service, &mut executor),
            Err(Error::ValidationError(_))
        ));
    }
}
