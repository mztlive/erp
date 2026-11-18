//! 已有记录、服务端候选和精确任务的事实执行。

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use erp_core::common::time::Instant;
use persistence_core::{ExecutionBoundary, Executor};

use super::approvals;
use super::rules::{self, TargetForm};
use crate::entity::access_control::scope_spec;
use crate::entity::authorization::{
    AccessContext, AccessRule, ObjectFacts, RelationKey, RelationMatch, ResourceTargets,
};
use crate::entity::resource_access::ResourceKind;
use crate::ports::AccessFactsPort;
use crate::repository::authorization::AccessProof;
use crate::{Error, Result};

type FactsBindings = BTreeMap<String, Arc<dyn AccessFactsPort>>;

/// 创建候选没有持久化版本；原对象和任务始终从拥有领域读取。
pub(super) enum Target<'a> {
    Records(&'a ResourceTargets),
    Candidate(&'a ObjectFacts),
    OwnershipChange { id: &'a str, candidate: &'a ObjectFacts },
    Task { id: &'a str, version: &'a str },
}

pub(super) struct TargetFacts {
    objects: Vec<ObjectFacts>,
    candidate: Option<ObjectFacts>,
    boundary: ExecutionBoundary,
    task: bool,
}

/// 来源证明保留独立加载的完整事实，供原读取阶段统一复验和最终期限检查。
#[derive(Default)]
pub(super) struct SourceEvidence {
    pub(super) proofs: Vec<AccessProof>,
    pub(super) facts: Vec<(AccessContext, TargetFacts)>,
}

impl SourceEvidence {
    /// 合并同一读取阶段的来源，不能丢弃只用于对象复验的事实。
    pub(super) fn extend(&mut self, other: Self) {
        self.proofs.extend(other.proofs);
        self.facts.extend(other.facts);
    }
}

impl TargetFacts {
    /// 使用原执行器读取完整目标；创建候选不读取尚未存在的根记录。
    pub(super) async fn load(
        bindings: &FactsBindings,
        context: &AccessContext,
        rule: &AccessRule,
        target: Target<'_>,
        executor: &mut dyn Executor,
    ) -> Result<Self> {
        if context.resource != context.permission.resource() {
            return Err(Error::ValidationError("目标资源与权限错配".into()));
        }
        let boundary =
            executor.boundary().ok_or_else(|| Error::ValidationError("目标读取缺少执行边界".into()))?;

        let (form, ids, candidate, version) = match target {
            Target::Records(ids) => (TargetForm::Record, ids.clone(), None, None),
            Target::Candidate(candidate) => {
                (TargetForm::Candidate, ResourceTargets::new(&[])?, Some(candidate), None)
            },
            Target::OwnershipChange { id, candidate } => {
                (TargetForm::OwnershipChange, ResourceTargets::new(&[id])?, Some(candidate), None)
            },
            Target::Task { id, version } => {
                if version.is_empty() {
                    return Err(Error::ValidationError("必须提供精确任务版本".into()));
                }
                (TargetForm::Task, ResourceTargets::new(&[id])?, None, Some(version))
            },
        };
        rules::ensure_target(&context.permission, scope_spec::kind(&context.permission)?, form)?;
        if (form == TargetForm::Task || !rules::is_read(&context.permission)) && !boundary.is_snapshot() {
            return Err(Error::ValidationError("对象写入授权必须使用快照事务".into()));
        }
        if !matches!(form, TargetForm::Candidate) && ids.ids().is_empty() {
            return Err(Error::ValidationError("授权目标不能为空".into()));
        }

        let task = matches!(form, TargetForm::Task);
        let objects = load_target_records(bindings, context, &ids, task, executor).await?;
        if version.is_some_and(|version| objects[0].version != version) {
            return Err(Error::ConflictError("任务版本已改变".into()));
        }

        let candidate = match candidate {
            Some(candidate) => Some(load_candidate(bindings, context, rule, candidate, executor).await?),
            None => None,
        };

        let result = Self { objects, candidate, boundary, task };
        if !result.boundary.is_current(executor) {
            return Err(Error::ValidationError("目标读取跨越执行阶段".into()));
        }
        result.ensure_current()?;
        Ok(result)
    }

    /// 所有根对象与新归属同时通过；任务条件与查询共用固定数据规则。
    pub(super) fn require(
        &self,
        context: &AccessContext,
        rule: &AccessRule,
        grants: &BTreeSet<(ResourceKind, String)>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if !self.boundary.is_current(executor) {
            return Err(Error::ValidationError("目标检查跨越执行阶段".into()));
        }
        ensure_whole_object(rule)?;
        self.ensure_current()?;
        for object in self.objects.iter().chain(&self.candidate) {
            if !rule.allows_object(context.actor.id(), object, grants)? {
                return Err(Error::Forbidden("操作包含不可访问的对象或目标归属".into()));
            }
        }
        self.ensure_current()
    }

    /// 仅暴露本次可信根对象；审批管理从冻结事实确定类型及真实来源。
    pub(super) fn roots(&self) -> impl Iterator<Item = &ObjectFacts> {
        self.objects.iter().chain(self.candidate.iter())
    }

    /// 有界展开复用同一个总量计数；异步检查结束后再次核对全部到期边界。
    pub(super) fn flattened(&self) -> Result<Vec<&ObjectFacts>> {
        let mut flattened = Vec::new();
        for object in &self.objects {
            object.flatten(&mut flattened, 0)?;
        }
        if let Some(candidate) = &self.candidate {
            candidate.ensure_candidate(&candidate.resource)?;
            flattened.push(candidate);
            for relation in candidate.relations.values() {
                relation.validate()?;
                for child in &relation.objects {
                    child.flatten(&mut flattened, 1)?;
                }
            }
        }
        Ok(flattened)
    }

    pub(super) fn ensure_current(&self) -> Result<()> {
        self.flattened().map(|_| ())
    }

    /// 非事务读取核对完整根及来源事实；快照事务不重读同一批对象。
    pub(super) async fn revalidate(
        &self,
        bindings: &FactsBindings,
        context: &AccessContext,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if !self.boundary.is_snapshot() {
            let ids = self.objects.iter().map(|object| object.id.as_str()).collect::<Vec<_>>();
            let current =
                load_target_records(bindings, context, &ResourceTargets::new(&ids)?, self.task, executor)
                    .await?;
            if current != self.objects {
                return Err(Error::ConflictError("DATA_SCOPE_CHANGED：对象事实已改变".into()));
            }
        }

        self.ensure_current()
    }
}

/// 精确任务从固定任务资源读取；其事实仍按原入口解释，调用方不能切换提供者。
async fn load_target_records(
    bindings: &FactsBindings,
    context: &AccessContext,
    ids: &ResourceTargets,
    task: bool,
    executor: &mut dyn Executor,
) -> Result<Vec<ObjectFacts>> {
    if !task {
        return load_records(bindings, context, ids, executor).await;
    }
    let resource = rules::task_facts_resource(&context.permission);
    let port = bindings.get(resource).ok_or_else(|| Error::ValidationError("缺少任务 Facts 绑定".into()))?;
    let mut objects = port.load(context, ids, executor).await?;
    ids.ensure_objects(&context.resource, &objects)?;
    ObjectFacts::canonicalize(&mut objects)?;
    Ok(objects)
}

/// 完整根集合必须一次核对，不能将根记录缺失解释为已知空。
async fn load_records(
    bindings: &FactsBindings,
    context: &AccessContext,
    ids: &ResourceTargets,
    executor: &mut dyn Executor,
) -> Result<Vec<ObjectFacts>> {
    if ids.ids().is_empty() {
        return Ok(vec![]);
    }
    let port = bindings
        .get(&context.resource)
        .ok_or_else(|| Error::ValidationError("缺少对象 Facts 绑定".into()))?;
    let mut objects = port.load(context, ids, executor).await?;
    ids.ensure_objects(&context.resource, &objects)?;
    ObjectFacts::canonicalize(&mut objects)?;
    Ok(objects)
}

/// 候选仅提供真实引用；现存来源必须通过拥有领域按原执行器重新读取。
async fn load_candidate(
    bindings: &FactsBindings,
    context: &AccessContext,
    rule: &AccessRule,
    candidate: &ObjectFacts,
    executor: &mut dyn Executor,
) -> Result<ObjectFacts> {
    candidate.ensure_candidate(&context.resource)?;
    let mut candidate = candidate.clone();
    let mut resources = BTreeMap::new();
    source_resources(rule, &mut resources)?;
    if approvals::supports(&context.permission) {
        let (key, resource) = approvals::candidate_source(&candidate)?;
        resources.insert(key, resource);
    }

    for (key, related) in &mut candidate.relations {
        if !related.objects.is_empty() {
            return Err(Error::ValidationError("候选不能提供现存来源的授权事实".into()));
        }
        let resource =
            resources.get(key).ok_or_else(|| Error::ValidationError("候选来源关系未登记".into()))?;
        if related.expected_ids.is_empty() {
            continue;
        }

        let port =
            bindings.get(resource).ok_or_else(|| Error::ValidationError("缺少来源 Facts 绑定".into()))?;
        let ids = related.expected_ids.iter().map(String::as_str).collect::<Vec<_>>();
        let context = AccessContext {
            permission: if approvals::supports(&context.permission) {
                approvals::source_permission(resource)?
            } else {
                context.permission.clone()
            },
            resource: resource.clone(),
            model: None,
            ..context.clone()
        };
        related.objects = port.load(&context, &ResourceTargets::new(&ids)?, executor).await?;
        related.validate()?;
        if related.objects.iter().any(|object| object.resource != *resource) {
            return Err(Error::ValidationError("来源 Facts 与资源错配".into()));
        }
    }
    Ok(candidate)
}

/// 只收集当前根的来源声明，不把来源自己的关系当作根关系。
fn source_resources(rule: &AccessRule, resources: &mut BTreeMap<RelationKey, String>) -> Result<()> {
    match rule {
        AccessRule::Related { relation, resource, .. } => {
            if resources.insert(relation.clone(), resource.clone()).is_some_and(|old| old != *resource) {
                return Err(Error::ValidationError("同一来源关系绑定多个资源".into()));
            }
        },
        AccessRule::All(rules) | AccessRule::Any(rules) => {
            for rule in rules {
                source_resources(rule, resources)?;
            }
        },
        AccessRule::Required { rule, .. } | AccessRule::Independent { rule, .. } => {
            source_resources(rule, resources)?;
        },
        _ => {},
    }
    Ok(())
}

/// 部分来源的存在性和份额结果不能用来放行完整对象。
fn ensure_whole_object(rule: &AccessRule) -> Result<()> {
    match rule {
        AccessRule::Related { mode: RelationMatch::Any, .. } => {
            return Err(Error::ValidationError("任一来源不能授权完整对象".into()));
        },
        AccessRule::All(rules) | AccessRule::Any(rules) => {
            for rule in rules {
                ensure_whole_object(rule)?;
            }
        },
        AccessRule::Related { rule, .. }
        | AccessRule::Required { rule, .. }
        | AccessRule::Independent { rule, .. } => ensure_whole_object(rule)?,
        _ => {},
    }
    Ok(())
}

impl ObjectFacts {
    /// 候选没有已有记录版本，不能用于替换当前持久化事实。
    fn ensure_candidate(&self, resource: &str) -> Result<()> {
        if self.resource != resource || !self.version.is_empty() {
            return Err(Error::ValidationError("创建或变更候选不能替代已有记录事实".into()));
        }
        if !self.id.is_empty() {
            ResourceTargets::new(&[&self.id])?;
        }
        if self.valid_until.is_some_and(|until| Instant::now() >= until) {
            return Err(Error::ConflictError("DATA_SCOPE_CHANGED：候选归属已到期".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use application_core::AuditActor;
    use async_trait::async_trait;
    use erp_core::AccountKind;
    use erp_permissions::{fulfillment, purchase_order, purchase_payment_request, sales_inquiry};
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::entity::access_control::{ResolvedScope, ScopeClause};
    use crate::entity::authorization::{FactValue, RelatedObjects};
    use crate::{Permission, QualifiedPermissions};

    #[derive(Default)]
    struct Facts {
        objects: Vec<ObjectFacts>,
        calls: Mutex<Vec<(String, BTreeSet<String>)>>,
    }

    #[async_trait]
    impl AccessFactsPort for Facts {
        async fn load(
            &self,
            context: &AccessContext,
            ids: &ResourceTargets,
            executor: &mut dyn Executor,
        ) -> Result<Vec<ObjectFacts>> {
            assert!(executor.boundary().unwrap().is_snapshot());
            self.calls.lock().unwrap().push((context.resource.clone(), ids.ids().clone()));
            Ok(self.objects.clone())
        }
    }

    fn context(permission: &Permission) -> AccessContext {
        AccessContext {
            actor: AuditActor::new("user".into(), "user".into(), AccountKind::Admin),
            permission: permission.clone(),
            resource: permission.resource().into(),
            model: None,
            as_of: Instant::now(),
        }
    }

    fn candidate(resource: &str, owner: &str) -> ObjectFacts {
        ObjectFacts {
            resource: resource.into(),
            owner_id: FactValue::Known(Some(owner.into())),
            org_unit_id: FactValue::Known(None),
            ..Default::default()
        }
    }

    fn owned_rule(permission: &Permission) -> AccessRule {
        let scope = ResolvedScope {
            role_clauses: vec![ScopeClause { self_owned: true, ..Default::default() }],
            user_limit: None,
        };
        rules::data_condition(permission, Some(&scope)).unwrap()
    }

    #[tokio::test]
    async fn create_checks_candidate_without_loading_nonexistent_root_and_rejects_nontransaction_write() {
        let context = context(&sales_inquiry::COMMANDS_SALES_CREATE);
        let rule = owned_rule(&context.permission);
        let candidate = candidate(&context.resource, "user");
        let mut executor = SnapshotExecutor::default();
        let facts = TargetFacts::load(
            &BTreeMap::new(),
            &context,
            &rule,
            Target::Candidate(&candidate),
            &mut executor,
        )
        .await
        .unwrap();
        facts.require(&context, &rule, &BTreeSet::new(), &mut executor).unwrap();
        assert!(facts.objects.is_empty());

        assert!(matches!(
            TargetFacts::load(
                &BTreeMap::new(),
                &context,
                &rule,
                Target::Candidate(&candidate),
                &mut ReadExecutor::default()
            )
            .await,
            Err(Error::ValidationError(_))
        ));

        let wrong = ObjectFacts { version: "persisted-v1".into(), ..candidate };
        assert!(matches!(
            TargetFacts::load(&BTreeMap::new(), &context, &rule, Target::Candidate(&wrong), &mut executor)
                .await,
            Err(Error::ValidationError(_))
        ));
    }

    #[tokio::test]
    async fn ownership_change_checks_persistent_original_and_confirmed_destination() {
        let context = context(&sales_inquiry::COMMANDS_SALES_SAVE);
        let rule = owned_rule(&context.permission);
        let current = ObjectFacts {
            id: "original".into(),
            version: "v1".into(),
            ..candidate(&context.resource, "user")
        };
        let port = Arc::new(Facts { objects: vec![current], ..Default::default() });
        let bindings: FactsBindings =
            [(context.resource.clone(), port.clone() as Arc<dyn AccessFactsPort>)].into();
        let mut executor = SnapshotExecutor::default();

        for (owner, allowed) in [("user", true), ("other", false)] {
            let candidate = candidate(&context.resource, owner);
            let facts = TargetFacts::load(
                &bindings,
                &context,
                &rule,
                Target::OwnershipChange { id: "original", candidate: &candidate },
                &mut executor,
            )
            .await
            .unwrap();
            assert_eq!(facts.require(&context, &rule, &BTreeSet::new(), &mut executor).is_ok(), allowed);
        }
        assert_eq!(
            port.calls.lock().unwrap().as_slice(),
            &[
                (context.resource.clone(), ["original".into()].into()),
                (context.resource.clone(), ["original".into()].into()),
            ]
        );

        let original_denied = Arc::new(Facts {
            objects: vec![ObjectFacts {
                id: "original".into(),
                version: "v1".into(),
                ..candidate(&context.resource, "other")
            }],
            ..Default::default()
        });
        let bindings: FactsBindings =
            [(context.resource.clone(), original_denied as Arc<dyn AccessFactsPort>)].into();
        let destination = candidate(&context.resource, "user");
        let facts = TargetFacts::load(
            &bindings,
            &context,
            &rule,
            Target::OwnershipChange { id: "original", candidate: &destination },
            &mut executor,
        )
        .await
        .unwrap();
        assert!(matches!(
            facts.require(&context, &rule, &BTreeSet::new(), &mut executor),
            Err(Error::Forbidden(_))
        ));
    }

    #[tokio::test]
    async fn candidate_sources_are_loaded_from_actual_references_and_missing_sources_do_not_pass() {
        let context = context(&purchase_payment_request::CREATE);
        let qualified = QualifiedPermissions::test_grants(&[("buyer", &[purchase_payment_request::CREATE])]);
        let scopes = [(
            purchase_order::DETAIL,
            ResolvedScope {
                role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
                user_limit: None,
            },
        )];
        let rule = rules::sources::data_condition(
            &context.permission,
            &qualified,
            &scopes,
            rules::sources::Output::Object,
        )
        .unwrap();
        let input_candidate = ObjectFacts {
            resource: context.resource.clone(),
            relations: [(
                RelationKey::parse("purchase_source").unwrap(),
                RelatedObjects { expected_ids: ["p1".into()].into(), objects: vec![] },
            )]
            .into(),
            ..Default::default()
        };
        let mut executor = SnapshotExecutor::default();
        for present in [true, false] {
            let port = Arc::new(Facts {
                objects: if present {
                    vec![ObjectFacts {
                        id: "p1".into(),
                        version: "v1".into(),
                        ..candidate("purchase_order", "user")
                    }]
                } else {
                    vec![]
                },
                ..Default::default()
            });
            let bindings: FactsBindings =
                [("purchase_order".into(), port.clone() as Arc<dyn AccessFactsPort>)].into();
            let facts = TargetFacts::load(
                &bindings,
                &context,
                &rule,
                Target::Candidate(&input_candidate),
                &mut executor,
            )
            .await
            .unwrap();
            let result = facts.require(&context, &rule, &BTreeSet::new(), &mut executor);
            if present {
                result.unwrap();
            } else {
                assert!(matches!(result, Err(Error::Forbidden(_))));
            }
            assert_eq!(
                port.calls.lock().unwrap().as_slice(),
                &[("purchase_order".into(), ["p1".into()].into())]
            );
        }
    }

    /// 复用列表权限的任务办理仍必须在快照事务中执行。
    #[tokio::test]
    async fn task_with_read_permission_rejects_nontransaction_executor() {
        let context = context(&fulfillment::DELIVERY_DELIVERY_LIST);
        let mut executor = ReadExecutor::default();
        let result = TargetFacts::load(
            &BTreeMap::new(),
            &context,
            &AccessRule::Allow,
            Target::Task { id: "task", version: "1" },
            &mut executor,
        )
        .await;
        assert!(matches!(result, Err(Error::ValidationError(message)) if message.contains("快照事务")));
    }

    #[tokio::test]
    async fn task_requires_exact_identity_version_and_current_assignment() {
        let context = context(&sales_inquiry::COMMANDS_PROCUREMENT_RENEW);
        let rule = rules::data_condition(&context.permission, None).unwrap();
        let task = ObjectFacts {
            resource: context.resource.clone(),
            id: "task".into(),
            version: "v2".into(),
            assignee_id: FactValue::Known(Some("user".into())),
            participants: FactValue::Known(BTreeSet::new()),
            completed: FactValue::Known(false),
            ..Default::default()
        };
        let port = Arc::new(Facts { objects: vec![task], ..Default::default() });
        let bindings: FactsBindings = [(context.resource.clone(), port as Arc<dyn AccessFactsPort>)].into();
        let mut executor = SnapshotExecutor::default();
        let facts = TargetFacts::load(
            &bindings,
            &context,
            &rule,
            Target::Task { id: "task", version: "v2" },
            &mut executor,
        )
        .await
        .unwrap();
        facts.require(&context, &rule, &BTreeSet::new(), &mut executor).unwrap();
        assert!(matches!(
            TargetFacts::load(
                &bindings,
                &context,
                &rule,
                Target::Task { id: "task", version: "v1" },
                &mut executor
            )
            .await,
            Err(Error::ConflictError(_))
        ));
        assert!(matches!(
            TargetFacts::load(
                &bindings,
                &context,
                &rule,
                Target::Task { id: "business-id", version: "v2" },
                &mut executor
            )
            .await,
            Err(Error::NotFound(_))
        ));
        assert!(matches!(
            TargetFacts::load(
                &bindings,
                &context,
                &rule,
                Target::Records(&ResourceTargets::new(&["task"]).unwrap()),
                &mut executor
            )
            .await,
            Err(Error::ValidationError(_))
        ));
    }

    #[tokio::test]
    async fn loaded_targets_expire_and_cannot_cross_writes_or_same_session_next_transaction() {
        let context = context(&sales_inquiry::COMMANDS_SALES_CREATE);
        let rule = owned_rule(&context.permission);
        let candidate = candidate(&context.resource, "user");
        let mut executor = SnapshotExecutor::default();
        let mut facts = TargetFacts::load(
            &BTreeMap::new(),
            &context,
            &rule,
            Target::Candidate(&candidate),
            &mut executor,
        )
        .await
        .unwrap();

        executor.next_read();
        assert!(matches!(
            facts.require(&context, &rule, &BTreeSet::new(), &mut executor),
            Err(Error::ValidationError(_))
        ));

        executor.restart();
        assert!(matches!(
            facts.require(&context, &rule, &BTreeSet::new(), &mut executor),
            Err(Error::ValidationError(_))
        ));

        facts.boundary = executor.boundary().unwrap();
        facts.candidate.as_mut().unwrap().valid_until = Some(Instant::from_unix_secs(1));
        assert!(matches!(
            facts.require(&context, &rule, &BTreeSet::new(), &mut executor),
            Err(Error::ConflictError(_))
        ));
    }

    #[test]
    fn candidate_and_its_sources_share_the_total_expansion_limit() {
        let executor = SnapshotExecutor::default();
        let children = (0..999)
            .map(|index| ObjectFacts {
                resource: "purchase_order".into(),
                id: format!("p{index}"),
                version: "v1".into(),
                ..Default::default()
            })
            .collect::<Vec<_>>();
        let mut related = RelatedObjects {
            expected_ids: children.iter().map(|child| child.id.clone()).collect(),
            objects: children,
        };
        let build = |related: RelatedObjects| TargetFacts {
            task: false,
            objects: vec![],
            boundary: executor.boundary().unwrap(),
            candidate: Some(ObjectFacts {
                resource: "purchase_payment_request".into(),
                relations: [(RelationKey::parse("purchase_source").unwrap(), related)].into(),
                ..Default::default()
            }),
        };
        build(related.clone()).ensure_current().unwrap();

        related.expected_ids.insert("extra".into());
        related.objects.push(ObjectFacts {
            resource: "purchase_order".into(),
            id: "extra".into(),
            version: "v1".into(),
            ..Default::default()
        });
        assert!(matches!(build(related).ensure_current(), Err(Error::ValidationError(_))));
    }
}
