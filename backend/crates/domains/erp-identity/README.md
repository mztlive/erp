# erp-identity

负责账号登录、角色权限、组织成员、数据范围及指定记录读取授权。业务授权统一使用 `AuthorizationService`。

所有权限判断必须统一通过本 crate 执行；业务模块只提供业务事实并调用授权入口，不得自行实现、解释或组合权限规则。

本 crate 不依赖其他业务领域。业务模块提供事实读取器和 Mapping；server 的 `wiring/authorization/` 装配事实读取器和 Mapping；identity 固定并执行授权规则。

公共权限值与目录由 `erp-permissions` 提供，identity 的 `Permission` 导出引用同一类型。业务用例使用统一公共 API，HTTP 和 worker 通过完整 Facts/Mapping 装配取得共享服务。接口与验收合同见 [授权服务改进](docs/authorization-service-improvement.md)。

## 共享服务

需要业务授权的进程启动时通过 server 的 `wiring/authorization/` 构造一个 `Arc<AuthorizationService>`。HTTP 和 worker 将实例保存在 `Application`；CLI 保持独立运行时，不依赖 server 或接入其全量业务授权装配。`HttpState` 通过 `Application` 使用它；业务 Service、Process 和 ReadModel 通过构造函数接收共享实例。

identity 固定授权规则，事实读取器和 Mapping 在 server 的 `wiring/authorization/` 统一装配。`Application::authorization()` 只克隆共享 `Arc`，请求处理期间不再创建授权服务或登记政策。

共享服务不保存当前账号、事务、候选对象或授权结果。每次检查使用本次调用的执行器读取权限、范围和对象事实。

## 授权规则从哪里读

| 要了解的问题 | 代码入口 |
| --- | --- |
| 库存仓库入口沿用哪个库存动作的资格和范围？ | [`scope_spec/catalog.rs` 的 `INVENTORY_WAREHOUSE_ENTRIES`](src/entity/access_control/scope_spec/catalog.rs) 同时供范围登记、资格检查和可查询入口枚举使用。 |
| 收款、付款怎样区分完整对象、可见份额和存在性？ | [`rules/sources.rs`](src/service/authorization/rules/sources.rs) 按真实来源构造条件；[`resolve.rs`](src/service/authorization/resolve.rs) 根据查询输出选择条件，并保留整账条件。 |
| 工作项的预检、按钮展示和实际办理各要求什么？ | [`work_items.rs` 的 `TASK_REQUIREMENTS`](src/service/authorization/work_items.rs) 按任务种类和真实来源登记读取权限与执行子集，同文件中的各入口按用途取用。 |
| 审批对象或升级候选可以对应哪些真实来源？ | [`approvals.rs` 的 `MANAGEMENT_SOURCES`](src/service/authorization/approvals.rs) 登记合法来源及整账附加资格；[`approval_query.rs`](src/service/authorization/approval_query.rs) 按同一登记构造管理列表。 |

跟踪一次查询时，从 [`resolve.rs`](src/service/authorization/resolve.rs) 看本次账号、动作和范围如何决定规则，再看[查询编译](src/repository/authorization/mod.rs)如何用 Mapping 将规则转换为 MongoDB 过滤条件，最后看 [`validate.rs`](src/service/authorization/validate.rs) 如何在交付前复验凭证。

## 业务授权 API

| 用途                                     | 方法                                   | 返回                                   |
| ---------------------------------------- | -------------------------------------- | -------------------------------------- |
| 操作、单条、批量对象检查                 | `AuthorizationService::require`        | `Result<()>`                           |
| 列表、计数、汇总、查询式导出，含条件编译 | `AuthorizationService::query`          | `Result<CompiledAccess>`               |
| 查询结果返回前复验                       | `AuthorizationService::validate`       | `Result<()>`                           |
| 单项功能资格、按钮展示                   | `AuthorizationService::has_permission` | `Result<bool>`                         |
| 批量功能资格                             | `AuthorizationService::permissions`    | `Result<Option<QualifiedPermissions>>` |

`has_permission` 检查账号有效及启用角色授予的单个功能权限，不包含对象资格。业务用例仍执行 `require` 或 `query`。数据库或策略读取错误不转换为 `false`。

`permissions` 批量读取各项功能资格，账号失效时返回 `None`；身份设施、策略版本或非法权限错误直接返回。批量结果不代表同一角色具备全部权限，需要此约束时使用 `QualifiedPermissions::same_role`。功能资格不能替代 `require` 或 `query` 的对象授权。

HTTP 层通过 JWT 认证身份，受保护的公共业务用例在返回业务数据或执行写入前调用 `require` 或 `query`。仅要求功能权限的操作使用 `require` 和 `AccessTarget::Operation`；`has_permission` 和 `permissions` 用于功能资格查询及按钮展示。路由中的 `permission_route` 只关联公共权限目录元数据，供构建生成器读取，不执行运行时 RBAC。

业务不得直接调用 `RbacService::enforce`、`DataScopeService`、`ResourceAccessService`，也不得手工合并基础范围和记录授权。范围解析和查询编译由 identity 内部执行。

静态选项、字典候选、期间口径和审计日志同样执行各自入口授权。审计展示使用 `AuditLogReadService`；内部审计写入继续使用独立的写入服务。协议读取及技术票据使用固定对象入口，在原快照中取得完整资格、连接和来源，不能用连接详情按钮资格代替。

业务附件写入对象存储前，先通过所属用例的固定 `prepare_attachment_upload` 入口检查完整功能资格；identity 使用固定内部权限和 `AccessTarget::Operation` 执行。该准备结果不证明对象、来源或任务资格，不能复用作写入凭证。文件存储后的业务事务仍须按真实目标重新完整授权；不得为业务附件额外要求独立文件上传菜单。

供应商门户继续使用 `PortalIdentityService`，按供应商绑定和门户岗位检查。

## 对象与命令

```rust
use erp_identity::AccessTarget;
use erp_permissions::sales_inquiry;

authorization.require(
    actor,
    &sales_inquiry::COMMANDS_SALES_SAVE,
    AccessTarget::record(inquiry_id)?,
    executor,
).await?;
```

- 单条和批量使用同一入口。目标集合最多 1000 项，空集合拒绝。
- 事实端口必须返回完整根目标及实际来源。缺失、重复、额外或资源错配的根对象导致整次失败。
- 来源保留全部引用 ID 和实际读取对象。`AllNonEmpty` 要求集合非空、全部来源存在且有权。
- 无对象操作使用 `AccessTarget::Operation`，仅接受统一定义中的无对象操作。
- 创建使用服务端确认的候选归属；归属变更同时检查原对象操作资格和目标归属。候选事实不得替代已有记录的当前事实。
- 任务操作必须提供所需的精确任务身份及版本，不能只提供关联业务 ID。
- 对象及来源事实展开最多 1000 项、最多 8 层，超限失败，不截断。
- 写入前在业务事务内执行检查，授权读取、业务读取和写入使用同一执行器。状态、金额、业务版本、幂等及所需并发冲突保护由业务用例负责。

## 列表查询

```rust
use erp_permissions::sales_inquiry;

let access = authorization
    .query(
        actor,
        &sales_inquiry::QUERY_SALES_LIST,
        executor,
    )
    .await?;

// 仓储检查凭证绑定，并在分页、计数和汇总前执行完整授权过滤。
let page = repository.list(&filter, &access, executor).await?;
// 返回前由当前读取流程统一复验。
authorization.validate(&access, executor).await?;
```

`query` 根据权限取得 identity 固定规则和已绑定的 Mapping，生成并编译完整授权条件。领域公共读取方法统一管理查询生成、仓储执行和返回前复验；业务调用不取得或传入 Mapping，也不选择查询模型。执行前只检查凭证绑定、执行器及有效期，不重复执行完整授权复验。

业务筛选只收窄候选集合，不能先用部分权限条件排除记录。列表、总数、汇总和导出使用同一完整授权条件。

`CompiledAccess` 不可由业务构造或修改，绑定操作、查询模型、共享服务、身份依赖及执行边界，只用于当前读取阶段。边界区分同一会话的不同事务及同一事务的不同读取阶段；写入尝试、新查询和交付完成使旧凭证失效。仓储执行前调用公开的 `ensure_current` 核对边界与有效期。

跨页游标使用 `CompiledAccess::version()`，包含身份和业务映射依赖。复验发现账号停用、权限撤销、版本变化或有效期到达时返回 `DATA_SCOPE_CHANGED`，丢弃结果并重新查询；数据库和接入错误保留原类别。

静态 Mapping 只描述字段和关系，不提供空的动态准备结果。确有查询外动态依赖时，才按需准备并记录实际依赖。

记录授权在数据库内按有效来源关联，不预加载账号全部授权 ID。关联采用存在性查询，不增加根记录数量。事务读取以原事务快照为准，交付检查核对事务边界、读取阶段和有效期，不重读同一快照。非事务读取复验身份、范围及实际动态依赖；动态准备缺少可靠修订号时只允许事务路径。

## 权限定义与事实装配

`erp-permissions` 提供权限值、内置常量、资源标识和公共目录。server 补充 HTTP 路由及接口说明。identity 不公开登记政策、选择授权主体或独立编译查询的接口。业务调用使用本文 API。

identity 按权限执行固定函数，检查功能资格、数据范围和必要记录关系。server 的 `wiring::authorization::build(db, rbac)` 创建真实 Facts 和 Mapping，并返回 `Result<Arc<AuthorizationService>>`。Builder 只绑定实现，不声明政策。

使用 `AuthorizationService::builder(db, rbac)` 开始装配，依次调用 `facts(resource, facts)?` 和 `mapping(&permission, mapping)?`，最后调用 `build()?`。全部固定数据规则所需资源必须有 Facts，每个支持查询的权限必须有唯一 Mapping；仅功能权限操作不需要对象或查询绑定。缺失、重复、未知引用、事实能力不足、来源资源或输出粒度错配均返回接入错误。不得用空实现补齐生产绑定。

Mapping 的 `model()` 和 `relations()` 必须提供不依赖请求或动态准备的描述。Builder 校验后冻结根及来源模型；`relation()` 必须返回启动时登记的同一个来源实例。多个权限或关系可以复用同一个实现实例，动态准备仍只按当前读取阶段缓存。

| 位置 | 职责 |
| --- | --- |
| `erp-permissions` | 权限值、常量、资源标识和公共展示目录 |
| identity | 固定授权规则、范围解析、对象判断与查询转换 |
| server 的 `wiring/authorization/` | 连接身份设施、Facts 和 Mapping |
| 业务 Facts 与 Mapping | 读取真实事实，提供查询字段及来源关系 |
| 应用入口 | 构造共享实例并注入业务服务 |

事实读取器可以由 repository 直接实现，也可以组合本域仓储；跨域事实通过窄 Port 获取。事实读取器不判断权限，不依赖需要授权服务的业务 Service，不保存请求对象。

要求同角色的入口及额外权限必须由同一个启用角色提供；独立来源资格分别取证。身份判断使用同次账号、策略及角色版本。

独立入口分别证明功能资格，并只用授予各自入口的有效角色解释范围。不同独立入口允许由不同角色满足。同资源的独立读取必须与根操作范围求交，不能替代修改等根操作范围。查询编译和对象判定使用相同动作上下文，交付前复验全部身份、映射版本和有效期。

文件材料转授先检查账号有效性，再检查有效上传者或通用预览资格，不要求额外的转授权限。此政策不能免除已有业务资源的入口权限。文件所有者事实必须完整读取，预览分支不能掩盖事实缺失。

后台任务中心按本人发起或启用管理员角色授权。全部取消在每个写事务内复验入口权限与管理员角色；角色停用、设施读取失败不得降级为普通账号后继续处理。

只做对象检查时不执行查询准备和编译。任一来源命中的政策只允许用于受限只读视图，不能用于完整对象检查。

## Mapping

| Mapping 能力 | 职责                                                                       |
| ------------ | -------------------------------------------------------------------------- |
| `relations` | 提供固定关系及来源 Mapping，供 Builder 校验，不读取动态事实 |
| `model`      | 固定模型、资源、集合、行主键、输出粒度和支持的事实维度                     |
| `key`        | 返回授权资源 ID 表达式，供记录授权和来源连接使用；支持字段引用及计算表达式 |
| `preparation` | 默认返回 `None`；只有查询外动态数据才提供 `MappingPreparation` |
| `records`    | 限定实际存在且未删除的模型记录；根查询、来源关联均应用                     |
| `predicate`  | 将一个 `AccessFact` 转成字段条件或关联存在性查询                           |
| `relation`   | 提供完整来源 ID 表达式及来源 Mapping                                       |

静态 Mapping 的转换方法接收 `None`，不执行准备。动态 `MappingPreparation::prepare` 经原执行器返回本次 `MappingFacts`；其中 `data` 保存条件转换实际使用的数据，`version` 覆盖相应写入，`valid_until` 限定最早到期时点。非事务路径必须提供可靠修订号，交付前重新准备并比较；事务路径不保存这些复验数据，仍保留摘要及到期边界。

同次查询编译中，同一个 Mapping 实例在完整 `AccessContext` 相同时只准备一次；不同实例、身份、权限或时点分别准备。非事务交付按唯一依赖逐项复验，缓存不得跨查询或读取阶段。同一集合管道内，集合、变量绑定和子管道完全相同的关联只执行一次；来源子管道使用独立别名作用域，完整来源检查与最终逻辑过滤保持不变。

非事务授权读取使用 `ReadExecutor`，统一事务回调使用设施签发的执行边界。原始 `ClientSession` 和 `NoTransaction` 不提供可核验的授权读取阶段。仓储使用 `mongo_ops`、`aggregate` 或通用 Repository；直接取得原始事务会话后，不再允许复用该执行器的授权凭证。

所有者事实在销售单中映射到负责销售字段，在客户中映射到有效负责人分配。字段不同，语义相同。Mapping 不决定权限条件的与或关系、个人上限或是否接受记录授权。

`QueryOutput::Object` 表示完整对象。`Share` 和 `Existence` 必须登记为独立只读视图，编译器执行字段投影。份额模型每行只对应一个来源，金额只能来自该份额；存在性视图的 `fields` 必须且只能包含 `id_field`，登记金额或内容字段时 Builder 返回错误。受限视图不能用于 `require` 完整对象检查。查询调用不通过选择模型切换授权政策或输出份额；Mapping 由启动绑定固定，identity 校验该权限允许的输出粒度。

查询能力由 identity 按权限固定声明，不根据动作名称推断。复用写权限的查询仍须绑定 Mapping；支持查询不免除该权限执行 `require` 时的快照事务要求。采购任务内部资料的 Mapping 必须支持指派、完成标记和参与事实，查询与精确任务检查使用相同条件。

identity 固定规则指定根对象及来源的必需事实，Mapping 声明支持的维度。对象字段使用 `FactValue`：`Missing` 表示未读取；`Known(None)` 或已知空集合表示确认没有值。必需事实缺失时返回接入错误，其他授权分支命中不能掩盖缺失。

Facts 返回的根对象和各层来源对象按资源与身份视为无序集合。非事务复验忽略排列变化，仍比较完整事实、版本和来源引用；缺失、重复或额外对象按原校验规则拒绝。

查询缺少映射、对象检查缺少事实端口、未知事实、资源不匹配或不支持维度均返回错误，不能降级成全量查询。Mapping 与事实端口执行相同责任定义，并验证对象判定与列表成员判断一致。

## 授权规则

1. 账号和同角色完整操作权限先于数据判断。
2. 历史参与和记录读取补充由identity 固定规则确定，默认关闭。基础数据范围为有效条款或已开启的历史参与，再与个人上限求交。
3. 销售单、采购单的列表和详情开启记录读取补充后增加有效记录授权分支；该分支不放行原单据修改、提交。
4. 独立范围继续与读取结果求交。
5. 来源业务默认检查入口权限及实际来源数据资格。独立来源入口资格由identity 固定规则确定，不由调用方临时追加。
6. 任务可见、资料可读、任务可办理分别定义；业务状态不由授权服务判断。
7. 公司共享资源仍检查账号和操作权限，不读取无关的人员范围或记录授权。

## 人员范围与权限检查

人员范围目录保留 `resource`、`actions`、`configurable_actions`、`dimensions`、`default_self_actions`、`default_self` 和 `policy_description`。接口不返回 `authorization_policy` 总分类。

- 前端只允许编辑 `configurable_actions` 中的动作；`dimensions` 为可配置动作支持维度的并集，服务保存时仍逐动作校验支持范围类型、支持维度和必需维度。
- `default_self_actions` 标识未配置时采用本人基础的可配置动作；汇总 `default_self` 只在全部可配置动作均有本人基础时成立。
- 公司共享、来源范围复用、纯任务或仅功能资格的操作不接受独立人员范围。历史记录保留为退役项，不得直接重新启用。
- `policy_description` 按动作解释实际能力，只作说明，不作为授权或编辑条件。
- 管理员通过 `AccessInspectionService` 发起检查。`steps` 分别返回 `passed`、`blocked` 或 `review`；缺少具体对象、任务或来源上下文的条件保持 `review`，不得据此报告对象授权通过。
- 具体单据检查沿用销售单 `detail`、`update` 的现有范围；检查结果不作为下一次业务请求的授权凭证。

## 管理记录读取授权

人工授权使用 `ResourceGrantService`：`grant` 创建或更新，`revoke` 永久撤销一个来源，`sources` 查询账号的授权来源。这些方法会检查管理员权限。

自动授权在业务事务内调用 `append_event`。事件中的接收账号必须来自实际任务分配，不能由客户端自选：

- 销售单、采购单审批分配分别使用 `SalesOrderApprovalAssigned`、`PurchaseOrderApprovalAssigned`。
- 开票最终审批通过并创建执行任务后，使用 `InvoiceApproved`。
- 开票任务转派使用 `InvoiceTaskReassigned`。

各事件必填字段见 [GrantEventPayload](src/entity/resource_access/event.rs)。`append_event` 必须加入业务事务：新事件返回 `true`，相同内容重放返回 `false`；内容非法、幂等冲突或写库失败时，业务事务也必须失败。

`erp-worker` 调用 `erp-resource-grants` 处理事件，再通过 `ResourceGrantService::apply_source` 写入授权。写入前会通过 `ResourceGrantFacts` 重新检查业务数据。事件已追加不代表授权已生效。

## 代码位置

| 位置                                                       | 内容                                                              |
| ---------------------------------------------------------- | ----------------------------------------------------------------- |
| [service/auth/](src/service/auth/)                         | 后台登录、密码检查                                                |
| [service/iam/](src/service/iam/)                           | 账号、角色、Casbin 权限                                           |
| [service/organization.rs](src/service/organization.rs)     | 组织和成员管理                                                    |
| [service/portal/](src/service/portal/)                     | 供应商门户账号和登录                                              |
| [service/access_control/](src/service/access_control/)     | 数据范围配置、解析和检查                                          |
| [service/resource_access/](src/service/resource_access/)   | `ResourceGrantService` 管理授权；`GrantEventService` 处理事件队列 |
| [service/authorization/](src/service/authorization/)       | 统一操作、对象、查询与复验                                        |
| [repository/authorization/](src/repository/authorization/) | 查询编译及不可伪造的查询结果类型                                  |
| [entity/](src/entity/)                                     | 数据结构和不访问数据库的判断规则                                  |
| [repository/](src/repository/)                             | MongoDB 读写和查询条件                                            |
| [ports/](src/ports/)                                       | 由外部实现的业务查询和审计接口                                    |
| [dto/](src/dto/)                                           | 请求和返回数据                                                    |
| [indexes.rs](src/indexes.rs)                               | MongoDB 索引                                                      |

## 测试

在 `backend/` 下执行：

```bash
env -u ERP_TEST_MONGO_URI cargo test -p erp-identity --lib --locked
```
