# erp-identity

负责账号登录、角色权限、组织成员、数据范围及指定记录读取授权。业务授权统一使用 `AuthorizationService`。

本 crate 不依赖其他业务领域。业务模块提供 Mapping 和对象事实，组合根登记资源动作及查询模型。完整合同见 [identity 统一权限 API 设计](../../../../docs/identity-authorization-api-design.md)。

## 业务授权 API

| 用途 | 方法 | 返回 |
| --- | --- | --- |
| 操作、单条、批量对象检查 | `AuthorizationService::require` | `Result<()>` |
| 列表、计数、汇总、查询式导出 | `AuthorizationService::query` | `AuthorizedQuery` |
| 查询条件编译 | `AuthorizedQuery::compile` | `CompiledAccess` |
| 仓储执行前后复验 | `AuthorizationService::validate` | `Result<()>` |
| HTTP 入口、按钮功能资格 | `AuthorizationService::has_permission` | `Result<bool>` |

`has_permission` 检查账号有效及启用角色授予的单个功能权限，不包含对象资格。业务用例仍执行 `require` 或 `query`。数据库或策略读取错误不转换为 `false`。

业务不再调用 `RbacService::enforce`、`DataScopeService`、`ResourceAccessService`，也不再手工合并基础范围和记录授权。旧查询结果类型及 `read_stages`、`lookup_stages` 已移除。identity 管理用例使用的范围解析器仅在 crate 内可见。

供应商门户继续使用 `PortalIdentityService`，按供应商绑定和门户岗位检查。

## 对象与命令

```rust
use erp_identity::{AccessSubject, AccessTarget, Permission, ResourceTargets};

let permission = Permission::parse("sales_order:update")?;
authorization.require(
    actor,
    &permission,
    AccessTarget::Records {
        subject: AccessSubject::Direct,
        ids: ResourceTargets::new(&[order_id])?,
    },
    executor,
).await?;
```

- 单条和批量使用同一入口。目标集合最多 1000 项，空集合拒绝。
- 事实端口必须返回完整根目标及实际来源。缺失、重复、额外或资源错配的根对象导致整次失败。
- 来源保留全部引用 ID 和实际读取对象。`AllNonEmpty` 要求集合非空、全部来源存在且有权。
- 无对象操作使用 `AccessTarget::Operation`，仅接受已登记的无对象政策。
- 对象及来源事实展开最多 1000 项、最多 8 层，超限失败，不截断。
- 写入前在业务事务内执行检查，授权读取、业务读取和写入使用同一执行器。状态、金额、业务版本、幂等及所需并发冲突保护由业务用例负责。

## 列表查询

```rust
let permission = Permission::parse("sales_order:list")?;
let access = authorization
    .query(actor, &permission, AccessSubject::Direct, executor)
    .await?;

// mapping 为组合根登记的同一个 Arc<dyn AccessMapping>。
let compiled = access.compile(mapping.as_ref(), executor).await?;
authorization.validate(&compiled, executor).await?;
// 仓储将 compiled.stages() 放在分页、计数、汇总之前执行。
let page = repository.list(&filter, &compiled, executor).await?;
authorization.validate(&compiled, executor).await?;
```

仓储统一封装编译、执行及前后复验。业务筛选只收窄候选集合，不能先用部分权限条件排除记录。列表、总数、汇总和导出使用同一完整授权条件。

`AuthorizedQuery` 不可由业务构造或修改，绑定账号、入口、资源、模型、授权时点及身份版本。`CompiledAccess` 继续绑定 Mapping 实例、准备事实、版本和有效期。两者只用于当前读取阶段，禁止跨请求或事务缓存。

跨页游标使用 `CompiledAccess::version()`，包含身份和业务映射依赖；`AuthorizedQuery::version()` 只包含编译前身份授权依赖。复验发现账号停用、权限撤销、版本变化或有效期到达时返回 `DATA_SCOPE_CHANGED`，丢弃结果并重新查询；数据库和接入错误保留原类别。

`MappingFacts` 仅携带业务映射的修订号与有效期。查询指纹使用 `authorization-query-v3` 格式；升级前的分页版本按 `DATA_SCOPE_CHANGED` 合同失效，调用方须重新查询。

记录授权在数据库内按有效来源关联，不预加载账号全部授权 ID。关联采用存在性查询，不增加根记录数量。事务读取以原事务快照为准；非事务映射必须提供完整修订号，不能用前后读取代替缺失的版本证据。

## 服务端登记与 Mapping

组合根创建 `AuthorizationRegistry`，传给 `AuthorizationService::new(db, rbac, registry)`。登记不可由 HTTP 参数生成；未登记操作拒绝。

每个 `AccessRegistration` 固定入口 `Permission`、额外必需权限和 `AccessMode`。全部必需权限须由同一个有效角色提供。接入形态按下表构造：

| `AccessMode` | 必需内容 | 可用入口 |
| --- | --- | --- |
| `Operation` | 无数据政策和端口，主体固定为 `Direct` | 无对象操作检查 |
| `Query` | `DataAccess`、Mapping | 查询 |
| `Object` | `DataAccess`、`AccessFactsPort` | 完整对象检查 |
| `QueryAndObject` | 同一份 `DataAccess`、Mapping、`AccessFactsPort` | 查询及完整对象检查 |

`DataAccess` 固定 `AccessSubject`、根对象必需事实和 `AccessPolicy`。数据政策支持范围、任务分配、参与、与或组合、来源关系；无对象操作不得嵌入数据政策。各范围节点通过 `ReadAccess` 声明读取补充开关。

只做对象检查时使用 `Object`，不执行查询准备和编译。`View` 的标识须与模型一致，`Source` 必须对应政策中的实际关系。任一来源命中的政策只允许用于受限只读视图；`QueryAndObject` 必须使用完整对象模型。

目录构造必须校验具体权限、动作范围、政策深度、来源关系、模型身份和必需事实。已有对象范围要求的动作不得登记为 `Operation`；读取范围不得替代同资源写入范围。

| Mapping 能力 | 职责 |
| --- | --- |
| `model` | 固定模型、资源、集合、行主键、输出粒度和支持的事实维度 |
| `key` | 返回授权资源 ID 表达式，供记录授权和来源连接使用；支持字段引用及计算表达式 |
| `prepare` | 在原执行器读取本次事实、修订号和有效期 |
| `records` | 限定实际存在且未删除的模型记录；根查询、来源关联均应用 |
| `predicate` | 将一个 `AccessFact` 转成字段条件或关联存在性查询 |
| `relation` | 提供完整来源 ID 表达式及来源 Mapping |

`Owned` 在销售单中映射到负责销售字段，在客户中映射到有效负责人分配。字段不同，语义相同。Mapping 不决定权限条件的与或关系、个人上限或是否接受记录授权。

`QueryOutput::Object` 表示完整对象。`Share` 和 `Existence` 必须登记为独立只读视图并列出输出字段，编译器执行字段投影。份额模型每行只对应一个来源，金额只能来自该份额；存在性视图不能登记其他来源内容或完整单据金额。受限视图不能用于 `require` 完整对象检查。

`DataAccess.required_facts` 声明根对象必需事实，`AccessPolicy::Related.required_facts` 声明来源必需事实，Mapping 在 `QueryModel.facts` 中声明支持的维度。对象字段使用 `FactValue`：`Missing` 表示未读取；`Known(None)` 或已知空集合表示确认没有值。必需事实缺失时返回接入错误，其他授权分支命中不能掩盖缺失。

查询缺少映射、对象检查缺少事实端口、未知事实、错误模型或不支持维度均返回错误，不能降级成全量查询。Mapping 与事实端口执行相同责任定义，并验证对象判定与列表成员判断一致。

## 授权规则

1. 账号和同角色完整操作权限先于数据判断。
2. 每个 `AccessPolicy::Scope` 通过 `ReadAccess { history, records }` 显式选择读取补充，默认关闭。基础数据范围为有效条款或已开启的历史参与，再与个人上限求交。
3. 销售单、采购单的列表和详情开启 `records` 后增加有效记录读取授权分支；该分支不放行原单据修改、提交。
4. 独立范围继续与读取结果求交。
5. 来源业务检查入口权限及实际来源数据资格，不额外要求来源详情功能权限。
6. 任务可见、资料可读、任务可办理分别登记；业务状态不由授权服务判断。
7. 公司共享资源仍检查账号和操作权限，不读取无关的人员范围或记录授权。

## 管理记录读取授权

人工授权使用 `ResourceGrantService`：`grant` 创建或更新，`revoke` 永久撤销一个来源，`sources` 查询账号的授权来源。这些方法会检查管理员权限。

自动授权在业务事务内调用 `append_event`。事件中的接收账号必须来自实际任务分配，不能由客户端自选：

- 销售单、采购单审批分配分别使用 `SalesOrderApprovalAssigned`、`PurchaseOrderApprovalAssigned`。
- 开票最终审批通过并创建执行任务后，使用 `InvoiceApproved`。
- 开票任务转派使用 `InvoiceTaskReassigned`。

各事件必填字段见 [GrantEventPayload](src/entity/resource_access/event.rs)。`append_event` 必须加入业务事务：新事件返回 `true`，相同内容重放返回 `false`；内容非法、幂等冲突或写库失败时，业务事务也必须失败。

`erp-worker` 调用 `erp-resource-grants` 处理事件，再通过 `ResourceGrantService::apply_source` 写入授权。写入前会通过 `ResourceGrantFacts` 重新检查业务数据。事件已追加不代表授权已生效。

## 代码位置

| 位置 | 内容 |
| --- | --- |
| [service/auth/](src/service/auth/) | 后台登录、密码检查 |
| [service/iam/](src/service/iam/) | 账号、角色、Casbin 权限 |
| [service/organization.rs](src/service/organization.rs) | 组织和成员管理 |
| [service/portal/](src/service/portal/) | 供应商门户账号和登录 |
| [service/access_control/](src/service/access_control/) | 数据范围配置、解析和检查 |
| [service/resource_access/](src/service/resource_access/) | `ResourceGrantService` 管理授权；`GrantEventService` 处理事件队列 |
| [service/authorization/](src/service/authorization/) | 统一操作、对象、查询与复验 |
| [repository/authorization/](src/repository/authorization/) | 查询编译及不可伪造的查询结果类型 |
| [entity/](src/entity/) | 数据结构和不访问数据库的判断规则 |
| [repository/](src/repository/) | MongoDB 读写和查询条件 |
| [ports/](src/ports/) | 由外部实现的业务查询和审计接口 |
| [dto/](src/dto/) | 请求和返回数据 |
| [indexes.rs](src/indexes.rs) | MongoDB 索引 |

## 测试

在 `backend/` 下执行：

```bash
env -u ERP_TEST_MONGO_URI cargo test -p erp-identity --lib --locked
```
