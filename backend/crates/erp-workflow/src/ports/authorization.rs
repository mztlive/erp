//! Authorization facts consumed by workflow; adapters live at the composition root.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;

use application_core::AuditActor;
use erp_core::AccountKind;
use mongodb::ClientSession;
use persistence_core::Executor;

use super::object_facts::OrderTaskSource;
use super::{WorkflowDataScope, WorkflowScopeObject, WorkflowScopeObjects};
use crate::entity::document_registry::DocumentType;
pub use crate::entity::work_item::WorkflowAccountFact;
use crate::error::{Error, Result};

/// Frozen role-to-permission grants captured under one enforcer revision.
#[derive(Debug, Clone)]
pub struct RolePermissionSnapshotFact {
    role_ids: Vec<String>,
    grants: HashMap<String, Vec<String>>,
    policy_revision: u64,
}

impl RolePermissionSnapshotFact {
    /// Construct a frozen snapshot.
    ///
    /// # Parameters
    /// * `role_ids` - account role ids in grant order
    /// * `grants` - permission codes granted by each role
    /// * `policy_revision` - enforcer revision used to compute grants
    ///
    /// # Returns
    /// Snapshot that does not expose Role or Permission aggregates.
    pub fn new(role_ids: Vec<String>, grants: HashMap<String, Vec<String>>, policy_revision: u64) -> Self {
        Self { role_ids, grants, policy_revision }
    }

    /// Role ids bound to the account under this revision.
    pub fn role_ids(&self) -> &[String] {
        &self.role_ids
    }

    /// Role ids that grant `permission_code`, preserving frozen order.
    pub fn granting_role_ids(&self, permission_code: &str) -> Vec<String> {
        self.role_ids
            .iter()
            .filter(|role_id| {
                self.grants
                    .get(*role_id)
                    .is_some_and(|codes| codes.iter().any(|code| permission_covers(code, permission_code)))
            })
            .cloned()
            .collect()
    }

    /// Role ids that grant every required permission inside the same role.
    pub fn granting_role_ids_for_all(&self, permissions: &[&str]) -> Vec<String> {
        if permissions.is_empty() {
            return Vec::new();
        }
        self.role_ids
            .iter()
            .filter(|role_id| {
                self.grants.get(*role_id).is_some_and(|codes| {
                    permissions
                        .iter()
                        .all(|required| codes.iter().any(|code| permission_covers(code, required)))
                })
            })
            .cloned()
            .collect()
    }

    /// Enforcer revision used to freeze this snapshot.
    pub fn policy_revision(&self) -> u64 {
        self.policy_revision
    }
}

/// Casbin-equivalent `resource:action` covering, including `*` wildcards.
pub fn permission_covers(owned: &str, required: &str) -> bool {
    let Some((owned_resource, owned_action)) = split_permission(owned) else {
        return false;
    };
    let Some((required_resource, required_action)) = split_permission(required) else {
        return false;
    };
    (owned_resource == "*" || owned_resource == required_resource)
        && (owned_action == "*" || owned_action == required_action)
}

fn split_permission(code: &str) -> Option<(&str, &str)> {
    let normalized = code.trim();
    let (resource, action) = normalized.split_once(':')?;
    if action.contains(':') || resource.is_empty() || action.is_empty() {
        return None;
    }
    Some((resource, action))
}

/// Closure type for a policy-bound MongoDB write.
pub type WorkflowPolicyWrite<T, E> = Box<
    dyn for<'a> FnOnce(
            &'a mut ClientSession,
        ) -> Pin<Box<dyn Future<Output = std::result::Result<T, E>> + Send + 'a>>
        + Send,
>;

/// Authorization facts and policy-bound transactions for workflow commands.
pub trait WorkflowAuthorizationPort: Clone + Send + Sync + 'static {
    /// 解析工作流自身资源动作；缺权限返回空，配置和基础设施错误原样传播。
    fn resolve_workflow_scope(
        &self,
        _actor: &AuditActor,
        _permission: &str,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<Option<WorkflowDataScope>>> + Send {
        async { Err(Error::Internal("工作流范围解析未装配".into())) }
    }

    /// 绑定前按当前或拟创建订单事实独立核对详情范围；未装配时失败关闭。
    fn binding_order_readable(
        &self,
        _actor: &AuditActor,
        _object: &WorkflowScopeObject,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<bool>> + Send {
        async { Err(Error::Internal("订单绑定读取范围未装配".into())) }
    }

    /// 有界批量读取当前审批对象事实；缺失对象不进入结果，配置失败不得吞掉。
    fn approval_scope_objects(
        &self,
        _keys: &HashSet<(DocumentType, String)>,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<WorkflowScopeObjects>> + Send {
        async { Err(Error::Internal("审批批量范围事实未装配".into())) }
    }

    /// 从当前强业务实体映射审批范围维度，不使用历史责任字段推断部门。
    fn approval_scope_object(
        &self,
        _document_type: DocumentType,
        _document_id: &str,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<WorkflowScopeObject>> + Send {
        async { Err(Error::Internal("审批范围事实未装配".into())) }
    }

    /// 解析 work_item:manage，并按当前有效内部组织关系编译任务负责人条件。
    ///
    /// # 返回
    /// None 仅表示显式公司范围；Some(空) 表示无管理范围。
    /// # 错误
    /// 配置、身份或版本错误失败关闭；不得用责任组织字段解释内部部门。
    fn managed_task_owners(
        &self,
        _actor: &AuditActor,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<Option<Vec<String>>>> + Send {
        async { Err(Error::Internal("任务管理范围未装配".into())) }
    }

    /// 获取任务队列身份授权版本；不提供对象授权结论。
    ///
    /// # 参数
    /// * `actor` - 服务端认证身份。
    /// * `executor` - 当前读取执行器。
    /// # 返回
    /// 包含策略、组织和关系有效期变化的版本。
    /// # 错误
    /// 未装配、账号失效或读取失败时拒绝。
    fn queue_scope_version(
        &self,
        _actor: &AuditActor,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<String>> + Send {
        async { Err(Error::Internal("任务队列范围版本未装配".into())) }
    }

    /// 在审批原事务内重验 S2 订单对象当前详情权限。
    ///
    /// # 参数
    /// * `actor` - 当前审批人。
    /// * `document_type` - 审批快照固定的订单类型。
    /// * `document_id` - 业务单据主键，变更单由 adapter 沿原单解析。
    /// * `executor` - 审批决定、恢复或读取的执行器。
    /// # 返回
    /// 当前对象可读时为 true；业务越界返回 false，供审批引擎记录受阻。
    /// # 错误
    /// 未装配和基础设施失败不得转换成成功。
    fn order_approval_readable(
        &self,
        _actor: &AuditActor,
        _document_type: DocumentType,
        _document_id: &str,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<bool>> + Send {
        async { Err(Error::Internal("订单审批范围授权未装配".into())) }
    }

    /// 批量返回当前详情范围允许的关联订单，用于任务列表、统计及详情。
    ///
    /// # 参数
    /// * `actor` - 服务端账号身份。
    /// * `sources` - 从业务实体外键得到的去重订单来源。
    /// * `executor` - 调用方执行器。
    /// # 返回
    /// 返回已授权来源的子集；空授权必须保持空集。
    /// # 错误
    /// 未装配、未知范围版本及基础设施错误失败关闭。
    fn readable_order_sources(
        &self,
        _actor: &AuditActor,
        _sources: &BTreeSet<OrderTaskSource>,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<BTreeSet<OrderTaskSource>>> + Send {
        async { Err(Error::Internal("任务对象范围授权未装配".into())) }
    }

    /// 在任务命令原事务内独立重验关联业务对象的详情范围。
    ///
    /// # 参数
    /// * `actor` - 当前操作人或待接收任务的有效账号。
    /// * `source` - 事务内业务实体证明的订单来源。
    /// * `executor` - 原任务事务执行器。
    /// # 返回
    /// 对象当前可读时成功；任务责任不替代对象授权。
    /// # 错误
    /// 未装配、未接入、来源缺失或范围越界时失败关闭。
    fn require_order_task_read(
        &self,
        _actor: &AuditActor,
        _source: &OrderTaskSource,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<()>> + Send {
        async { Err(Error::Internal("任务对象范围授权未装配".into())) }
    }

    /// Return role ids granted to `account_id`.
    fn role_ids(
        &self,
        account_kind: AccountKind,
        account_id: &str,
    ) -> impl Future<Output = Result<Vec<String>>> + Send;

    /// Return role ids visible to `executor`.
    fn role_ids_with_executor(
        &self,
        account_kind: AccountKind,
        account_id: &str,
        executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<Vec<String>>> + Send;

    /// Return permission codes granted to `account_id`.
    fn permission_codes(
        &self,
        account_kind: AccountKind,
        account_id: &str,
    ) -> impl Future<Output = Result<Vec<String>>> + Send;

    /// Enforce `permission_code` for Casbin `subject`.
    fn enforce(&self, subject: &str, permission_code: &str) -> impl Future<Output = Result<bool>> + Send;

    /// Return whether `owned` permission codes cover every `required` code.
    fn permissions_cover(&self, owned: &[String], required: &[&str]) -> Result<bool>;

    /// Role ids among `role_ids` that grant `permission_code`.
    fn roles_granting_permission(
        &self,
        role_ids: &[String],
        permission_code: &str,
    ) -> impl Future<Output = Result<Vec<String>>> + Send;

    /// Freeze role grants for `required` permission codes under one enforcer revision.
    fn role_permission_snapshot(
        &self,
        account_kind: AccountKind,
        account_id: &str,
        required: &[&str],
    ) -> impl Future<Output = Result<RolePermissionSnapshotFact>> + Send;

    /// Role ids that are still enabled in the executor snapshot.
    fn enabled_role_ids(
        &self,
        role_ids: &[String],
        executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<Vec<String>>> + Send;

    /// Prove the frozen enforcer revision matches the executor snapshot.
    fn ensure_policy_snapshot_with_executor(
        &self,
        expected_revision: u64,
        executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<()>> + Send;

    /// Current policy revision loaded by the local enforcer.
    fn current_policy_revision(&self) -> impl Future<Output = Result<u64>> + Send;

    /// Current policy revision visible to `executor`.
    fn policy_revision_with_executor(
        &self,
        executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<u64>> + Send;

    /// Load an account snapshot by id.
    fn load_account(
        &self,
        account_id: &str,
        executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<Option<WorkflowAccountFact>>> + Send;

    /// Load account snapshots by id.
    fn load_accounts(
        &self,
        account_ids: &[String],
        executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<Vec<WorkflowAccountFact>>> + Send;

    /// List accounts of one kind for candidate pickers.
    fn list_accounts_by_kind(
        &self,
        account_kind: AccountKind,
        executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<Vec<WorkflowAccountFact>>> + Send;

    /// List active backoffice accounts that may be chosen as definition assignees.
    fn list_active_approval_candidates(
        &self,
        search: Option<&str>,
        limit: u32,
        executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<Vec<WorkflowAccountFact>>> + Send;

    /// Run a write transaction bound to the caller's policy revision.
    fn run_authorized_policy_transaction<T, E, F>(
        &self,
        policy_revision: u64,
        transaction: F,
    ) -> impl Future<Output = std::result::Result<T, E>> + Send
    where
        T: Send + 'static,
        E: From<Error>
            + From<persistence_core::Error>
            + From<application_core::Error>
            + std::error::Error
            + std::fmt::Display
            + Send
            + 'static,
        F: for<'a> FnOnce(
                &'a mut ClientSession,
            )
                -> Pin<Box<dyn Future<Output = std::result::Result<T, E>> + Send + 'a>>
            + Send
            + 'static;
}

/// Fail-closed authorization port used when composition has not injected an adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedWorkflowAuthorizationPort;

fn unwired_auth<T>() -> Result<T> {
    Err(Error::Internal("授权端口未接线".to_string()))
}

#[allow(clippy::manual_async_fn)]
impl WorkflowAuthorizationPort for FailClosedWorkflowAuthorizationPort {
    fn role_ids(
        &self,
        _account_kind: AccountKind,
        _account_id: &str,
    ) -> impl Future<Output = Result<Vec<String>>> + Send {
        async move { unwired_auth() }
    }

    fn role_ids_with_executor(
        &self,
        _account_kind: AccountKind,
        _account_id: &str,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<Vec<String>>> + Send {
        async move { unwired_auth() }
    }

    fn permission_codes(
        &self,
        _account_kind: AccountKind,
        _account_id: &str,
    ) -> impl Future<Output = Result<Vec<String>>> + Send {
        async move { unwired_auth() }
    }

    fn enforce(&self, _subject: &str, _permission_code: &str) -> impl Future<Output = Result<bool>> + Send {
        async move { unwired_auth() }
    }

    fn permissions_cover(&self, _owned: &[String], _required: &[&str]) -> Result<bool> {
        unwired_auth()
    }

    fn roles_granting_permission(
        &self,
        _role_ids: &[String],
        _permission_code: &str,
    ) -> impl Future<Output = Result<Vec<String>>> + Send {
        async move { unwired_auth() }
    }

    fn role_permission_snapshot(
        &self,
        _account_kind: AccountKind,
        _account_id: &str,
        _required: &[&str],
    ) -> impl Future<Output = Result<RolePermissionSnapshotFact>> + Send {
        async move { unwired_auth() }
    }

    fn enabled_role_ids(
        &self,
        _role_ids: &[String],
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<Vec<String>>> + Send {
        async move { unwired_auth() }
    }

    fn ensure_policy_snapshot_with_executor(
        &self,
        _expected_revision: u64,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<()>> + Send {
        async move { unwired_auth() }
    }

    fn current_policy_revision(&self) -> impl Future<Output = Result<u64>> + Send {
        async move { unwired_auth() }
    }

    fn policy_revision_with_executor(
        &self,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<u64>> + Send {
        async move { unwired_auth() }
    }

    fn load_account(
        &self,
        _account_id: &str,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<Option<WorkflowAccountFact>>> + Send {
        async move { unwired_auth() }
    }

    fn load_accounts(
        &self,
        _account_ids: &[String],
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<Vec<WorkflowAccountFact>>> + Send {
        async move { unwired_auth() }
    }

    fn list_accounts_by_kind(
        &self,
        _account_kind: AccountKind,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<Vec<WorkflowAccountFact>>> + Send {
        async move { unwired_auth() }
    }

    fn list_active_approval_candidates(
        &self,
        _search: Option<&str>,
        _limit: u32,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<Vec<WorkflowAccountFact>>> + Send {
        async move { unwired_auth() }
    }

    fn run_authorized_policy_transaction<T, E, F>(
        &self,
        _policy_revision: u64,
        _transaction: F,
    ) -> impl Future<Output = std::result::Result<T, E>> + Send
    where
        T: Send + 'static,
        E: From<Error>
            + From<persistence_core::Error>
            + From<application_core::Error>
            + std::error::Error
            + std::fmt::Display
            + Send
            + 'static,
        F: for<'a> FnOnce(
                &'a mut ClientSession,
            )
                -> Pin<Box<dyn Future<Output = std::result::Result<T, E>> + Send + 'a>>
            + Send
            + 'static,
    {
        async move { Err(E::from(Error::Internal("授权端口未接线".to_string()))) }
    }
}
