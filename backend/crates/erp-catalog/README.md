# erp-catalog：商品目录与规格

管理公司卖的商品是什么，以及每种商品有哪些可区分的 SKU、规格和上下架状态。

商品、SKU、品牌、分类和规格需要有统一来源，销售选品与供应商供给均引用这些商品身份，避免各自建立一套商品主档。SKU 表示一个可独立识别的商品规格。

## 使用场景

- 新增或维护商品、SKU、品牌、分类、单位和规格属性。
- 调整商品修订、上下架及可售 SKU 查询。

## 协作示例

同一件商品有不同容量时，商品和各容量 SKU 在此维护；供应商为某个 SKU 提供的价格、税率和可供状态由 [erp-supply](../erp-supply/README.md) 维护，仓内数量由 [erp-inventory](../erp-inventory/README.md) 维护。

## 负责的数据与能力

- 商品、SKU、修订、上下架、品牌、分类、计量单位及卡券分类资料。
- 规格属性、属性值、规格签名计算和可售 SKU 查询。

## 依赖与协作边界

术语与统一分工见[总索引](../README.md#代码中常见名称的含义)。

- 本领域的数据结构、业务规则、服务和数据库仓储在此维护。常规、构建和测试依赖均不得指向其他业务领域或组合层。
- 规格签名与商品规则保留本域唯一实现；供应商供给通过消费方 Port 获取。
- 附件和审计通过 Port 注入；库存、价格结算及履约事实由其拥有领域维护。
- 需要其他领域的能力时，由组合层或应用将本域接口接到实际提供方；公开入口见 [src/lib.rs](src/lib.rs)。

## 代码入口

| 入口 | 用途 |
| --- | --- |
| [src/service/catalog/mod.rs](src/service/catalog/mod.rs) | CatalogService 与可售查询 |
| [src/entity/catalog/mod.rs](src/entity/catalog/mod.rs) | 商品、SKU 和规格值对象 |
| [src/ports/supply.rs](src/ports/supply.rs) | 可售供给查询合同 |
| [src/entity/mod.rs](src/entity/mod.rs) | 本域实体、值对象和确定性规则 |
| [src/repository/mod.rs](src/repository/mod.rs) | 本域 MongoDB 仓储与集合访问器 |
| [src/indexes/mod.rs](src/indexes/mod.rs) | 公开索引注册入口 |

## 修改执行要求

1. 将无 I/O 的校验、状态迁移和不变式放入本域实体或值对象；Service 组织本域用例。
2. Repository 使用调用方传入的 `persistence_core::Executor`；跨集合原子写入由本域用例或 Process 控制事务。
3. 新增或调整集合查询时同步评估索引；组合根复用本域公开索引入口，保持既有逐集合注册顺序。
4. HTTP 请求和响应优先复用本域 DTO；扩展公开合同须同步检查 Process、ReadModel 和应用调用方。
5. 业务改动补充本域库单元测试，覆盖成功、失败、边界及相关幂等或版本冲突路径。

## 验证要求

以下命令在 `backend/` 目录执行：

```bash
cargo check -p erp-catalog --locked
env -u ERP_TEST_MONGO_URI cargo test -p erp-catalog --lib --locked
```

代码变更还须执行[统一质量门禁](../README.md#质量门禁)。测试范围限定为库单元测试，不执行集成测试或真实外部服务测试。
