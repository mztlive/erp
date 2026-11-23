use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use application_core::AuditActor;
use async_trait::async_trait;
use entity_core::BaseModel;
use erp_core::AccountKind;
use erp_core::common::time::Instant;
use erp_permissions::MODULES;
use persistence_core::testing::SnapshotExecutor;
use persistence_core::{Executor, ReadExecutor};

use super::super::facilities::Facilities;
use super::super::{AuthorizationBuilder, AuthorizationService, rules};
use crate::entity::access_control::scope_spec::{self, ActionKind};
use crate::entity::access_control::{ResolvedScope, ScopeClause};
use crate::entity::authorization::{
    AccessContext, AccessTarget, FactValue, ObjectFacts, RelatedObjects, RelationKey, ResourceTargets,
};
use crate::entity::organization::{OrgMembership, OrgValidity};
use crate::entity::organization_change::OrganizationState;
use crate::entity::resource_access::{GrantTarget, ResourceKind, SourceKey};
use crate::ports::AccessFactsPort;
pub(super) use crate::repository::authorization::testing::{inquiry_members, mapping};
use crate::service::access_control::resolve::AuthorizedDataScope;
use crate::{CompiledAccess, Error, Permission, QualifiedPermissions, Result};

/// 身份替身中可直接切换的账号、角色和故障状态。
pub(super) struct IdentityState {
    pub active: bool,
    pub grants: Vec<(String, Vec<Permission>)>,
    pub fail: bool,
    pub denied_scope: bool,
}

/// 提供可调权限、范围及记录授权的内存身份设施。
pub(super) struct Identity {
    pub state: Mutex<IdentityState>,
    pub revision: AtomicU64,
    pub permissions_calls: AtomicU64,
    pub grant_revision: AtomicU64,
    pub record_grants: Mutex<BTreeSet<(ResourceKind, String)>>,
    pub source_grants: Mutex<BTreeSet<(String, ResourceKind, String)>>,
    pub scopes: Mutex<BTreeMap<(Permission, Permission), ResolvedScope>>,
    pub role_scopes: Mutex<BTreeMap<(String, Permission), ScopeClause>>,
    pub scope_deadlines: Mutex<BTreeMap<Permission, Instant>>,
}

impl Default for Identity {
    fn default() -> Self {
        let mut permissions: Vec<Permission> = MODULES
            .iter()
            .flat_map(|module| module.entries)
            .map(|(_, entry)| entry.permission.clone())
            .collect();
        permissions.extend(rules::permissions().into_iter().cloned());
        Self {
            state: Mutex::new(IdentityState {
                active: true,
                grants: vec![("role".into(), permissions)],
                fail: false,
                denied_scope: false,
            }),
            revision: AtomicU64::new(1),
            permissions_calls: AtomicU64::new(0),
            grant_revision: AtomicU64::new(1),
            record_grants: Mutex::new(BTreeSet::new()),
            source_grants: Mutex::new(BTreeSet::new()),
            scopes: Mutex::new(BTreeMap::new()),
            role_scopes: Mutex::new(BTreeMap::new()),
            scope_deadlines: Mutex::new(BTreeMap::new()),
        }
    }
}

#[async_trait]
impl Facilities for Identity {
    async fn permissions(
        &self,
        _: &AuditActor,
        permissions: &[Permission],
        _: &mut dyn Executor,
    ) -> Result<Option<QualifiedPermissions>> {
        self.permissions_calls.fetch_add(1, Ordering::SeqCst);
        let state = self.state.lock().unwrap();
        if state.fail {
            return Err(Error::Internal("identity storage unavailable".into()));
        }
        if !state.active {
            return Ok(None);
        }

        let grants = state
            .grants
            .iter()
            .map(|(role, granted)| {
                (
                    role.clone(),
                    granted
                        .iter()
                        .filter(|permission| permissions.contains(permission))
                        .cloned()
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>();
        let grants = grants
            .iter()
            .map(|(role, permissions)| (role.as_str(), permissions.as_slice()))
            .collect::<Vec<_>>();
        Ok(Some(QualifiedPermissions::test_grants(&grants)))
    }

    async fn scope(
        &self,
        context: &AccessContext,
        source: &Permission,
        required: &[Permission],
        qualified: &QualifiedPermissions,
        _: &mut dyn Executor,
    ) -> Result<AuthorizedDataScope> {
        let role_scopes = self.role_scopes.lock().unwrap();
        let scoped_roles =
            role_scopes.keys().any(|(_, permission)| permission == source).then(|| ResolvedScope {
                role_clauses: qualified
                    .roles_for_all(required)
                    .filter_map(|role| role_scopes.get(&(role.base.id.clone(), source.clone())).cloned())
                    .collect(),
                user_limit: None,
            });

        let mut organizations = OrganizationState::default();
        if let Some(deadline) = self.scope_deadlines.lock().unwrap().get(&context.permission) {
            organizations.memberships.push(OrgMembership {
                base: BaseModel::fake(),
                user_id: context.actor.id().into(),
                org_unit_id: "test".into(),
                validity: OrgValidity { valid_from: Instant::from_unix_secs(1), valid_to: Some(*deadline) },
                changed_by: "admin".into(),
                reason: "test".into(),
            });
        }

        Ok(AuthorizedDataScope {
            scope: scoped_roles.unwrap_or_else(|| {
                self.scopes
                    .lock()
                    .unwrap()
                    .get(&(context.permission.clone(), source.clone()))
                    .cloned()
                    .unwrap_or_else(|| {
                        if self.state.lock().unwrap().denied_scope {
                            ResolvedScope { role_clauses: vec![], user_limit: None }
                        } else {
                            self_scope()
                        }
                    })
            }),
            organizations,
            policy_version: 1,
            scope_version: self.revision.load(Ordering::SeqCst).to_string(),
            as_of: context.as_of,
        })
    }

    async fn grant_revision(&self, _: &AuditActor, _: ResourceKind, _: &mut dyn Executor) -> Result<u64> {
        Ok(self.grant_revision.load(Ordering::SeqCst))
    }

    async fn grants(
        &self,
        _: &AccessContext,
        resource: ResourceKind,
        ids: &ResourceTargets,
        _: &mut dyn Executor,
    ) -> Result<BTreeSet<String>> {
        Ok(self
            .record_grants
            .lock()
            .unwrap()
            .iter()
            .filter(|(kind, id)| *kind == resource && ids.ids().contains(id))
            .map(|(_, id)| id.clone())
            .collect())
    }

    async fn source_grant(
        &self,
        source: &SourceKey,
        target: &GrantTarget,
        _: &mut dyn Executor,
    ) -> Result<bool> {
        Ok(self.source_grants.lock().unwrap().contains(&(source.id()?, target.resource, target.id.clone())))
    }
}

/// 可替换对象并累计读取次数的内存事实端口。
#[derive(Default)]
pub(super) struct Facts {
    pub objects: Mutex<Vec<ObjectFacts>>,
    pub calls: AtomicU64,
}

impl Facts {
    pub fn new(objects: Vec<ObjectFacts>) -> Self {
        Self { objects: Mutex::new(objects), calls: AtomicU64::new(0) }
    }
}

#[async_trait]
impl AccessFactsPort for Facts {
    async fn load(
        &self,
        _: &AccessContext,
        ids: &ResourceTargets,
        _: &mut dyn Executor,
    ) -> Result<Vec<ObjectFacts>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(self
            .objects
            .lock()
            .unwrap()
            .iter()
            .filter(|object| ids.ids().contains(&object.id))
            .cloned()
            .collect())
    }
}

/// 统一测试夹具：集中管理内存身份、资源事实及已装配的真实 `AuthorizationService`。
pub(super) struct AuthHarness {
    pub identity: Arc<Identity>,
    pub facts: BTreeMap<String, Arc<Facts>>,
    pub service: AuthorizationService,
}

impl AuthHarness {
    pub fn new() -> Self {
        Self::with_multi_facts(&[])
    }

    pub fn with_facts(resource: &str, objects: Vec<ObjectFacts>) -> Self {
        Self::with_multi_facts(&[(resource, objects)])
    }

    pub fn with_multi_facts(entries: &[(&str, Vec<ObjectFacts>)]) -> Self {
        let identity = Arc::new(Identity::default());
        let mut builder = complete();
        builder.facilities = identity.clone();
        let mut facts = BTreeMap::new();
        for (resource, objects) in entries {
            let port = Arc::new(Facts::new(objects.clone()));
            builder.facts.insert((*resource).into(), port.clone());
            facts.insert((*resource).into(), port);
        }
        Self { identity, facts, service: builder.build().unwrap() }
    }

    pub fn grants(&self, roles: &[(&str, &[Permission])]) {
        self.identity.state.lock().unwrap().grants =
            roles.iter().map(|(role, permissions)| ((*role).into(), permissions.to_vec())).collect();
    }

    pub fn clear_grants(&self) {
        self.identity.state.lock().unwrap().grants.clear();
    }

    pub fn set_active(&self, active: bool) {
        self.identity.state.lock().unwrap().active = active;
    }

    pub fn set_fail(&self, fail: bool) {
        self.identity.state.lock().unwrap().fail = fail;
    }

    pub fn set_denied_scope(&self, denied: bool) {
        self.identity.state.lock().unwrap().denied_scope = denied;
    }

    pub fn set_scope(&self, entry: &Permission, source: &Permission, scope: ResolvedScope) {
        self.identity.scopes.lock().unwrap().insert((entry.clone(), source.clone()), scope);
    }

    pub fn set_company_scope(&self, entry: &Permission, source: &Permission) {
        self.set_scope(entry, source, company_scope());
    }

    pub fn set_role_scope(&self, role: &str, source: &Permission, clause: ScopeClause) {
        self.identity.role_scopes.lock().unwrap().insert((role.into(), source.clone()), clause);
    }

    pub fn add_record_grant(&self, kind: ResourceKind, id: &str) {
        self.identity.record_grants.lock().unwrap().insert((kind, id.into()));
    }

    pub fn add_source_grant(&self, source: &SourceKey, kind: ResourceKind, id: &str) {
        self.identity.source_grants.lock().unwrap().insert((source.id().unwrap(), kind, id.into()));
    }

    pub fn set_objects(&self, resource: &str, objects: Vec<ObjectFacts>) {
        *self.facts[resource].objects.lock().unwrap() = objects;
    }

    pub fn mutate_first(&self, resource: &str, f: impl FnOnce(&mut ObjectFacts)) {
        f(&mut self.facts[resource].objects.lock().unwrap()[0]);
    }

    pub fn calls(&self, resource: &str) -> u64 {
        self.facts[resource].calls.load(Ordering::SeqCst)
    }

    pub fn permission_calls(&self) -> u64 {
        self.identity.permissions_calls.load(Ordering::SeqCst)
    }

    pub async fn require(
        &self,
        permission: &Permission,
        target: AccessTarget,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.service.require(&actor(), permission, target, executor).await
    }

    pub async fn require_snap(&self, permission: &Permission, target: AccessTarget) -> Result<()> {
        self.require(permission, target, &mut SnapshotExecutor::default()).await
    }

    pub async fn require_read(&self, permission: &Permission, target: AccessTarget) -> Result<()> {
        self.require(permission, target, &mut ReadExecutor::default()).await
    }

    pub async fn query(
        &self,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<CompiledAccess> {
        self.service.query(&actor(), permission, executor).await
    }

    pub async fn validate(&self, access: &CompiledAccess, executor: &mut dyn Executor) -> Result<()> {
        self.service.validate(access, executor).await
    }

    /// 校验 `required` 中的每一项权限都必须由同一角色持有：缺少或拆到另一角色时均返回 `Forbidden`。
    pub async fn assert_same_role_required(
        &self,
        prefix_roles: &[(&str, &[Permission])],
        required: &[Permission],
        permission: &Permission,
        target: impl Fn() -> AccessTarget,
    ) {
        let mut required = required.to_vec();
        required.sort();
        required.dedup();

        let mut full_roles = prefix_roles.to_vec();
        full_roles.push(("combined", &required));
        self.grants(&full_roles);
        self.require_snap(permission, target()).await.unwrap();

        for missing in 0..required.len() {
            let partial = without_index(&required, missing);
            let splits: &[bool] = if required.len() > 1 { &[false, true] } else { &[false] };
            for &split in splits {
                let mut roles = prefix_roles.to_vec();
                roles.push(("partial", &partial));
                if split {
                    roles.push(("split", &required[missing..missing + 1]));
                }
                self.grants(&roles);
                assert!(
                    matches!(self.require_snap(permission, target()).await, Err(Error::Forbidden(_))),
                    "{permission} 缺少或拆分 {} (split={split}) 必须拒绝",
                    required[missing]
                );
            }
        }
    }
}

pub(super) fn without_index(permissions: &[Permission], missing: usize) -> Vec<Permission> {
    permissions
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != missing)
        .map(|(_, permission)| permission.clone())
        .collect()
}

pub(super) fn company_scope() -> ResolvedScope {
    ResolvedScope {
        role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
        user_limit: None,
    }
}

pub(super) fn self_scope() -> ResolvedScope {
    ResolvedScope {
        role_clauses: vec![ScopeClause { self_owned: true, ..Default::default() }],
        user_limit: None,
    }
}

pub(super) fn attach_relation(root: &mut ObjectFacts, relation: &str, source: ObjectFacts) {
    root.relations.insert(
        RelationKey::parse(relation).unwrap(),
        RelatedObjects { expected_ids: [source.id.clone()].into(), objects: vec![source] },
    );
}

pub(super) fn empty() -> AuthorizationBuilder {
    AuthorizationBuilder {
        facilities: Arc::new(Identity::default()),
        facts: BTreeMap::new(),
        mappings: BTreeMap::new(),
    }
}

pub(super) fn complete() -> AuthorizationBuilder {
    let mut builder = empty();
    for permission in rules::permissions() {
        if !matches!(scope_spec::kind(permission).unwrap(), ActionKind::Function) {
            builder.facts.entry(permission.resource().into()).or_insert_with(|| Arc::new(Facts::default()));
        }
        if rules::query_supported(permission).unwrap() {
            builder.mappings.insert(permission.clone(), mapping(permission));
        }
    }
    builder
}

pub(super) fn service(identity: Arc<Identity>, facts: Arc<Facts>) -> AuthorizationService {
    let mut builder = complete();
    builder.facilities = identity;
    builder.facts.insert("sales_inquiry".into(), facts);
    builder.build().unwrap()
}

pub(super) fn actor() -> AuditActor {
    AuditActor::new("user".into(), "login".into(), AccountKind::Admin)
}

pub(super) fn candidate(resource: &str, owner: &str) -> ObjectFacts {
    ObjectFacts {
        resource: resource.into(),
        owner_id: FactValue::Known(Some(owner.into())),
        org_unit_id: FactValue::Known(None),
        ..Default::default()
    }
}

pub(super) fn object(resource: &str, id: &str, owner: &str) -> ObjectFacts {
    ObjectFacts {
        id: id.into(),
        version: "v1".into(),
        history_participants: FactValue::Known(BTreeSet::new()),
        ..candidate(resource, owner)
    }
}
