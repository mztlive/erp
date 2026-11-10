# erp-identity

负责账号登录、角色权限、组织成员和数据范围，也支持给指定账号授予销售单、采购单的读取权限。

本 crate 不依赖其他业务领域。客户归谁负责、订单分配给谁等信息，由业务代码提供。

## 检查权限

管理端按以下顺序检查：

1. 账号是否有效。
2. 是否有本次操作的功能权限。
3. 是否允许访问这条业务数据。

第三步按业务类型处理，不能由调用方随意省略：

- 客户、销售单等，按负责人、组织和人员配置的数据范围检查。
- 采购单、商品、供应商等，有对应操作权限的有效角色可以访问公司范围内的数据，不受旧人员范围限制。修改、审批等操作还要检查单据状态和任务分配。
- 合同复用客户的范围设置，但按合同自己的负责人和组织判断。选品方案使用所属选品册的范围。
- 任务操作检查任务分配、参与关系和管理权限。能查看任务不代表能办理任务。
- 人员、仓库等候选列表的可见范围，只用于选择候选项，不授予关联业务权限。

各资源的规则见 [authorization_policy.rs](src/entity/access_control/authorization_policy.rs)，已接入的动作见 [consumers.rs](src/service/access_control/consumers.rs)。范围解析会拒绝未登记的资源或动作。工作项管理等操作需要显式配置范围；有效超级管理员使用公司范围。

供应商门户使用 `PortalIdentityService`，检查账号绑定的供应商和门户岗位，不使用管理端的角色及组织范围。

### 只检查功能权限

在不需要检查数据范围的入口调用 `RbacService::enforce`。调用前先检查账号是否有效；该方法只做 Casbin 权限匹配。

```rust
use erp_identity::service::iam::subject;
use erp_identity::Error;

let sub = subject(actor.kind(), actor.id());
if !rbac.enforce(&sub, &entry_permission).await? {
    return Err(Error::Forbidden("无权执行此操作".into()));
}
```

示例中的 `entry_permission` 由服务端根据当前操作确定，不能接受客户端传入的权限码。`db`、`rbac`、`actor` 和 `executor` 均由调用方提供。

### 检查数据范围

调用 `DataScopeService::resolve` 检查账号、角色和操作权限，并取得数据范围。需要同时检查多个权限时，使用 `resolve_permissions`；同一个有效角色必须拥有这些权限。

业务代码再提供 `ScopedObject`，说明这条数据是否由当前账号负责、属于哪个组织、是否存在历史参与等。以客户详情为例：

```rust
use erp_identity::{Error, Permission};
use erp_identity::service::access_control::consumers::registration;
use erp_identity::service::access_control::resolve::DataScopeService;

let perm = Permission::parse("customer:detail")?;
let access = DataScopeService::new(db.clone(), rbac.clone())
    .resolve(actor, &perm, executor)
    .await?;

// object 由客户业务代码按当前账号和 access.as_of 查询、组装。
let allow_history = registration(perm.resource(), perm.action())?.allows_history;
if !access.scope.allows(&object, allow_history) {
    return Err(Error::Forbidden("无权访问该客户".into()));
}
```

客户负责人来自有效期内的 `CustomerAssignment`，不能从 `CustomerAccount` 直接读取负责人和组织字段。现有客户服务通过 `CustomerDataScopePort` 调用权限检查，具体实现见 [CustomerAccess](../erp-customer/src/service/customer/access.rs)。

普通人员范围读取 `person_data_scopes`。生成列表查询条件时，必须保留完整规则：

- 任一 `role_clauses` 条件满足，或该动作允许的历史参与条件满足，才算命中范围。
- 存在 `user_limit` 时，还必须满足它。
- 每条条件中的组织、结算主体、仓库等要求，按 `ScopeClause::covers` 处理。
- 空范围不能改成“本人”或“全公司”。列表与 `ResolvedScope::allows` 的判断结果必须一致。

需要读取关联单据的业务，使用 `resolve_source_scope`，分别传入当前操作权限和来源范围。合同、选品方案已在 `scope_source` 中配置映射；其他来源由业务代码确定。任务操作不能直接套用普通数据范围检查。

## 授予指定单据的读取权限

目前支持销售单和采购单。账号有效且有当前操作的功能权限时，满足以下任一条件即可读取：

- 原数据范围允许读取。
- 账号获得了这条单据的读取授权。

这类授权不允许修改原单据，也不授予任务办理权限。采购单的原数据范围仍按公司共享规则处理。

### 列表查询

按以下顺序执行，不能先分页再过滤权限：

1. 调用 `ResourceAccessService::resolve`，得到 `query`。
2. 通过 `query.constraints()` 取得账号、资源、数据范围和版本。业务仓储将完整范围转换成 `base_filter`，保留限制条件和允许的历史参与，不能固定写成“本人负责”。
3. 调用 `read_stages(&query, base_filter)`，将返回的阶段放在业务筛选之后、分页、计数和汇总之前。记录须保留业务主键 `id`。
4. 用同一执行器执行聚合查询，再调用 `ensure_current` 检查读取期间授权修订号是否变化。

`read_stages` 只把 `base_filter` 与指定单据授权合并，不会自动生成数据范围条件。前面的业务筛选也不能先排除“超出原范围但有单独授权”的记录。

需要自行组合查询时可用 `lookup_stages`，调用方负责合并条件并清除临时字段。不要将账号的全部授权 ID 加载到内存，也不要用批量检查代替列表查询。

`ensure_current` 只检查指定单据授权的修订号，不能代替账号、功能权限或完整数据范围的重新检查。

### 单条或批量检查

调用 `resolve_targets`，传入本次要检查的全部 ID，每次最多 1000 项。只会查询这些 ID 的授权。

```rust
use erp_identity::{Error, Permission};
use erp_identity::entity::resource_access::ResourceKind;
use erp_identity::entity::resource_access::query::ResourceTargets;
use erp_identity::service::resource_access::ResourceAccessService;

let service = ResourceAccessService::new(db.clone(), rbac.clone());
let entry_perm = Permission::parse("sales_order:detail")?;
let targets = ResourceTargets::new(&[order_id])?;
let access = service
    .resolve_targets(actor, &entry_perm, ResourceKind::SalesOrder, targets, executor)
    .await?;

// object 是业务代码为这张销售单组装的 ScopedObject。
if !access.allows(order_id, &object) {
    return Err(Error::Forbidden("无权访问该单据".into()));
}
```

财务等业务写入前，需要检查来源单据是否可读时，必须在写事务内重新调用 `resolve_targets`，传入本次写操作的权限及来源资源类型。不能拿之前的详情检查结果直接放行，也不能用 `sales_order:detail` 替代写操作的权限。来源动作的功能权限不额外要求，但业务仍需检查状态、版本和任务分配。

检查销售单本身时只接受 `list`、`detail`，不能用这类授权放行 `sales_order:update`。决定写入的权限检查和数据读取，必须使用写事务的同一执行器。

### 创建与撤销授权

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
| [service/resource_access/](src/service/resource_access/) | `ResourceAccessService` 检查读取权限；`ResourceGrantService` 管理授权；`GrantEventService` 处理事件队列 |
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
