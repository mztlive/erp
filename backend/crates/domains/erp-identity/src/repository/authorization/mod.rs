//! 将权限语义编译为数据库内条件；根记录只在完整表达式完成后过滤。
mod query;
use std::sync::Arc;

use erp_core::common::time::Instant;
use futures_util::future::BoxFuture;
use mongodb::bson::{Document, doc};
use persistence_core::Executor;
pub(crate) use query::{AccessProof, MappingDependency};
pub use query::{AuthorizedQuery, CompiledAccess};

use crate::entity::authorization::{
    AccessContext, AccessRule, MappingFacts, QueryOutput, RelationKey, RelationMatch,
};
use crate::ports::{AccessMapping, QueryFragment};
use crate::repository::resource_grants::grant_lookup;
use crate::{Error, Result};

/// 一次编译共享别名分配和业务事实复验依赖。
struct Compiler<'a> {
    executor: &'a mut dyn Executor,
    next_alias: usize,
    dependencies: Vec<MappingDependency>,
}

struct Fragment {
    stages: Vec<Document>,
    filter: Document,
    aliases: Vec<String>,
}

impl Fragment {
    /// 普通字段条件不改变根记录。
    fn condition(filter: Document) -> Self {
        Self { stages: vec![], filter, aliases: vec![] }
    }

    /// 只有这里应用完整条件，再移除准备字段。
    fn finish(mut self) -> Vec<Document> {
        self.stages.push(doc! { "$match": self.filter });
        if !self.aliases.is_empty() {
            self.stages.push(doc! { "$unset": self.aliases });
        }
        self.stages
    }
}

/// 编译服务端生成的完整表达式，保留所有事实依赖。
pub(crate) async fn compile(query: &AuthorizedQuery, executor: &mut dyn Executor) -> Result<CompiledAccess> {
    let proof = &query.proof;
    let mut compiler = Compiler { executor, next_alias: 0, dependencies: vec![] };
    let fragment = compiler.model(&proof.rule, &query.mapping, &proof.context, 0).await?;
    let mut stages = fragment.finish();
    if let QueryOutput::Share { fields } | QueryOutput::Existence { fields } = &query.model.output {
        let mut projection = doc! { "_id": 0 };
        for field in fields {
            projection.insert(field, 1);
        }
        stages.push(doc! { "$project": projection });
    }
    Ok(CompiledAccess {
        proof: proof.clone(),
        model: query.model.clone(),
        stages,
        dependencies: compiler.dependencies,
    })
}

impl Compiler<'_> {
    /// 每个模型都限定有效行，来源公司范围也不能读取已删除记录。
    fn model<'a>(
        &'a mut self,
        rule: &'a AccessRule,
        mapping: &'a Arc<dyn AccessMapping>,
        context: &'a AccessContext,
        depth: usize,
    ) -> BoxFuture<'a, Result<Fragment>> {
        Box::pin(async move {
            let facts = self.prepare(mapping, context).await?;
            let records = self.fact(mapping.records(context, &facts)?)?;
            let access = self.node(rule, mapping, context, &facts, depth).await?;
            if records.stages.is_empty() && records.filter.is_empty() {
                return Ok(access);
            }
            Ok(combine("$and", vec![records, access]))
        })
    }

    /// 读取准备事实并登记版本，复验时使用相同映射。
    async fn prepare(
        &mut self,
        mapping: &Arc<dyn AccessMapping>,
        context: &AccessContext,
    ) -> Result<MappingFacts> {
        mapping.model().validate()?;
        if context.resource != mapping.model().resource
            || context.model.as_ref() != Some(&mapping.model().key)
        {
            return Err(Error::ValidationError("业务事实映射与授权资源不匹配".into()));
        }
        let facts = mapping.prepare(context, self.executor).await?;
        facts.validate(self.executor.session().is_some(), Instant::now())?;
        self.dependencies.push(MappingDependency {
            mapping: mapping.clone(),
            model: mapping.model().clone(),
            context: context.clone(),
            facts: facts.clone(),
            key: mapping.key(context, &facts)?,
        });
        Ok(facts)
    }

    /// 节点递归只在查询模型内执行，拒绝过深登记。
    fn node<'a>(
        &'a mut self,
        rule: &'a AccessRule,
        mapping: &'a Arc<dyn AccessMapping>,
        context: &'a AccessContext,
        facts: &'a MappingFacts,
        depth: usize,
    ) -> BoxFuture<'a, Result<Fragment>> {
        Box::pin(async move {
            if depth > 16 {
                return Err(Error::ValidationError("授权查询嵌套超限".into()));
            }
            match rule {
                AccessRule::Allow => Ok(Fragment::condition(doc! { "$expr": true })),
                AccessRule::Deny => Ok(Fragment::condition(doc! { "$expr": false })),
                AccessRule::All(rules) | AccessRule::Any(rules) => {
                    let operator = if matches!(rule, AccessRule::All(_)) { "$and" } else { "$or" };
                    let mut children = Vec::new();
                    for child in rules {
                        children.push(self.node(child, mapping, context, facts, depth + 1).await?);
                    }
                    Ok(combine(operator, children))
                },
                AccessRule::Required { facts: required, rule } => {
                    if !required.is_subset(&mapping.model().facts) {
                        return Err(Error::ValidationError("Mapping 缺少登记的必需事实".into()));
                    }
                    self.node(rule, mapping, context, facts, depth + 1).await
                },
                AccessRule::Independent { permission, rule } => {
                    let context = AccessContext { permission: permission.clone(), ..context.clone() };
                    self.model(rule, mapping, &context, depth + 1).await
                },
                AccessRule::Fact(fact) => {
                    if !mapping.model().facts.contains(&fact.kind()) {
                        return Err(Error::ValidationError("Mapping 未声明所需事实维度".into()));
                    }
                    self.fact(mapping.predicate(fact, context, facts)?)
                },
                AccessRule::RecordReadGrant(resource) => {
                    if mapping.model().resource != resource.code() {
                        return Err(Error::ValidationError("读取授权资源与查询记录不匹配".into()));
                    }
                    let alias = self.alias();
                    Ok(exists(
                        alias.clone(),
                        grant_lookup(context.actor.id(), *resource, mapping.key(context, facts)?, &alias),
                    ))
                },
                AccessRule::Related { relation, resource, mode, rule } => {
                    self.related((relation, resource, *mode), rule, mapping, context, facts, depth).await
                },
            }
        })
    }

    /// 生成唯一临时字段，业务模型不得使用此前缀。
    fn alias(&mut self) -> String {
        let value = format!("__identity_fact_{}", self.next_alias);
        self.next_alias += 1;
        value
    }

    /// 存在性关联只准备数组，不 unwind、不提前过滤根记录。
    fn fact(&mut self, fragment: QueryFragment) -> Result<Fragment> {
        match fragment {
            QueryFragment::Match(filter) => Ok(Fragment::condition(filter)),
            QueryFragment::Exists { collection, bindings, mut pipeline } => {
                if collection.is_empty() || collection.contains('$') {
                    return Err(Error::ValidationError("事实关联集合非法".into()));
                }
                pipeline.push(doc! { "$limit": 1 });
                let alias = self.alias();
                let lookup = doc! { "$lookup": { "from": collection, "let": bindings, "pipeline": pipeline, "as": &alias } };
                Ok(exists(alias, lookup))
            },
        }
    }

    /// 来源条件只作用于实际引用；全量判定比较完整引用集合与通过集合。
    async fn related(
        &mut self,
        relation: (&RelationKey, &str, RelationMatch),
        rule: &AccessRule,
        mapping: &Arc<dyn AccessMapping>,
        context: &AccessContext,
        facts: &MappingFacts,
        depth: usize,
    ) -> Result<Fragment> {
        let (key, resource, mode) = relation;
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
        let dependency_index = self.dependencies.len();
        let child = self.model(rule, &related.mapping, &child_context, depth + 1).await?;
        let expected = doc! { "$setUnion": [{ "$ifNull": [related.expected_ids, []] }, []] };
        let key_expression = self.dependencies[dependency_index].key.clone();
        let mut pipeline =
            vec![doc! { "$match": { "$expr": { "$in": [&key_expression, "$$__identity_expected"] } } }];
        pipeline.extend(child.finish());
        pipeline.push(doc! { "$project": { "_id": 0, "__identity_id": key_expression } });
        let alias = self.alias();
        let lookup = doc! { "$lookup": { "from": &related.mapping.model().collection,
        "let": { "__identity_expected": expected.clone() }, "pipeline": pipeline, "as": &alias } };
        let mut result = exists(alias.clone(), lookup);
        if mode == RelationMatch::AllNonEmpty {
            result.filter = doc! { "$expr": { "$and": [
                { "$gt": [{ "$size": expected.clone() }, 0] },
                { "$setEquals": [expected, format!("${alias}.__identity_id")] }
            ] } };
        } else if matches!(mapping.model().output, QueryOutput::Share { .. }) {
            result.filter = doc! { "$and": [
                result.filter,
                { "$expr": { "$eq": [{ "$size": expected }, 1] } }
            ] };
        }
        Ok(result)
    }
}

/// 拼接准备阶段，最终只执行一次逻辑过滤。
fn combine(operator: &str, fragments: Vec<Fragment>) -> Fragment {
    if fragments.is_empty() {
        return Fragment::condition(doc! { "$expr": false });
    }
    let mut result = Fragment::condition(Document::new());
    let mut conditions = Vec::new();
    for fragment in fragments {
        result.stages.extend(fragment.stages);
        result.aliases.extend(fragment.aliases);
        conditions.push(fragment.filter);
    }
    result.filter.insert(operator, conditions);
    result
}

/// 存在性查询不扩展根记录数量。
fn exists(alias: String, lookup: Document) -> Fragment {
    Fragment {
        stages: vec![lookup],
        filter: doc! { format!("{alias}.0"): { "$exists": true } },
        aliases: vec![alias],
    }
}

#[cfg(test)]
pub(crate) use tests::model_mapping;

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use application_core::AuditActor;
    use async_trait::async_trait;
    use erp_core::AccountKind;
    use mongodb::bson::Bson;
    use persistence_core::NoTransaction;

    use super::*;
    use crate::Permission;
    use crate::entity::authorization::{AccessFact, AccessSubject, FactKind, QueryModel, QueryModelKey};
    use crate::entity::resource_access::ResourceKind;
    use crate::ports::RelationMapping;

    struct Mapping {
        model: QueryModel,
        child: Option<Arc<dyn AccessMapping>>,
        versioned: bool,
        revision: AtomicU64,
        live_only: bool,
        key_expression: Option<Bson>,
        action_sensitive: bool,
    }

    #[async_trait]
    impl AccessMapping for Mapping {
        fn model(&self) -> &QueryModel {
            &self.model
        }
        fn key(&self, _: &AccessContext, _: &MappingFacts) -> Result<Bson> {
            Ok(self.key_expression.clone().unwrap_or_else(|| format!("${}", self.model.id_field).into()))
        }
        async fn prepare(&self, context: &AccessContext, _: &mut dyn Executor) -> Result<MappingFacts> {
            Ok(MappingFacts {
                version: self.versioned.then(|| {
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
        fn records(&self, context: &AccessContext, _: &MappingFacts) -> Result<QueryFragment> {
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
            _: &MappingFacts,
        ) -> Result<QueryFragment> {
            match fact {
                AccessFact::Owned => {
                    let mut filter = doc! { "sales_owner_user_id": context.actor.id() };
                    if self.action_sensitive {
                        filter.insert("owner_action", context.permission.action());
                    }
                    Ok(QueryFragment::Match(filter))
                },
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
            _: &MappingFacts,
        ) -> Result<RelationMapping> {
            if key.as_str() != "sales" {
                return Err(Error::ValidationError("unknown relation".into()));
            }
            Ok(RelationMapping {
                expected_ids: "$sales_ids".into(),
                mapping: self
                    .child
                    .clone()
                    .ok_or_else(|| Error::ValidationError("missing relation".into()))?,
            })
        }
    }

    /// 复用查询编译器的无 I/O Mapping 替身，验证服务端模型登记。
    pub(crate) fn model_mapping(model: QueryModel) -> Arc<dyn AccessMapping> {
        Arc::new(Mapping {
            model,
            child: None,
            versioned: true,
            revision: AtomicU64::new(1),
            live_only: false,
            key_expression: None,
            action_sensitive: false,
        })
    }

    fn mapping(resource: &str, child: Option<Arc<dyn AccessMapping>>) -> Arc<dyn AccessMapping> {
        Arc::new(Mapping {
            model: QueryModel {
                key: QueryModelKey::parse(resource).unwrap(),
                resource: resource.into(),
                collection: "access_rows".into(),
                id_field: "business_id".into(),
                output: QueryOutput::Object,
                facts: [FactKind::Owner, FactKind::HistoryParticipants].into(),
            },
            child,
            versioned: true,
            revision: AtomicU64::new(1),
            live_only: false,
            key_expression: None,
            action_sensitive: false,
        })
    }

    fn query(rule: AccessRule, mapping: Arc<dyn AccessMapping>) -> AuthorizedQuery {
        let model = mapping.model().clone();
        AuthorizedQuery {
            proof: AccessProof {
                context: AccessContext {
                    actor: AuditActor::new("u".into(), "user".into(), AccountKind::Admin),
                    permission: Permission::parse(format!("{}:list", model.resource)).unwrap(),
                    resource: model.resource.clone(),
                    model: Some(model.key.clone()),
                    as_of: Instant::now(),
                },
                subject: AccessSubject::Direct,
                rule,
                version: "identity-v1".into(),
                valid_until: None,
                policy_version: 7,
                organization_version: 11,
                session: None,
                registry: Arc::new(()),
            },
            mapping,
            model,
        }
    }

    #[tokio::test]
    async fn compiled_metadata_comes_from_the_original_authorization_proof() {
        let mapping = mapping("sales_order", None);
        let query = query(AccessRule::Allow, mapping.clone());
        let as_of = query.proof.context.as_of;
        let compiled = query.compile(mapping.as_ref(), &mut NoTransaction).await.unwrap();
        assert_eq!(compiled.policy_version(), 7);
        assert_eq!(compiled.organization_version(), 11);
        assert_eq!(compiled.as_of(), as_of);
    }

    #[tokio::test]
    async fn union_prepares_relations_without_discarding_other_branches() {
        let mapping = mapping("sales_order", None);
        let rule = AccessRule::any(vec![
            AccessRule::Fact(AccessFact::Owned),
            AccessRule::Fact(AccessFact::HistoryParticipant),
        ]);
        let plan = query(rule, mapping.clone()).compile(mapping.as_ref(), &mut NoTransaction).await.unwrap();
        assert_eq!(plan.stages.len(), 3);
        assert!(plan.stages[0].contains_key("$lookup"));
        assert_eq!(
            plan.stages[1],
            doc! { "$match": { "$or": [
                { "sales_owner_user_id": "u" }, { "__identity_fact_0.0": { "$exists": true } }
            ] } }
        );
        assert_eq!(plan.stages[2], doc! { "$unset": ["__identity_fact_0"] });
    }

    #[tokio::test]
    async fn grants_use_mapped_key_and_remain_inside_independent_constraints() {
        let mapping = mapping("sales_order", None);
        let rule = AccessRule::all(vec![
            AccessRule::Fact(AccessFact::Owned),
            AccessRule::any(vec![
                AccessRule::Fact(AccessFact::HistoryParticipant),
                AccessRule::RecordReadGrant(ResourceKind::SalesOrder),
            ]),
        ]);
        let plan = query(rule, mapping.clone()).compile(mapping.as_ref(), &mut NoTransaction).await.unwrap();
        assert_eq!(
            plan.stages[1].get_document("$lookup").unwrap().get_document("let").unwrap(),
            &doc! { "__identity_key": "$business_id" }
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
        let child = mapping("sales_order", None);
        let root = mapping("invoice", Some(child));
        let rule = AccessRule::Related {
            relation: RelationKey::parse("sales").unwrap(),
            resource: "sales_order".into(),
            mode: RelationMatch::AllNonEmpty,
            rule: Box::new(AccessRule::Fact(AccessFact::Owned)),
        };
        let plan = query(rule, root.clone()).compile(root.as_ref(), &mut NoTransaction).await.unwrap();
        let lookup = plan.stages[0].get_document("$lookup").unwrap();
        assert_eq!(lookup.get_str("from").unwrap(), "access_rows");
        let pipeline = lookup.get_array("pipeline").unwrap();
        assert_eq!(
            pipeline[0],
            doc! { "$match": { "$expr": { "$in": ["$business_id", "$$__identity_expected"] } } }.into()
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
        let sales = mapping("sales_order", None);
        let unknown = query(AccessRule::Fact(AccessFact::WarehouseIn(["w".into()].into())), sales.clone());
        assert!(unknown.compile(sales.as_ref(), &mut NoTransaction).await.is_err());
        let same_name_different_instance = mapping("sales_order", None);
        assert!(
            query(AccessRule::Allow, sales.clone())
                .compile(same_name_different_instance.as_ref(), &mut NoTransaction)
                .await
                .is_err()
        );
        let wrong_resource = query(AccessRule::RecordReadGrant(ResourceKind::PurchaseOrder), sales.clone());
        assert!(wrong_resource.compile(sales.as_ref(), &mut NoTransaction).await.is_err());
    }

    #[tokio::test]
    async fn unversioned_nontransaction_facts_are_rejected() {
        let mapping: Arc<dyn AccessMapping> = Arc::new(Mapping {
            model: mapping("sales_order", None).model().clone(),
            child: None,
            versioned: false,
            revision: AtomicU64::new(1),
            live_only: false,
            key_expression: None,
            action_sensitive: false,
        });
        assert!(
            query(AccessRule::Allow, mapping.clone())
                .compile(mapping.as_ref(), &mut NoTransaction)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn denied_query_compiles_false_instead_of_missing_filter() {
        let mapping = mapping("sales_order", None);
        let plan = query(AccessRule::Deny, mapping.clone())
            .compile(mapping.as_ref(), &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(plan.stages, vec![doc! { "$match": { "$expr": false } }]);
    }

    #[tokio::test]
    async fn partial_query_projects_only_registered_fields_and_share_has_one_source() {
        let rule = AccessRule::Related {
            relation: RelationKey::parse("sales").unwrap(),
            resource: "sales_order".into(),
            mode: RelationMatch::Any,
            rule: Box::new(AccessRule::Fact(AccessFact::Owned)),
        };
        let child = mapping("sales_order", None);
        let root = mapping("invoice", Some(child.clone()));
        assert!(query(rule.clone(), root.clone()).compile(root.as_ref(), &mut NoTransaction).await.is_err());
        let mut model = root.model().clone();
        model.output = QueryOutput::Share { fields: ["business_id".into(), "share_amount".into()].into() };
        let share: Arc<dyn AccessMapping> = Arc::new(Mapping {
            model,
            child: Some(child),
            versioned: true,
            revision: AtomicU64::new(1),
            live_only: false,
            key_expression: None,
            action_sensitive: false,
        });
        let plan = query(rule, share.clone()).compile(share.as_ref(), &mut NoTransaction).await.unwrap();
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
        let live = Arc::new(Mapping {
            model: mapping("sales_order", None).model().clone(),
            child: None,
            versioned: true,
            revision: AtomicU64::new(1),
            live_only: false,
            key_expression: None,
            action_sensitive: false,
        });
        let query = query(AccessRule::Allow, live.clone());
        let plan = query.compile(live.as_ref(), &mut NoTransaction).await.unwrap();
        assert!(plan.dependencies[0].validate(&mut NoTransaction).await.is_ok());
        live.revision.store(2, Ordering::Relaxed);
        assert!(matches!(
            plan.dependencies[0].validate(&mut NoTransaction).await,
            Err(Error::ConflictError(_))
        ));
        let updated = query.compile(live.as_ref(), &mut NoTransaction).await.unwrap();
        assert_ne!(plan.version(), updated.version());
    }

    #[tokio::test]
    async fn expired_proof_or_wrong_session_cannot_compile() {
        let mapping = mapping("sales_order", None);
        let mut query = query(AccessRule::Allow, mapping.clone());
        query.proof.valid_until = Some(Instant::from_unix_secs(1));
        assert!(matches!(
            query.compile(mapping.as_ref(), &mut NoTransaction).await,
            Err(Error::ConflictError(_))
        ));
        query.proof.valid_until = None;
        query.proof.session = Some(doc! { "id": "another-session" });
        assert!(matches!(
            query.compile(mapping.as_ref(), &mut NoTransaction).await,
            Err(Error::ValidationError(_))
        ));
    }

    #[tokio::test]
    async fn independent_scope_compiles_and_revalidates_its_own_action_context() {
        let mapping = Arc::new(Mapping {
            model: mapping("sales_order", None).model().clone(),
            child: None,
            versioned: true,
            revision: AtomicU64::new(1),
            live_only: false,
            key_expression: None,
            action_sensitive: true,
        });
        let rule = AccessRule::Independent {
            permission: Permission::parse("sales_order:detail").unwrap(),
            rule: Box::new(AccessRule::Fact(AccessFact::Owned)),
        };
        let query = query(rule, mapping.clone());
        let plan = query.compile(mapping.as_ref(), &mut NoTransaction).await.unwrap();
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
        assert!(plan.dependencies[1].validate(&mut NoTransaction).await.is_ok());
        mapping.revision.store(2, Ordering::Relaxed);
        assert!(matches!(
            plan.dependencies[1].validate(&mut NoTransaction).await,
            Err(Error::ConflictError(_))
        ));
    }

    #[tokio::test]
    async fn related_shared_scope_still_requires_live_source_record() {
        let child: Arc<dyn AccessMapping> = Arc::new(Mapping {
            model: mapping("purchase_order", None).model().clone(),
            child: None,
            versioned: true,
            revision: AtomicU64::new(1),
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
        let plan = query(rule, root.clone()).compile(root.as_ref(), &mut NoTransaction).await.unwrap();
        let pipeline = plan.stages[0].get_document("$lookup").unwrap().get_array("pipeline").unwrap();
        assert_eq!(
            pipeline[1],
            doc! { "$match": { "$and": [{ "deleted_at": 0 }, { "$expr": true }] } }.into()
        );
    }
    #[tokio::test]
    async fn calculated_keys_are_used_by_grants_and_related_record_matching() {
        let expression = doc! { "$toString": "$_id" };
        let calculated: Arc<dyn AccessMapping> = Arc::new(Mapping {
            model: mapping("sales_order", None).model().clone(),
            child: None,
            versioned: true,
            revision: AtomicU64::new(1),
            live_only: false,
            key_expression: Some(expression.clone().into()),
            action_sensitive: false,
        });
        let grant = query(AccessRule::RecordReadGrant(ResourceKind::SalesOrder), calculated.clone());
        let compiled = grant.compile(calculated.as_ref(), &mut NoTransaction).await.unwrap();
        assert_eq!(
            compiled.stages[0].get_document("$lookup").unwrap().get_document("let").unwrap(),
            &doc! { "__identity_key": expression.clone() }
        );
        let root = mapping("invoice", Some(calculated));
        let rule = AccessRule::Related {
            relation: RelationKey::parse("sales").unwrap(),
            resource: "sales_order".into(),
            mode: RelationMatch::AllNonEmpty,
            rule: Box::new(AccessRule::Allow),
        };
        let compiled = query(rule, root.clone()).compile(root.as_ref(), &mut NoTransaction).await.unwrap();
        let pipeline = compiled.stages[0].get_document("$lookup").unwrap().get_array("pipeline").unwrap();
        assert_eq!(
            pipeline[0],
            doc! { "$match": { "$expr": { "$in": [expression.clone(), "$$__identity_expected"] } } }.into()
        );
        assert_eq!(
            pipeline.last().unwrap(),
            &doc! { "$project": { "_id": 0, "__identity_id": expression } }.into()
        );
    }

    #[tokio::test]
    async fn required_mapping_facts_are_checked_even_when_rule_allows_every_record() {
        let mapping = mapping("sales_order", None);
        let rule = AccessRule::Allow.requiring(&[FactKind::Warehouse].into());
        assert!(matches!(
            query(rule, mapping.clone()).compile(mapping.as_ref(), &mut NoTransaction).await,
            Err(Error::ValidationError(_))
        ));
        let rule = AccessRule::Allow.requiring(&[FactKind::Owner].into());
        assert!(query(rule, mapping.clone()).compile(mapping.as_ref(), &mut NoTransaction).await.is_ok());
    }
}
