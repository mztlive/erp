# erp-processes：跨领域命令与事务编排

## 职责合同

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
| [src/supply_execution/mod.rs](src/supply_execution/mod.rs) | 供应执行 |

## 验证要求

以下命令在 `backend/` 目录执行：

```bash
cargo check -p erp-processes --locked
env -u ERP_TEST_MONGO_URI cargo test -p erp-processes --lib --locked
```

代码变更还须执行[统一质量门禁](../README.md#质量门禁)。测试范围限定为库单元测试，不执行集成测试或真实外部服务测试。
