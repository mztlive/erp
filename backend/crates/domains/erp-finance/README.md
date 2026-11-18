# erp-finance：应收、应付、资金与成本

## 职责

订单记录交易约定；`erp-finance` 记录这些交易形成的债权债务、实际收付款和核销、发票以及成本事实。采购付款申请和销项开票申请获批只授予相应额度，不等于已经付款或登记发票。

### 负责的数据与能力

| 子域 | 财务事实与规则 | 源码入口 |
| --- | --- | --- |
| 应收与销项 | 维护应收子账、分录与冲抵、客户回款及核销分配。回款不能跨往来主体核销，分配受回款金额、分录和子账开放余额限制。销项开票申请批准不增加已开票额；正式发票分配的含税额、不含税额、税额分别与原票守恒。 | [回款核销](src/service/receivable/customer_receipt_posting.rs)、[销项开票申请](src/entity/receivable/invoice_request.rs)、[销项发票分配](src/entity/receivable/sales_invoice_allocation_plan.rs) |
| 应付与进项 | 维护应付子账、分录与冲抵、供应商付款、核销分配和供应商留抵。付款核销受实际资金和开放余额约束；留抵只能用于同一供应商和付款主体，不产生新的银行付款。进项发票登记复用发票实体，进项分配按应付子账维护并校验三项金额守恒。 | [付款核销](src/service/payable/payment.rs)、[供应商留抵](src/entity/payable/supplier_credit.rs)、[进项发票登记](src/service/payable/invoice.rs)、[进项发票分配](src/entity/payable/purchase_invoice_allocation_plan.rs) |
| 采购付款申请（应付子域） | 保存草稿、逐采购冻结来源、付款主体及资金账户，维护批准额度、未付占用、净已付与关闭金额。银行付款只能使用原申请对应采购的批准额度；预存款执行和核销另有资金证据与占用规则。 | [申请状态与额度](src/entity/payable/payment_request/lifecycle.rs)、[应付占用](src/repository/payable/account/reservation.rs)、[提交快照仓储](src/repository/payable/payment_request_snapshot.rs) |
| 供应商预存款 | 分别记录真实到账、批次和占用、实际扣款、核销与返款，以及供对账使用的资金证据。先记录已核实扣款，再依批准范围核销应付；充值或占用本身不是采购付款，不伪造银行付款。 | [预存款服务](src/service/supplier_deposit/mod.rs)、[扣款入账](src/service/supplier_deposit/usage.rs)、[核销与返款](src/service/supplier_deposit/settlement.rs)、[资金证据仓储](src/repository/funds_evidence.rs) |
| 成本与分摊 | 保存成本条目和按销售单或明细归属的分摊。采购首次正式化形成确认成本；满足条件的首次履约按冻结的采购销售分配形成实际供货成本。手工分摊校验含税、不含税两个口径的守恒及尾差归属。 | [初始采购成本](src/service/cost/purchase_initial.rs)、[履约实际成本](src/service/cost/fulfillment_actual.rs)、[成本分配计划](src/entity/cost/cost_allocation_set.rs) |
| 财务命令回执（共用机制） | 独立保存结构化命令身份、版本化指纹和强类型结果引用，以唯一命令身份支持重试和提交结果查证；不是审计日志。 | [回执实体](src/entity/command_receipt.rs)、[回执服务](src/service/command_receipt.rs)、[唯一索引](src/indexes/command_receipt.rs) |

### 使用场景

修改金额、状态、核销、分摊等财务规则，或修改财务集合及单域查询，归本 crate。涉及外域单据、审批、任务或成功审计的完整写入找对应 Process；需要拼接多个领域并汇总、展示或导出的查询找 ReadModel。

## 目录结构

### 代码入口

| 目录或文件 | 分工 |
| --- | --- |
| [Cargo.toml](Cargo.toml)、[src/lib.rs](src/lib.rs) | crate 依赖和公开入口。`lib.rs` 导出各层模块、领域错误及命令回执的公共类型和服务。 |
| [entity/](src/entity/mod.rs) | `receivable/`、`payable/`（含 `payment_request/`）、`supplier_deposit/`、`cost/` 维护实体、值对象、状态迁移和确定性规则；`command_receipt.rs` 维护回执结构与匹配合同。不读写数据库。 |
| [dto/](src/dto/mod.rs)、[ports/](src/ports/mod.rs) | DTO 定义财务请求、查询参数和返回视图。端口层定义需要外部提供的窄能力或事实类型，例如组织筛选展开、原付款申请占用恢复所需的合同，以及应收使用的销售版本快照。 |
| [service/](src/service/mod.rs) | [应收](src/service/receivable.rs)、[应付](src/service/payable.rs)、[成本](src/service/cost.rs)、[预存款](src/service/supplier_deposit/mod.rs)与[回执](src/service/command_receipt.rs)组织本域查询和命令步骤：装载事实、调用领域规则，并按调用方执行器完成财务读写。 |
| [repository/](src/repository/mod.rs) | 财务 MongoDB 集合访问器、过滤分页、批量事实查询及条件写入。应收、应付、成本各有仓储模块，付款申请和提交快照在应付仓储下；预存款、资金证据和命令回执各有对应仓储。 |
| [indexes/](src/indexes/mod.rs) | 维护查询索引与唯一约束，公开整体和分组注册入口。应用组合根使用分组入口，保持既有跨领域逐集合注册顺序（见[服务端索引接线](../../../apps/erp-server/src/indexes.rs)）。 |

## 架构设计

### 依赖与协作边界

术语与统一分工见[总索引](../../README.md#代码中常见名称的含义)。常规、构建和测试依赖均不得直接指向其他业务领域或组合层；[后端依赖规则](../../README.md#依赖与接入要求)允许使用 `erp-identity` 的公开身份与授权服务。外域事实通过窄 Port 或显式输入提供，由组合层或应用接线。金额、单价、数量和税率复用 [erp-core::money](../../foundation/erp-core/src/money.rs)。

- **领域规则与持久化**：Entity 和值对象决定状态迁移、金额守恒等不变式；Service 装载事实、核验对象关系并组织本域步骤；Repository 复用调用方 `persistence_core::Executor`，用条件更新和唯一索引保护并发写入。跨集合原子写由拥有事务的 Service 或 Process 控制，仓储和被调用的事务内步骤不另开事务。
- **过账编排**：[erp-finance-posting](../../orchestration/erp-finance-posting/README.md) 将回款、付款申请、供应商付款、开票申请、发票和附加费用等财务写入与来源校验、审批、任务、审计和回执协调；审批及任务规则仍归 `erp-workflow`。其他链路由相应 Process 注入事实、复用财务步骤：[供应结算复核](../../orchestration/erp-supply-settlement/src/supply_settlement/review_posting.rs)协调应付与成本的写入，[供应商订单编排](../../orchestration/erp-supplier-orders/src/supplier_deposits/mod.rs)处理预存款跨域资格与资金调查。退款与收付款冲正单据归 [erp-returns](../erp-returns/README.md)，本域负责由此产生的财务反向分配与余额调整；外部 HTTP、文件存储等调用不放入数据库事务。
- **报表与展示**：本域提供财务明细、汇总所需事实与[按销售单读取成本归属](src/repository/cost/profit_loss.rs)等窄查询。[erp-read-models](../../read-models/erp-read-models/README.md) 的[财务读模型](../../read-models/erp-read-models/src/finance/mod.rs)组合外域展示与审批上下文；[非卡券实际经营盈亏](../../read-models/erp-read-models/src/finance/actual_profit_loss/mod.rs)在一致快照中读取正式收入与成本归属、计算盈亏和覆盖率，不写入财务事实，也不根据供应商猜测成本归属。

### 协作示例

客户回款过账时，[回款 Process](../../orchestration/erp-finance-posting/src/finance_posting/receivable/customer_receipt_posting.rs)在审批运行事务中调用[财务核销](src/service/receivable/customer_receipt_posting.rs)校验回款与应收余额、更新子账并写入回款分配，再记录成功审计并刷新相关销售单的回款进度。财务中心的混合视图由 ReadModel 组装，不承担过账职责。

### 修改执行要求

1. 进项发票登记、供应商付款、客户回款登记与分配、销项发票登记、开票申请提交或撤销等使用独立财务回执的命令，必须在同一事务提交 `finance_command_receipts`、正式财务事实和成功审计。回放须核对原命令身份、定位及版本化指纹，验证强类型结果后按当前授权读取原对象的当前视图。异载荷或损坏回执必须明确失败，不反查审计，不追加第二次成功事件；回执不得随日志归档删除，也不得设置 TTL。
2. 采购付款申请提交、撤回和关闭复用独立回执。在同一事务写入申请、来源应付占用、相应审批或执行任务、成功审计和回执。批准仅授予本轮冻结的逐采购额度，不得修改已批准内容或挪用额度。[银行付款执行](../../orchestration/erp-finance-posting/src/finance_posting/payable/payment.rs)先扣减原申请对应占用，再核销应付，任一步失败整体回滚。来源余额、正式采购版本、主体和收款账户由 Process 在同一事务复验。关闭只释放未付占用；冲正保留原付款与申请的关系，已关闭申请不得恢复原授权。
3. [每轮付款申请提交](../../orchestration/erp-finance-posting/src/finance_posting/payable/payment_request/submit.rs)须在创建审批实例的同一事务插入完整、不可变的提交快照。快照按「申请 ID + 提交版本」和「审批实例 ID」分别唯一；[历史审核](../../read-models/erp-read-models/src/finance/payment_request/submissions.rs)只能按精确关系装载快照，缺失时拒绝，不能补读当前申请。
4. 新增或调整集合查询时同步评估索引。HTTP 请求和响应优先复用领域 DTO；变更公开合同时检查 Process、ReadModel 和应用调用方。业务代码改动补充本域库单元测试，覆盖成功、失败、边界和适用的幂等或版本冲突路径。

## 验证要求

仅改文档时按[后端开发规则](../../../AGENTS.md#验证)执行 `git diff --check`。业务代码变更时，在 `backend/` 执行定向库单元测试，并按[统一质量门禁](../../README.md#质量门禁)验证；不运行集成测试或依赖真实外部服务的测试。

```bash
env -u ERP_TEST_MONGO_URI cargo test -p erp-finance --lib --locked
```
