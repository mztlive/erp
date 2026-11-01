//! 工作台库单元测试授权替身；只运行生产编排，不发起数据库或 HTTP 请求。

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use application_core::AuditActor;
use erp_core::AccountKind;
use erp_workflow::ports::{RolePermissionSnapshotFact, WorkflowAccountFact, WorkflowQueueAccessFact};
use erp_workflow::{Error, Result, WorkflowAuthorizationPort};
use persistence_core::Executor;

/// 共享观测轨迹与可变 policy revision，验证只读阶段缓存的命中和重建。
#[derive(Clone, Default)]
pub(super) struct TestAuth {
    pub(super) trace: Arc<Mutex<Vec<String>>>,
    pub(super) revision: Arc<AtomicU64>,
    pub(super) fail_revision: Arc<AtomicBool>,
    pub(super) queue: Arc<Mutex<Option<WorkflowQueueAccessFact>>>,
    pub(super) fail_queue: Arc<AtomicBool>,
    pub(super) portal_readable: Arc<Mutex<HashMap<String, bool>>>,
    pub(super) portal_reviewable: Arc<Mutex<HashMap<String, bool>>>,
    pub(super) fail_portal_reviewable: Arc<AtomicBool>,
}

#[allow(clippy::manual_async_fn)]
impl WorkflowAuthorizationPort for TestAuth {
    /// 记录精确申请范围读取，并返回当前可配置资格。
    ///
    /// # 参数
    /// * `actor` - 当前内部账号；轨迹记为 `portal:` 加账号 ID 与 `request_id`。
    /// * `request_id` - 精确申请 ID，用作 `portal_readable` 的查找键。
    /// * `_executor` - 未使用。
    ///
    /// # 返回
    /// 命中预设资格时返回该布尔值；未配置时返回 `false`。
    ///
    /// # 错误
    /// 没有失败路径，始终返回 `Ok`。
    ///
    /// # Panics
    /// 观测 `Mutex` 中毒时 `unwrap` 会 panic；这是测试替身的进程内不变量，不是授权失败。
    async fn supplier_portal_request_readable(
        &self,
        actor: &AuditActor,
        request_id: &str,
        _executor: &mut dyn Executor,
    ) -> Result<bool> {
        self.trace.lock().unwrap().push(format!("portal:{}:{request_id}", actor.id()));
        Ok(self.portal_readable.lock().unwrap().get(request_id).copied().unwrap_or(false))
    }
    /// 记录当前申请业务动作资格，明确区分越界与基础设施失败。
    ///
    /// # 参数
    /// * `actor` - 当前确认人；轨迹记为 `portal-review:` 加账号 ID 与 `request_id`。
    /// * `request_id` - 精确申请 ID，用作 `portal_reviewable` 的查找键。
    /// * `_executor` - 未使用。
    ///
    /// # 返回
    /// 未注入失败时，命中预设资格返回该布尔值；未配置时返回 `false`，表示越界而不是错误。
    ///
    /// # 错误
    /// `fail_portal_reviewable` 为真时，仍先写入轨迹，再返回 `Error::Rbac`，文案为 `portal business authorization failed`。
    ///
    /// # Panics
    /// 观测 `Mutex` 中毒时 `unwrap` 会 panic；这是测试替身的进程内不变量，不是授权失败。
    async fn supplier_portal_request_reviewable(
        &self,
        actor: &AuditActor,
        request_id: &str,
        _executor: &mut dyn Executor,
    ) -> Result<bool> {
        self.trace.lock().unwrap().push(format!("portal-review:{}:{request_id}", actor.id()));
        if self.fail_portal_reviewable.load(Ordering::SeqCst) {
            return Err(Error::Rbac("portal business authorization failed".into()));
        }
        Ok(self.portal_reviewable.lock().unwrap().get(request_id).copied().unwrap_or(false))
    }
    /// 返回可配置的队列身份事实，并验证原调用人的身份透传。
    ///
    /// # 参数
    /// * `actor` - 原调用人；轨迹记为 `queue:` 加账号 ID 与 `include_version`。
    /// * `include_version` - 原入口是否要求身份版本；只写入轨迹，不改变返回形状。
    /// * `_executor` - 未使用。
    ///
    /// # 返回
    /// 返回 `queue` 中预设的 `WorkflowQueueAccessFact`；未配置时为 `None`，不授予访问权。
    ///
    /// # 错误
    /// `fail_queue` 为真时，仍先写入轨迹，再返回 `Error::Rbac`，文案为 `queue failed`。
    ///
    /// # Panics
    /// 观测 `Mutex` 中毒时 `unwrap` 会 panic；这是测试替身的进程内不变量，不是授权失败。
    async fn queue_access_facts(
        &self,
        actor: &AuditActor,
        include_version: bool,
        _executor: &mut dyn Executor,
    ) -> Result<Option<WorkflowQueueAccessFact>> {
        self.trace.lock().unwrap().push(format!("queue:{}:{include_version}", actor.id()));
        if self.fail_queue.load(Ordering::SeqCst) {
            return Err(Error::Rbac("queue failed".into()));
        }
        Ok(self.queue.lock().unwrap().clone())
    }

    /// 未使用授权方法保持失败关闭，替身不连接真实基础设施。
    ///
    /// # 参数
    /// * `_account_kind` - 未使用。
    /// * `_account_id` - 未使用。
    ///
    /// # 返回
    /// 不返回角色 ID。
    ///
    /// # 错误
    /// 总是返回 `Error::Internal`，文案为 `unexpected authorization call`。
    fn role_ids(
        &self,
        _account_kind: AccountKind,
        _account_id: &str,
    ) -> impl Future<Output = Result<Vec<String>>> + Send {
        async move { Err(Error::Internal("unexpected authorization call".into())) }
    }

    /// 未使用授权方法保持失败关闭，替身不连接真实基础设施。
    ///
    /// # 参数
    /// * `_account_kind` - 未使用。
    /// * `_account_id` - 未使用。
    /// * `_executor` - 未使用。
    ///
    /// # 返回
    /// 不返回角色 ID。
    ///
    /// # 错误
    /// 总是返回 `Error::Internal`，文案为 `unexpected authorization call`。
    fn role_ids_with_executor(
        &self,
        _account_kind: AccountKind,
        _account_id: &str,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<Vec<String>>> + Send {
        async move { Err(Error::Internal("unexpected authorization call".into())) }
    }

    /// 未使用授权方法保持失败关闭，替身不连接真实基础设施。
    ///
    /// # 参数
    /// * `_account_kind` - 未使用。
    /// * `_account_id` - 未使用。
    ///
    /// # 返回
    /// 不返回权限代码。
    ///
    /// # 错误
    /// 总是返回 `Error::Internal`，文案为 `unexpected authorization call`。
    fn permission_codes(
        &self,
        _account_kind: AccountKind,
        _account_id: &str,
    ) -> impl Future<Output = Result<Vec<String>>> + Send {
        async move { Err(Error::Internal("unexpected authorization call".into())) }
    }

    /// 未使用授权方法保持失败关闭，替身不连接真实基础设施。
    ///
    /// # 参数
    /// * `_subject` - 未使用。
    /// * `_permission_code` - 未使用。
    ///
    /// # 返回
    /// 不返回允许或拒绝。
    ///
    /// # 错误
    /// 总是返回 `Error::Internal`，文案为 `unexpected authorization call`。
    fn enforce(&self, _subject: &str, _permission_code: &str) -> impl Future<Output = Result<bool>> + Send {
        async move { Err(Error::Internal("unexpected authorization call".into())) }
    }

    /// 未使用授权方法保持失败关闭，替身不连接真实基础设施。
    ///
    /// # 参数
    /// * `_owned` - 未使用。
    /// * `_required` - 未使用。
    ///
    /// # 返回
    /// 不返回覆盖结论。
    ///
    /// # 错误
    /// 总是返回 `Error::Internal`，文案为 `unexpected authorization call`。
    fn permissions_cover(&self, _owned: &[String], _required: &[&str]) -> Result<bool> {
        Err(Error::Internal("unexpected authorization call".into()))
    }

    /// 未使用授权方法保持失败关闭，替身不连接真实基础设施。
    ///
    /// # 参数
    /// * `_role_ids` - 未使用。
    /// * `_permission_code` - 未使用。
    ///
    /// # 返回
    /// 不返回命中的角色 ID。
    ///
    /// # 错误
    /// 总是返回 `Error::Internal`，文案为 `unexpected authorization call`。
    fn roles_granting_permission(
        &self,
        _role_ids: &[String],
        _permission_code: &str,
    ) -> impl Future<Output = Result<Vec<String>>> + Send {
        async move { Err(Error::Internal("unexpected authorization call".into())) }
    }

    /// 返回替身配置的事实并记录实际生产调用。
    ///
    /// # 参数
    /// * `_kind` - 未使用。
    /// * `account` - 账号标识；轨迹记为 `snapshot:` 加该账号。
    /// * `required` - 需要冻结的权限代码；全部记入角色 `executor` 的授予列表。
    ///
    /// # 返回
    /// 返回角色仅为 `executor`、授予等于 `required`、修订号为当前 `revision` 的 `RolePermissionSnapshotFact`。
    ///
    /// # 错误
    /// 没有失败路径，始终返回 `Ok`。
    ///
    /// # Panics
    /// 观测 `Mutex` 中毒时 `unwrap` 会 panic；这是测试替身的进程内不变量，不是授权失败。
    async fn role_permission_snapshot(
        &self,
        _kind: AccountKind,
        account: &str,
        required: &[&str],
    ) -> Result<RolePermissionSnapshotFact> {
        self.trace.lock().unwrap().push(format!("snapshot:{account}"));
        Ok(RolePermissionSnapshotFact::new(
            vec!["executor".into()],
            HashMap::from([("executor".into(), required.iter().map(|code| (*code).to_string()).collect())]),
            self.revision.load(Ordering::SeqCst),
        ))
    }

    /// 返回替身配置的事实并记录实际生产调用。
    ///
    /// # 参数
    /// * `roles` - 候选角色 ID；原样回传，不筛掉停用角色。
    /// * `_executor` - 未使用。
    ///
    /// # 返回
    /// 返回 `roles` 的克隆，并在轨迹写入 `enabled_roles`。
    ///
    /// # 错误
    /// 没有失败路径，始终返回 `Ok`。
    ///
    /// # Panics
    /// 观测 `Mutex` 中毒时 `unwrap` 会 panic；这是测试替身的进程内不变量，不是授权失败。
    async fn enabled_role_ids(&self, roles: &[String], _executor: &mut dyn Executor) -> Result<Vec<String>> {
        self.trace.lock().unwrap().push("enabled_roles".into());
        Ok(roles.to_vec())
    }

    /// 未使用授权方法保持失败关闭，替身不连接真实基础设施。
    ///
    /// # 参数
    /// * `_expected_revision` - 未使用；不核对修订。
    /// * `_executor` - 未使用。
    ///
    /// # 返回
    /// 不确认修订一致。
    ///
    /// # 错误
    /// 总是返回 `Error::Internal`，文案为 `unexpected authorization call`。
    fn ensure_policy_snapshot_with_executor(
        &self,
        _expected_revision: u64,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<()>> + Send {
        async move { Err(Error::Internal("unexpected authorization call".into())) }
    }

    /// 返回替身配置的事实并记录实际生产调用。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 未注入失败时返回当前 `revision`。
    ///
    /// # 错误
    /// `fail_revision` 为真时，仍先写入 `revision` 轨迹，再返回 `Error::Rbac`，文案为 `revision failed`。
    ///
    /// # Panics
    /// 观测 `Mutex` 中毒时 `unwrap` 会 panic；这是测试替身的进程内不变量，不是授权失败。
    async fn current_policy_revision(&self) -> Result<u64> {
        self.trace.lock().unwrap().push("revision".into());
        if self.fail_revision.load(Ordering::SeqCst) {
            return Err(Error::Rbac("revision failed".into()));
        }
        Ok(self.revision.load(Ordering::SeqCst))
    }

    /// 未使用授权方法保持失败关闭，替身不连接真实基础设施。
    ///
    /// # 参数
    /// * `_executor` - 未使用。
    ///
    /// # 返回
    /// 不返回修订号。
    ///
    /// # 错误
    /// 总是返回 `Error::Internal`，文案为 `unexpected authorization call`。
    fn policy_revision_with_executor(
        &self,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<u64>> + Send {
        async move { Err(Error::Internal("unexpected authorization call".into())) }
    }

    /// 未使用授权方法保持失败关闭，替身不连接真实基础设施。
    ///
    /// # 参数
    /// * `_account_id` - 未使用。
    /// * `_executor` - 未使用。
    ///
    /// # 返回
    /// 不返回账号事实，也不以 `None` 表示不存在。
    ///
    /// # 错误
    /// 总是返回 `Error::Internal`，文案为 `unexpected authorization call`。
    fn load_account(
        &self,
        _account_id: &str,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<Option<WorkflowAccountFact>>> + Send {
        async move { Err(Error::Internal("unexpected authorization call".into())) }
    }

    /// 未使用授权方法保持失败关闭，替身不连接真实基础设施。
    ///
    /// # 参数
    /// * `_account_ids` - 未使用。
    /// * `_executor` - 未使用。
    ///
    /// # 返回
    /// 不返回账号事实。
    ///
    /// # 错误
    /// 总是返回 `Error::Internal`，文案为 `unexpected authorization call`。
    fn load_accounts(
        &self,
        _account_ids: &[String],
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<Vec<WorkflowAccountFact>>> + Send {
        async move { Err(Error::Internal("unexpected authorization call".into())) }
    }

    /// 未使用授权方法保持失败关闭，替身不连接真实基础设施。
    ///
    /// # 参数
    /// * `_account_kind` - 未使用。
    /// * `_executor` - 未使用。
    ///
    /// # 返回
    /// 不返回该类型的账号事实。
    ///
    /// # 错误
    /// 总是返回 `Error::Internal`，文案为 `unexpected authorization call`。
    fn list_accounts_by_kind(
        &self,
        _account_kind: AccountKind,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<Vec<WorkflowAccountFact>>> + Send {
        async move { Err(Error::Internal("unexpected authorization call".into())) }
    }

    /// 未使用授权方法保持失败关闭，替身不连接真实基础设施。
    ///
    /// # 参数
    /// * `_search` - 未使用。
    /// * `_limit` - 未使用。
    /// * `_executor` - 未使用。
    ///
    /// # 返回
    /// 不返回审批候选人。
    ///
    /// # 错误
    /// 总是返回 `Error::Internal`，文案为 `unexpected authorization call`。
    fn list_active_approval_candidates(
        &self,
        _search: Option<&str>,
        _limit: u32,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<Vec<WorkflowAccountFact>>> + Send {
        async move { Err(Error::Internal("unexpected authorization call".into())) }
    }

    /// 未使用授权方法保持失败关闭，替身不连接真实基础设施。
    ///
    /// # 参数
    /// * `_policy_revision` - 未使用；不核对修订。
    /// * `_transaction` - 未使用；闭包不会执行。
    ///
    /// # 返回
    /// 不返回事务结果 `T`。
    ///
    /// # 错误
    /// 总是返回由 `Error::Internal` 转入的 `E`，文案为 `授权端口未接线`。
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
