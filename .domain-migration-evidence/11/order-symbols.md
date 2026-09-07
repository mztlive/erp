# 阶段 11 采购订单核心符号迁移合同

## 输入与边界

- 输入提交固定为 `07da7863`；输出为 `/private/tmp/erp-domain-crate-11-procurement` 未提交工作树。
- 表内源路径相对 `backend/services/src/purchase_order`；目标相对 `backend/crates`。
- 普通采购领域不依赖销售、供应商、财务、身份、审计、工作流或旧三层。Process 持有跨域根事务、审批、授权、审计和回执；Read Model 持有跨域展示与唯一共享事实来源。
- 保留 HTTP/DTO/错误分类、持久化字段、编号、幂等身份、时间取值及执行器语义。不执行真实数据库，不修改历史 `tests/**`。

## 核心逐文件归属

| 原文件 | 单域目标与实际符号 | 跨域目标与实际符号 |
| --- | --- | --- |
| `line_input.rs` | `erp-procurement/src/service/purchase_order/line_input.rs`：`SavePurchaseOrderLine::to_line_input`、`to_line_inputs`、提交/变更提交行构造、金额汇总和原解析器 | 无 |
| `view_mapping.rs` | 同域 `view_mapping.rs`：`revision_line_to_view`、`submission_line_to_view`、`revision_totals` | Read Model 直接消费以上唯一投影 |
| `shared.rs` | 同域 `shared.rs`：`zero_amount`、`zero_rate`、`Versioned`、`PurchaseOrderService::ensure_version` | `erp-processes/src/procure_to_pay/shared.rs`：供应商付款条件快照适配 |
| `formalization.rs` | 同域 `formalization.rs`：`next_revision_no`、`build_effective_revision`、`build_change_revision` | 原变更应付构造由变更分片迁 `erp-finance` |
| `allocation_maintenance.rs` | 同域 `allocation_maintenance.rs`：`PreparedSalesAllocations`、原分配 ID/计划规则、`persist_current_sales_allocations` | `procure_to_pay/allocation_maintenance.rs` 装配 `adapters::SalesAllocationAdapter` |
| `draft_edit.rs` | 同域 `draft_edit.rs`：采购读取、创建人优先验证、草稿读取、`DraftReplacement`、金额与实体构造、`persist_replacement` | `procure_to_pay/draft_edit.rs`：授权根、回执重放/恢复、销售 guard、覆盖与任务同步、审计 |
| `submission.rs` | 同域 `submission.rs`：正式号分配、提交序号、冻结草稿与补丁提交构造 | `procure_to_pay/submission.rs`：独立提交根、审批启动、幂等回放/恢复、`PurchaseSubmitReceipt` |
| `create_submit.rs` | 同域 `create_submit.rs`：同事务采购草稿读取、从已创建草稿冻结正式提交 | `procure_to_pay/create_submit.rs`：创建后提交根、注册行编号、审批构造、启动与审计 |
| `review.rs` | 同域 `formalization.rs`：原冻结金额校验、版本/分配/订单与提交状态/CAS 顺序；Finance 两个 `purchase_initial.rs`：原始应付与逐行确认成本构造和写入 | `procure_to_pay/review.rs`：冻结跨域计划、财务事实装配与三个履约草稿；`formalization_root.rs`、`formalization_posting.rs`：根事务与生产步骤 |
| `adapter.rs` | 同域 `lifecycle.rs`：启动、撤回与最终通过状态守卫 | `procure_to_pay/adapter.rs`：审批政策动作分派、责任组织、对象读取、审批快照；展示投影由 Read Model 分片迁移 |
| `start_approval.rs` | 同域同名 leaf：失效草稿持久化、提交/行/订单原子写入 | Process 同名 leaf：绑定定义、启动执行、运行事实、任务、快照、审计及回执 |
| `cancel_approval.rs` | 同域同名 leaf：取消状态的采购 CAS | Process 同名 leaf：撤回命令、运行时恢复与审计 |
| `void_order.rs` | 同域同名 leaf：创建人/版本/状态顺序、草稿检查、作废状态和 CAS | Process 同名 leaf：授权、销售 guard、覆盖释放、任务同步和回执 |
| `authorization.rs` | 无 | `procure_to_pay/authorization.rs`：身份账号、RBAC 快照、事务授权版本 |
| `procurement_task_sync.rs` | 覆盖计算由采购 `coverage` Port 服务消费 | `procure_to_pay/procurement_task_sync.rs`：工作项资格、开放任务更新、完成与释放后新任务 |
| `mod.rs` | `PurchaseOrderService::new(Database)`：只持有采购数据访问 | `PurchaseOrderProcess::new/with_rbac/with_object_read`、两个审批取消事务回调、`PurchaseOrderFormalizationProcess`、任务同步出口 |

## 共用事实与入口

1. `erp_procurement::ports::purchase_order::SalesAllocationPort::current_lines` 仅返回 `CurrentSalesAllocationLine`，接收原 `&mut dyn Executor`。实际适配顺序为销售单读取 → 当前版本指针 → 当前版本行；保留不存在和缺当前版本的原首错。
2. `procure_to_pay::adapters::payment_term::{parse,parse_snapshot}` 返回 `entity::facts::PaymentTermFact { canonical_code, prepay_gate, days_after_delivery }`。新实体构造/更新/快照 API 在原付款条件解析位置调用同步回调；快照仍先拆历史编码。
3. `procure_to_pay::adapters::audit::audit_receipt_fact` 只投影成功、操作人、动作、资源和消息；采购收据仍按身份 → 消息格式 → 指纹 → 结果解码顺序执行。
4. 覆盖事实复用 `erp_read_models::purchase_center::repository::load_sales_procurement_coverage` 唯一实现；写流程传入根执行器，禁止改用 `NoTransaction`。
5. 金融初次正式化分别消费 `erp_finance::service::payable::purchase_initial::{InitialPurchasePayable,prepare,persist}` 与 `service::cost::purchase_initial::{PurchaseCostLine,prepare,persist}`。ID、逐行成本时间和首错保留原相对位置。
6. 保留 `PurchaseOrderFormalizationProcess::formalize_approved_order` 与 `formalize_approved_order_in_transaction` 原公开签名；外层先构造原审计，再依次执行采购、应付、付款任务、逐行成本、履约、审计。

## 核验边界

- 核心 15 个原 leaf 的 46 个原 `#[test]` / `#[tokio::test]` 函数名已静态核对，迁后缺失数为 0；展示测试在 Read Model，创建人优先规则测试在采购领域。
- 新增启动、撤回、作废与正式化生产步骤替身测试共 8 个；另新增 3 个真实供应商付款解析适配测试。使用非零大小执行器，验证原序、同一执行器及各步骤失败停止。测试执行结果由集成负责人统一日志登记，本文件不宣称测试已通过。
- 根负责人已执行采购与 Finance 窄 `cargo check` 并通过；唯一采购提交模块未用导入已清理。全工作区门禁须继续执行。
- 正式化源码语义对照见 `/private/tmp/procurement11-formalization-semantic-review.md`；该对照不等于数据库执行或回滚证明。
- 旧 `services/src/purchase_order` 各分片在完成迁移后逐项删除，最后核实只剩 `mod.rs` 才删除根和空目录。
- 真实数据库运行未验证。
