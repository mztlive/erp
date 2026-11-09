# ERP 统一权限与行级授权改造

## 1. 扩展统一权限 crate

扩展 [erp-identity](../backend/crates/domains/erp-identity/)，集中执行身份、功能权限、数据范围、指定记录授权及任务访问检查，提供单对象检查、批量检查、查询约束、授权管理和按来源更新授权的接口。

业务代码通过统一接口或本域注入的 Port 请求判定，提供负责人、部门、关联对象、任务分配和版本等可信事实；将现有分散的权限判断迁入该 crate。业务写入在同一 `Executor` 内重验授权。

```text
允许读取 = 身份有效
        AND 具备当前入口的功能权限
        AND（原有效数据范围允许 OR 存在指定记录读取授权）
```

| 入口 | 接入后的检查 |
| --- | --- |
| 销售单列表、详情 | 分别检查 `sales_order:list`、`sales_order:detail`，查询范围包含获授权的销售单 |
| 客户往来的应收、回款、开票操作 | 检查对应财务功能权限及来源销售单的数据资格，解除对销售列表、详情功能权限的办理依赖 |
| 任务与审批办理 | 统一检查功能权限、当前实际分配和所需数据资格 |

## 2. 增加指定记录授权

在 `erp-identity` 中增加以下存储及管理接口，首批支持销售单、采购单的读取授权。

| 新增集合 | 保存内容 |
| --- | --- |
| `resource_grants` | 接收账号、资源类型、资源 ID、动作、来源 ID、状态和版本 |
| `resource_grant_sources` | 规则及版本、业务来源、接收账号、启用或终止状态、撤销人及原因、已应用事实版本 |
| `resource_grant_revisions` | 账号与资源类型的授权修订号，与来源变更同事务递增 |
| `resource_grant_events` | 唯一事件键、业务来源、事实版本、类型化业务证据、发生时间、处理状态、重试和领取信息 |

1. 指定记录授权与原有效数据范围取并集，允许账号访问本人、部门范围之外的被授权记录。
2. 授权按来源分别保存、撤销；有效来源取并集。自动来源以“规则 + 业务来源类型 + 来源 ID + 接收账号”稳定标识。
3. 任务完成、审批结束、撤回或转派后保留读取授权，直至明确撤销；办理资格按当前分配和状态即时判断。
4. 撤销提交后立即排除该来源，保存终止记录，供重试、重算和回填持续排除。
5. 增加管理员授予、撤销和查询来源的能力，记录操作人与原因；用于没有自动分配来源的指定销售单授权。
6. 建立账号与目标资源查询索引、来源内授权唯一索引及主键分页索引、待处理事件领取索引；`scope_version` 绑定原范围版本、入口、资源及账号资源修订号。来源变更对修改前后受影响资源递增修订号，撤销、授权行和修订号必须同事务提交；幂等重放不得递增。
7. 单条和批量授权只查询本次目标及关联来源；列表在数据库内应用原范围与有效指定记录的并集，再分页、计数或汇总。禁止预加载账号全部授权 ID，禁止通过全量枚举计算版本，禁止把局部目标检查结果用于完整列表查询。来源管理按主键分页。

## 3. 增加独立自动授权处理

新增 `backend/crates/orchestration/erp-resource-grants/`，在独立 `erp-worker` 进程中运行，使用 Rust 规则根据事件和可信业务事实计算应有授权。

| 触发事实 | 接收人 | 授权目标 |
| --- | --- | --- |
| 开票审批通过，并在同一事务创建开票执行任务 | 开票执行任务的实际分配账号 | 经开票申请、应收子账确认的销售单读取权 |
| 销售单审批节点激活并正式分配 | 实际分配的审批账号 | 被审销售单读取权 |
| 采购单审批节点激活并正式分配 | 实际分配的审批账号 | 被审采购单读取权 |

开票事件固定为 `InvoiceApproved`；销售、采购审批节点分配分别使用 `SalesOrderApprovalAssigned`、`PurchaseOrderApprovalAssigned`。业务事件类型与处理状态分别枚举。禁止把开票任务创建、普通分配或转派伪装为开票审批通过事件；后续转派授权须接入独立事件。

开票规则须核对最终审批通过事实及审批实例与申请的关联，并核对：申请的 `work_item_id` 对应任务；任务 `business_object_id` 与申请 `receivable_account_id` 指向同一子账；申请和子账的 `sales_order_id` 一致。接收人取正式分配事实。

处理流程：

```text
业务事务：保存开票审批通过及执行任务事实，或审批节点正式分配事实 + 对应授权事件
    → worker 领取事件
    → 读取当前事实及历史正式分配记录，计算应有授权
    → 授权事务：重验来源、规则和事实版本，提交差异 + 审计 + 处理结果
```

1. 分配准入与授权就绪分开：先完成合法分配，再异步生成读取授权，消除“必须先有数据权才能分配”的循环依赖。
2. 对存在待处理合法来源的请求返回授权未就绪状态，前端可刷新重试；业务提交结果独立于授权处理结果。
3. 按事件键幂等处理，以来源和事实版本防止乱序、并发覆盖；授权失败仅重试授权步骤。
4. 事实读取失败或关联冲突时保留已有结果并记录故障；撤销来源在发布时再次核验。提交结果未知时按事件键和已应用版本确认结果。
5. 支持按来源、资源类型和规则重算，并定期有界对账。历史读取从开票审批通过事实、正式分配记录及同事务事件重建，完成、转派状态不清除读取来源。
6. 接入 worker 启停、监督和恢复机制，记录积压量、最老未完成时长、失败原因及最近应用版本。

## 4. 接入业务查询和命令

| 改动位置 | 执行内容 |
| --- | --- |
| [SalesAccess](../backend/crates/read-models/erp-read-models/src/sales_center/access.rs)、[销售查询范围](../backend/crates/domains/erp-sales/src/repository/sales_order/scope.rs) | 将销售单访问判定和查询范围生成接到统一权限接口，合并指定记录读取授权 |
| [采购授权接线](../backend/apps/erp-server/src/wiring/purchase_data_scope.rs) | 接入统一查询约束及指定采购单读取来源 |
| [财务来源授权](../backend/crates/read-models/erp-read-models/src/finance/funds_scope/source_authorization.rs)、[命令守卫](../backend/crates/read-models/erp-read-models/src/finance/funds_scope/guard.rs)、[开票申请查询](../backend/crates/read-models/erp-read-models/src/finance/funds_scope/request.rs) | 统一来源范围、列表、详情、分配候选及命令检查，替换 `authorized_sales_ids`、`FundsAccess::allows` 等分散判断 |
| [应收创建](../backend/crates/orchestration/erp-finance-posting/src/finance_posting/receivable/account.rs)、[财务编排](../backend/crates/orchestration/erp-finance-posting/src/finance_posting/receivable/) | 在应收创建、回款提交、发票登记事务中检查实际来源集合与办理资格 |
| [开票派工](../backend/crates/orchestration/erp-finance-posting/src/finance_posting/receivable/invoice_request/execution.rs)、[工作项服务](../backend/crates/domains/erp-workflow/src/service/work_item/)、[审批执行](../backend/crates/domains/erp-workflow/src/service/approval/execution/) | 在开票最终审批通过并创建执行任务、销售或采购审批节点正式分配的事务追加对应事件；将任务访问判定接到统一接口 |
| [worker 装配](../backend/apps/erp-server/src/worker/mod.rs) | 装配自动授权消费、重试、重算与对账 |
| [客户往来](../erp-client/features/customer-receivables/)、[开票工作面](../erp-client/features/workspace/components/workspace-invoice-task.tsx) | 办理所需资料改由授权后的财务接口返回；展示授权处理状态，收到范围失效后刷新缓存 |

查询统一按“授权范围与业务筛选 → 去重 → 分页、计数、汇总”执行，单条检查使用同一规则。财务数据按真实销售来源取数；跨单回款、发票只输出允许份额，完整操作检查全部必要来源。

实施按“统一接口与存储 → 自动授权进程 → 业务读写接入 → 存量回填与切换”推进。新增 crate 同步登记 workspace、层次依赖、索引及 README。按对应审批通过及正式分配证据回填存量授权，按完整业务链路切换并移除对应旧判定。

## 5. 验收

使用非销售部门账号，无全公司、销售部门范围及其他销售读取依据，仅授予 S001；S002 与其属于同一客户。

| 场景 | 必须结果 |
| --- | --- |
| 有财务功能，无销售列表、详情功能 | 能办理已授权 S001 的合法财务操作；销售列表、详情入口仍按功能权限控制 |
| 增加销售列表、详情功能 | 可见 S001，S002 不进入列表、详情、候选和金额汇总 |
| 开票最终审批通过、销售或采购审批节点正式分配 | 分别产生准确的单据读取来源；关联错配不授权 |
| 任务完成或转派 | 原读取来源保留；办理资格随当前任务变化；新接收人独立获得来源 |
| 多来源授权、明确撤销后重算 | 来源独立生效与撤销，终止来源不会恢复；身份或功能失效时拒绝访问 |
| 重复、乱序、并发、停机及事实读取失败 | 授权可安全恢复，无重复业务执行、版本倒退或历史授权误删 |
| 从列表进入详情、登记和提交 | 同一来源授权贯通全部入口，跨单数据按授权范围隔离 |
