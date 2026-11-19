use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use application_core::AuditActor;
use async_trait::async_trait;
use entity_core::BaseModel;
use erp_core::AccountKind;
use erp_core::common::time::Instant;
use erp_permissions::MODULES;
use persistence_core::Executor;

use super::super::facilities::Facilities;
use super::super::{AuthorizationBuilder, AuthorizationService, rules};
use crate::entity::access_control::scope_spec::{self, ActionKind};
use crate::entity::access_control::{ResolvedScope, ScopeClause};
use crate::entity::authorization::{AccessContext, FactValue, ObjectFacts, ResourceTargets};
use crate::entity::organization::{OrgMembership, OrgValidity};
use crate::entity::organization_change::OrganizationState;
use crate::entity::resource_access::{GrantTarget, ResourceKind, SourceKey};
use crate::ports::AccessFactsPort;
pub(super) use crate::repository::authorization::testing::{inquiry_members, mapping};
use crate::service::access_control::resolve::AuthorizedDataScope;
use crate::{Error, Permission, QualifiedPermissions, Result};

/// 身份替身中可直接切换的账号、角色和故障状态。
pub(super) struct IdentityState {
    /// 控制账号当前是否有效。
    pub active: bool,
    /// 按角色保存权限，用于构造同角色或跨角色的资格组合。
    pub grants: Vec<(String, Vec<Permission>)>,
    /// 为功能资格读取注入身份存储故障。
    pub fail: bool,
    /// 没有范围覆盖时，控制是否返回空的默认范围。
    pub denied_scope: bool,
}
/// 提供可调权限、范围及记录授权的内存身份设施。
pub(super) struct Identity {
    /// 功能资格与默认范围读取共用的可变状态。
    pub state: Mutex<IdentityState>,
    /// 返回给授权服务的范围版本，供交付复验测试推进。
    pub revision: AtomicU64,
    /// 记录资格读取次数，以检查提前拒绝或快照内不重复读取。
    pub permissions_calls: AtomicU64,
    /// 供记录授权复验比较的版本。
    pub grant_revision: AtomicU64,
    /// 按资源种类和记录标识保存额外读取授权。
    pub record_grants: Mutex<BTreeSet<(ResourceKind, String)>>,
    /// 按来源键、目标资源和目标标识保存来源授权。
    pub source_grants: Mutex<BTreeSet<(String, ResourceKind, String)>>,
    /// 按入口动作和范围来源覆盖默认解析结果。
    pub scopes: Mutex<BTreeMap<(Permission, Permission), ResolvedScope>>,
    /// 按角色和来源动作配置范围条款，用于验证完整角色资格。
    pub role_scopes: Mutex<BTreeMap<(String, Permission), ScopeClause>>,
    /// 按入口动作注入组织关系的截止时间。
    pub scope_deadlines: Mutex<BTreeMap<Permission, Instant>>,
}
// 默认身份具备完整角色资格，数据范围仅含本人归属。
impl Default for Identity {
    // 建立启用且持有全部已登记权限的账号，记录授权与范围覆盖初始为空。
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
// 从内存状态解析资格、范围和授权记录，不访问外部设施。
#[async_trait]
impl Facilities for Identity {
    // 按角色筛出请求的权限，并区分停用账号与存储故障。
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
    // 按合格角色和来源解析范围，可附带指定的有效期与版本。
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
                    .into_iter()
                    .filter_map(|role| role_scopes.get(&(role.to_string(), source.clone())).cloned())
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
                            ResolvedScope {
                                role_clauses: vec![ScopeClause { self_owned: true, ..Default::default() }],
                                user_limit: None,
                            }
                        }
                    })
            }),
            organizations,
            policy_version: 1,
            scope_version: self.revision.load(Ordering::SeqCst).to_string(),
            as_of: context.as_of,
        })
    }
    // 返回当前记录授权版本，供读取凭证复验。
    async fn grant_revision(&self, _: &AuditActor, _: ResourceKind, _: &mut dyn Executor) -> Result<u64> {
        Ok(self.grant_revision.load(Ordering::SeqCst))
    }
    // 筛出当前资源和请求标识中获额外授权的记录。
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
    // 检查指定来源键是否确实授给了这一目标。
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
    /// 按标识读取的持久化对象样本及其来源关系。
    pub objects: Mutex<Vec<ObjectFacts>>,
    /// 统计事实加载次数，以验证调用顺序或没有访问事实。
    pub calls: AtomicU64,
}
// 提供从指定对象集合初始化事实替身的入口。
impl Facts {
    /// 绑定一批初始事实，读取计数从零开始。
    pub fn new(objects: Vec<ObjectFacts>) -> Self {
        Self { objects: Mutex::new(objects), calls: AtomicU64::new(0) }
    }
}
// 用内存对象模拟按目标加载授权事实。
#[async_trait]
impl AccessFactsPort for Facts {
    // 统计调用次数，只返回标识在请求中的对象。
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

/// 创建仅带身份替身、尚未绑定事实和映射的装配器，用于测试登记。
pub(super) fn empty() -> AuthorizationBuilder {
    AuthorizationBuilder {
        facilities: Arc::new(Identity::default()),
        facts: BTreeMap::new(),
        mappings: BTreeMap::new(),
    }
}
/// 为所有受支持入口登记所需的空事实端口和测试映射。
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
/// 基于完整装配换入指定身份和询价事实，用于询价授权测试。
pub(super) fn service(identity: Arc<Identity>, facts: Arc<Facts>) -> AuthorizationService {
    let mut builder = complete();
    builder.facilities = identity;
    builder.facts.insert("sales_inquiry".into(), facts);
    builder.build().unwrap()
}
/// 返回与身份替身默认范围匹配的测试管理账号。
pub(super) fn actor() -> AuditActor {
    AuditActor::new("user".into(), "login".into(), AccountKind::Admin)
}
/// 构造尚无持久化标识的候选，只提供资源类型和归属。
pub(super) fn candidate(resource: &str, owner: &str) -> ObjectFacts {
    ObjectFacts {
        resource: resource.into(),
        owner_id: FactValue::Known(Some(owner.into())),
        org_unit_id: FactValue::Known(None),
        ..Default::default()
    }
}
/// 为候选补上标识、初始版本及空历史参与人，模拟持久化记录。
pub(super) fn object(resource: &str, id: &str, owner: &str) -> ObjectFacts {
    ObjectFacts {
        id: id.into(),
        version: "v1".into(),
        history_participants: FactValue::Known(BTreeSet::new()),
        ..candidate(resource, owner)
    }
}
