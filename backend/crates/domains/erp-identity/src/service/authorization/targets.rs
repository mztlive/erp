//! 已有记录、服务端候选和精确任务的事实执行。

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use erp_core::common::time::Instant;
use persistence_core::{ExecutionBoundary, Executor};

use super::{approvals, rules};
use crate::entity::authorization::{
    AccessContext, AccessRule, AccessTarget, ObjectFacts, RelationKey, RelationMatch, ResourceTargets,
};
use crate::entity::resource_access::ResourceKind;
use crate::ports::AccessFactsPort;
use crate::repository::authorization::AccessProof;
use crate::{Error, Result};

type FactsBindings = BTreeMap<String, Arc<dyn AccessFactsPort>>;

/// 一次对象授权加载的实际根和候选归属，绑定原执行阶段并带完整来源事实。
pub(super) struct TargetFacts {
    // 按资源及 ID 读取并核验的持久化根对象或精确任务。
    objects: Vec<ObjectFacts>,
    // 创建或归属变更使用的服务端候选，现存来源由原执行器另行读取。
    candidate: Option<ObjectFacts>,
    // 加载对象时的事务或读取阶段，用于拒绝跨阶段复用。
    boundary: ExecutionBoundary,
}

/// 保留附加来源证明和独立加载的事实，供原读取阶段统一复验依赖及检查期限。
#[derive(Default)]
pub(super) struct SourceEvidence {
    /// 各附加来源使用的规则和有效性证明。
    pub(super) proofs: Vec<AccessProof>,
    /// 需单独复验的来源对象批次及其原授权上下文。
    pub(super) facts: Vec<(AccessContext, TargetFacts)>,
}

impl SourceEvidence {
    /// 汇集不需另附独立事实批次的来源证明，供对象交付统一复验。
    pub(super) fn from_proofs(proofs: Vec<AccessProof>) -> Self {
        Self { proofs, facts: Vec::new() }
    }

    /// 合并同一授权阶段的来源证明和独立加载事实，保留对象复验所需的全部依据。
    pub(super) fn extend(&mut self, other: Self) {
        self.proofs.extend(other.proofs);
        self.facts.extend(other.facts);
    }
}

impl TargetFacts {
    /// 在原阶段加载完整目标及来源；写入或精确任务须用快照，任务还须核对版本。创建候选不读不存在的根记录，现存来源仍须真实取证。
    pub(super) async fn load(
        bindings: &FactsBindings,
        context: &AccessContext,
        rule: &AccessRule,
        target: AccessTarget,
        executor: &mut dyn Executor,
    ) -> Result<Self> {
        if context.resource != context.permission.resource() {
            return Err(Error::ValidationError("目标资源与权限错配".into()));
        }
        let boundary =
            executor.boundary().ok_or_else(|| Error::ValidationError("目标读取缺少执行边界".into()))?;

        let (ids, version) = match &target {
            AccessTarget::Records(ids) => (Some(Cow::Borrowed(ids)), None),
            AccessTarget::Candidate(_) => (None, None),
            AccessTarget::OwnershipChange { id, .. } => {
                (Some(Cow::Owned(ResourceTargets::new(&[id])?)), None)
            },
            AccessTarget::Task { id, version } => {
                if version.is_empty() {
                    return Err(Error::ValidationError("必须提供精确任务版本".into()));
                }
                (Some(Cow::Owned(ResourceTargets::new(&[id])?)), Some(version))
            },
            AccessTarget::Operation => return Err(Error::ValidationError("无对象目标不能读取事实".into())),
        };
        rules::ensure_target(&context.permission, &target)?;
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
        let candidate = match target {
            AccessTarget::Candidate(candidate) | AccessTarget::OwnershipChange { candidate, .. } => {
                Some(load_candidate(bindings, context, rule, candidate, executor).await?)
            },
            _ => None,
        };

        let result = Self { objects, candidate, boundary };
        if !result.boundary.is_current(executor) {
            return Err(Error::ValidationError("目标读取跨越执行阶段".into()));
        }
        result.flattened()?;
        Ok(result)
    }

    /// 逐一用固定条件检查根对象，归属变更同时检查原记录和新归属，精确任务使用与查询共用的规则；不允许部分通过。
    pub(super) fn require(
        &self,
        context: &AccessContext,
        rule: &AccessRule,
        grants: &BTreeSet<(ResourceKind, String)>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.ensure_stage(executor)?;
        ensure_whole_object(rule)?;
        self.ensure_current()?;
        for object in self.roots() {
            if !rule.allows_object(context.actor.id(), object, grants)? {
                return Err(Error::Forbidden("操作包含不可访问的对象或目标归属".into()));
            }
        }
        self.ensure_current()
    }

    /// 借出已核验的持久化根和服务端候选，供对象判定及按冻结事实识别实际来源。
    pub(super) fn roots(&self) -> impl Iterator<Item = &ObjectFacts> {
        self.objects.iter().chain(self.candidate.iter())
    }

    /// 用统一预算展开根、候选和嵌套来源，供补权和到期收集；异步取证后仍须核对期限。
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

    /// 检查整批根、候选及嵌套来源均未到期，防止用已失效的责任事实授权。
    pub(super) fn ensure_current(&self) -> Result<()> {
        for object in &self.objects {
            ensure_unexpired(object)?;
        }
        if let Some(candidate) = &self.candidate {
            if candidate.valid_until.is_some_and(|until| Instant::now() >= until) {
                return Err(Error::ConflictError("DATA_SCOPE_CHANGED：候选归属已到期".into()));
            }
            for child in source_objects(candidate) {
                ensure_unexpired(child)?;
            }
        }
        Ok(())
    }

    /// 对象交付前同时校验原执行阶段和整批事实有效期。
    pub(super) fn ensure_current_stage(&self, executor: &mut dyn Executor) -> Result<()> {
        self.ensure_stage(executor)?;
        self.ensure_current()
    }

    // 检查原事务或读取阶段仍有效，失败时在任何重读前拒绝。
    fn ensure_stage(&self, executor: &mut dyn Executor) -> Result<()> {
        if self.boundary.is_current(executor) {
            Ok(())
        } else {
            Err(Error::ValidationError("目标检查跨越执行阶段".into()))
        }
    }

    /// 对象交付时，非事务重读并以规范顺序比较完整根与来源事实；原快照不重读，两者均检查有效期。
    pub(super) async fn revalidate(
        &self,
        bindings: &FactsBindings,
        context: &AccessContext,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.ensure_stage(executor)?;
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

// 枚举当前对象已加载的直接来源，供递归检查后续层级的期限。
fn source_objects(object: &ObjectFacts) -> impl Iterator<Item = &ObjectFacts> {
    object.relations.values().flat_map(|relation| &relation.objects)
}

// 递归检查持久化对象及实际来源的责任有效期，到期报告范围变化。
fn ensure_unexpired(object: &ObjectFacts) -> Result<()> {
    if object.valid_until.is_some_and(|until| Instant::now() >= until) {
        return Err(Error::ConflictError("DATA_SCOPE_CHANGED：对象责任关系已到期".into()));
    }
    for child in source_objects(object) {
        ensure_unexpired(child)?;
    }
    Ok(())
}

// 通过固定事实端口按 ID 加载记录；精确任务使用专用端口，结果仍按原入口核验资源、完整性并规范排序。
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

// 验证服务端候选和真实引用，沿原执行器读取各现存来源；拒绝候选自带已加载事实、未登记关系或资源错配。
async fn load_candidate(
    bindings: &FactsBindings,
    context: &AccessContext,
    rule: &AccessRule,
    mut candidate: ObjectFacts,
    executor: &mut dyn Executor,
) -> Result<ObjectFacts> {
    candidate.ensure_candidate(&context.resource)?;
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

// 为候选只收集根层关系的固定资源声明；不把嵌套关系提升到根，拒绝同一关系绑定多个资源。
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

// 执行完整对象检查前递归拒绝任一来源匹配规则；受限份额或存在性不能证明整个对象可访问。
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
    // 确认候选资源、可选 ID 与有效期合法且没有持久化版本；已有记录的权限须用当前真实事实判断。
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

    // 记录快照调用并返回预设对象的测试事实端口，供目标取证和复验断言使用。
    #[derive(Default)]
    struct Facts {
        // 事实端口原样返回的对象样本，便于模拟未匹配或不完整结果。
        objects: Vec<ObjectFacts>,
        // 记录每次请求的资源和 ID 集合，用于断言实际取证及复验是否重读。
        calls: Mutex<Vec<(String, BTreeSet<String>)>>,
    }

    #[async_trait]
    impl AccessFactsPort for Facts {
        // 断言使用原快照、记录请求的资源及 ID，并返回预设事实。
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

    // 为指定测试权限构造固定账号的对象授权上下文。
    fn context(permission: &Permission) -> AccessContext {
        AccessContext {
            actor: AuditActor::new("user".into(), "user".into(), AccountKind::Admin),
            permission: permission.clone(),
            resource: permission.resource().into(),
            model: None,
            as_of: Instant::now(),
        }
    }

    // 构造指定负责人且没有持久化版本的归属候选样本。
    fn candidate(resource: &str, owner: &str) -> ObjectFacts {
        ObjectFacts {
            resource: resource.into(),
            owner_id: FactValue::Known(Some(owner.into())),
            org_unit_id: FactValue::Known(None),
            ..Default::default()
        }
    }

    // 用生产规则构造仅本人范围，供候选和归属变更测试。
    fn owned_rule(permission: &Permission) -> AccessRule {
        let scope = ResolvedScope {
            role_clauses: vec![ScopeClause { self_owned: true, ..Default::default() }],
            user_limit: None,
        };
        rules::data_condition(permission, Some(&scope)).unwrap()
    }

    // 验证新建只检查候选、不读取不存在的根，并拒绝非事务写入或伪造持久化版本。
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
            AccessTarget::Candidate(candidate.clone()),
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
                AccessTarget::Candidate(candidate.clone()),
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
                AccessTarget::Candidate(wrong),
                &mut executor
            )
            .await,
            Err(Error::ValidationError(_))
        ));
    }

    // 验证变更归属必须同时放行数据库原记录和服务端确认的新归属，并实际读取原记录。
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
                AccessTarget::OwnershipChange { id: "original".into(), candidate },
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
            AccessTarget::OwnershipChange { id: "original".into(), candidate: destination },
            &mut executor,
        )
        .await
        .unwrap();
        assert!(matches!(
            facts.require(&context, &rule, &BTreeSet::new(), &mut executor),
            Err(Error::Forbidden(_))
        ));
    }

    // 验证候选只提交真实引用，授权按引用回读来源；未匹配的来源不能放行。
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
                AccessTarget::Candidate(input_candidate.clone()),
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

    // 验证即使复用列表权限，精确任务仍须使用快照事务。
    #[tokio::test]
    async fn task_with_read_permission_rejects_nontransaction_executor() {
        let context = context(&fulfillment::DELIVERY_DELIVERY_LIST);
        let mut executor = ReadExecutor::default();
        let result = TargetFacts::load(
            &BTreeMap::new(),
            &context,
            &AccessRule::Allow,
            AccessTarget::Task { id: "task".into(), version: "1".into() },
            &mut executor,
        )
        .await;
        assert!(matches!(result, Err(Error::ValidationError(message)) if message.contains("快照事务")));
    }

    // 验证精确任务核对真实 ID、版本和指派，跨执行阶段在重读前拒绝。
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
            AccessTarget::Task { id: "task".into(), version: "v2".into() },
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
                AccessTarget::Task { id: "task".into(), version: "v1".into() },
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
                AccessTarget::Task { id: "business-id".into(), version: "v2".into() },
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
                AccessTarget::record("task").unwrap(),
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

    // 验证已加载目标到期、执行过写入或进入同一会话新事务后不能复用。
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
            AccessTarget::Candidate(candidate.clone()),
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

    // 验证候选根及来源共用事实数量上限，超限报错而非截断。
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
        build(related.clone()).flattened().unwrap();

        related.expected_ids.insert("extra".into());
        related.objects.push(ObjectFacts {
            resource: "purchase_order".into(),
            id: "extra".into(),
            version: "v1".into(),
            ..Default::default()
        });
        assert!(matches!(build(related).flattened(), Err(Error::ValidationError(_))));
    }
}
