# MongoDB 事务

跨实体或跨集合的业务事务边界由 Service 控制。Repository 的每个方法都接收
`executor: &mut dyn Executor`，由 Service 决定传入事务会话还是 `NoTransaction`；
Repository 不自行启动、提交或回滚事务。`MongoCasbinAdapter` 的业务写接口同样只接收
调用方执行器。涉及 policy 的事务由
`RbacService` 的授权或系统 policy 事务运行器统一持有并提交，确保请求取消后仍能完成提交
结果处理和本地 Enforcer 刷新。该事务还会递增 `casbin_policy_state` 单例文档中的全局 revision；
所有实例写入同一文档，跨实例并发 policy 事务因此产生可见冲突，而不是同时提交错误快照。

管理资源写入通过 Service 的审计事务模板执行。普通 RBAC 管理写使用
`run_authorized_audited_policy_transaction`，把授权快照 revision、业务写入、policy 规则和
审计日志放入同一 session；其他跨审计集合的写入使用 `run_audited_transaction`。两者都在
独立所有权任务中完成事务收尾。

MongoDB 事务要求副本集或支持事务的分片集群；单机 standalone MongoDB 不能执行事务。
事务统一使用 `majority` write concern，确保成功返回的业务写与 policy revision 不会在
副本集主节点切换后回滚并复用旧 revision。

## 基本用法

`Transactional` 为 `mongodb::Client` 提供 `with_transaction`：

```rust
use database::{DatabaseExt, Transactional};

let transaction_db = db.clone();
db.client()
    .with_transaction(move |session| {
        Box::pin(async move {
            transaction_db.accounts().create(&account, session).await?;
            transaction_db.audit_logs().create(&audit_log, session).await?;

            Ok::<(), database::Error>(())
        })
    })
    .await?;
```

闭包返回 `Ok` 后提交；返回 `Err` 时会尽力回滚并保留原始业务错误。回滚自身失败只记录
警告，不覆盖原始错误。`with_transaction` 接受调用方错误类型，只要求该类型实现
`From<database::Error>`。

## Repository 执行器

`database::Executor` 只回答一个问题：本次操作使用哪个 MongoDB 会话。

```rust
pub trait Executor: Send {
    fn session(&mut self) -> Option<&mut ClientSession>;
}
```

已有实现：

- `NoTransaction`：返回 `None`，按自动提交语义执行。
- `ClientSession`：返回自身，因此 `with_transaction` 闭包参数可以直接传给 Repository。
- `&mut E`：透传内层执行器，支持在方法之间逐层重借用。

同一个仓储方法既可用于事务内也可用于事务外，不再需要成对的 `_with_session` 方法。
驱动层“带会话 / 不带会话”两套调用形态（包括 `find` 返回 `SessionCursor` 的差异）
收敛在 `database::mongo_ops`。

先删后写、读后写等多步骤方法（如 `replace_subject_roles`、`replace_role_permissions`、
`remove_role`、`ensure_roles_assignable`）本身不构成原子边界，必须收到事务执行器；
方法注释中已注明该约束。

## 乐观锁与软删除

`update`、`soft_delete` 和 `restore` 共享相同的比较并交换规则：

- 更新和软删除只匹配未删除实体的当前 `id + version`。
- 恢复只匹配已删除实体的当前 `id + version`。
- 成功写入会递增版本并更新内存实体的持久化元数据。
- 版本或删除状态不匹配时返回 `OptimisticLockingError`。

因此，事务不能替代实体级并发控制；调用方仍需把冲突作为可见失败处理。

角色引用使用相同协议：分配角色与仅更新角色权限时，事务内重读角色并通过
`update` 递增版本；角色删除也通过同一 `id + version` CAS 写入。多实例并发
绑定、权限更新或删除同一角色时，MongoDB 因文档写冲突中止其中一个事务，避免遗留指向
已删除角色的 `g`/`p` 规则。角色版本与更新时间因此也表示最近一次引用或权限变更。

## Casbin policy revision

policy rule 与全局 revision 必须在同一个 MongoDB 事务中提交。授权读取先比较数据库
revision 与本地 Enforcer 已加载版本；版本变化时，只有在 reload 前后 revision 保持一致
才发布新快照。事务的 `majority` write concern 保证 revision 单调持久，避免主节点切换后
回退并复用旧版本。数据库不可读、reload 失败或 policy 持续变化时均失败关闭，不使用旧缓存。

禁止直接修改 `casbin_rules`，也禁止在 `RbacService` 的 policy 事务运行器之外调用
Adapter 的 policy 写方法。否则 revision 不会变化，其他实例无法感知该写入。Casbin
`Adapter` trait 的独立写入口会自行开启事务，并在规则确有变化时原子递增 revision。

普通角色和账号管理必须使用授权版本运行器：事务内以 CAS 比较授权检查捕获的 revision，
然后递增版本。即使本次只修改账号字段而没有改变 policy，也必须触碰 revision；这样检查
目标权限与最终写入之间发生的并发 policy 变化会使整个业务事务回滚。

## 提交结果未知

MongoDB 为提交错误标记 `UnknownTransactionCommitResult` 时，实现在同一个 session 上重试
`commit_transaction`，最长 120 秒。超过期限仍无法确认时返回
`database::Error::CommitOutcomeUnknown`。

这个错误表示事务可能已经提交，也可能没有提交。调用方不得把它当作确定回滚并立即重放
整个业务事务，否则可能造成重复写入。Service 将其单独映射为
`services::Error::OutcomeUnknown`；Web API 稳定返回 `500` 与非敏感提示“操作结果暂无法确认，
请查询当前状态后再决定是否重试”，不会把驱动错误暴露给客户端。

涉及 Casbin policy 的 Service 还会把当前进程标记为一致性未知，停止授权读取和后续 policy
写入；单次 reload 不会解除这个状态。policy 事务运行在独立所有权任务中，因此 HTTP 超时
或客户端断开不会取消已经开始的提交重试与状态收尾。

标记为 `TransientTransactionError` 的并发写冲突返回
`TransientTransactionConflict`，Web API 映射为 `409 Conflict`。其他提交错误作为确定
失败返回 `DatabaseError`。当前实现不会自动重跑整个事务闭包；需要业务级重试时，必须先
设计幂等键或去重语义。

## 使用原则

- 单集合且不要求跨步骤原子性的 CRUD 不使用事务，传入 `&mut NoTransaction`。
- 多集合写入、角色实体与 policy 同步、账号与 Profile/角色绑定同步时使用事务。
- 在事务闭包内完成依赖事务快照的存在性和状态校验。
- 保持事务短小，不在事务内执行外部 HTTP、文件 I/O 或 CPU 密集工作。
