# erp-processes：跨领域命令与事务编排

执行需要多个业务领域共同完成的操作，协调调用顺序、事务、审计、审批动作和外部调用。

一个用户操作可能同时影响订单、库存、财务和待办。各领域维护自己的规则，Process 把这些步骤组成一个完整用例，使领域 crate 保持独立。

## 使用场景

- 一个命令需要多个领域的事实或写入，例如提交销售单、采购生效、收货过账、退货退款。
- 需要为领域接口接入其他领域的实际实现，或协调数据库事务与供应商、S3 调用。

## 协作示例

保存供应商完整档案时，supplier_profile 协调主体资料、供应商记录及命令结果的原子写入；各领域仍执行自己的规则。查询供应商中心展示数据时，使用 [erp-read-models](../erp-read-models/README.md) 的对应读取入口。

## 用例执行要求

1. 由 HTTP、CLI 或后台 worker 选择具体命名流程，传入当前操作人及请求参数。
2. Process 获取各领域需要的事实，将领域 Port 接入实际提供方，并调用拥有领域的校验与写入入口。
3. 需要原子性时，由约定的事务入口统一协调写入、审计与命令结果，向相关仓储传递同一个 Executor。
4. 需要外部调用时，先按既有用例保存调用意图，在事务外执行 HTTP/S3/供应商调用，再按流程确认结果。
5. 读取型关联信息复用 ReadModel 或领域读取合同；业务状态与金额数量规则继续由所属领域维护。

`Process` 在本项目中表示完整业务用例的协调代码，不是操作系统进程。`order_to_cash`、`procure_to_pay` 等名称用于归组销售和采购相关流程，具体支持的动作以模块公开入口为准。

## 负责的数据与能力

- 命名跨域用例、领域 Port 的实际适配器、审批动作分派与审计事务。
- 订单到收款、采购到付款、销售变更、履约、逆向及财务过账流程。
- 导入应用、资料保存、附件关联、供应执行、治理、结算和集成问题解决。

## 使用与边界要求

1. 允许依赖业务领域和 erp-read-models；业务领域与 erp-read-models 均不得反向依赖本 crate。
2. Process 调用拥有领域的 Service、Repository 或窄 Port；不得自行打开外域 collection 或复制领域不变式。
3. 一个原子用例沿用同一 Executor，保留写入顺序、幂等恢复、审计及错误映射合同。
4. 外部 HTTP、S3 和供应商 I/O 必须位于数据库事务之外；按既有流程持久化意图、执行外部调用、再确认结果。
5. 应用入口选择具体命名 Process；不得重新建立通用 services 中转层。

## 代码入口

| 入口 | 用途 |
| --- | --- |
| [src/lib.rs](src/lib.rs) | 公开命名用例与模块总表 |
| [src/adapters/mod.rs](src/adapters/mod.rs) | 实际消费方适配器 |
| [src/approval_dispatch/mod.rs](src/approval_dispatch/mod.rs) | ApprovalActionRegistry 与审批分派 |
| [src/audit/mod.rs](src/audit/mod.rs) | run_audited |
| [src/order_to_cash/mod.rs](src/order_to_cash/mod.rs) | 销售到收款流程 |
| [src/procure_to_pay/mod.rs](src/procure_to_pay/mod.rs) | 采购到付款流程 |
| [src/fulfillment_execution/mod.rs](src/fulfillment_execution/mod.rs) | 履约执行 |
| [src/reverse_flow/mod.rs](src/reverse_flow/mod.rs) | 逆向流程 |
| [src/supply_execution/mod.rs](src/supply_execution/mod.rs) | 供应商侧下单与结果确认 |
| [src/supplier_profile/mod.rs](src/supplier_profile/mod.rs) | 保存供应商及主体完整档案 |
| [src/customer_profile/mod.rs](src/customer_profile/mod.rs) | 保存客户及主体完整档案 |
| [src/sales_change/mod.rs](src/sales_change/mod.rs) | 销售变更与应收差额、相关任务协调 |
| [src/finance_posting/mod.rs](src/finance_posting/mod.rs) | 财务过账及跨域事实写入 |
| [src/product_import/mod.rs](src/product_import/mod.rs) | 商品模板解析、任务登记和逐行导入 |
| [src/supplier_import/mod.rs](src/supplier_import/mod.rs) | 供应商导入任务和逐行执行 |
| [src/import_apply/mod.rs](src/import_apply/mod.rs) | 历史数据导入的正式应用 |
| [src/supply_settlement/mod.rs](src/supply_settlement/mod.rs) | 供应商结算、复核和财务关联 |
| [src/supplier_connection_execution/mod.rs](src/supplier_connection_execution/mod.rs) | 供应商连接后台任务与事务外调用 |
| [src/integration_resolution/mod.rs](src/integration_resolution/mod.rs) | 集成问题处理与正式业务动作协调 |

## 验证要求

以下命令在 `backend/` 目录执行：

```bash
cargo check -p erp-processes --locked
env -u ERP_TEST_MONGO_URI cargo test -p erp-processes --lib --locked
```

代码变更还须执行[统一质量门禁](../README.md#质量门禁)。测试范围限定为库单元测试，不执行集成测试或真实外部服务测试。
