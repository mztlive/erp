# erp-identity：身份与访问控制

管理登录账号、组织与成员关系、角色权限和数据范围，提供后台身份认证与访问控制能力。

系统需要统一回答“当前操作人是谁、具有什么权限、属于哪些组织”。各业务领域在这些授权事实基础上，继续执行自身对象的访问和操作规则。

## 使用场景

- 修改后台认证、账号管理、角色及权限配置。
- 维护组织、成员和数据范围规则，调整授权解析。
- 校验、预览、原子应用和导出授权 JSON 文件；角色、真实绑定、人员范围及命令回执由本域维护。

## 协作示例

给账号配置角色和组织关系时使用本 crate；客户归谁负责由 [erp-customer](../erp-customer/README.md) 维护。HTTP 请求中的 JWT 校验和路由装配由 [web-api](../../../apps/erp-server/README.md) 接入，显式权限目录维护规则见 [权限目录合同](../../../docs/permissions-gen.md)。

## 负责的数据与能力

- 账号、角色、权限、用户角色绑定和数据范围规则。
- 内部组织节点与成员关系，组织变更预览与执行；旧部门管理关系仅保留历史记录。
- 后台认证、IAM 管理、RBAC 服务与 MongoDB Casbin 适配。
- 独立供应商门户账号、固定维护或只读岗位、单供应商绑定及会话版本失效。门户不建立内部角色或组织关系。

## 依赖与协作边界

术语与统一分工见[总索引](../../README.md#代码中常见名称的含义)。

- 本领域的数据结构、业务规则、服务和数据库仓储在此维护。常规、构建和测试依赖均不得指向其他业务领域或组合层。
- 账号与授权规则由本域提供；HTTP JWT 中间件与路由装配由 web-api 负责。
- 审计等外域能力通过本域 Port 注入；不得在身份仓储中读写其他领域集合。
- 需要其他领域的能力时，由组合层或应用将本域接口接到实际提供方；公开入口见 [src/lib.rs](src/lib.rs)。

## 代码入口

| 入口 | 用途 |
| --- | --- |
| [src/service/iam/mod.rs](src/service/iam/mod.rs) | 账号、角色与 RBAC 服务入口 |
| [src/service/account_permissions.rs](src/service/account_permissions.rs) | 按功能权限筛选有效后台账号及重验目标资格；不替代业务对象授权 |
| [src/service/access_control/policy_bundle/mod.rs](src/service/access_control/policy_bundle/mod.rs) | 授权文件预览、应用及导出 |
| [src/entity/authorization_bundle/mod.rs](src/entity/authorization_bundle/mod.rs) | 文件格式、权限差异、审核摘要和重放规则 |
| [src/service/auth/mod.rs](src/service/auth/mod.rs) | 后台认证入口 |
| [src/service/portal/mod.rs](src/service/portal/mod.rs) | 供应商身份、绑定、账号管理及密码步骤 |
| [src/entity/portal.rs](src/entity/portal.rs) | 门户固定岗位及账号绑定不变式 |
| [src/service/organization.rs](src/service/organization.rs) | 组织状态、变更预览与执行 |
| [src/entity/organization.rs](src/entity/organization.rs) | 部门、团队、成员及旧管理关系存储类型 |
| [src/ports/mod.rs](src/ports/mod.rs) | 审计及授权合同 |
| [src/entity/mod.rs](src/entity/mod.rs) | 本域实体、值对象和确定性规则 |
| [src/repository/mod.rs](src/repository/mod.rs) | 本域 MongoDB 仓储与集合访问器 |
| [src/indexes.rs](src/indexes.rs) | 公开索引注册入口 |

## 修改执行要求

1. 将无 I/O 的校验、状态迁移和不变式放入本域实体或值对象；Service 组织本域用例。
2. Repository 使用调用方传入的 `persistence_core::Executor`；跨集合原子写入由本域用例或 Process 控制事务。
3. 新增或调整集合查询时同步评估索引；组合根复用本域公开索引入口，保持既有逐集合注册顺序。
4. HTTP 请求和响应优先复用本域 DTO；扩展公开合同须同步检查 Process、ReadModel 和应用调用方。
5. 业务改动补充本域库单元测试，覆盖成功、失败、边界及相关幂等或版本冲突路径。
6. 门户密码哈希在事务外完成；开通、启停及自助密码的持久化步骤接收调用方 `Executor`。供应商启用事实、内部管理权限和对象范围由组合层在同一写入事务中重验。每次门户访问必须重验账号版本、绑定版本、绑定启用及供应商启用；后台认证固定接受 `Admin`。

## 部署岗位角色

1. 使用超级管理员登录，进入“角色与权限”→“从内建岗位生成”。
2. 选择销售、销售领导、采购、运营、仓储、财务总监、出纳、开票、管理层及系统管理员；默认选择尚未生成且可授权的十个分岗模板。综合财务是可选模板，包含完整财务能力，不应与分岗限制混用。
3. 生成前核对各岗位操作权限及人员上岗配置。批量生成沿用角色创建权限和授予上限，角色、权限策略、审计及策略版本在同一事务提交。
4. 既有固定角色身份保持原配置，包括改名、权限调整、停用和软删除。生成动作不补权、不恢复账号、不修改绑定。启动仅维护超级管理员、组织基础及目录资格，不再创建或升级业务角色。
5. 在“组织与人员”分配生成的角色，并设置本人以外业务范围、目录可见和仓库范围；审批、付款、开票及履约责任另行指定。角色生成不创建演示账号、密码、部门、业务数据或审批定义。

接口及部署约束执行[人员配置合同第 9 节](../../../../docs/identity-access-workflow-contract.md#9-内建岗位角色生成)。

## 外部授权配置

使用仓库根 `scripts/authorization-policy.mjs` 调用正式后台 API；顺序为 validate、preview、审核差异、apply。export 必须明确选择人员或角色。文件格式、权限要求、命令示例及回滚要求执行[授权 JSON 配置合同](../../../../docs/authorization-policy-json-contract.md)；不得直接写入 Casbin 或人员范围集合。

## 验证要求

以下命令在 `backend/` 目录执行：

```bash
env -u ERP_TEST_MONGO_URI cargo test -p erp-identity --lib --locked
```

代码变更还须执行[统一质量门禁](../../README.md#质量门禁)。测试范围限定为库单元测试，不执行集成测试或真实外部服务测试。

## 指定记录读取授权

1. 统一入口为 `service::resource_access::ResourceAccessService`。首批资源限定销售单和采购单；`read` 不授予修改、审批或任务办理权限。
2. 调用 `resolve` 时必须通过 `entry_permission: &Permission` 传入已解析的服务端实际入口权限，例如 `Permission::parse("invoice:create")`；`resource` 独立指定来源业务类型。销售列表采用销售列表范围，其他来源读取采用详情范围。财务入口只检查其自身功能权限，再解析销售来源范围。
3. 列表调用 `resolve` 取得 `AuthorizedResourceQuery`；只读取原范围和账号资源修订号，不预加载授权 ID。业务仓储将原范围编译为条件，调用 `repository::resource_grants::read_stages`，按“业务筛选 → 授权阶段 → 分页、计数、汇总”执行聚合。授权阶段按业务主键 `id` 关联有效授权和来源，使用存在性判断避免多来源重复计数。查询使用本次解析的同一执行器，返回结果前调用 `ensure_current`，版本变化必须拒绝并重读。
4. 单条、批量和任务检查调用 `resolve_targets`，通过 `ResourceTargets::new` 传入服务端确定的完整目标集合，单条传一个 ID。数据库只查询这些目标并校验关联来源，去重后返回至多目标数个 ID。`AuthorizedResourceAccess::allows`、`batch` 只对本次目标有效，目标之外一律拒绝；该类型不能提供完整列表约束。`ensure_task` 还要求当前正式分配、可办理状态及全部必要来源通过，禁止客户端裁剪来源。业务写入须在同一事务执行器内重新解析目标授权及当前业务事实，禁止复用先前响应。
5. 人工授予通过 `grant` 执行，来源 `rule`、`business_type` 均固定为 `manual`，`business_id` 使用稳定命令号。管理操作要求有效后台账号、`data_scope:create`、`org_unit:manage` 及无个人上限的全公司组织管理范围。接收账号必须有效；目标存在性通过 `ResourceGrantFacts` 校验。
6. 自动发布使用 `apply_source`，必须加入外层事务并通过事实端口重验规则、开票最终审批通过或审批节点分配事实、正式接收人、事实版本和完整目标。事实读取失败须返回错误；不得提交空集。规则计算必须包含需保留的历史正式分配来源。
7. 来源版本和事实版本只允许前进；同版本同目标重放零写入，同版本异目标拒绝。撤销使用 `revoke` 和期望版本，永久保留终止状态，后续重算不能复活。多个来源独立生效。来源、授权行、操作人及原因审计同事务提交。
8. `append_event` 接收 `GrantEventEvidence`，业务载荷 `GrantEventPayload` 固定为 `InvoiceApproved`、`SalesOrderApprovalAssigned`、`PurchaseOrderApprovalAssigned`。开票事件必须在最终审批通过并创建执行任务的同一事务追加，携带开票申请、审批实例、执行任务标识，接收人取执行任务实际分配账号；销售、采购事件在审批节点正式分配事务追加，携带单据、审批实例、节点及分配记录标识。来源规则和业务类型由变体派生，调用方不得自选。
9. 事件幂等键包含类型、正式业务事件身份、接收人及事实版本。开票事实版本采用申请版本，销售和采购审批采用审批实例版本，不得混用节点局部版本。同键更换执行任务、单据或发生时间必须返回冲突；同证据重放不重置处理状态。未知类型、缺失证据及不规范身份必须拒绝。`Pending / Processing / Applied / Failed` 仅表示处理状态；领取、重试和授权未就绪响应由后续 worker 接入。
10. `scope_version` 包含原身份范围版本、入口、资源及账号资源修订号。来源变更必须对修改前后资源类型的并集递增修订号，并与来源、授权行及审计同事务提交；幂等重放不递增。来源撤销后旧授权行不产生资格，查询每次校验来源终止状态、版本和目标成员。禁止通过枚举全量记录计算版本。单次操作目标和单来源目标各最多 1000；账号授权总量没有 10000 条读取门槛。来源管理接口每页最多 100 项并返回游标，来源同步每次只装载 200 行，按稳定主键继续处理。
11. 新增集合为 `resource_grants`、`resource_grant_sources`、`resource_grant_events`、`resource_grant_audits`、`resource_grant_revisions`。发布前执行 [索引迁移](../../../scripts/migrations/20261109-resource-grants.mongosh.js)，启动时复用本域 `indexes::ensure_authorization`。不得修改旧人员范围数据；回滚时停止新入口及授权写入、恢复旧应用，保留新集合、事件与终止记录。
12. 当前交付为身份领域接口、存储和单元规则。业务适配器、HTTP 管理入口、worker、历史回填和生产切换必须分别完成后才可启用。库测试不能替代 MongoDB 事务、并发及业务链路验收。

事件结构升级必须先执行索引迁移的旧记录检查。发现没有 `evidence.payload.type` 的旧事件时停止发布，按正式业务证据重建；禁止默认转换为 `InvoiceApproved`。升级增加 `idx_resource_grant_events_typed_source` 索引并保留旧索引。回滚必须停止类型化事件生产和消费，保留已写事件及授权来源，禁止将新事件交给旧结构消费者。

修订号使用结构化账号资源身份作为 MongoDB `_id`；内建唯一索引承担精确读写。不存在的修订号按初始版本 0 处理，不重置已有记录。启用前必须切换全部授权写入方，禁止旧程序绕过修订号发布授权。回滚须同时停止新授权入口与写入，保留修订号；重新启用后必须重新解析授权上下文。

## 授权接口合同

- 单次与批量范围解析共用身份取证和范围判定。仅同一事务读取阶段允许复用事实；非事务调用逐次重验账号、角色及策略版本。
- `AuthorizedDataScope` 只读暴露身份、范围和角色资格。调用方使用 `map_read_version` 绑定业务事实版本，禁止自行构造授权结果或改写资格。`test-support` feature 只允许通过测试依赖启用，测试夹具不得用于生产授权。
- `ResourceAccessService` 只提供读取判定；`ResourceGrantService` 负责人工授权管理和可信来源发布；`GrantEventService` 负责后台事件队列。外部调用方按职责装配对应服务。
- 组织管理与工作项负责人编译保留完整成员事实；其他范围解析读取有界组织树及当前操作人的关系。组织启用检查不读取成员，负责人主属查询限定账号和有效时点。
- 授权来源、历史行版本、目标差异、账号资源修订号和审计必须在同一事务内同步。历史行保留乐观锁；新增行按页批量写入；失败不得提交部分授权。

## 身份类型与输入合同

- 角色实体构造必须接收 `RoleId`；角色授予和账号管理的内部接口必须传递 `RoleIdSet`。原始角色字符串在请求转换或外部数据读取边界解析一次，持久化时使用规范化身份。
- 功能权限检查必须传递 `Permission` 或权限切片；语法错误在调用边界返回，身份服务内部按账号、角色权限、策略版本顺序重验。权限目录实体使用 `PermissionDefinition`，其 `resource`、`action` 持久化字段由权限值对象编码，解码时必须校验。
- 用户角色分配请求的 `effective_from`、`effective_to` 使用正整数秒级时间戳；生效时间缺省时取当前时刻。输入超过 `i64` 或可表示日历范围时返回 HTTP 400、`INVALID_REQUEST`，不得整数截断或 panic。有效期倒挂仍由实体拒绝。响应时间字段使用秒级数字编码。
- `Instant` 反序列化必须对不可表示的整数返回解码错误；调用方不得依靠 panic 拒绝外部时间输入。
- 人员范围表达式直接编译为范围条款；每个分支独立满足必需维度，限制条件与授权结果求交，显式空限制拒绝普通及历史读取。旧角色范围和个人扩展授权仅通过显式迁移转换为人员表达式。
