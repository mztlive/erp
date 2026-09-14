# erp-supply：供给、供应商连接、履约与结算

管理公司 SKU 的供应来源，以及供应商接口连接、供应商侧订单执行结果和结算单。

供应商主档只说明合作对象；实际供应还需要商品供给、商业条款、接口能力、履约回执及结算记录。本 crate 按这四组业务维护相应规则和数据。

## 使用场景

- 修改供应商供给、商业条款修订、实时可供状态及资格校验。
- 修改供应商 API 连接、命令意图、履约回执或结算差异规则。

## 协作示例

公司 SKU 由 [erp-catalog](../erp-catalog/README.md) 定义，供应商资质由 [erp-supplier](../erp-supplier/README.md) 定义；本 crate 建立“这个供应商供应这个 SKU”的供给关系，并维护供给价格、税率和可供状态。外部下单调用由 Process 执行，结果再按本域规则登记。

## 负责的数据与能力

| 业务组 | 负责内容 | 示例 |
| --- | --- | --- |
| supplier_offering | 公司 SKU 与供应商的供给关系、商业条款修订、实时可供状态 | 为一个 SKU 登记供应商报价，改价时保留原商业修订 |
| supplier_api | 供应商 API 连接、能力、调用意图、确认与引用登记 | 登记连接支持的能力，保存一次调用意图及确认记录 |
| supplier_fulfillment | 供应商侧履约订单、动作、状态历史、结果与退款事实 | 保存外部下单结果和供应商回执 |
| supplier_settlement | 供应商结算单、明细、差异及复核规则 | 对账后保留结算差异及其处理结果 |

## 依赖与协作边界

术语与统一分工见[总索引](../README.md#代码中常见名称的含义)。

- 本领域的数据结构、业务规则、服务和数据库仓储在此维护。常规、构建和测试依赖均不得指向其他业务领域或组合层。
- 供应商账户、资质与商业档案由 erp-supplier 提供；通过 Port 获取资格和引用事实。
- 外部网关调用与跨域事务由 erp-processes 的 supply_* 和 supplier_connection_execution 模块编排，不得把外部 I/O 放入数据库事务。
- 需要其他领域的能力时，由组合层或应用将本域接口接到实际提供方；公开入口见 [src/lib.rs](src/lib.rs)。

## 代码入口

| 入口 | 用途 |
| --- | --- |
| [src/service/supplier_offering/mod.rs](src/service/supplier_offering/mod.rs) | 供给创建、商业修订与可供状态 |
| [src/service/supplier_api/mod.rs](src/service/supplier_api/mod.rs) | 连接、能力和命令意图 |
| [src/service/supplier_fulfillment/mod.rs](src/service/supplier_fulfillment/mod.rs) | 供应商侧订单与结果回执 |
| [src/service/supplier_settlement/mod.rs](src/service/supplier_settlement/mod.rs) | 结算单、差异和复核 |
| [src/ports/mod.rs](src/ports/mod.rs) | 供给资格、网关及引用登记合同 |
| [src/indexes/mod.rs](src/indexes/mod.rs) | 四组索引入口 |
| [src/entity/mod.rs](src/entity/mod.rs) | 本域实体、值对象和确定性规则 |
| [src/repository/mod.rs](src/repository/mod.rs) | 本域 MongoDB 仓储与集合访问器 |

## 修改执行要求

1. 将无 I/O 的校验、状态迁移和不变式放入本域实体或值对象；Service 组织本域用例。
2. Repository 使用调用方传入的 `persistence_core::Executor`；跨集合原子写入由本域用例或 Process 控制事务。
3. 新增或调整集合查询时同步评估索引；组合根复用本域公开索引入口，保持既有逐集合注册顺序。
4. HTTP 请求和响应优先复用本域 DTO；扩展公开合同须同步检查 Process、ReadModel 和应用调用方。
5. 业务改动补充本域库单元测试，覆盖成功、失败、边界及相关幂等或版本冲突路径。

## 验证要求

以下命令在 `backend/` 目录执行：

```bash
cargo check -p erp-supply --locked
env -u ERP_TEST_MONGO_URI cargo test -p erp-supply --lib --locked
```

代码变更还须执行[统一质量门禁](../README.md#质量门禁)。测试范围限定为库单元测试，不执行集成测试或真实外部服务测试。
