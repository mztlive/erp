//! Authorization facts consumed by workflow; adapters live at the composition root.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;

use application_core::AuditActor;
use erp_core::AccountKind;
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

/// 一次工作台只读阶段的身份事实；对象读取权和具体任务资格仍须独立验证。
///
/// `identity_version` 仅在调用方原入口需要首拍队列版本时填入；终拍版本必须重新读取。
/// 管理范围的 `None` 仅代表公共 DataScope 解析器证明的公司范围。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowQueueAccessFact {
    /// 按原权限查询合同返回的权限代码，不以任务责任代替操作权。
    pub permission_codes: Vec<String>,
    /// 当前账号参与的正式单据身份。
    pub participant_document_ids: Vec<String>,
    /// 当前管理范围覆盖的具体负责人，空集合保持失败关闭。
    pub managed_owner_ids: Option<Vec<String>>,
    /// 已启用角色是否证明任务管理动作资格。
    pub can_manage: bool,
    /// 首拍身份授权版本，不是对象或任务版本。
    pub identity_version: Option<String>,
}

/// Closure type for a policy-bound MongoDB write.
pub type WorkflowPolicyWrite<T, E> = Box<
    dyn for<'a> FnOnce(
            &'a mut dyn Executor,
        ) -> Pin<Box<dyn Future<Output = std::result::Result<T, E>> + Send + 'a>>
        + Send,
>;

/// Authorization facts and policy-bound transactions for workflow commands.
pub trait WorkflowAuthorizationPort: Clone + Send + Sync + 'static {
    /// 证明内部账号可读取指定供应商申请的当前对象范围。
    ///
    /// # 参数
    /// * `actor` - 当前内部账号，调用方已在同一执行器验证登录资格。
    /// * `request_id` - 精确供应商申请 ID，不从展示根对象推断。
    /// * `executor` - 调用方读取或业务事务执行器。
    /// # 返回
    /// detail 权限和当前 DataScope 均满足时返回 true；越界返回 false。
    /// # 错误
    /// 未装配、资格配置或基础设施失败时拒绝；具体任务责任不得替代对象授权。
    fn supplier_portal_request_readable(
        &self,
        _actor: &AuditActor,
        _request_id: &str,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<bool>> + Send {
        async { Err(Error::Internal("供应商申请读取范围未装配".into())) }
    }

    /// 在当前申请对象范围内证明真实商品与供给业务确认资格。
    ///
    /// # 参数
    /// * `actor` - 同一执行器已验证为启用内部账号的当前确认人或转交候选。
    /// * `request_id` - 当前精确申请，商品/供给创建或复用资格由组合层读取真实事实判定。
    /// * `executor` - 任务创建、决定、转交或工作台读取使用的当前执行器。
    /// # 返回
    /// 当前所需业务动作权限与责任范围全部满足为 true；资格撤销或越界为 false。
    /// # 错误
    /// 未装配及基础设施错误保持失败关闭；此 Port 不复验已写入业务前的旧版本。
    fn supplier_portal_request_reviewable(
        &self,
        _actor: &AuditActor,
        _request_id: &str,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<bool>> + Send {
        async { Err(Error::Internal("供应商申请业务确认资格未装配".into())) }
    }

    /// 在同一只读阶段复用工作台身份事实，保留原查询及失败顺序。
    ///
    /// # 参数
    /// * `actor` - 服务端认证身份。
    /// * `include_version` - 原入口是否先读取身份授权版本。
    /// * `executor` - 调用方同一事务执行器。
    /// # 返回
    /// 已装配优化器返回身份事实；`None` 要求调用方沿原授权路径读取，不授予访问权。
    /// # 错误
    /// 保留首拍版本、角色、权限、参与单据和管理范围的原首错顺序。
    fn queue_access_facts(
        &self,
        _actor: &AuditActor,
        _include_version: bool,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<Option<WorkflowQueueAccessFact>>> + Send {
        async { Ok(None) }
    }

    /// 解析工作流自身资源动作；缺权限返回空，配置和基础设施错误原样传播。
    fn resolve_workflow_scope(
        &self,
        _actor: &AuditActor,
        _permission: &str,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<Option<WorkflowDataScope>>> + Send {
        async { Err(Error::Internal("工作流范围解析未装配".into())) }
    }

    /// 按真实来源领域证明管理者可读取整个审批主体。
    ///
    /// # 参数
    /// * `actor` - 当前管理者。
    /// * `document_type` / `document_id` - 精确业务主体。
    /// * `executor` - 当前事务执行器。
    /// # 返回
    /// 真实来源全部可读时返回 true；部分财务分摊不等于整单可读。
    /// # 错误
    /// 未装配、来源缺失或授权基础设施失败时拒绝。
    fn approval_source_readable(
        &self,
        _actor: &AuditActor,
        _document_type: DocumentType,
        _document_id: &str,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<bool>> + Send {
        async { Err(Error::Internal("审批来源读取授权未装配".into())) }
    }

    /// 在原提交事务内证明提交者可以读取本次锁定的合同。
    ///
    /// # 参数
    /// * `actor` - 当前提交者，不是审批材料的后续读取人。
    /// * `contract_id` - 本次销售提交锁定的合同身份。
    /// * `executor` - 业务提交沿用的事务执行器。
    /// # 返回
    /// 当前合同详情动作和真实客户来源边界均允许时返回 true。
    /// # 错误
    /// 未装配、授权配置或基础设施失败时拒绝；对象不可见时返回 false。
    /// # 关键业务约束
    /// 调用方仍须证明锁定修订及唯一 PDF 归属；此事实不授予审批人普通合同或文件读取权。
    fn approval_contract_readable(
        &self,
        _actor: &AuditActor,
        _contract_id: &str,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<bool>> + Send {
        async { Err(Error::Internal("审批合同材料读取授权未装配".into())) }
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
                &'a mut dyn Executor,
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
                &'a mut dyn Executor,
            )
                -> Pin<Box<dyn Future<Output = std::result::Result<T, E>> + Send + 'a>>
            + Send
            + 'static,
    {
        async move { Err(E::from(Error::Internal("授权端口未接线".to_string()))) }
    }
}

#[cfg(test)]
mod permission_tests {
    use persistence_core::NoTransaction;

    use super::*;

    #[tokio::test]
    async fn unwired_contract_material_authorization_fails_closed() {
        let actor = AuditActor::new("submitter".into(), "submitter".into(), AccountKind::Admin);
        let result = FailClosedWorkflowAuthorizationPort
            .approval_contract_readable(&actor, "contract-1", &mut NoTransaction)
            .await;
        assert!(matches!(result, Err(Error::Internal(message)) if message == "审批合同材料读取授权未装配"));
    }

    #[tokio::test]
    async fn unwired_supplier_portal_business_authorization_fails_closed() {
        let actor = AuditActor::new("reviewer".into(), "reviewer".into(), AccountKind::Admin);
        let result = FailClosedWorkflowAuthorizationPort
            .supplier_portal_request_reviewable(&actor, "request", &mut NoTransaction)
            .await;
        assert!(matches!(result, Err(Error::Internal(message)) if message == "供应商申请业务确认资格未装配"));
    }

    #[test]
    fn approval_read_and_decide_must_be_granted_by_one_role() {
        let required = ["approval_instance:read", "approval_instance:decide"];
        let split = RolePermissionSnapshotFact::new(
            vec!["reader".into(), "decider".into()],
            HashMap::from([
                ("reader".into(), vec![required[0].into()]),
                ("decider".into(), vec![required[1].into()]),
            ]),
            1,
        );
        assert!(split.granting_role_ids_for_all(&required).is_empty());
        let combined = RolePermissionSnapshotFact::new(
            vec!["approver".into()],
            HashMap::from([("approver".into(), required.map(str::to_owned).to_vec())]),
            1,
        );
        assert_eq!(combined.granting_role_ids_for_all(&required), vec!["approver"]);
        assert!(combined.granting_role_ids("sales_order:detail").is_empty());
        assert!(combined.granting_role_ids("approval_instance:resume").is_empty());
    }
}
