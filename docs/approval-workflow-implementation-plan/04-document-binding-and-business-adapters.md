# 阶段 04：单据绑定与业务适配

> 阶段性质：P3 业务 Adapter 工作包；按 DocumentType 拆分独立子阶段和 PR
>
> 阶段目标：在 ERP Service 中完成 `DocumentType`、业务单据、WorkItem 与纯 `bpm` 引擎的适配，先完成一个签署试点，再按固定 DocumentType 逐项接入
>
> 允许状态：可依赖阶段 00—03 的冻结端口；共享导出不得由本阶段修改

## 1. 文件责任

本阶段负责：

- `backend/services/src/approval/binding.rs`；
- `backend/services/src/approval/business_adapter.rs`；
- `backend/services/src/document_registry/**` 的单据注册与绑定查询；
- 各 `DocumentType` 的创建 Service 和提交/撤回业务用例；一个 DocumentType 一个独立子阶段和 PR；
- 各业务详情 DTO 中的只读审批绑定与运行摘要；
- 对应业务 Service 测试。

本阶段不负责定义管理、BPM 内部状态机、HTTP 公共审批 Handler、前端和集成测试。不得在一个 PR 中同时修改多个无共同聚合边界的业务域。`bpm` 不得依赖本阶段任何文件；业务适配器只能从 Service 调用 BPM，不得注册为 BPM 回调。

## 2. 子阶段顺序与准入

本阶段必须按以下顺序执行：

1. `P3-ADAPTER-BASE`：完成 `BusinessDocument` 注册清点与补齐（见 2.1），并建立统一 binding、action、snapshot 和查询端口；
2. `P3-ADAPTER-PILOT`：实现唯一试点 **`StockAdjustment`**（合同 §4.5）；
3. `P6-PILOT` 对试点完成创建、绑定、提交、通过、驳回、撤回、恢复、改派、并发和专用空数据库重置演练；
4. `P6-PILOT` 通过后，其余 11 个 `PROCESS_REQUIRED` 类型各自建立独立子阶段；
5. 8 个 `NO_APPROVAL` 类型只证明创建不绑定、不启动，不得实现空审批适配器。

每个子阶段的生命周期行已在合同 §4.4.1（状态与版本）、§4.4.4（强类型动作与撤回合同）、§4.4.5（快照与 WorkItem 归属）中以唯一确定值签署，并由 `docs/dev-plan/approval-workflow.md` §1 登记引用位置。任一行被发现有空值、二选一、候选动作或未决说明时，必须停止编码并退回 DOC 阶段修订合同；实施人员不得代替业务合同作决定。

### 2.1 `BusinessDocument` 注册清点（`P3-ADAPTER-BASE` 必须先完成）

统一绑定端口以 `BusinessDocument` 为落地点，但当前基线只有下列域接触 `BusinessDocument`：

```text
services/src/sales_order/**
services/src/sales_review/**
services/src/receivable/**
services/src/legacy_import/**
services/src/file_asset/**
```

因此 `purchase_order`、`inventory`、`payable`、`returns`、`fulfillment` 全域的创建事务都缺少 `BusinessDocument` 注册。这五个域同时是各 `DocumentType` 子阶段的 `owns`，因此 `_meta.json` 已用 `ownsWithin` 分段：`P3-ADAPTER-BASE` 在这些目录内**只允许**补 `BusinessDocument` 注册，不得修改状态机、审批适配或提交命令。`P3-ADAPTER-BASE` 必须：

1. 在 PR 中列出全部 20 个 `DocumentType` 的 `BusinessDocument` 注册现状清单，逐行标注「已注册 / 需新增」及创建入口文件与行号；
2. 为所有缺失类型补齐注册，使注册与业务实体创建处于同一事务；尚未分配正式号的草稿以空 `document_no` 注册，不得生成占位业务号；
3. 证明补齐后每个 `DocumentType` 都能构造唯一 `bpm::SubjectRef`。

该清点不得推迟到各类型批次里临时补，也不得把审批绑定复制到 20 个业务实体中。业务实体只保留自身状态，跨类型公共绑定由 `BusinessDocument` 统一承载。

`P3-ADAPTER-BASE` 必须复用阶段 00 的唯一映射。`SalesOrder` 与 `VoucherSalesOrder` 共用 `sales_order` 实体，但必须按 `BusinessType` 穷尽 `match` 分派到两个不同的 `DocumentType` 与两条不同的 `ProcessKind`；不得在同一 `ProcessKind` 内按 `BusinessType` 二次分流。`ProcessKind` 稳定值一旦进入已发布定义或运行实例不得重命名；本次重写期间发现命名错误必须在硬切换前一次性修正，启用后不得以读取 fallback 兼容别名。

## 3. 统一绑定端口

实现：

```text
bind_published_definition_on_document_create(
  document_type,
  business_object_id,
  business_object_version,
  actor,
  executor
) -> ApprovalDefinitionBinding?
```

执行规则：

1. 对 `NO_APPROVAL` 返回空绑定并记录政策事实，不查询定义；
2. 对 `PROCESS_REQUIRED` 查询唯一当前 `PUBLISHED` 定义；
3. 不存在时返回 `APPROVAL_PROCESS_NOT_CONFIGURED`；
4. 将 `DocumentType` 转换为 `ProcessKind`，加载 BPM 定义图并调用 `bpm::graph` 重验结构；随后由业务 Adapter 重验指定用户、审批权限，并以当前单据组织和创建人上下文重验 DataScope、对象读取权和岗位分离；
5. 返回定义 ID、版本和绑定时间；
6. 调用方必须在创建业务实体和 `BusinessDocument` 的同一事务中持久化绑定并写 `approval.definition.bound` 审计；
7. 任一失败必须回滚业务实体，不得留下“以后补流程”的单据。

绑定端口必须接收调用方 `Executor`，不得自行开启嵌套事务。`Executor` 只允许停留在 Service/Repository 调用链，不得传入 BPM API。

## 4. 单据生命周期与改造清单

下列每个创建事务都必须注册 `BusinessDocument` 并执行统一绑定端口。现有已注册路径也必须改为原子写入绑定。「现有代码定位」列只用于定位既有实现；生效的强类型动作名以合同 §4.4.4 为准。

#### 4.1 `PROCESS_REQUIRED`（12 个，各自一个子阶段）

| `DocumentType` | 创建入口 | 现有代码定位 | 子阶段必须完成 |
| --- | --- | --- | --- |
| `StockAdjustment`（试点） | `inventory/mod.rs::create_stock_adjustment` | `submit_stock_adjustment`、`post_stock_adjustment` | 移除人工 approve 中间旁路：删除 `approve_stock_adjustment`、`reject_stock_adjustment` 及其 `handler/inventory/mod.rs` 端点和 `routes/inventory.rs` 路由（两者已在 `P3-ADAPTER-PILOT` 的 `owns` 内）；两个复核态合并为 `IN_APPROVAL`；按 §4.4.2 删除 `StockAdjustmentState::Rejected` |
| `SalesOrder` | `sales_order/command.rs::create_sales_order` | `submit_sales_order`、`database/repository/sales_order/revision.rs::formalize_submission` | 按 `BusinessType::GoodsService` 分派；`formalize_submission` 由 Service 强类型端口包装编排；审批启动点是销售的**提交命令**，与 `VoucherSalesOrder` 完全一致（合同 §4.4.1）。同批停止采购二次确认与低毛利上级确认的新写入和全部 HTTP 可达路径；对应旧实体、Repository、Service 文件和旧状态由 P0-D 删除。选源改由采购单创建路径承担 |
| `VoucherSalesOrder` | 同上，按 `BusinessType::Voucher` 分派 | 现有卡券两级审批路径 | 新提交只进入通用审批，卡券专用决定路径立即失败关闭；`CardSalesApprovalActionPort`、卡券专用 WorkItem 类型和两个旧解析器由 P0-D 删除；无提交准入，销售提交直接启动；最终通过后发送商城执行投影 |
| `SalesChangeOrder` | `sales_review/sales_change_order.rs::create_sales_change_order` | `submit_sales_change`、`sales_change_review::{confirm_impact,confirm_finance}` | 两个确认动作不得充当流程节点，最终动作唯一 |
| `PurchaseOrder` | `purchase_order/creation_basis.rs::create_from_basis`、`draft_from_confirmation.rs` | `purchase_order/submission.rs::submit` | 两条生成路径必须汇入同一绑定和提交端口；新增 `approval_subject_version`，不得使用最终通过后才生成的 `purchase_revision.revision_no`；草稿 `purchase_no` 为空，首次提交事务分配不可复用正式号，并同步写入 `BusinessDocument` 编号 |
| `PurchaseChangeOrder` | `purchase_order/change.rs::start_change` | `purchase_order/change.rs::submit_change` | 新变更单独立绑定，不继承原采购单定义；新增 `approval_subject_version` |
| `CustomerReceipt` | `receivable/mod.rs::create_customer_receipt` | `post_customer_receipt` | 应收和资金副作用与最终审批同事务；审批中可受控撤回 |
| `SupplierPayment` | `payable/mod.rs::create_supplier_payment` | `post_supplier_payment` | 应付和资金副作用与最终审批同事务；审批中可受控撤回 |
| `CustomerRefund` | `returns/customer_refund.rs::create_customer_refund` | `post_customer_refund` | 退款副作用与最终审批同事务；审批中可受控撤回 |
| `SupplierRefund` | `returns/supplier_refund.rs::create_supplier_refund` | `post_supplier_refund` | 退款副作用与最终审批同事务；审批中可受控撤回 |
| `ReceiptReversal` | `returns/receipt_reversal.rs::create_receipt_reversal` | `post_receipt_reversal` | 冲正副作用与最终审批同事务；审批中可受控撤回 |
| `PaymentReversal` | `returns/payment_reversal.rs::create_payment_reversal` | `post_payment_reversal` | 冲正副作用与最终审批同事务；审批中可受控撤回 |

#### 4.2 `NO_APPROVAL`（8 个，不建适配器）

| `DocumentType` | 创建入口 | 子阶段必须完成 |
| --- | --- | --- |
| `PurchaseReceipt` | `fulfillment/purchase_receipt.rs::create_purchase_receipt` | 补 `BusinessDocument` 注册；证明创建不绑定、不启动、不建任务 |
| `Delivery` | `fulfillment/delivery.rs::create_delivery` | 同上 |
| `ElectronicDelivery` | `fulfillment/electronic_delivery.rs::create_electronic_delivery` | 同上 |
| `ServiceFulfillment` | `fulfillment/service_fulfillment.rs::create_service_fulfillment` | 同上 |
| `CustomerAcceptance` | `fulfillment/customer_acceptance.rs::create_customer_acceptance` | 同上 |
| `Invoice` | `receivable/mod.rs::create_invoice` | 同上 |
| `SalesReturnCase` | `returns/sales_return.rs::create_sales_return_case` | 同上。当前无统一正式动作，签署为 `NO_APPROVAL` 后本期不需要新建正式化命令 |
| `PurchaseReturnOrder` | `returns/purchase_return.rs::create_purchase_return_order` | 同上 |

一个业务过程中已有的复核、确认、过账步骤不得直接等同为多个审批节点。`SalesOrder` 定义必须包含合同 §4.4.3 规定的唯一采购确认用途节点；用途只在发布校验中生效，运行时不得据此分支。低毛利上级确认必须整体删除，不保留替代环节。

## 5. 提交与启动适配

每个 `PROCESS_REQUIRED` 单据的提交用例必须调用：

```text
start_approval(document_ref, expected_document_version, idempotency_key, actor, executor)
```

适配步骤固定为：

1. 业务 Service 锁定单据并校验允许提交；
2. 按该 DocumentType 已签署的权威字段形成不可变 `subject_version` 和 `subject_snapshot`，并构造 `bpm::SubjectRef`；不得统一复用 `BaseModel.version`；
3. 从 `BusinessDocument` 读取冻结绑定，不接受客户端 definition ID；
4. 调用 ERP 审批启动端口；该端口在阶段 05 内调用纯 BPM 引擎生成 `TransitionPlan`；
5. 由政策注册的强类型提交动作修改业务状态为审批中；
6. 单据、revision、审批实例、强类型业务对象快照、第一执行、任务、审计和通知在同一事务提交。

已存在的提交入口至少包括 `sales_order/command.rs::submit_sales_order`、`sales_review/sales_change_order.rs::submit_sales_change`、`purchase_order/change.rs::submit_change`、`inventory/mod.rs::submit_stock_adjustment`。`SalesOrder` 与 `VoucherSalesOrder` 都必须由 `submit_sales_order` 直接启动审批，不得经过 `sales_review` 准入命令或第二条启动路径；销售单子阶段必须让采购确认、低毛利和卡券专用决定路径不可达，P0-D 再删除其跨域旧符号。资金类 6 个类型当前只有 `post_*` 过账命令、没有独立提交命令，其子阶段必须先建立合同 §4.4.4 指定的 `submit_*` 强类型端口，再由该端口调用统一审批启动端口；不得为了接审批新增绕开原业务状态机的通用状态更新。

## 6. 强类型业务动作清单

每个 `PROCESS_REQUIRED` 类型必须显式实现：

- `on_approval_start`：冻结提交版本并进入审批中；
- `on_final_approve`：调用该单据既有的正式化、确认、过账或生效领域方法；
- `cancel_action`：审批最终通过前由业务撤回和管理员受阻取消共同调用，成功后回到可修正草稿。

适配器必须调用现有实体方法和 Repository；不得用 MongoDB `$set` 绕过领域不变式。最终动作必须接收审批实例 ID 和幂等上下文，确保仅执行一次。

当前卡券专用 `CardSalesApprovalActionPort` 必须拆为 `SalesOrder` 与 `VoucherSalesOrder` 两个独立政策适配器。两者的提交与责任模型完全一致：都由销售提交直接启动，都不存在提交准入，都不得为某个节点写业务专用分支。

`PurchaseOrder` 子阶段必须同批承接选源：采购单创建时录入供应商供给修订、最新成本、供货数量、预计交期和履约方式（`erp-phase-1.md` §7.4）。该能力属于采购单创建路径的业务字段，不属于审批合同，不得回写销售单审批状态，也不得新建第二个「确认」实体。

`PurchaseOrder` 首次提交事务的顺序固定为：锁定采购单与 `BusinessDocument`；若尚无正式号则分配不可复用 `purchase_no` 并一次性写入两者；冻结采购提交；以该正式号构造 `subject_snapshot.document_no`；递增 `approval_subject_version` 并启动审批。幂等回读必须返回原正式号，事务失败不得消耗出一个已挂到业务对象上的半成品编号。

每个 Adapter 必须显式声明：

```text
document_type
process_kind
subject_ref_builder
subject_version_source
subject_snapshot_builder
on_approval_start
on_final_approve
cancel_action
owner_role
owner_organization_snapshot
read_scope
```

Adapter 缺少任何字段时必须在注册完整性测试中失败。

业务 Adapter 可以读取实体和 Repository、执行权限/DataScope 校验并调用强类型领域命令；不得将 `DocumentType`、业务实体、Repository、`Executor`、权限结果细节、WorkItem 或通知模板传入 `bpm`。传给 BPM 的资格结果必须收敛为稳定的 `Eligible` 或结构化 `BlockedReason`。

## 7. 未提交单据升级

实现 `upgrade_unsubmitted_document_definition`：

1. 只允许审批运行管理员调用；
2. 锁定 `BusinessDocument` 和业务单据；
3. 校验单据未提交、未启动、业务版本和审批绑定版本分别匹配；
4. 读取该类型当前发布定义，并以当前单据资源重验全部人员的资格、DataScope、读取权和岗位分离；
5. 以一个值对象整体替换绑定 ID、定义版本、绑定版本和时间；
6. 写原定义、新定义、原因、actor 和动作审计；
7. 使用业务单据版本与 `approval_binding_version` 两个 CAS 原子提交。

不得提供“只换某节点审批人”或升级到任意历史版本的接口。

## 8. 撤回与取消

业务撤回用例必须调用 `cancel_approval`。取消动作必须锁定单据、实例和当前执行；实例为 `RUNNING` 时锁定当前任务，实例为 `BLOCKED` 时证明不存在开放任务。校验业务允许撤回及全部适用版本后，再由强类型撤回动作修改单据。

不得把审批取消暴露为工作项决定；已 `APPROVED` 的实例不得取消。现有销售单 `cancel_approval` 入口可以保留业务路由，但内部必须只调用统一端口。

## 9. 详情查询

12 个 `PROCESS_REQUIRED` 类型的单据详情必须返回统一 `approval` 结构：

```text
requirement
definition { id, name, version, nodes[] }
instance { id, status, current_round_no, current_node, current_assignee, latest_rejection }
recent_history[]
history_page { next_cursor, has_more }
allowed_actions[]
```

`recent_history` 必须有固定上限；完整历史由阶段 06 的分页端点读取。创建后未提交只返回绑定定义；运行后返回实例事实。敏感字段继续按业务详情权限脱敏，审批责任不得自动扩大读取权。

## 10. 阶段验收

- [ ] 统一 Adapter 注册能穷尽发现缺少的政策、快照、版本或强类型动作。
- [ ] 每个 `DocumentType` 均有唯一稳定 `ProcessKind` 和 `SubjectRef` 构造，未注册映射失败关闭。
- [ ] 试点 DocumentType 已先完成 P6-PILOT 端到端验收，其他类型才允许进入批次。
- [ ] 每个 DocumentType 一个独立子阶段和 PR，并拥有签署的生命周期行。
- [ ] 全部创建入口均有政策测试和原子绑定单元测试；试点真实事务测试由 P6-PILOT 编写，其他类型由 P6-FINAL 补齐。
- [ ] `PROCESS_REQUIRED` 无发布定义或人员失效时，业务单据零写入。
- [ ] `NO_APPROVAL` 单据不创建绑定、实例或任务。
- [ ] 创建只绑定，提交才产生审批任务。
- [ ] `PurchaseOrder` 草稿不预分配正式号；首次提交在绑定、冻结提交和启动审批的同一事务中分配 `purchase_no`，并与 `BusinessDocument.document_no` 保持一致。
- [ ] 客户端无法选择定义或审批人。
- [ ] 发布新版本不修改已有单据；退役旧版本不阻止已绑定单据提交。
- [ ] 已启动单据无法升级绑定。
- [ ] 强类型动作失败时业务单据与审批运行写入整体回滚。
- [ ] `bpm` 未接收 `Executor`、Repository、业务实体、权限、WorkItem、通知模板或强类型业务动作。
- [ ] 详情不返回无界历史，列表不通过逐实例历史查询获取最近驳回。
- [ ] 每个业务模块的目标测试通过；共享入口缺失必须通过 P0 amendment 解决。
