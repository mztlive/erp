//! 工作台消费的授权读取合同；事务写入能力不进入读模型调用链。

use std::collections::BTreeSet;

use application_core::AuditActor;
use erp_core::AccountKind;
use erp_workflow::ports::{
    ObjectFactMap, OrderTaskSource, RolePermissionSnapshotFact, WorkflowAccountFact, WorkflowQueueAccessFact,
};
use erp_workflow::service::approval::approval_participant_permissions_with_executor;
use erp_workflow::service::approval::execution::runtime_service::{
    approval_task_readable_with_executor, approval_tasks_readable_with_executor,
};
use erp_workflow::service::work_item::order_access::filter_order_facts;
use erp_workflow::{Result, WorkItem, WorkflowAuthorizationPort};
use futures_util::future::BoxFuture;
use mongodb::Database;
use persistence_core::Executor;

/// 工作台所需的身份、对象范围与任务资格读取能力。
///
/// 各方法保持原查询顺序、错误分类及执行器；身份事实不替代对象授权，
/// 首拍版本不替代末拍重验。实现不能缓存跨批次的授权结论。
pub trait WorkbenchAuthorizationReadPort: Send + Sync {
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
    fn queue_access_facts<'a>(
        &'a self,
        actor: &'a AuditActor,
        include_version: bool,
        executor: &'a mut dyn Executor,
    ) -> BoxFuture<'a, Result<Option<WorkflowQueueAccessFact>>>;

    /// 获取任务队列身份授权版本；不提供对象授权结论。
    ///
    /// # 参数
    /// * `actor` - 服务端认证身份。
    /// * `executor` - 当前读取执行器。
    /// # 返回
    /// 包含策略、组织和关系有效期变化的版本。
    /// # 错误
    /// 未装配、账号失效或读取失败时拒绝。
    fn queue_scope_version<'a>(
        &'a self,
        actor: &'a AuditActor,
        executor: &'a mut dyn Executor,
    ) -> BoxFuture<'a, Result<String>>;

    /// 返回 `executor` 可见的角色 ID。
    ///
    /// # 参数
    /// * `account_kind` - 账号类型。
    /// * `account_id` - 账号 ID。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 成功时返回该执行器快照中的角色 ID。
    ///
    /// # 错误
    /// 实现无法读取角色时返回错误。
    fn role_ids_with_executor<'a>(
        &'a self,
        account_kind: AccountKind,
        account_id: &'a str,
        executor: &'a mut dyn Executor,
    ) -> BoxFuture<'a, Result<Vec<String>>>;

    /// 返回授予 `account_id` 的权限代码。
    ///
    /// # 参数
    /// * `account_kind` - 账号类型。
    /// * `account_id` - 账号 ID。
    ///
    /// # 返回
    /// 成功时返回权限代码。
    ///
    /// # 错误
    /// 实现无法读取权限时返回错误。
    fn permission_codes<'a>(
        &'a self,
        account_kind: AccountKind,
        account_id: &'a str,
    ) -> BoxFuture<'a, Result<Vec<String>>>;

    /// 解析 work_item:manage，并按当前有效内部组织关系编译任务负责人条件。
    ///
    /// # 参数
    /// * `actor` - 当前操作人。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// None 仅表示显式公司范围；Some(空) 表示无管理范围。
    /// # 错误
    /// 配置、身份或版本错误失败关闭；不得用责任组织字段解释内部部门。
    fn managed_task_owners<'a>(
        &'a self,
        actor: &'a AuditActor,
        executor: &'a mut dyn Executor,
    ) -> BoxFuture<'a, Result<Option<Vec<String>>>>;

    /// 对 Casbin `subject` 执行 `permission_code` 判定。
    ///
    /// # 参数
    /// * `subject` - Casbin 主体。
    /// * `permission_code` - 所需权限代码。
    ///
    /// # 返回
    /// 允许时返回 `true`，拒绝时返回 `false`。
    ///
    /// # 错误
    /// 实现无法完成判定时返回错误。
    fn enforce<'a>(&'a self, subject: &'a str, permission_code: &'a str) -> BoxFuture<'a, Result<bool>>;

    /// 按 ID 加载账号快照。
    ///
    /// # 参数
    /// * `account_id` - 账号 ID。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 找到时返回账号事实；不存在时返回 `None`。
    ///
    /// # 错误
    /// 实现无法读取账号时返回错误。
    fn load_account<'a>(
        &'a self,
        account_id: &'a str,
        executor: &'a mut dyn Executor,
    ) -> BoxFuture<'a, Result<Option<WorkflowAccountFact>>>;

    /// 按 ID 批量加载账号快照。
    ///
    /// # 参数
    /// * `account_ids` - 账号 ID。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 成功时返回已加载的账号事实。
    ///
    /// # 错误
    /// 实现无法读取账号时返回错误。
    fn load_accounts<'a>(
        &'a self,
        account_ids: &'a [String],
        executor: &'a mut dyn Executor,
    ) -> BoxFuture<'a, Result<Vec<WorkflowAccountFact>>>;

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
    fn readable_order_sources<'a>(
        &'a self,
        actor: &'a AuditActor,
        sources: &'a BTreeSet<OrderTaskSource>,
        executor: &'a mut dyn Executor,
    ) -> BoxFuture<'a, Result<BTreeSet<OrderTaskSource>>>;

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
    fn supplier_portal_request_readable<'a>(
        &'a self,
        actor: &'a AuditActor,
        request_id: &'a str,
        executor: &'a mut dyn Executor,
    ) -> BoxFuture<'a, Result<bool>>;

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
    fn supplier_portal_request_reviewable<'a>(
        &'a self,
        actor: &'a AuditActor,
        request_id: &'a str,
        executor: &'a mut dyn Executor,
    ) -> BoxFuture<'a, Result<bool>>;

    /// 返回本地执行器已加载的当前策略修订。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 成功时返回当前修订号。
    ///
    /// # 错误
    /// 实现无法读取修订时返回错误。
    fn current_policy_revision<'a>(&'a self) -> BoxFuture<'a, Result<u64>>;

    /// 在同一执行器修订下冻结 `required` 权限的角色授权。
    ///
    /// # 参数
    /// * `account_kind` - 账号类型。
    /// * `account_id` - 账号 ID。
    /// * `required` - 需要冻结的权限代码。
    ///
    /// # 返回
    /// 成功时返回角色权限快照。
    ///
    /// # 错误
    /// 实现无法冻结授权时返回错误。
    fn role_permission_snapshot<'a>(
        &'a self,
        account_kind: AccountKind,
        account_id: &'a str,
        required: &'a [&'a str],
    ) -> BoxFuture<'a, Result<RolePermissionSnapshotFact>>;

    /// 返回执行器快照中仍然启用的角色 ID。
    ///
    /// # 参数
    /// * `role_ids` - 候选角色 ID。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 成功时返回仍启用的角色 ID。
    ///
    /// # 错误
    /// 实现无法读取角色状态时返回错误。
    fn enabled_role_ids<'a>(
        &'a self,
        role_ids: &'a [String],
        executor: &'a mut dyn Executor,
    ) -> BoxFuture<'a, Result<Vec<String>>>;

    /// 过滤订单与供应商申请事实，保留原授权顺序。
    ///
    /// # 参数
    /// * `actor_id` - 当前账号身份。
    /// * `facts` - 待过滤的批量业务事实。
    /// * `executor` - 调用方原执行器。
    ///
    /// # 返回
    /// 原地保留当前账号可读的事实，分页与计数继续由调用方完成。
    ///
    /// # 错误
    /// 原授权、版本或基础设施错误原样传播，保持失败关闭。
    fn filter_order_facts<'a>(
        &'a self,
        actor_id: &'a str,
        facts: &'a mut ObjectFactMap,
        executor: &'a mut dyn Executor,
    ) -> BoxFuture<'a, Result<()>>;

    /// 批量证明精确审批任务的运行读取资格。
    ///
    /// # 参数
    /// * `db` - 调用方工作流数据库。
    /// * `actor` - 当前真实账号。
    /// * `items` - 原顺序的精确任务。
    /// * `executor` - 调用方原执行器。
    ///
    /// # 返回
    /// 按输入顺序返回资格，不授予对应业务单据的详情权限。
    ///
    /// # 错误
    /// 原授权、版本或基础设施错误原样传播，保持失败关闭。
    fn approval_tasks_readable<'a>(
        &'a self,
        db: &'a Database,
        actor: &'a AuditActor,
        items: &'a [&'a WorkItem],
        executor: &'a mut dyn Executor,
    ) -> BoxFuture<'a, Result<Vec<bool>>>;

    /// 证明单个精确审批任务的运行读取资格。
    ///
    /// # 参数
    /// * `db` - 调用方工作流数据库。
    /// * `actor` - 当前真实账号。
    /// * `item` - 当前精确任务。
    /// * `executor` - 调用方原执行器。
    ///
    /// # 返回
    /// 返回当前任务的读取资格，不扩大业务对象范围。
    ///
    /// # 错误
    /// 原授权、版本或基础设施错误原样传播，保持失败关闭。
    fn approval_task_readable<'a>(
        &'a self,
        db: &'a Database,
        actor: &'a AuditActor,
        item: &'a WorkItem,
        executor: &'a mut dyn Executor,
    ) -> BoxFuture<'a, Result<bool>>;

    /// 证明审批参与者当前有效角色具备原审批权限。
    ///
    /// # 参数
    /// * `actor` - 待验证的审批参与者。
    /// * `executor` - 调用方原执行器。
    ///
    /// # 返回
    /// 返回原审批参与者权限结论，保留同角色和版本校验。
    ///
    /// # 错误
    /// 原授权、版本或基础设施错误原样传播，保持失败关闭。
    fn approval_participant_permissions<'a>(
        &'a self,
        actor: &'a AuditActor,
        executor: &'a mut dyn Executor,
    ) -> BoxFuture<'a, Result<bool>>;
}

// 组合层继续提供已有工作流授权实现；仅这些读取适配方法按实现生成代码，
// 工作台查询和展示状态机通过固定接口调用，原领域授权函数仍是唯一判断来源。
impl<A: WorkflowAuthorizationPort> WorkbenchAuthorizationReadPort for A {
    fn queue_access_facts<'a>(
        &'a self,
        actor: &'a AuditActor,
        include_version: bool,
        executor: &'a mut dyn Executor,
    ) -> BoxFuture<'a, Result<Option<WorkflowQueueAccessFact>>> {
        Box::pin(WorkflowAuthorizationPort::queue_access_facts(self, actor, include_version, executor))
    }

    fn queue_scope_version<'a>(
        &'a self,
        actor: &'a AuditActor,
        executor: &'a mut dyn Executor,
    ) -> BoxFuture<'a, Result<String>> {
        Box::pin(WorkflowAuthorizationPort::queue_scope_version(self, actor, executor))
    }

    fn role_ids_with_executor<'a>(
        &'a self,
        account_kind: AccountKind,
        account_id: &'a str,
        executor: &'a mut dyn Executor,
    ) -> BoxFuture<'a, Result<Vec<String>>> {
        Box::pin(WorkflowAuthorizationPort::role_ids_with_executor(self, account_kind, account_id, executor))
    }

    fn permission_codes<'a>(
        &'a self,
        account_kind: AccountKind,
        account_id: &'a str,
    ) -> BoxFuture<'a, Result<Vec<String>>> {
        Box::pin(WorkflowAuthorizationPort::permission_codes(self, account_kind, account_id))
    }

    fn managed_task_owners<'a>(
        &'a self,
        actor: &'a AuditActor,
        executor: &'a mut dyn Executor,
    ) -> BoxFuture<'a, Result<Option<Vec<String>>>> {
        Box::pin(WorkflowAuthorizationPort::managed_task_owners(self, actor, executor))
    }

    fn enforce<'a>(&'a self, subject: &'a str, permission_code: &'a str) -> BoxFuture<'a, Result<bool>> {
        Box::pin(WorkflowAuthorizationPort::enforce(self, subject, permission_code))
    }

    fn load_account<'a>(
        &'a self,
        account_id: &'a str,
        executor: &'a mut dyn Executor,
    ) -> BoxFuture<'a, Result<Option<WorkflowAccountFact>>> {
        Box::pin(WorkflowAuthorizationPort::load_account(self, account_id, executor))
    }

    fn load_accounts<'a>(
        &'a self,
        account_ids: &'a [String],
        executor: &'a mut dyn Executor,
    ) -> BoxFuture<'a, Result<Vec<WorkflowAccountFact>>> {
        Box::pin(WorkflowAuthorizationPort::load_accounts(self, account_ids, executor))
    }

    fn readable_order_sources<'a>(
        &'a self,
        actor: &'a AuditActor,
        sources: &'a BTreeSet<OrderTaskSource>,
        executor: &'a mut dyn Executor,
    ) -> BoxFuture<'a, Result<BTreeSet<OrderTaskSource>>> {
        Box::pin(WorkflowAuthorizationPort::readable_order_sources(self, actor, sources, executor))
    }

    fn supplier_portal_request_readable<'a>(
        &'a self,
        actor: &'a AuditActor,
        request_id: &'a str,
        executor: &'a mut dyn Executor,
    ) -> BoxFuture<'a, Result<bool>> {
        Box::pin(WorkflowAuthorizationPort::supplier_portal_request_readable(
            self, actor, request_id, executor,
        ))
    }

    fn supplier_portal_request_reviewable<'a>(
        &'a self,
        actor: &'a AuditActor,
        request_id: &'a str,
        executor: &'a mut dyn Executor,
    ) -> BoxFuture<'a, Result<bool>> {
        Box::pin(WorkflowAuthorizationPort::supplier_portal_request_reviewable(
            self, actor, request_id, executor,
        ))
    }

    fn current_policy_revision<'a>(&'a self) -> BoxFuture<'a, Result<u64>> {
        Box::pin(WorkflowAuthorizationPort::current_policy_revision(self))
    }

    fn role_permission_snapshot<'a>(
        &'a self,
        account_kind: AccountKind,
        account_id: &'a str,
        required: &'a [&'a str],
    ) -> BoxFuture<'a, Result<RolePermissionSnapshotFact>> {
        Box::pin(WorkflowAuthorizationPort::role_permission_snapshot(
            self,
            account_kind,
            account_id,
            required,
        ))
    }

    fn enabled_role_ids<'a>(
        &'a self,
        role_ids: &'a [String],
        executor: &'a mut dyn Executor,
    ) -> BoxFuture<'a, Result<Vec<String>>> {
        Box::pin(WorkflowAuthorizationPort::enabled_role_ids(self, role_ids, executor))
    }

    fn filter_order_facts<'a>(
        &'a self,
        actor_id: &'a str,
        facts: &'a mut ObjectFactMap,
        executor: &'a mut dyn Executor,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(filter_order_facts(self, actor_id, facts, executor))
    }

    fn approval_tasks_readable<'a>(
        &'a self,
        db: &'a Database,
        actor: &'a AuditActor,
        items: &'a [&'a WorkItem],
        executor: &'a mut dyn Executor,
    ) -> BoxFuture<'a, Result<Vec<bool>>> {
        Box::pin(approval_tasks_readable_with_executor(db, self, actor, items, executor))
    }

    fn approval_task_readable<'a>(
        &'a self,
        db: &'a Database,
        actor: &'a AuditActor,
        item: &'a WorkItem,
        executor: &'a mut dyn Executor,
    ) -> BoxFuture<'a, Result<bool>> {
        Box::pin(approval_task_readable_with_executor(db, self, actor, item, executor))
    }

    fn approval_participant_permissions<'a>(
        &'a self,
        actor: &'a AuditActor,
        executor: &'a mut dyn Executor,
    ) -> BoxFuture<'a, Result<bool>> {
        Box::pin(approval_participant_permissions_with_executor(self, actor, executor))
    }
}
