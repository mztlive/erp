# 阶段 03：流程定义管理 Service

> 阶段性质：P3 Service 工作包
>
> 阶段目标：交付草稿、节点替换、发布、退役和版本查询的 ERP 应用编排，并复用 `bpm` 的纯图模型与验证器
>
> 允许状态：依赖阶段 00—02 冻结接口；HTTP 由阶段 06 实现，共享入口不得由本阶段修改

## 1. 文件责任

本阶段负责新增或重写：

- `backend/services/src/approval/definition.rs`；
- `backend/services/src/approval/definition_dto.rs`；
- `backend/services/src/approval/policy.rs` 中定义发布所需的资格与岗位分离校验端口；
- `backend/services/src/approval/scope.rs` 中定义管理类型权限可见范围；该范围不是具体单据 DataScope；
- `backend/services/src/approval/tests/definition.rs` 或同模块测试。

本阶段不得修改 HTTP 路由、前端、数据库共享导出、集成测试和应用启动入口。缺少共享符号时必须发起 P0 amendment。

本阶段不得在 `services` 重新定义流程图、状态枚举、BPM ID、线性连线生成器或图验证算法。业务 DTO 必须在边界转换为 `bpm` 模型；BPM 错误必须由 Service 映射为稳定应用错误。

## 2. 应用端口

`ApprovalDefinitionService` 必须提供：

```text
definition_catalog(actor, visibility)
create_definition_draft(document_type, name, draft_source, actor, idempotency_key)
replace_definition_nodes(definition_id, expected_definition_lock_version, nodes, actor)
publish_definition(definition_id, expected_definition_lock_version, actor, idempotency_key)
retire_definition(definition_id, expected_definition_lock_version, actor, idempotency_key)
definition_versions(document_type, actor, visibility)
definition_detail(definition_id, actor, visibility)
```

`draft_source` 只允许 `EMPTY` 或 `CURRENT_PUBLISHED`。`EMPTY` 创建无节点草稿；`SalesOrder` 的零节点状态只允许持续到第一次 `replace_definition_nodes`。`CURRENT_PUBLISHED` 要求当前发布定义存在，并复制其节点顺序、节点名称、指定审批人和既有 `node_purpose` 到新的定义 ID/版本，同时为全部节点生成新的节点 ID 和不可预测 `node_key`，不得复用旧版本身份。客户端不得提交源定义 ID，也不得从任意历史版本复制。

写端口必须由 Service 开启单一 MongoDB 事务；读端口必须在查询条件中执行权限范围过滤。

所有接收 `DocumentType` 的端口必须先通过 P0 冻结的穷尽映射取得 `bpm::ProcessKind`。Repository 和 BPM 不得接收 `DocumentType`；响应层再由同一映射和已注册政策把 `ProcessKind` 映射回单据类型和中文名称。未注册或反向映射不唯一必须作为部署不变量错误失败关闭。本阶段不得修改或复制该映射。

## 3. 请求与响应合同

### 3.1 草稿节点请求

节点请求只允许：

```text
node_id（仅编辑已有节点时可选）
node_name
display_order
assignee_user_id
```

新增节点的 `node_key` 必须由服务端生成。编辑已有节点时，`node_id` 只用于定位本草稿内节点，其 `node_key` 保持不变；来自其他定义、已删除节点或客户端提交的任意 `node_key` 必须拒绝。服务端固定 `node_type=USER_APPROVAL`，并读取审批人显示名。`node_purpose` 也只能由服务端按政策生成和保持：`SalesOrder` 空白草稿第一次整组保存时，服务端把顺序第一节点赋予唯一 `SALES_ORDER_PROCUREMENT_CONFIRMATION`；之后每次替换都必须保留该既有节点 ID 及用途，客户端不得删除、复制或改写；其他类型不得包含用途。请求必须使用 `deny_unknown_fields`，不得接受 `node_key`、`node_type`、`node_purpose`、`assignment_mode`、角色、候选池、resolver、handler、action、任意 transition 或终点。

### 3.2 查询视图

目录视图必须逐个返回固定 `DocumentType` 的：

- 中文单据类型名称；
- `NO_APPROVAL` 或 `PROCESS_REQUIRED`；
- 当前发布版本、活动草稿版本；
- `NOT_APPLICABLE`、`MISSING_CONFIGURATION`、`DRAFT`、`PUBLISHED` 配置状态；
- 当前用户的 `allowed_actions`。

定义详情必须返回版本状态、入口、按 `display_order` 排序的节点、指定人员快照、发布/退役审计和锁版本。普通单据页面只使用只读绑定视图，不复用管理写 DTO。

## 4. 草稿创建与替换

### 4.1 创建草稿

事务必须按下列顺序执行：

1. 校验该 `DocumentType` 已注册且政策为 `PROCESS_REQUIRED`，取得唯一 `bpm::ProcessKind`；
2. 校验 actor 拥有该类型的 `definition_admin_permission`；定义草稿没有具体业务对象，不得伪造实例 DataScope 校验；
3. 按本节 canonical payload 查询幂等收据；同载荷收据存在时，在当前权限重验通过后回读原草稿引用和最新授权视图，异载荷返回幂等冲突；
4. 无已提交收据时，查询并拒绝已有活动草稿；
5. 读取历史最高版本并使用 checked add 生成下一版本；
6. 严格按 `draft_source` 创建空草稿或从当前发布版本复制节点；缺失值、未知值或请求复制但当前无发布定义均失败关闭；
7. 写入不可变审计和幂等收据；
8. 提交事务。

不得通过重用已退役定义 ID 建立新版本。

### 4.2 替换节点

`replace_definition_nodes` 必须：

1. 以 `definition_id + DRAFT + expected_definition_lock_version` 锁定草稿；
2. 校验节点数为 1—20；
3. 校验已有 `node_id` 属于当前草稿且不重复；为新节点生成不可预测且稳定的 `node_key`；校验名称非空，顺序从 1 连续且无重复；`SalesOrder` 首次保存由服务端给顺序第一节点赋予采购确认用途，后续保存必须包含既有用途节点 ID 并保持其用途；其他类型用途必须为空；
4. 批量读取用户，拒绝不存在、停用或任职失效账号；
5. 对每个用户执行账户有效、静态审批权限和可在定义期判断的节点间岗位分离校验；
6. 将入口固定为第一节点；
7. 调用 `bpm::graph::generate_linear_transitions` 确定性生成 `APPROVE` 和 `REJECT` 连线；
8. 整组替换节点和连线，定义锁版本加一；
9. 写入包含变更前后摘要的审计；
10. 原子提交。

请求顺序是唯一编辑输入；客户端不得直接提交入口或连线。

## 5. 连线生成器

使用阶段 01 已交付的纯函数 `bpm::graph::generate_linear_transitions(nodes)`，输出必须固定为：

```text
Ni APPROVE -> Ni+1
Nlast APPROVE -> APPROVED
Ni REJECT -> entry_node
```

生成器必须拒绝空列表、重复 key、非连续顺序和超过 20 个节点。返回顺序必须稳定，使请求摘要、审计 diff 和测试 fixture 可重复。“只有末节点通过进入终态”和“所有驳回回到入口”由 `bpm::graph` 的完整图生成器及发布图校验器证明，不得放入单条 transition 实体或 Service 私有 helper。

## 6. 发布

`publish_definition` 必须在一个事务中：

1. 读取并 CAS 锁定目标草稿；
2. 读取同类型当前 `PUBLISHED` 定义；
3. 重验政策仍为 `PROCESS_REQUIRED`；
4. 重新加载完整图，禁止信任草稿上次校验结果；
5. 调用 `bpm::graph::validate_definition` 重验节点数、连续顺序、唯一入口、可达性、单线无环、唯一终点和全部驳回回入口；
6. 按 `DocumentType` 政策校验节点用途完整性：`SalesOrder` 恰好一个采购确认用途，其他类型为零；
7. 重验所有指定账号、静态审批权限和节点间岗位分离；具体单据 DataScope、对象读取权、提交人与审批人隔离在单据创建绑定、启动、进入节点和决定时校验；
8. 校验该 `DocumentType` 的提交、最终通过和 `cancel_action` 均已注册；业务撤回和受阻取消必须复用该动作；
9. 刷新并冻结审批人名称快照；
10. 将旧发布版本置为 `RETIRED`；
11. 将草稿置为 `PUBLISHED`；
12. 写入发布、自动退役审计和幂等收据；
13. 原子提交。

任一校验失败必须零写入。不得发布存在占位用户或 `Noop` 领域动作的定义。若政策明确要求审批人具备全组织范围，发布时必须调用该政策的全组织资格校验；未签署该规则时不得声称发布已完成实例级 DataScope 校验。

## 7. 退役

`retire_definition` 只允许退役当前 `PUBLISHED` 版本。退役不会修改已绑定单据和运行实例，也不得级联删除节点、连线或审批人快照。

退役后该 `DocumentType` 必须显示 `MISSING_CONFIGURATION`，新的 `PROCESS_REQUIRED` 单据创建返回 `APPROVAL_PROCESS_NOT_CONFIGURED`。退役操作必须使用锁版本、幂等键和不可变审计。

## 8. 权限与审计

权限采用两层 AND 门禁：阶段 06 Handler 先校验动作级权限，本 Service 再以 `DocumentType` 政策校验类型级管理权限。不得仅凭动作级权限或“系统管理员”角色名管理全部类型。每次写入都必须记录：actor、定义 ID、单据类型、版本、期望/实际锁版本、节点摘要、结果和 correlation ID。

日志不得写入 Token、完整用户对象或敏感单据数据。

### 8.1 定义命令幂等摘要

必须使用固定字段顺序、明确 null、UTF-8 和稳定枚举值生成 canonical payload：

| 命令 | scope | canonical payload |
| --- | --- | --- |
| create draft | `process_kind` | `document_type`、trim 后 name、`draft_source`、actor ID |
| publish | `definition_id` | expected definition lock version、actor ID |
| retire | `definition_id` | expected definition lock version、actor ID |

三个命令都必须在执行状态前置校验前先处理已提交收据，但不得跳过 actor 当前动作权限和该类型 `definition_admin_permission` 重验。定义命令没有具体业务对象，不执行实例 DataScope；同键同载荷回读不可变命令结果引用与当前授权视图，同键异载荷冲突。并发 duplicate key 必须整体回滚，并在事务外的新会话重读收据；不得重做发布、退役或自动退役副作用。

## 9. 旧路径隔离与删除

本阶段交付目标定义管理，不得调用旧结构注册或启动 bootstrap。为保证未切换调用方和 workspace 在准备阶段持续可编译，以下文件由 `P0-D` 在全量类型切换后删除：

- `backend/services/src/approval/bootstrap.rs`；
- `backend/apps/web-api/src/main.rs` 中 `ensure_approval_definitions` 调用；
- `backend/services/src/approval/registry.rs` 中 `CARD_SALES_APPROVAL` 和步骤结构；
- 启动时写入 `approval_definitions`、`approval_step_definitions` 的测试与日志。

强类型政策和领域动作注册必须保留并迁入目标模块。

## 10. 阶段验收

- [ ] 同一单据类型不能创建第二个活动草稿。
- [ ] 陈旧锁版本返回冲突且无部分节点替换。
- [ ] 客户端提交连线、角色池或处理器字段会被拒绝。
- [ ] 新节点 key 仅由服务端生成；已有节点不能借编辑请求更换 key 或跨定义引用。
- [ ] `SalesOrder` 发布定义恰好包含一个采购确认用途节点，其他类型不能使用该用途；客户端不能提交或改写用途。
- [ ] 生成器对 1、2、20 节点产生唯一确定结果。
- [ ] 对 `DocumentType` 的政策注册和 `ProcessKind` 映射均为穷尽 match，任何新增类型都会触发编译失败或完整性测试失败。
- [ ] Service 中不存在 BPM 图算法、状态枚举或流程模型第二定义源。
- [ ] 发布前重新校验全部账号、静态权限和岗位分离，不伪造无具体单据的 DataScope 校验。
- [ ] 发布事务能同时退役旧版本，任何失败均整体回滚。
- [ ] 已发布、已退役定义的结构永远不可改。
- [ ] 两层权限中的类型级权限由 Service 强制执行并有单元测试。
- [ ] `conventions.md` 第 6 节全部后端门禁、`cargo test -p bpm graph`、`cargo test -p services approval::definition` 和 `./scripts/check-bpm-boundaries.sh` 通过。
