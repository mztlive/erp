# erp-read-models：跨领域查询与展示投影

## 职责合同

- 工作台、客户中心、履约队列及各业务中心的混合查询和展示数据。
- 财务汇总、列表视图以及正式任务事实的统一读取。

## 使用与边界要求

1. 允许读取多个领域的公开仓储事实；不得依赖 erp-processes，亦不得作为业务领域依赖。
2. 业务事实和权限政策仍由拥有领域维护；本 crate 负责聚合与展示，不执行正式业务命令。
3. 通过拥有领域 Repository 和窄合同取数；不得绕过其集合边界或复制事实写入规则。
4. 调整列表或统计时保持同一授权范围，验证过滤、排序、分页、总数和明细口径一致。

## 代码入口

| 入口 | 用途 |
| --- | --- |
| [src/lib.rs](src/lib.rs) | 中心页与工作台公开模块 |
| [src/workbench/mod.rs](src/workbench/mod.rs) | WorkbenchReadService 与工作项视图 |
| [src/customer_center/mod.rs](src/customer_center/mod.rs) | CustomerCenterReadService |
| [src/fulfillment_queue/mod.rs](src/fulfillment_queue/mod.rs) | 履约队列事实和查询 |
| [src/finance/mod.rs](src/finance/mod.rs) | 财务聚合读取 |
| [src/ports/work_item_authorization.rs](src/ports/work_item_authorization.rs) | 工作项授权合同 |

## 验证要求

以下命令在 `backend/` 目录执行：

```bash
cargo check -p erp-read-models --locked
env -u ERP_TEST_MONGO_URI cargo test -p erp-read-models --lib --locked
```

代码变更还须执行[统一质量门禁](../README.md#质量门禁)。测试范围限定为库单元测试，不执行集成测试或真实外部服务测试。
