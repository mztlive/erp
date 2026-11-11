# 授权服务改进

状态：待实施。本文规定目标接口及实施要求，代码示例为目标 API。系统架构与模块关系见 [ERP 权限系统设计](../../../../../docs/authorization-system-design.md)。

## 共享服务

删除 `AuthorizationRegistry`。需要业务授权的进程启动时调用 `erp-authorization-wiring` 的统一入口，完成事实读取器和查询映射的装配，构造一个进程内共享的 `Arc<AuthorizationService>`。

HTTP、worker 将实例保存在 `Application`，需要业务授权的 CLI 命令将实例保存在自身命令运行时。`HttpState` 通过 `Application` 使用该实例；业务 Service、Process 和 ReadModel 通过构造函数接收共享实例。`Application::authorization()` 只克隆 `Arc`。

共享服务只保存数据库句柄、身份设施、固定操作定义、事实读取器和查询映射。账号、事务、候选对象和授权结果按调用传入或生成，不保存在共享服务中。账号权限、数据范围和对象事实每次检查时读取。

## 统一操作定义

在 `erp-identity` 中统一定义操作标识、权限编码、授权方式、额外必需权限及必需事实。操作与 Mapping 的绑定由组合根装配。业务模块和权限目录引用同一份操作常量，删除业务模块自行维护的动作白名单及权限字符串拼接。

现有 `ConsumerSpec` 的动作和范围规格并入统一操作定义。普通对象操作按该动作的功能资格和有效数据范围检查；任务、来源继承、读取补充及独立入口资格由操作定义固定。业务调用不提交政策或临时追加权限组合。

`erp-server/permission_catalog/` 保留接口说明和展示分组，引用统一操作常量。构建生成器使用同一份定义生成前端权限；权限代码只定义一次。

`Permission::parse` 继续负责格式校验。授权入口还必须确认操作存在于统一定义中；未知操作返回接入错误。

## 集中装配

新增独立 library crate `erp-authorization-wiring`，位于 `backend/apps/erp-authorization-wiring/`，承担应用层授权装配职责。HTTP、worker 和需要业务授权的 CLI 命令通过 `erp_authorization_wiring::build` 构造授权服务。

`erp-authorization-wiring` 依赖 `erp-identity` 及提供事实读取器、查询映射的业务领域和 ReadModel，集中连接这些实现。授权操作定义、政策、判断、查询编译及服务构造校验由 `erp-identity` 实现。`erp-authorization-wiring` 不定义或追加授权政策，不启动 HTTP、worker 或外部连接器。

业务 Service、Process 和 ReadModel 依赖 `erp-identity` 的授权接口并接收共享实例，不得依赖 `erp-authorization-wiring` 或自行构造授权服务。`erp-identity` 不得反向依赖 `erp-authorization-wiring`。

```rust
pub fn build(
    db: Database,
    rbac: SharedRbacService,
) -> Result<Arc<AuthorizationService>> {
    let service = AuthorizationService::builder(db.clone(), rbac)
        .facts(
            Resource::SalesInquiry,
            Arc::new(SalesInquiryFacts::new(db.clone())),
        )?
        .facts(
            Resource::Customer,
            Arc::new(CustomerFacts::new(db.clone())),
        )?
        .mapping(
            &permissions::sales_inquiry::LIST,
            Arc::new(SalesInquiryMapping::new()),
        )?
        .build()?;
    Ok(Arc::new(service))
}
```

builder 只用于构造服务。`build()` 完成后，服务不提供注册、替换或删除接口，不再新增独立的注册表类型。

构造时必须检查：

- 同一资源的事实读取器唯一，每个查询操作绑定一个 Mapping，重复绑定拒绝。
- Mapping 与操作要求的资源一致，并支持操作要求的事实维度。

同一 Mapping 可以由多个查询操作复用。业务调用方不选择查询模型，不提供 `models` 常量或模型选择参数。Mapping 中的集合、主键和事实维度只作为内部查询编译及复验信息。

运行时缺少操作、Mapping 或必需事实返回错误。

初始化顺序为 repository、事实读取器、授权服务、业务服务。事实读取器不能依赖需要授权服务的业务 Service，避免循环构造。

## 事实读取与查询映射

保留 `AccessFactsPort`。拥有领域的 repository 可以直接实现；需要组合本域仓储时，由本域事实读取器实现。跨域事实通过窄 Port 获取，授权所需的跨域适配实现在 `erp-authorization-wiring` 中装配。

```rust
#[async_trait]
pub trait AccessFactsPort: Send + Sync {
    async fn load(
        &self,
        context: &AccessContext,
        ids: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ObjectFacts>>;
}
```

事实接口返回对象身份、版本、负责人、所属组织、任务指派及实际来源关系。接口不判断是否有权，不选择或组合授权政策。读取必须使用本次调用的 `executor`，不能保存某次请求的候选对象。

查询继续使用 `AccessMapping`。字段映射、有效记录及来源关系由业务实现；权限条件的与或关系、范围和读取补充由 identity 决定。`prepare` 每次通过当前执行器取得事实和版本，不能复用请求传入的预筛选授权 ID。

对象事实和 Mapping 使用相同的业务归属定义。销售询价负责人取真实负责销售字段，客户负责人取有效负责人分配。

## 业务 API

| 用途               | 目标方法                                       | 返回                                   |
| ------------------ | ---------------------------------------------- | -------------------------------------- |
| 单项功能资格       | `has_permission(actor, permission, executor)`  | `Result<bool>`                         |
| 批量功能资格       | `permissions(actor, permissions, executor)`    | `Result<Option<QualifiedPermissions>>` |
| 操作及对象检查     | `require(actor, permission, target, executor)` | `Result<()>`                           |
| 查询条件生成及编译 | `query(actor, permission, executor)`           | `Result<CompiledAccess>`               |
| 查询复验           | `validate(access, executor)`                   | `Result<()>`                           |

功能资格不替代对象授权。额外必需权限仍须由同一有效角色证明；固定独立入口按各自的有效角色检查。

删除 `with_permission` 鉴权中间件及路由包装调用。HTTP 层通过 JWT 认证身份，Handler 调用受保护的业务用例；每个受保护的公共业务用例通过 `query` 或 `require` 统一检查功能权限与操作要求的数据资格。所有执行路径必须在返回业务数据或执行写入前完成对应操作的授权。

只需功能权限的操作使用 `require` 和 `AccessTarget::Operation`。`has_permission`、`permissions` 保留用于功能资格查询和按钮展示，不作为 HTTP 主流程的另一条授权分支。CLI、worker 调用相同的受保护业务用例，不依赖 HTTP 中间件。

权限目录继续用于角色权限配置、前端权限生成及按钮展示，直接引用统一操作定义，不依赖路由鉴权中间件。

`require` 根据操作定义选择政策，通过已装配的事实接口读取完整目标，再统一判断。调用方只指定身份、操作和目标。

```rust
self.authorization
    .require(
        actor,
        &permissions::sales_inquiry::UPDATE,
        AccessTarget::record(inquiry_id)?,
        executor,
    )
    .await?;
```

`AccessTarget` 明确区分无对象操作、已有记录、创建候选和归属变更。任务操作还须携带该操作要求的精确任务身份及版本，不能只用关联业务 ID 替代任务目标。

已有记录从持久化数据读取当前归属。创建候选由业务模块从已验证输入生成服务端事实，校验目标归属；归属变更同时检查原对象操作资格和目标归属。候选事实不能从 HTTP 参数直接反序列化为授权凭证，也不能替代已有记录的当前事实。

`query` 根据统一操作定义取得该操作绑定的 Mapping，在服务内部生成并编译完整授权条件。业务代码不再取得或传入 Mapping，也不再调用 `AuthorizedQuery::compile`；`AuthorizedQuery` 收为内部类型。

领域公共读取方法封装以下流程：

```rust
let access = self.authorization
    .query(
        actor,
        &permissions::sales_inquiry::LIST,
        executor,
    )
    .await?;

// 仓储检查凭证绑定，并在分页、计数和汇总前执行完整授权过滤。
let page = self.repository.list(filter, &access, executor).await?;
// 返回前由当前读取流程统一复验，仓储不重复调用 validate。
self.authorization.validate(&access, executor).await?;
```

领域公共读取方法统一管理查询生成、仓储执行和返回前复验。编译后不立即重复执行完整授权复验；执行前只检查凭证绑定、执行器及有效期。返回前的 `validate` 按原事务快照或非事务完整依赖版本检查，不在业务层与仓储重复叠加。凭证延后使用或切换执行器时，重新取得相应读取阶段的授权。

授权条件在数据库内先于分页、计数、汇总和导出执行。查询结果绑定操作、Mapping、共享服务、身份与业务版本及执行器会话，只用于当前读取阶段。共享服务不缓存授权结果；复验失效时丢弃读取结果。

写入前在业务事务内完成授权，事实读取、业务读取和写入沿用同一执行器。对象版本、归属和授权变化必须通过当前版本复验及业务并发冲突保护处理。

## 实施顺序

### 第一阶段：完成 identity 新 API 和装配基础

- 删除被新 API 替代的旧类型和接口，包括 `AuthorizationRegistry`、`AccessRegistration`、业务侧政策构造入口及旧公开查询编译入口。
- 在 `erp-identity` 实现统一操作定义、Builder、构造校验、授权判断及查询编译。
- 新建 `erp-authorization-wiring`，建立装配入口和基础结构；同步 workspace 成员、`scripts/crate-layers.json`、依赖边界配置和 `crates/README.md`。

阶段通过条件：`erp-identity` 编译通过。`erp-authorization-wiring` 编译通过。 允许尚未迁移的下游 crate 编译失败，完整装配及其编译通过在第二阶段完成。 不保留旧接口兼容分支来维持中间状态的 workspace 编译。

### 第二阶段：逐 crate 接入 erp-identity 以及 erp-authorization-wiring 装配

- 将本 crate 的权限政策及判断迁入 identity，删除自行拼装政策、数据范围、额外权限的代码、重复动作白名单和临时授权服务工厂。保留业务状态校验、事实读取及对统一授权服务的调用。
- 如果可以用一个结构体实现两个trait，就不要分为Facts和Mapping两个对象， 例如： CustomerAccess

阶段通过条件：每个已迁移 crate 可以编译；执行对应的授权行为单元测试。第二阶段结束时，`erp-authorization-wiring` 完成全部装配并编译通过。受影响 crate 已通过 `cargo test --lib` 时，不重复执行该 crate 的 `cargo check`。 允许下游carte编译失败

### 第三阶段：完成应用入口迁移和全 workspace 验证

- 适配 handler、CLI、worker 和服务工厂，确认全部受保护业务入口已执行对应操作的 `query` 或 `require`，再删除 `with_permission`、RBAC 中间件及路由包装调用。迁移期间不得出现缺少业务授权的接口。
- 调整权限目录、前端权限生成器及路由合同检查，移除对 `with_permission` 调用形式的依赖；同步 `backend/AGENTS.md` 的 HTTP 授权规则。保留 JWT 认证和已有公开入口的认证规则。
- 各进程通过 `erp-authorization-wiring` 构造共享授权服务，由 `Application` 或 CLI 命令运行时持有，并完成业务服务的依赖注入。
- 对照迁移清单清理剩余旧接口调用、临时工厂和业务侧重复权限判断。
- 同步 `erp-identity` README、`erp-authorization-wiring` README 和根目录的统一权限 API 设计。
- 完成全 workspace 编译、授权行为单元测试及依赖边界、权限漂移检查。

阶段通过条件：全 workspace 编译通过，授权行为验证及相关门禁通过，并满足下列完成条件。编译通过不能替代权限行为验证。

## 完成条件

- 生产路径统一通过 `erp-authorization-wiring` 构造授权服务；各进程持有自己的共享实例，所有业务入口使用注入的实例。
- `erp-authorization-wiring` 只负责授权装配，政策、判断和查询编译集中在 `erp-identity`；业务领域、Process 和 ReadModel 不依赖装配 crate。
- 操作编码及政策统一定义，业务侧只提供事实、映射和操作目标。
- `with_permission` 鉴权中间件及路由包装调用全部移除；受保护业务入口通过 `query` 或 `require` 完成授权，只有功能权限要求的操作使用 `AccessTarget::Operation`。
- `AuthorizationRegistry`、`AccessRegistration` 及业务侧政策构造调用全部移除。
- 查询调用不传模型标识，Mapping 由操作定义固定；不保留通过模型选择不同授权政策或输出份额的接口。
- 对象、创建、归属变更、任务和来源查询全部使用目标 API，HTTP、worker 与需要业务授权的 CLI 入口无临时授权服务。
- 缺少事实、Mapping 与操作资源不匹配和重复装配均被拒绝；权限撤销、对象归属变化和任务改派按版本及事务约束处理。
- 行为测试覆盖功能资格与对象范围、同角色额外权限、独立入口、对象与查询成员一致性、创建和归属变更、任务及查询失效。
- README 和统一权限 API 设计与实际接口一致，全部原调用已迁移，无旧入口兼容分支。
