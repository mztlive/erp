# erp-core：ERP 共享内核

## 职责

`erp-core` 为销售、采购、库存和财务等领域定义共享的业务值类型与规则。各域用同一套类型传递金额、数量、日期和标识，避免分别定义精度、舍入和序列化规则；业务数据仍由所属领域维护。

### 负责的数据与能力

- 提供金额、单价、数量、税率、稳定 ID、业务时间和自然周期，以及 `StableBase`、`RevisionBase`、`FactBase` 等公共字段基元。
- 提供可空字段更新意图、固定状态迁移校验、操作人类别 `AccountKind`、文本和联系方式校验，以及共享 `Error` / `Result`。
- 提供行政区划、配送规则判定与覆盖汇总，以及后台投影使用的变更扫描时间窗和进度计算。
- `assignment` 为采购与财务提供保序的负责人池校验和轮询选择；领域持有分派规则实体和轮次状态，本 crate 不推进轮次。

### 使用场景

- 需要跨域传递或计算金额、数量、日期、ID 时，复用已有类型，不另建一套表示。
- 更新可空字段时，用 `FieldUpdate` 表达保留、清空或设置；字段是否允许清空、如何规范化，由所属领域决定。

## 目录结构

### 代码入口

| 路径 | 职责 |
| --- | --- |
| [Cargo.toml](Cargo.toml)、[src/lib.rs](src/lib.rs) | 依赖清单、公开模块及 `Error`、`Result`、`FieldUpdate`、`AccountKind` 等重导出。 |
| [src/money.rs](src/money.rs)、[src/ids.rs](src/ids.rs) | 定点数值、舍入与拆税；各域稳定主表、修订和行记录的类型化 ID。 |
| [src/common/](src/common/mod.rs)（[基元选用说明](src/common/README.md)） | 自然日与时刻、自然周期、来源、公共字段和状态迁移校验。 |
| [src/field_update.rs](src/field_update.rs)、[src/validation.rs](src/validation.rs) | 可空字段补丁、文本规范化和联系方式校验。 |
| [src/identity.rs](src/identity.rs)、[src/errors.rs](src/errors.rs) | 稳定操作人类别和共享业务值错误，不定义账号实体或授权逻辑。 |
| [src/region.rs](src/region.rs)、[data/regions.tsv](data/regions.tsv) | 内置省、市、区县数据及区划代码校验。 |
| [src/delivery_policy.rs](src/delivery_policy.rs)、[src/delivery_coverage.rs](src/delivery_coverage.rs) | 按最具体的地区例外判定代发或集采规则，并汇总 SKU 的包邮和可送覆盖。 |
| [src/assignment.rs](src/assignment.rs)、[src/change_scan.rs](src/change_scan.rs) | 保序轮询与负责人池校验；按 `updated_at` 计算即时扫描和延迟复扫的半开时间窗。 |

## 架构设计

### 使用与边界要求

- **依赖单向。** 业务域使用本 crate 的值类型；本 crate 不依赖业务领域、组合层、HTTP 框架或 MongoDB 客户端。`bson` 依赖仅用于维持值的序列化合同，不负责数据库读写。
- **共享语义，不共享实体。** 账号、角色、审计日志、工作项、客户和订单不放在本 crate。各域自己定义业务状态和合法迁移，复用 `DocumentState` 与 `ensure_transition`；按对象性质组合公共字段，[`entity-core`](../../platform/entity-core/README.md) 的 `BaseModel` 仍负责持久化元数据。配送模板、分派规则等数据和用例也留在所属领域。
- **共享类型不替代业务校验。** 例如 `Quantity` 限制有效小数位，但[库存流水](../../domains/erp-inventory/src/entity/inventory/stock_movement.rs)要求数量为正，增减方向另设字段；规则由库存域校验，而不是在所有 `Quantity` 上全局禁用零或负值。

### 协作示例

**金额与数量：** [销售明细](../../domains/erp-sales/src/entity/sales_order/types.rs)和[采购明细](../../domains/erp-procurement/src/entity/purchase_order/line_amounts.rs)用 `line_amounts` 计算商品行金额。含税单价 `9.9900` 元、数量 `3.000000`、税率 `0.130000`（13%）得到含税额 `29.97` 元；按本系统口径，以含税额乘税率并舍入得税额 `3.90` 元，再相减得净额 `26.07` 元，保证含税额精确等于净额加税额。`round_to_cent` 在计算中显式舍入到分，中点取偶（如 `0.025` 舍为 `0.02`）。

`Amount`、`UnitPrice`、`Quantity`、`Rate` 的有效小数位上限分别为 2、4、6、6；尾零不计入上限，`Amount` 可接受 `1.230000`，但拒绝 `1.234`，不会在构造时舍入；`Quantity` 同样拒绝 `1.2345678`。HTTP JSON 使用十进制字符串，例如 `"29.97"` 和 `"3.000000"`，不接受 JSON 整数或浮点数；内部运算也不使用浮点金额。持久化使用非 human-readable 的 BSON 路径，写成原生 `Decimal128`，不是 BSON 字符串；参见[持久化文档编码](../persistence-core/src/repository/base.rs)。

**日期与时刻：** [财务应付分录](../../domains/erp-finance/src/entity/payable/payable_entry.rs)的到期日用 `BusinessDate`，`"2026-09-30"` 是无时区的自然日，不转换成午夜时间戳；入账时刻 `posted_at` 用 `Instant`，例如序列化为 UTC Unix 整秒 `1700000000`（`i64`，不是毫秒或字符串）。展示时间点时再转业务时区；需要取业务当天时，用 `BusinessDate::today()` 按上海时间（UTC+08:00）取日。

**稳定 ID 与业务编号：** [采购行](../../domains/erp-procurement/src/entity/purchase_order/line_amounts.rs)和[库存流水](../../domains/erp-inventory/src/entity/inventory/stock_movement.rs)都用 `SkuId` 引用 SKU。比如源码测试中的 `SkuId::new("sku-1")` 序列化为字符串 `"sku-1"`，不附带类型标签；在 Rust 中它不能替代 `SkuRevisionId` 或 `WarehouseId`。ID 包装不校验值的格式或关联对象是否存在。[`id-generator`](../../platform/id-generator/README.md) 负责生成实际内部主键（无连字符的 32 位十六进制 UUID v4），也负责另行分配形如 `SO20260701-000123` 的业务单号。

**可空字段更新：** [客户账户的 `CustomerAccountUpdate`](../../domains/erp-customer/src/entity/customer/customer_account.rs)对 `default_payment_term_id` 使用 `FieldUpdate<String>`：`{}` 保留原付款条件（`Unchanged`），`{"default_payment_term_id":null}` 清空（`Clear`），`{"default_payment_term_id":"PREPAY_50"}` 设置新值（`Set`）。容器字段须用 `#[serde(default, skip_serializing_if = "FieldUpdate::is_unchanged")]`：缺失字段靠 `default` 读成 `Unchanged`；序列化必须省略 `Unchanged`，因为它单独序列化也是 `null`，读回就会变成 `Clear`。客户实体和更新规则归 [erp-customer](../../domains/erp-customer/README.md)。

## 验证要求

仅修改本文档时，按[后端开发规则](../../../AGENTS.md#验证)只对本文件执行 `git diff --check`。修改代码时，在 `backend/` 下按后端开发规则和[统一质量门禁](../../README.md#质量门禁)检查；本 crate 的定向测试命令为：

```bash
env -u ERP_TEST_MONGO_URI cargo test -p erp-core --lib --locked
```

测试范围限于库单元测试，不运行集成测试或依赖真实 MongoDB 等外部服务的测试。
