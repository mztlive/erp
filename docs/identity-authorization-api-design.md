# identity 统一权限 API 设计

状态：设计合同。identity 内部提供下列接口；业务 Mapping、组合根装配和入口迁移分别验收。第 10 节包含内部单元验证与业务发布验收，不能互相替代。

业务权限检查统一使用 `AuthorizationService`。操作和对象检查使用 `require`，列表、计数、汇总和查询式导出使用 `query`。`query` 返回固定权限语义的 `AuthorizedQuery`，业务 Mapping 将其中的事实条件转换为本域查询，公共编译器组合完整授权条件。

本合同细化 [ERP 统一权限与行级授权改造](resource-authorization-design.md) 的业务调用接口。

外部业务调用只保留两个授权入口。业务不再根据是否启用数据范围、记录授权选择不同服务或结果类型。

| 调用目的 | API | 结果 |
| --- | --- | --- |
| 执行操作，检查单条或批量对象 | `require` | 成功或错误 |
| 查询列表、计数、汇总或导出 | `query` | `AuthorizedQuery` |
| 仓储执行授权查询 | `AuthorizedQuery::compile`、`validate` | 查询阶段及复验结果，由公共执行流程使用 |

`AuthorizedQuery` 的含义固定：描述本次账号在指定入口和查询模型下可以访问哪些记录。它不向业务暴露数据库条件。业务 Mapping 提供模型信息，将权限事实转换为字段条件或关联查询；identity 决定这些条件如何组合。

## 1. 产品规则

### 1.1 操作资格与数据资格

所有检查先证明账号有效、存在授予本次操作权限的有效角色。需要多个权限时，按登记要求由同一个有效角色同时满足，禁止用多个角色各自的部分权限拼接证明。

业务要求多个独立入口时，使用服务端固定的 `AccessPolicy::Independent { permission, policy }`。每个子树分别证明其入口资格，范围只采用授予该入口的有效角色；不同子树允许由不同角色满足。每个独立子树必须包含自身动作的 `Scope`。同资源的独立读取范围不能替代根写操作范围；根政策已声明当前动作范围时，包含独立入口的每个放行分支仍须受该范围约束。独立入口动作上下文、身份与映射版本、有效期必须随原凭证编译和复验。

| 场景 | 放行条件 |
| --- | --- |
| 无对象范围的操作 | 账号有效且具备功能权限 |
| 普通业务数据 | 账号有效、具备功能权限且对象命中数据范围 |
| 支持指定记录授权的读取 | 账号有效、具备功能权限，且命中数据范围或有效记录读取授权 |
| 原单据创建、修改、提交 | 检查该写操作的范围；记录读取授权和历史读取参与不参与放行 |
| 来源业务操作 | 检查入口功能权限及真实来源的数据资格 |
| 任务操作 | 检查该动作登记的分配、参与或管理资格；业务用例另行检查任务状态 |

普通数据范围按以下规则组合：

```text
基础范围 =（任一有效范围条款 OR 本动作允许的历史参与）AND 个人上限
读取范围 = 基础范围 OR 本动作允许的有效记录读取授权
最终数据范围 = 读取范围 AND 本操作要求的其他独立范围
```

没有个人上限时，该交集项为真。动作不接受历史参与或记录授权时，对应分支为假。空范围为假；只有明确的公司共享或公司范围政策可以生成无限制条件。

每条范围条款继续采用 `ScopeClause::covers` 的语义：负责、协作和组织条件取并集，再与适用的结算主体、仓库条件取交集。未启用的维度不参与交集，只有仓库等维度的条款可以独立匹配；完全空条款为假，公司条款为真。

### 1.2 资源规则

| 资源或用途 | 规则 |
| --- | --- |
| 客户 | 责任事实取当前有效的客户分配，不能把客户主档创建人当作负责人 |
| 销售单 | 采用销售单责任范围；列表、详情可纳入有效记录读取授权 |
| 采购单、商品、供应商等共享资源 | 由登记的共享政策提供公司范围；仍须证明账号和操作资格 |
| 合同 | 复用客户范围配置，使用合同自己的责任事实 |
| 选品方案 | 使用实际所属选品册的责任范围 |
| 财务来源 | 使用实际关联的销售、采购等来源，不额外要求来源详情入口的功能权限 |
| 人员、仓库候选 | 范围只控制候选列表，不授予关联业务的数据或操作权限 |
| 本人待办 | 先应用 WorkItem 自身范围，再限定本人负责、开放且未删除；来源单据权限和授权就绪状态不影响本人待办计数 |

任务可见、资料可读和任务可办理分别检查。已有任务与来源规则通过服务端登记表达，不能统一替换为普通人员范围。

### 1.3 完整对象与授权份额

1. 单条和批量命令检查本次完整目标。任一必要对象不存在、事实缺失或无权，整次检查失败。
2. 来源集合由服务端沿实际业务关联取得。客户端不能删去无权来源后提交剩余部分。
3. 财务列表允许按授权份额展示时，查询记录必须对应份额；金额汇总仅包含这些份额。
4. 读取一个来源不能放行整个多来源单据。完整详情、下载和写命令按登记规则检查全部必要来源。
5. 状态、金额、业务版本和幂等校验由业务用例执行。`require` 成功只表示本次授权检查通过。

## 2. 服务端登记

以现有 `AuthorizationPolicy` 和消费者登记为基础，合并资源动作的授权合同。

| 登记项 | 内容 |
| --- | --- |
| 入口 | 资源、动作及额外必需权限；复用权限目录中的 `Permission` |
| 目标要求 | 无对象操作或已有对象；列出允许的调用形态 |
| 数据政策 | 公司共享、人员范围、来源范围、任务资格或管理范围 |
| 范围配置来源 | 从哪个资源动作读取范围配置 |
| 对象事实来源 | 使用当前对象还是指定关系中的对象事实 |
| 读取补充 | 是否接受历史参与、指定记录读取授权 |
| 来源关系 | 关系标识、根资源、来源资源、是否允许为空及集合判定方式 |
| 查询模型 | 查询行的身份、资源、粒度及 Mapping 标识 |
| 必需事实 | 所需组织、结算主体、仓库、责任或任务事实 |

登记规则执行以下要求：

1. 权限码和来源关系由服务端确定，不接受客户端选择授权政策。
2. 缺少登记或必需事实时拒绝执行。查询要求 Mapping，对象检查要求事实端口；只做对象检查不要求 Mapping。
3. 需要对象检查的动作不能使用无对象目标调用 `require`。
4. 同一操作在不同业务视图需要不同来源规则时，按入口和已登记的查询模型区分。
5. 范围配置来源与对象事实来源分别登记。合同使用客户配置，不代表检查客户对象。
6. 按调用形态登记对象事实端口或查询映射；同时支持两者时，补充对象判定与查询成员判断的等价性用例。

读取补充按每个范围节点登记，默认关闭。资源动作的既有能力只限定可开启的范围，不代替入口选择。

```rust
pub struct ReadAccess {
    pub history: bool,
    pub records: bool,
}

// AccessPolicy 的范围节点
Scope {
    permission: Permission,
    read: ReadAccess,
}
```

`history` 仅允许登记到支持历史参与的列表、详情范围；`records` 仅允许销售单、采购单的列表、详情范围。写范围不能开启读取补充。不同入口、视图和来源节点分别选择开关，HTTP 参数不能覆盖。

`AccessRegistration.required_facts: BTreeSet<FactKind>` 声明根对象必需事实；`AccessPolicy::Related.required_facts` 声明该来源对象必需事实。对象检查校验事实已读取，查询编译校验对应 Mapping 声明了这些维度。必需事实不因公司范围或其他放行分支成立而省略。

## 3. 业务 API

以下签名为目标合同。类型构造、文档注释和错误转换按仓库 Rust 规范实现。

```rust
impl AuthorizationService {
    pub async fn require(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        target: AccessTarget,
        executor: &mut dyn Executor,
    ) -> Result<()>;

    pub async fn query(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        subject: AccessSubject,
        executor: &mut dyn Executor,
    ) -> Result<AuthorizedQuery>;
}
```

### 3.1 require

1. 解析登记规则，检查账号、有效角色及本次入口权限。
2. 在原执行器中取得对象事实，按同一授权时点解释有效关系。
3. 对已有对象检查完整目标；需要读取授权时，只查询本次目标的授权及来源。
4. 任一目标不满足时返回错误；成功返回 `()`，不产生可跨请求复用的放行凭证。

单条使用一个元素的目标集合。单次目标上限沿用 1000 项；空集合和超限请求拒绝，禁止静默截断。需要分批的大操作由业务入口保证完整集合及同一事务，任一批失败时整体失败。

### 3.2 query

1. 检查账号、有效角色、入口权限、查询主体和模型登记。
2. 解析范围、读取补充和来源规则，生成只读授权表达式。
3. 绑定账号、入口、资源、查询模型、授权时点及身份授权版本。
4. 不预加载账号全部授权 ID，不把某批目标的授权结果转换成列表范围。
5. 无功能权限返回错误；具备功能权限但数据范围为空时，返回空查询约束。

只加载本次政策需要的事实。无范围的操作不加载组织范围；已证明无需范围过滤的共享查询不增加无用的记录授权关联。

业务调用：

```rust
let access = authorization
    .query(actor, &permission, AccessSubject::Direct, executor)
    .await?;

let page = sales_orders.list(&filter, &access, executor).await?;
```

`filter` 只包含业务筛选。仓储使用登记的销售单 Mapping 编译 `access`，完成授权查询和复验后返回结果。

### 3.3 功能资格查询

HTTP 入口和按钮展示可以查询功能资格。该结果不包含对象授权，不能替代业务用例的 `require` 或 `query`。底层 Casbin 的 `enforce` 不作为业务完整授权入口。

```rust
pub async fn has_permission(
    &self,
    actor: &AuditActor,
    permission: &Permission,
    executor: &mut dyn Executor,
) -> Result<bool>;
```

账号无效或没有启用角色授予该权限时返回 `false`。通配入口、策略及数据库错误返回错误。该方法只查询单个功能资格；业务完整权限要求由 `require`、`query` 按登记检查。

## 4. 数据类型

### 4.1 日常调用类型

| 类型 | 内容与约束 |
| --- | --- |
| `Permission` | 复用现有类型，表示实际入口权限 |
| `AccessSubject` | 直接资源或已登记的来源关系与查询模型，不携带任意集合名和字段名 |
| `AccessTarget` | 无对象操作或已有记录集合 |
| `ResourceTargets` | 复用现有目标 ID 值对象；校验格式、去重和数量 |
| `AuthorizedQuery` | identity 生成的只读查询约束，禁止业务代码构造、修改和反序列化 |

```rust
pub enum AccessSubject {
    Direct,
    Source(RelationKey),
    View(QueryModelKey),
}

pub enum AccessTarget {
    Operation,
    Records {
        subject: AccessSubject,
        ids: ResourceTargets,
    },
}
```

查询时，`Direct` 使用入口资源的默认模型，`Source` 使用登记关系的模型。纯对象检查按相同主体定位事实端口，不要求查询模型。`View` 用于同一入口的其他查询粒度，必须登记所需来源与判定规则。`RelationKey`、`QueryModelKey` 使用服务端登记值，非法组合在授权入口拒绝。

`Records.ids` 表示登记主体的根对象。来源检查由事实端口加载根对象的完整实际关联，不能将客户端传入的来源 ID 当作已验证关联。

不得接受客户端直接提交的 `owned=true`、`authorized=true` 等授权结论。

### 4.2 AuthorizedQuery

字段由 identity 私有持有：

| 字段 | 含义 |
| --- | --- |
| `actor` | 本次账号身份 |
| `permission` | 实际入口权限及已证明的额外权限要求 |
| `subject` | 已解析的根资源、来源规则和查询模型 |
| `rule` | 固定语义的授权表达式 |
| `as_of` | 责任、组织和参与关系的解释时点 |
| `snapshot` | 账号、角色、策略、组织、范围配置及所用读取授权的版本 |
| `valid_until` | 已知关系的下次生效或到期边界；到达边界后重新解析 |

该类型只用于当前授权阶段，使用原执行器，不得跨请求缓存。它不包含业务筛选、分页参数或全部可见对象 ID。只提供编译、读取版本等必要方法，不提供拆出基础范围后绕过其他分支的业务 API。

### 4.3 授权表达式

表达式在 identity 内构造。公共编译器处理组合节点，Mapping 只处理业务事实及关系。

```rust
enum AccessRule {
    Allow,
    Deny,
    All(Vec<AccessRule>),
    Any(Vec<AccessRule>),
    Fact(AccessFact),
    Required {
        facts: BTreeSet<FactKind>,
        rule: Box<AccessRule>,
    },
    RecordReadGrant(ResourceKind),
    Related {
        relation: RelationKey,
        resource: String,
        mode: RelationMatch,
        rule: Box<AccessRule>,
    },
}
```

`AccessFact` 使用以下固定类型。本人、协作和参与条件中的账号取自 `AccessContext`，组织等集合由 identity 解析。Mapping 不得替换账号或扩大集合。示例中的 `String` 表示已有 ID 类型的序列化值。

```rust
pub enum AccessFact {
    Owned,
    Collaborating,
    OrganizationIn(BTreeSet<String>),
    SettlementPartyIn(BTreeSet<String>),
    WarehouseIn(BTreeSet<String>),
    HistoryParticipant,
    AssignedTo,
    Participant,
}
```

`RelationMatch` 只接受下文定义的 `Any` 和 `AllNonEmpty`。这些类型供授权实现和适配层使用，不作为业务请求参数。

| 节点 | 含义 | 处理方 |
| --- | --- | --- |
| `Allow` / `Deny` | 当前授权分支为真或假 | 公共编译器 |
| `All` / `Any` | 子条件全部或任一成立 | 公共编译器 |
| `Required` | 先校验该对象的必需事实，再判定子规则 | 对象判定器与公共编译器 |
| `Owned` | 当前有效负责人为本次账号 | Mapping |
| `Collaborating` | 存在有效协作关系 | Mapping |
| `OrganizationIn` | 责任组织命中集合 | Mapping |
| `SettlementPartyIn` | 结算主体命中集合 | Mapping |
| `WarehouseIn` | 仓库命中集合 | Mapping |
| `HistoryParticipant` | 存在符合该动作政策的历史参与事实 | Mapping |
| `AssignedTo` / `Participant` | 当前任务分配或登记的参与关系成立 | Mapping |
| `RecordReadGrant` | 当前账号具有目标记录的有效读取授权 | identity 授权查询编译器 |
| `Related` | 对登记关系中的来源对象应用子规则 | 公共编译器与关系映射 |

范围配置继续使用现有 `ResolvedScope` 和条款语义，由 identity 转成表达式。业务不得自行组装 `All`、`Any` 或添加读取授权节点。

`Related` 使用登记的集合规则：

- `Any`：至少一个实际来源存在且满足子规则，只用于允许按来源份额或存在性读取的模型。
- `AllNonEmpty`：完整来源集合非空，所有引用均能解析且全部满足子规则，用于完整对象检查。

存在性读取不得输出其他来源的内容和金额。缺失来源不能从集合中删除后继续判断；全量条件不能用非空子集代替。

### 4.4 适配层类型

| 类型 | 内容 |
| --- | --- |
| `AccessContext` | 账号、入口、资源、授权时点及可选模型；纯对象检查可以没有模型 |
| `QueryModel` | 模型标识、根资源、集合、行主键字段、输出形态及支持的事实维度；每个模型使用固定查询粒度 |
| `QueryOutput` | 完整对象、单来源份额或存在性视图；受限视图声明输出字段 |
| `MappingFacts` | 本次准备的有界事实集合、修订号及下次有效期边界 |
| `ObjectFacts` | 根对象身份、版本、责任事实、任务事实及完整来源关系；由事实端口返回 |
| `FactKind` | 可登记的责任、组织、结算主体、仓库、历史参与和任务事实维度 |
| `FactValue<T>` | 区分未读取的事实与已读取的值；已读取值可以为空 |
| `QueryFragment` | 事实条件所需的关联阶段与最终布尔条件 |
| `RelationMapping` | 根记录与来源记录的连接方式、来源 Mapping 和完整性检查方式 |
| `CompiledAccess` | 编译后的授权阶段、绑定的模型和完整复验信息；只交给仓储执行流程 |

适配层类型不进入 HTTP DTO。公开路径按业务调用与仓储接入分开，普通业务 Service 不需要处理查询片段。

MongoDB 适配使用以下片段类型。`Match` 只表示一个事实条件，公共编译器负责将其放入完整授权表达式；`Exists` 表示关联集合中是否存在符合条件的记录。

```rust
pub enum QueryFragment {
    Match(Document),
    Exists {
        collection: String,
        bindings: Document,
        pipeline: Vec<Document>,
    },
}

pub struct RelationMapping {
    pub expected_ids: Bson,
    pub mapping: Arc<dyn AccessMapping>,
}
```

`expected_ids` 为根记录全部实际来源 ID 的数组表达式。来源 Mapping 只翻译来源事实，是否要求任一或全部来源由登记规则决定。

```rust
pub enum QueryOutput {
    Object,
    Share { fields: BTreeSet<String> },
    Existence { fields: BTreeSet<String> },
}
```

`QueryModel.output` 固定输出形态。`Share`、`Existence` 只能登记为 `View`，字段集合必须包含行主键。编译器在授权过滤后执行字段投影；`Share` 的任一来源条件还要求每行仅有一个来源。份额金额必须来自该行对应来源；存在性视图不得登记其他来源内容或完整单据金额。完整对象模型禁止 `Related::Any`，受限视图不能用于 `require`。

## 5. Mapping 和对象事实

### 5.1 接口职责

查询接入实现 `AccessMapping`，已有对象检查实现 `AccessFactsPort`。两者使用同一套业务责任定义。接口定义在 identity，业务模块实现，组合根装配。

| 接口能力 | 输入 | 输出 |
| --- | --- | --- |
| `AccessMapping::model` | 无 | 登记的模型、根资源及查询行粒度 |
| `AccessMapping::key` | `AccessContext`、本次准备事实 | 当前记录的资源 ID 表达式，供记录授权及来源连接使用 |
| `AccessMapping::prepare` | `AccessContext`、原执行器 | 本次映射需要的事实、版本和有效期；无额外事实时返回空事实集及固定映射版本 |
| `AccessMapping::records` | `AccessContext`、本次准备事实 | 存在且未删除的记录条件；根查询和来源关联均应用 |
| `AccessMapping::predicate` | 事实节点、`AccessContext`、本次准备事实 | `QueryFragment` |
| `AccessMapping::relation` | 已登记关系、`AccessContext`、本次准备事实 | `RelationMapping` |
| `AccessFactsPort::load` | 上下文、根目标集合、原执行器 | 实际存在的 `ObjectFacts`；identity 对比完整目标集合，缺失对象导致失败 |

Mapping 必须支持字段条件和关联存在性查询。需要准备额外事实时，在原执行器中按 `as_of` 读取，将事实版本和有效期边界加入 `CompiledAccess`。不允许读取后丢弃这些复验依据。

只做查询接入时实现 Mapping。对象检查需要的事实读取由事实端口承担，不要求 identity 读取其他领域集合。

`AccessRegistration` 固定入口、主体、额外权限、必需事实、`AccessPolicy`、可选 Mapping、可选事实端口，由 `AuthorizationRegistry::new` 校验后冻结。

- 只做查询：提供 Mapping。
- 只做对象检查：提供事实端口，Mapping 可以为空。
- 同时支持查询和对象检查：提供两者，使用同一套责任定义。
- 调用 `query` 时没有 Mapping，或调用对象检查时没有事实端口，返回接入错误。

存在 Mapping 时，`View` 标识必须等于模型标识。`Source` 必须对应根政策声明的关系。纯对象检查不执行 `prepare`、`records`、`predicate` 或 `relation`。

`QueryModel.facts: BTreeSet<FactKind>` 声明 Mapping 支持的事实维度。`QueryModel.id_field` 标识输出行，供投影和业务去重使用；`AccessMapping::key` 标识该行对应的授权资源。两者可以不同。例如份额行以份额 ID 为行主键，以来源单据 ID 为授权资源主键。资源主键表达式必须返回与授权存储、来源引用一致的标量 ID；可以使用字段引用或 `$toString` 等计算表达式。

Mapping 接口如下。一个固定查询模型对应一个登记实例，不按业务请求临时替换。

```rust
#[async_trait]
pub trait AccessMapping: Send + Sync {
    fn model(&self) -> &QueryModel;

    async fn prepare(
        &self,
        context: &AccessContext,
        executor: &mut dyn Executor,
    ) -> Result<MappingFacts>;

    fn key(
        &self,
        context: &AccessContext,
        prepared: &MappingFacts,
    ) -> Result<Bson>;

    fn records(
        &self,
        context: &AccessContext,
        prepared: &MappingFacts,
    ) -> Result<QueryFragment>;

    fn predicate(
        &self,
        fact: &AccessFact,
        context: &AccessContext,
        prepared: &MappingFacts,
    ) -> Result<QueryFragment>;

    fn relation(
        &self,
        relation: &RelationKey,
        context: &AccessContext,
        prepared: &MappingFacts,
    ) -> Result<RelationMapping>;
}
```

`ObjectFacts` 保存对象身份、业务版本、有效期、当前负责人、协作者、组织、结算主体、仓库、任务参与人及来源关系。每个来源关系分别保存完整引用 ID 集合和实际加载的来源对象；引用存在但对象缺失时保留引用，由 identity 判定失败。

责任和任务字段使用以下状态：

```rust
pub enum FactValue<T> {
    Missing,
    Known(T),
}
```

单值事实使用 `FactValue<Option<String>>`，多值事实使用 `FactValue<BTreeSet<String>>`。`Missing` 表示未读取，不能当作无负责人或空参与集合；`Known(None)`、`Known(BTreeSet::new())` 表示已确认没有值。默认值为 `Missing`。

登记的必需事实和表达式实际使用的事实缺失时返回接入错误，其他 `OR` 分支命中不能掩盖缺失。查询编译同时检查 `QueryModel.facts` 和实际 `predicate` 支持情况，不能丢弃不支持的维度。

事实端口只返回实际存在且未删除的对象，定义与 `records` 一致。根对象与来源总计最多 1000 项、关系最多 8 层；超限返回错误，不截断。

### 5.2 业务映射

| 权限事实 | 销售单 Mapping | 客户 Mapping |
| --- | --- | --- |
| 本人负责 | `sales_owner_user_id` 等于账号 | 存在有效且岗位为主负责人的 `CustomerAssignment` |
| 责任组织 | `business_org_unit_id` | 当前主负责人对应的组织关系 |
| 资源主键 | 销售单 `id` | 客户 `id` |
| 历史参与 | 由拥有领域提供的合法单据参与关系 | 客户合法历史参与关系 |

Mapping 不查询角色权限，不决定是否使用历史参与，不组合个人上限与记录授权。跨领域的事实和查询片段由拥有领域提供，经组合层接线，禁止借 Mapping 绕过领域数据访问边界。

以没有个人上限、历史参与及其他独立限制的销售单读取为例，identity 解析出以下语义条件：

```text
（本人负责 OR 责任组织属于集合 A）OR 有效记录读取授权
```

销售单 Mapping 只完成两项转换：

```text
Owned             → sales_owner_user_id == context.actor.id
OrganizationIn(A) → business_org_unit_id IN A
```

identity 生成记录授权关联，并将三个分支按 `OR` 组合。客户 Mapping 对同一个 `Owned` 节点生成有效负责人分配的存在性查询。两个模块返回相同结构的片段，字段和关系可以不同。

新增业务字段不要求修改 identity。新增“本人负责”等既有语义的实现，只修改对应 Mapping 和事实端口；确实新增权限语义时，才扩展 `AccessFact`、对象判定器和编译验收用例。

### 5.3 编译规则

```rust
impl AuthorizedQuery {
    pub async fn compile(
        &self,
        mapping: &dyn AccessMapping,
        executor: &mut dyn Executor,
    ) -> Result<CompiledAccess>;
}
```

`compile` 调用 Mapping 的事实准备和条件转换，将所用业务版本、有效期与身份授权快照绑定到 `CompiledAccess`。

跨页游标使用 `CompiledAccess::version()`，包含身份和映射依赖。

Mapping 由组合根以 `Arc<dyn AccessMapping>` 登记，仓储向 `compile` 传入同一实例的借用。`CompiledAccess` 保留登记实例、准备事实和资源主键表达式，供执行前后复验，不接受仅名称相同但实现不同的 Mapping 替换。

1. 编译前检查账号上下文、资源、入口、查询模型和 Mapping 是否一致。
2. 事实节点生成 `QueryFragment`；公共编译器按表达式组合其最终条件。
3. 片段中的关联阶段只能准备事实，不能提前排除其他授权分支可能放行的根记录。
4. 记录授权查询由 identity 生成，校验账号、资源、读取动作、有效来源、来源版本和目标成员关系。Mapping 通过 `key` 提供当前记录的资源 ID 表达式；来源连接也使用该表达式，不能硬编码字段名。
5. 关联采用存在性判断或等价的去重查询，不能因多个授权来源重复计算业务记录。
6. 缺少所需映射、未知节点、模型不匹配或不支持的维度返回错误，禁止降级为无限制。
7. 空集合编译为假；空表达式必须在授权解析时明确归一化，不能利用空 `All` 获得权限。
8. 临时字段使用编译器分配的名字，完成判断后清除，不进入业务 DTO。

## 6. 查询执行与复验

仓储共用以下执行顺序：

```text
校验查询模型并编译授权约束
    → 复验本次身份、权限和事实版本
    → 应用业务筛选
    → 准备授权事实与关联
    → 应用完整授权条件
    → 按业务记录身份去重
    → 排序、分页、计数或汇总
    → 复验并返回
```

业务筛选只能收窄候选集合，不能提前应用部分权限条件。依赖关联字段的业务筛选在相应关联准备后执行。分页、计数和汇总必须使用同一授权条件。关联读取不得使用全量授权 ID 或全量历史参与 ID 代替数据库查询。

仓储复验使用一个辅助入口：

```rust
impl AuthorizationService {
    pub async fn validate(
        &self,
        access: &CompiledAccess,
        executor: &mut dyn Executor,
    ) -> Result<()>;
}
```

`validate` 由共用仓储执行流程调用，复验以下内容：

1. 账号、启用角色和入口权限仍满足本次要求。
2. 策略、组织、范围配置、读取授权及参与关系版本符合本次快照。
3. Mapping 引用的业务事实版本、来源关系及有效期未失效。
4. 结果仍绑定原资源、入口和查询模型。

事实版本使用拥有领域提供的修订号或有界依赖版本，不得枚举全部业务对象计算版本。没有完整复验依据的查询必须使用同一事务快照，不能依靠非事务前后读取声称结果一致。

复验发现账号停用、角色或入口权限撤销、策略冲突、依赖版本变化或有效期到达时，统一返回 `DATA_SCOPE_CHANGED`。调用方丢弃本次结果并重新查询；初次授权仍按正常无权错误处理。数据库和接入错误保留原类别。禁止仅比较记录授权修订号后宣称完整授权有效。

读取一致性按执行器区分：事务读取以该事务可见的快照为准；非事务读取须保证前后复验通过。事务内重复读取修订号不代表已经看见其他事务的新提交，不得据此承诺撤权与读取返回的全局先后顺序。

写入前在业务事务内重新执行 `require`。授权读取、对象读取和写入使用同一执行器；需要防止授权变更与写入并发提交的入口，使用对应版本的冲突保护。仅持有查询结果或读取过版本不能代替写入保护。

## 7. 错误合同

| 情况 | 处理 |
| --- | --- |
| 账号无效、缺少有效角色或功能权限 | 拒绝访问 |
| 对象无权或必要来源不完整 | 整次对象检查失败；保留业务入口已有的不存在信息隐藏规则 |
| 功能权限成立但查询范围为空 | 返回空列表、零计数和零授权汇总 |
| 根目标为空、超限或 ID 非法 | 参数错误 |
| 未登记动作、查询缺少 Mapping、对象检查缺少事实端口、必需事实缺失、模型错配或不支持维度 | 配置或接入错误，停止执行 |
| 身份、权限或业务事实版本变化 | 范围失效，重新读取 |
| 已核验的合法自动来源尚未生成授权 | 沿用授权未就绪合同，不重放业务命令 |
| 数据库或事实端口失败 | 返回原错误类别，不转换为空授权或无限制 |

沿用现有应用错误分类和 HTTP 映射，不通过展示文案判断错误类型。普通无权不得自动解释为授权未就绪。

## 8. 代码归属

| 位置 | 改动 |
| --- | --- |
| `erp-identity/src/service/authorization/` | `AuthorizationService`、公共解析、对象判断及复验 |
| `erp-identity/src/entity/authorization/` | 请求目标、事实、查询模型及纯授权表达式 |
| `erp-identity/src/ports/` | Mapping、对象事实及版本读取合同 |
| `erp-identity/src/repository/authorization/` | `AuthorizedQuery`、`CompiledAccess`、MongoDB 编译器和读取授权查询组合 |
| 各业务领域的 `repository/` | 本域 Mapping、授权后的业务查询 |
| 各业务领域的 `service/` | 对象事实准备；状态和业务校验 |
| `erp-server/src/wiring/` | Mapping、事实端口和跨领域提供方装配 |

复用现有 RBAC、范围解析、读取授权存储和版本机制。迁移完成后，普通业务不再直接使用 `RbacService::enforce`、`DataScopeService`、`ResourceAccessService` 或手工拼接授权查询阶段。

`AuthorizedDataScope`、`AuthorizedResourceQuery` 和 `AuthorizedResourceAccess` 的中间职责收回 identity；对外查询结果使用 `AuthorizedQuery`。仓储自己的业务筛选 DTO 继续表达业务查询参数。

## 9. 迁移顺序

1. 实现统一登记、请求类型、授权表达式和 `AuthorizationService`，内部复用已有解析器。
2. 实现 Mapping 合同、对象事实合同、查询编译与完整复验流程。
3. 接入共享资源操作、客户对象检查、销售单列表和详情，覆盖无范围、关系范围和指定记录授权。
4. 接入财务来源、完整批量操作、任务及目录，验证来源集合和查询粒度。
5. 按业务入口切换列表、计数、详情、导出和写命令，删除该入口的旧判断与手工授权组合。
6. 关闭旧检查 API 的业务可见性，更新 README、依赖边界检查和调用清单。

每个入口只使用一套完整判定。旧结果与新结果不得取并集作为迁移兼容。未完成 Mapping 和验收的入口不得登记为已切换。

## 10. 验收

| 场景 | 必须结果 |
| --- | --- |
| 有记录授权但没有入口功能权限 | 拒绝 |
| 账号或授予权限的角色失效 | 拒绝 |
| 多个角色分别拥有部分必需权限 | 不能通过同角色完整权限检查 |
| 对象范围动作使用 `Operation` | 拒绝，不能省略对象检查 |
| 基础范围外的销售单有有效读取授权 | 列表、详情可读；修改和提交不因此放行 |
| 历史参与命中但个人上限不覆盖 | 历史参与分支不放行 |
| 多个有效记录授权来源 | 记录及金额只计算一次；撤销一个来源不影响其他有效来源 |
| 客户负责人转移或分配到期 | 对象检查与列表按同一有效责任规则变化 |
| 销售单约束传给客户 Mapping | 编译失败 |
| Mapping 缺少仓库等必需条件 | 编译失败，不能丢弃条件 |
| 必需事实未读取，其他分支已命中 | 接入错误，不能放行 |
| 已读取事实为空 | 按空值判断，不作为事实缺失 |
| 同一资源不同读取入口分别开启或关闭补充 | 只使用各自登记的历史参与和记录授权分支 |
| Mapping 资源主键使用计算表达式 | 记录授权和来源连接均使用该表达式 |
| 仅对象检查登记事实端口，没有 Mapping | 可以登记和执行对象检查；查询返回接入错误 |
| 已授权后账号停用或权限撤销 | 复验返回 `DATA_SCOPE_CHANGED` |
| 空范围或空目标 | 查询为空；对象命令拒绝 |
| 同一对象、同一授权及事实快照 | 对象判定与列表成员判断一致 |
| 财务操作有自身功能权限，无来源详情功能权限 | 按实际来源数据资格判断 |
| 多来源对象有缺失或无权来源 | 完整操作失败；份额读取仅输出已授权份额 |
| 多个来源关联、分页和金额汇总 | 授权先于分页和汇总，页数据、总数及汇总使用相同条件 |
| WorkItem 自身范围允许，本人任务的来源授权未就绪 | 本人待办仍显示并计数，资料读取单独返回就绪状态 |
| 查询期间版本变化或关系到期 | 不返回旧授权结果，按执行器的一致性合同处理 |
| 详情结果用于后续写入 | 写事务重新执行授权检查 |

开发阶段使用内联单元测试验证公共表达式、Mapping 编译和对象判定等价性。发布验收验证真实数据库的聚合结果、索引、分页、版本复验和写入冲突；单元测试不替代这些结果。
