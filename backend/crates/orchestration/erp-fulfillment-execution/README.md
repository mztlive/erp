# erp-fulfillment-execution：履约执行编排

## 职责

本 crate 编排采购收货、实物发货、电子交付、服务履约、客户验收、库存调整审批及供应商履约执行。它决定跨域命令的事务边界和步骤顺序，不接管各领域的实体规则、仓储或索引。例如，收货单和发货单由 `erp-fulfillment` 管理，库存调整单、库存余额、出入库流水和销售预占由 `erp-inventory` 管理，正式工作项由 `erp-workflow` 管理。

## 目录结构

| 入口 | 职责 |
| --- | --- |
| [`Cargo.toml`](Cargo.toml)、[`src/lib.rs`](src/lib.rs) | 声明领域、读模型及共享能力依赖；公开流程模块、凭证确认入口和 `Error` / `Result`。 |
| [`fulfillment_execution/`](src/fulfillment_execution/mod.rs) | `FulfillmentProcess` 处理收货、发货、电子交付与服务履约；[`customer_acceptance/`](src/fulfillment_execution/customer_acceptance/mod.rs) 的 `CustomerAcceptanceProcess` 处理验收登记、过账和反向验收；[`task.rs`](src/fulfillment_execution/task.rs) 协调履约草稿及 W01 执行任务。 |
| [`inventory_adjustment/`](src/inventory_adjustment/mod.rs) | `InventoryAdjustmentProcess` 处理库存调整单创建、送审、撤回和审批结果；最终通过时[沿用审批决定事务过账](src/inventory_adjustment/post.rs)。 |
| [`supply_execution/`](src/supply_execution/mod.rs) | `SupplierFulfillmentProcess` 协调供应商子订单下单、取消与退款、结果登记、调查及 W26 异常工作项；旧动作入口排除由采购 API 流程执行的订单，避免重复办理。 |
| [`workflow.rs`](src/workflow.rs)、[`attachments.rs`](src/attachments.rs)、[`errors.rs`](src/errors.rs) | 分别提供工作流能力接口、已上传凭证的同事务登记入口及编排层错误分类。 |

## 架构设计

### 授权与事务

应用按流程注入共享的 `AuthorizationService`、工作项与审批运行能力、带授权和事实接口的库存服务，以及供应商网关和异常处理能力。HTTP 负责提取请求和传递操作人，不代替用例授权；权限定义归 `erp-permissions`，权限判断归 `erp-identity`（见[业务授权 API](../../domains/erp-identity/README.md#业务授权-api)）。

跨域写命令由根入口开启事务；受审批运行时调用的业务动作沿用运行时持有的事务。决定写入的授权、业务读取、领域写入和成功审计复用同一个 `Executor`，领域服务、仓储和事务内步骤不另开事务。

### 实际链路：采购收货过账

1. [收货 HTTP 入口](../../../apps/erp-server/src/http/handler/fulfillment/purchase_receipt.rs) 将请求和认证操作人交给 [`FulfillmentProcess::post_purchase_receipt`](src/fulfillment_execution/purchase_receipt_posting.rs)。Process 开启事务后，先按收货单身份找到唯一开放的 W01 执行任务，再用共享授权服务按[任务 ID 和版本核验办理资格](src/fulfillment_execution/task_access.rs)；任务缺失、重复或操作人无资格时，不读取收货单据。
2. 授权通过后，[履约领域服务](../../domains/erp-fulfillment/src/service/purchase_receipt_posting.rs) 校验收货草稿、期望版本、冻结仓库和明细。Process 加载采购当前生效版本，把历史版本的累计合格收货归并到当前明细后逐行防止超收。采购单须处于可履约状态；[预付款门槛](src/fulfillment_execution/purchase_context.rs) 用于供应商 API 下单，不阻止登记实际到货。
3. 只有合格数量进入库存。[库存领域服务](../../domains/erp-inventory/src/service/fulfillment.rs) 更新或建立余额并记录入库流水；存在采购销售分配时，Process 按分配比例把本次合格数量分摊到稳定销售明细，末项吸收舍入尾差，再由库存领域写入预占、冻结可用量并记录预占分录。
4. 同一事务继续由履约域标记收货单已过账、由工作流域完成入库任务，再更新采购履约进度。若形成销售预占，按销售单和仓库创建或补充履约域的仓发草稿，草稿行引用该预占，并确保有指派给仓发经办人的 W01 任务。最后写成功审计，并[唤醒既有供应商执行的收货核验](../../domains/erp-supply/src/api_order/receipt_wakeup.rs)，不在事务内发出供应商请求。任一步失败都不会留下部分过账结果。

### 外部调用与未知提交

外部 HTTP、对象存储等调用在数据库事务之外；已上传凭证的资产登记与交付确认可以同事务写入。以[供应商下单](src/supply_execution/place.rs)为例：先在事务内保存供给域的子订单和首个动作，以及集成域的消息信封；确认意图已提交后才经网关派发，再另开事务写回结果。需要人工介入时，集成域记录错误任务，工作流域承接 W26 正式工作项。

提交结果未知时只按原命令身份查证，不换键重做或自动重派供应商动作。[客户验收登记](src/fulfillment_execution/customer_acceptance/commit.rs)按原操作号和回执恢复已提交结果；普通收货过账没有这类回执重放语义，以草稿状态、版本及流水唯一约束防止重复，提交无法确认时返回 `OutcomeUnknown`。

## 验证

在 `backend/` 运行库单元测试：

```bash
env -u ERP_TEST_MONGO_URI cargo test -p erp-fulfillment-execution --lib --locked
```

不运行集成测试、`ignored` 测试或真实服务验收。仅修改本 README 时，只对本文件运行 `git diff --check`。
