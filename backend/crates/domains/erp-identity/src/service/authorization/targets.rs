//! 已有记录、服务端候选和精确任务的事实执行。

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use erp_core::common::time::Instant;
use persistence_core::{ExecutionBoundary, Executor};

use super::approvals;
use super::rules;
use crate::entity::authorization::{
    AccessContext, AccessRule, AccessTarget, ObjectFacts, RelationKey, RelationMatch, ResourceTargets,
};
use crate::entity::resource_access::ResourceKind;
use crate::ports::AccessFactsPort;
use crate::repository::authorization::AccessProof;
use crate::{Error, Result};

type FactsBindings = BTreeMap<String, Arc<dyn AccessFactsPort>>;

pub(super) struct TargetFacts {
    objects: Vec<ObjectFacts>,
    candidate: Option<ObjectFacts>,
    boundary: ExecutionBoundary,
}

/// 来源证明保留独立加载的完整事实，供原读取阶段统一复验和最终期限检查。
#[derive(Default)]
pub(super) struct SourceEvidence {
    pub(super) proofs: Vec<AccessProof>,
    pub(super) facts: Vec<(AccessContext, TargetFacts)>,
}

impl SourceEvidence {
    // 合并同一授权阶段的附加来源证明及事实；对象复验专用的事实也要保留。
    pub(super) fn extend(&mut self, other: Self) {
        self.proofs.extend(other.proofs);
        self.facts.extend(other.facts);
    }
}

impl TargetFacts {
    // 授权时用原执行器取得完整目标及来源；候选创建对象尚未存在，不查根记录，但现存来源仍须真实取证。
    pub(super) async fn load(
        bindings: &FactsBindings,
        context: &AccessContext,
        rule: &AccessRule,
        target: &AccessTarget,
        executor: &mut dyn Executor,
    ) -> Result<Self> {
        if context.resource != context.permission.resource() {
            return Err(Error::ValidationError("目标资源与权限错配".into()));
        }
        let boundary =
            executor.boundary().ok_or_else(|| Error::ValidationError("目标读取缺少执行边界".into()))?;

        let (ids, candidate, version) = match target {
            AccessTarget::Records(ids) => (Some(Cow::Borrowed(ids)), None, None),
            AccessTarget::Candidate(candidate) => (None, Some(candidate), None),
            AccessTarget::OwnershipChange { id, candidate } => {
                (Some(Cow::Owned(ResourceTargets::new(&[id])?)), Some(candidate), None)
            },
            AccessTarget::Task { id, version } => {
                if version.is_empty() {
                    return Err(Error::ValidationError("必须提供精确任务版本".into()));
                }
                (Some(Cow::Owned(ResourceTargets::new(&[id])?)), None, Some(version))
            },
            AccessTarget::Operation => return Err(Error::ValidationError("无对象目标不能读取事实".into())),
        };
        rules::ensure_target(&context.permission, target)?;
        if (version.is_some() || !rules::is_read(&context.permission)) && !boundary.is_snapshot() {
            return Err(Error::ValidationError("对象写入授权必须使用快照事务".into()));
        }

        let objects = if let Some(ids) = ids.as_deref() {
            if ids.ids().is_empty() {
                return Err(Error::ValidationError("授权目标不能为空".into()));
            }
            let objects = load_records(bindings, context, ids, version.is_some(), executor).await?;
            if version.is_some_and(|version| objects[0].version != *version) {
                return Err(Error::ConflictError("任务版本已改变".into()));
            }
            objects
        } else {
            Vec::new()
        };
        let candidate = match candidate {
            Some(candidate) => Some(load_candidate(bindings, context, rule, candidate, executor).await?),
            None => None,
        };

        let result = Self { objects, candidate, boundary };
        if !result.boundary.is_current(executor) {
            return Err(Error::ValidationError("目标读取跨越执行阶段".into()));
        }
        result.ensure_current()?;
        Ok(result)
    }

    // 对本次根对象逐一应用固定数据条件；归属变更还须核验新归属。精确任务同查询共用数据规则，不允许部分通过。
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

    // 借出已经核验的根对象，审批管理以其中冻结事实识别真实类型和来源，不能信任调用方声明。
    pub(super) fn roots(&self) -> impl Iterator<Item = &ObjectFacts> {
        self.objects.iter().chain(self.candidate.iter())
    }

    // 使用统一总量预算有界展开根目标及来源；异步取证结束还须校验已经收集的全部到期边界。
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

    // 交付对象检查时，非事务路径重读并比较完整根与来源事实；原快照不重复读同一批对象。
    pub(super) async fn revalidate(
        &self,
        bindings: &FactsBindings,
        context: &AccessContext,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if !self.boundary.is_current(executor) {
            return Err(Error::ValidationError("目标检查跨越执行阶段".into()));
        }
        if !self.boundary.is_snapshot() && !self.objects.is_empty() {
            let ids = self.objects.iter().map(|object| object.id.as_str()).collect::<Vec<_>>();
            let current =
                load_records(bindings, context, &ResourceTargets::new(&ids)?, false, executor).await?;
            if current != self.objects {
                return Err(Error::ConflictError("DATA_SCOPE_CHANGED：对象事实已改变".into()));
            }
        }

        self.ensure_current()
    }
}

// 精确任务从固定任务端口读取，返回对象仍按原入口资源核对。
async fn load_records(
    bindings: &FactsBindings,
    context: &AccessContext,
    ids: &ResourceTargets,
    task: bool,
    executor: &mut dyn Executor,
) -> Result<Vec<ObjectFacts>> {
    let (resource, missing) = if task {
        (rules::task_facts_resource(&context.permission), "缺少任务 Facts 绑定")
    } else {
        (context.resource.as_str(), "缺少对象 Facts 绑定")
    };
    let port = bindings.get(resource).ok_or_else(|| Error::ValidationError(missing.into()))?;
    let mut objects = port.load(context, ids, executor).await?;
    ids.ensure_objects(&context.resource, &objects)?;
    ObjectFacts::canonicalize(&mut objects)?;
    Ok(objects)
}

// 候选只提供已核对身份和真实引用；关联的已有记录沿原执行器由拥有领域重新读取。
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

// 只为根层记录收集固定来源关系的资源声明，不把嵌套来源关系提升为直接根关系。
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

// 完整对象授权必须从完整覆盖条件取证，不能用部分来源存在性或份额匹配放行整个对象。
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
    // 检查新建或变更的候选事实没有冒充持久化记录版本；已有对象权限必须重新读取当前记录。
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
            &AccessTarget::Candidate(candidate.clone()),
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
                &AccessTarget::Candidate(candidate.clone()),
                &mut ReadExecutor::default()
            )
            .await,
            Err(Error::ValidationError(_))
        ));

        let wrong = ObjectFacts { version: "persisted-v1".into(), ..candidate };
        assert!(matches!(
            TargetFacts::load(
                &BTreeMap::new(),
                &context,
                &rule,
                &AccessTarget::Candidate(wrong),
                &mut executor
            )
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
                &AccessTarget::OwnershipChange { id: "original".into(), candidate },
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
            &AccessTarget::OwnershipChange { id: "original".into(), candidate: destination },
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
                &AccessTarget::Candidate(input_candidate.clone()),
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

    // 验证：复用列表权限的任务办理仍必须在快照事务中执行。
    #[tokio::test]
    async fn task_with_read_permission_rejects_nontransaction_executor() {
        let context = context(&fulfillment::DELIVERY_DELIVERY_LIST);
        let mut executor = ReadExecutor::default();
        let result = TargetFacts::load(
            &BTreeMap::new(),
            &context,
            &AccessRule::Allow,
            &AccessTarget::Task { id: "task".into(), version: "1".into() },
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
        let bindings: FactsBindings =
            [(context.resource.clone(), port.clone() as Arc<dyn AccessFactsPort>)].into();
        let mut executor = SnapshotExecutor::default();
        let facts = TargetFacts::load(
            &bindings,
            &context,
            &rule,
            &AccessTarget::Task { id: "task".into(), version: "v2".into() },
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
                &AccessTarget::Task { id: "task".into(), version: "v1".into() },
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
                &AccessTarget::Task { id: "business-id".into(), version: "v2".into() },
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
                &AccessTarget::record("task").unwrap(),
                &mut executor
            )
            .await,
            Err(Error::ValidationError(_))
        ));

        let reads = port.calls.lock().unwrap().len();
        facts.revalidate(&bindings, &context, &mut executor).await.unwrap();
        executor.next_read();
        assert!(matches!(
            facts.revalidate(&bindings, &context, &mut executor).await,
            Err(Error::ValidationError(message)) if message == "目标检查跨越执行阶段"
        ));
        assert_eq!(port.calls.lock().unwrap().len(), reads);
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
            &AccessTarget::Candidate(candidate.clone()),
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
