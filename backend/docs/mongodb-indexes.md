# MongoDB 索引迁移

Web API 启动时会调用 `database::ensure_indexes`，创建账号、角色、
审计日志和 Casbin policy 所需的命名索引。索引创建失败会阻止应用
启动，避免在没有唯一约束的情况下继续提供写服务。

## 部署前查重

历史数据首次启用唯一索引前，应在目标数据库执行以下聚合。每条查询都应返回空集：

```javascript
db.accounts.aggregate([
    { $group: { _id: "$id", count: { $sum: 1 } } },
    { $match: { count: { $gt: 1 } } },
])
db.accounts.aggregate([
    { $group: { _id: "$account", count: { $sum: 1 } } },
    { $match: { count: { $gt: 1 } } },
])
```

对 `roles` 和 `audit_logs` 检查 `id`。发现重复值
时必须先根据业务记录合并或归档，禁止自动保留任意一条。

账号字段采用全局唯一约束：软删除不会释放账号。这样可以保留恢复语义，避免新记录
占用旧账号后导致恢复失败。

## RBAC 事务边界

账号角色绑定、角色权限替换和角色删除都应复用业务事务中的 MongoDB
`ClientSession`。`MongoCasbinAdapter` 的 `*_with_session` 方法只执行规则写入，
不会自行提交事务：

- `replace_subject_roles_with_session`：覆盖主体的 `g` 规则。
- `replace_role_permissions_with_session`：覆盖角色的 `p` 规则。
- `remove_role_with_session`：删除角色的 `p` 规则及指向该角色的 `g` 规则。

RBAC Service 通过取消安全的 owned task 串行持有事务、提交重试和本地 Enforcer
刷新，并在同一事务内递增 `casbin_policy_state` 的单例 revision 文档。该文档使用
MongoDB 内建 `_id` 唯一索引，不需要额外索引。其他实例在授权读取前检测 revision，
只加载版本前后稳定的 policy 快照。提交结果未知时授权边界进入 fail-closed 状态，
普通 reload 不会擅自解除。
Casbin 的 `values.0` 与 `values.1` 查询索引分别支撑角色权限和主体角色绑定的清理。

## 回滚

应用代码回滚后，可按需使用 `dropIndex` 删除本次新增的命名索引。不要删除 MongoDB
自动创建的 `_id_` 索引。

```javascript
db.accounts.dropIndex("uk_accounts_id")
db.accounts.dropIndex("uk_accounts_account")
db.roles.dropIndex("uk_roles_id")
db.audit_logs.dropIndex("uk_audit_logs_id")
```

查询辅助索引可以保留；它们不改变写入合同。若要一并回滚，索引名称以
`idx_accounts_`、`idx_roles_`、
`idx_audit_logs_` 和 `idx_casbin_` 开头。
