# erp-read-models：跨领域查询与展示投影

组合多个业务领域的数据，返回客户中心、采购列表、工作台和财务报表等场景需要的查询结果。

一个页面往往同时需要客户、订单、财务和审批信息。由这一层统一取数、汇总和组装结果，可以保持领域独立，并集中维护展示口径。ReadModel 表示面向读取的数据形状及组装逻辑，包含查询服务和只读仓储。

## 使用场景

- 查询需要多个领域的信息，例如采购单列表补齐供应商名称、来源销售单号和负责人姓名。
- 提供中心页摘要、任务事实、统计或导出所需的跨域读取。

## 协作示例

客户中心的合同与销售摘要由 `CustomerCenterReadService::related` 提供，应收汇总由 `receivable` 提供；合同、销售单和应收记录各自由所属领域维护。一个只查询客户本身的普通列表仍由 [erp-customer](../erp-customer/README.md) 提供。

## 查询归属判定

| 查询或操作 | 归属 |
| --- | --- |
| 查询客户自身资料和分配记录 | erp-customer 的 Service/Repository |
| 查询客户关联的合同、销售和应收摘要 | 本 crate 的 customer_center |
| 查询采购列表并补齐供应商、销售单、负责人信息 | 本 crate 的 purchase_center |
| 查询工作台任务及关联单据摘要 | 本 crate 的 workbench |
| 执行订单提交、生效或财务过账 | erp-processes 协调相应领域 |

本 crate 中已有查询直接读取当前 MongoDB 业务数据并组装结果，例如客户中心聚合与采购列表关联事实读取。采用 ReadModel 不要求另建数据库或复制一套业务数据。独立 crate 的用途是明确跨域查询的依赖边界；查询性能仍须依据具体实现验证。

## 负责的数据与能力

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
| [src/customer_center/service.rs](src/customer_center/service.rs) | 客户最近合同、销售单及应收汇总 |
| [src/customer_center/repository/related.rs](src/customer_center/repository/related.rs) | 客户合同与销售单摘要的聚合查询 |
| [src/purchase_center/mod.rs](src/purchase_center/mod.rs) | 采购列表、中心页、创建依据与责任规则查询 |
| [src/purchase_center/repository/list_facts.rs](src/purchase_center/repository/list_facts.rs) | 当前采购页的供应商名称、销售单号和负责人批量读取 |
| [src/sales_center/order/mod.rs](src/sales_center/order/mod.rs) | 销售列表与详情中的财务、采购及审批信息 |
| [src/supplier_center/mod.rs](src/supplier_center/mod.rs) | 供应商供给、连接、履约与结算视图 |
| [src/finance/actual_profit_loss/mod.rs](src/finance/actual_profit_loss/mod.rs) | 正式收入、成本归属与完整性的实际盈亏查询 |
| [src/integration_center/mod.rs](src/integration_center/mod.rs) | 集成异常和对账差异的责任、证据详情 |
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
