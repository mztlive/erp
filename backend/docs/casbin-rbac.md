# Casbin RBAC

项目使用一套 Casbin RBAC 模型：

- 主体：`user:{account_kind}:{account_id}`
- 角色：`role:{role_id}`
- 权限：`resource:action`
- 主体到角色：Casbin `g`
- 角色到权限：Casbin `p`

MongoDB 的 `roles` 集合保存角色展示信息与启用状态，`casbin_rules` 集合保存 `g`/`p`
policy，`casbin_policy_state` 的单例文档保存全局 policy revision。
`database::MongoCasbinAdapter` 位于 `database/src/casbin_adapter.rs`，既实现 Casbin
Adapter，也提供事务内替换角色绑定、替换角色权限、删除角色规则和递增 revision 的 session
方法。

## 请求授权链路

1. `authenticate` 中间件验证 JWT，并为后台账号注入稳定的 RBAC 主体。
2. admin 路由通过 `with_permission` 绑定 handler 宏生成的 `Permission`。
3. `services::iam::RbacService` 使用 Casbin Enforcer 按主体、资源和动作判定。
4. 未认证返回 `401`，无权限返回 `403`，授权引擎或存储异常返回不泄露内部细节的 `500`。

## 写入与事务边界

`RbacService` 是角色、账号角色绑定和授权判定的统一 Service 入口。涉及角色实体、
账号/Profile 与 policy 的业务事务由 Service 开启，Adapter 和 Repository 通过调用方传入
的 `ClientSession` 加入同一事务：

- 创建或更新角色及其权限：角色实体与完整 `p` 规则在同一事务中写入。
- 删除角色：角色软删除、对应 `p` 规则及指向该角色的 `g` 绑定在同一事务中删除。
- 账号创建、更新或删除：账号/Profile 与 `g` 绑定在同一业务事务中变更。
- 事务内分配角色时，在同一个 session 中校验角色存在且启用，并 CAS touch 角色文档。
- 仅更新角色权限时同样 CAS touch 角色；它与角色删除共享 `id + version` 写入冲突边界。

角色 CAS touch 会递增角色的内部 `version` 与 `updated_at`。这样多实例同时绑定、更新权限
或删除同一角色时，不会因事务快照的旧读而提交指向已删除角色的 `g`/`p` 规则。角色版本是
内部持久化元数据，不属于 HTTP 响应契约。

`MongoCasbinAdapter` 确实持久化 policy；不能把它描述为只由 Service 或角色 Repository
保存。Repository 负责领域实体，Adapter 负责 Casbin rule，二者由 Service 统一编排。
业务代码只调用 Adapter 的 policy 写接口，并传入事务执行器。普通管理操作由
`RbacService::run_authorized_policy_transaction` 把授权快照 revision 与最终写事务绑定；
内建角色初始化才使用无操作人快照的 `run_system_policy_transaction`。禁止绕过这个边界自行提交
policy 事务。Casbin `Adapter` trait 暴露的独立写方法也会自行开启事务，并只在规则确有
变化时原子递增 revision。

每个包含 policy 变更的事务都会在同一 MongoDB session 内递增全局 revision。全部实例写入
同一个 revision 文档，因此跨实例并发 policy 写会产生事务写冲突并向调用方返回 `409`，
而不会让两个基于不同快照的写同时提交。调用方收到并发冲突后可以基于最新数据重新发起
请求。

## 多实例 Enforcer 刷新与失败关闭

事务中的 policy 写入不会直接修改内存 Enforcer。所有写入由
`RbacService` 的 policy 事务运行器在独立 Tokio 任务中持有事务与刷新流程：

- 提交成功后立即从 MongoDB 重新加载本实例的 policy。
- 每次授权判定和 policy 查询都会先读取数据库 revision；发现其他实例已经提交新版本时，
  当前实例会在继续读取缓存前重新加载。
- reload 前后再次读取 revision；只有两个值相同才发布为可用快照。连续三次遇到并发变更
  时本次请求失败关闭，后续请求继续重试，禁止发布不稳定快照。
- 业务事务已经提交但本实例 reload 失败时，写请求仍按已提交成功返回，避免调用方误重试；
  本实例保持 stale 并在后续授权前重试 reload。
- 提交结果为 `CommitOutcomeUnknown` 时将当前进程标记为 policy 一致性未知；Service 映射为
  独立的 `OutcomeUnknown`，Web API 返回稳定 `500` 和非敏感的查询后再重试提示。
- 明确回滚或提交失败时不把未提交 policy 加载到内存。
- HTTP 超时或客户端断开只会取消等待者，不会中断已经开始的提交重试和 Enforcer 刷新。

刷新开始或失败时，当前进程的 `policy_stale` 保持为 `true`。后续授权和 policy 查询会先
重试加载；加载仍失败就返回错误，不继续使用旧授权缓存。这是当前进程内的 fail-closed
保障。提交结果未知时，单次 reload 无法证明稍后不会出现提交结果，因此普通 reload 不会
解除该状态；授权读取与后续 policy 写入会持续失败关闭，直到进程重启并重新加载。

进程内写锁仍只负责本实例串行化；跨实例协调由 MongoDB revision 文档承担，不依赖额外的
Redis、消息广播或变更流。revision 查询失败时授权请求直接失败，不回退到旧缓存。运维或
迁移脚本不得直接修改 `casbin_rules`；必须复用同一事务边界并递增 revision，否则运行中
实例无法感知该绕过写入。

## 内置角色与领域值对象

应用启动时确保 `role-root` 存在、未删除、启用、标记为系统角色且只包含直接 `*:*`
权限；历史错误元数据与 policy 会在同一 revision 事务中修复。即使历史数据把
`role-root.system` 错写为 `false`，普通接口仍按固定 root ID 禁止修改、删除或分配。
账号仍须显式绑定该角色才能获得对应能力。

### 业务预定义角色

在 `ensure_root_role` 之后，启动流程还会调用 `ensure_predefined_roles`，按固定角色 ID
幂等写入业务岗位角色（销售、销售领导、采购、运营、仓储、财务、管理层、系统管理员）。

行为约定：

- **不存在则创建**：写入角色实体与推荐 Casbin 权限（`system=false`，可分配、可后续调整）。
- **已存在且仍等于已知旧种子**：整集升级到当前推荐权限（用于权限替换，例如删旧加新）。
- **已存在但不等于旧种子**：只追加当前种子中尚未覆盖的权限；**不删除**管理员额外授予的
  权限，也**不覆盖**名称、启停状态与 system 标记。软删除记录不重建、不补权限。
- **空范围补齐公司级数据范围**：角色尚无任何生效 `data_scope` 时写入 `company` 范围，使
  `scope=team` 具备可证明的责任范围；管理员已配置或软删除的范围不覆盖、不重建。
- **不替代 root**：`role-sysadmin` 仅含同步/集成/导入等运维权限，不含账号与角色超级管理，
  也不含 `*:*`。
- 业务状态机仍禁止按角色枚举硬编码；预定义角色只是可配置权限的启动种子。

推荐权限定义见 `services/src/iam/predefined_roles.rs`，对齐第一期部门职责与 W01 角色入口。

系统超级管理员只能通过 `AdminService::initialize_super_admin` 创建或修复。该方法按包含软删除记录的全局
账号查找已有身份，并在同一事务中写入传入的名称和新 Argon2 密码、恢复账号、设置启用状态
并绑定 root；因此也可用于 root 凭证轮换。已绑定系统角色的账号不能通过普通账号更新、
删除或恢复入口操作。

普通角色创建、更新、删除和账号管理会使用同一个稳定 Enforcer 快照比较权限范围：

- 新角色权限和待分配角色的隐式权限不得超过操作人的隐式权限。
- 更新或删除角色时，角色当前隐式权限也不得超过操作人权限。
- 更新、删除或恢复账号时，目标账号当前隐式权限不得超过操作人权限。
- 授权检查捕获的 revision 会在最终 MongoDB 事务内比较并递增；授权后 policy 已变化时，
  整个业务写入以并发冲突回滚，避免检查与写入之间的越权窗口。

`entities::RoleId`、`RoleIdSet` 和 `Permission` 负责角色 ID、集合去重及
`resource:action` 的解析与规范化。Handler 不重复实现这些规则。

修改权限模型后运行后端全仓门禁；若生成的前端权限定义或管理端代码发生变化，再运行前端
lint/typecheck。
