# 管理写操作审计

管理端资源写入遵循“业务数据与成功审计同时提交或同时回滚”的约束。

## 职责边界

- 认证中间件从已验证 token 构造 `AuditActor`；Handler 只提取并传递该身份，不能指定
  `action`、`resource_type` 或 `resource_id`。
- Service 根据用例决定审计动作和资源类型，并使用最终实体 ID 作为
  `resource_id`。创建操作必须先生成实体 ID，再构造审计。
- `AuditLog` 必须在启动事务前完成领域校验。审计插入失败会使同一事务中的业务写入回滚。
- Repository 只提供 `create_with_session` 等会话方法，不决定哪些操作需要审计。

当前管理写动作覆盖管理员、角色等 ERP 管理对象的创建、更新、删除，
以及管理员角色更新。审计列表的 JSON 合同保持
`items + total`，单项字段保持 `actor_id`、`actor_account`、`actor_type`、`action`、
`resource_type`、`resource_id`、`success`、`message` 与 `created_at` 不变。

## 事务与取消

- 普通 RBAC 管理写通过 `RbacService::run_authorized_audited_policy_transaction` 执行，
  授权 revision、业务写入、policy 与审计共用同一 MongoDB session。
- 其他管理写入通过 `run_audited_transaction` 执行，业务写入与审计同样共用一个事务。
- 两条路径都由独立 Tokio 任务持有。HTTP 超时、客户端断开或调用方 future 被丢弃后，
  已开始的事务仍会完成提交或回滚收尾；任务失败会在所有权任务内部记录结构化错误，
  不依赖请求等待者仍然存在。
- `CommitOutcomeUnknown` 表示提交结果无法确认，调用方不得盲目重放。Service 使用独立
  `OutcomeUnknown` 语义，Web API 返回非敏感的查询后再重试提示；policy 事务还会将当前
  进程置为一致性未知并停止授权。
- `TransientTransactionError` 映射为 `409 Conflict`，允许调用方重新读取状态后重试。
  当前实现不自动重跑 `FnOnce` 事务闭包。

调用方重试仍可能生成新的审计 ID；项目尚未提供跨请求幂等键，因此不承诺网络重试下的
全局 exactly-once。单次成功事务只写入一条管理资源审计。

## 例外

登录审计记录的是认证事件，不与业务资源写入组成事务，仍采用 best-effort 写入。系统启动
时的 root 角色与超级管理员初始化没有已认证操作人，也不伪造管理资源审计。
