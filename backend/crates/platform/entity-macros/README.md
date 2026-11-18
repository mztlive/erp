# entity-macros：实体与 ID 过程宏

## 职责

本 crate 在编译期生成两类重复代码：`#[derive(Entity)]` 为实体实现基础元数据访问，`id_type!` 为字符串 ID 生成独立的 Rust 类型。宏不定义实体元数据或业务规则，也不分配 ID 值。

### 使用场景

- 持久化实体已嵌入 `BaseModel`，需要让仓储以统一接口读写元数据时，派生 `Entity`。
- 新声明的 ID 在 Rust 中需要与其他 ID 区分，但序列化仍须保持字符串时，使用 `id_type!`。

## 目录结构

### 代码入口

| 文件 | 内容 |
| --- | --- |
| [Cargo.toml](Cargo.toml) | 声明 `proc-macro` 库及 `syn`、`quote` 依赖。 |
| [src/lib.rs](src/lib.rs) | `derive_entity`、`id_type` 两个宏入口、各自的展开函数及内联库单元测试。 |

## 架构设计

宏入口用 `syn` 解析输入，用 `quote` 生成交给消费 crate 编译的代码。生成代码引用 `::entity_core` 或 `::serde`；这些依赖由消费 crate 声明，而不是由宏 crate 引入。

### `#[derive(Entity)]`：访问现有元数据

- **输入**：含 `base: entity_core::BaseModel` 的具名字段结构体。宏检查结构体形态及 `base` 字段名；不符合时报告编译错误。字段类型由展开后的 Rust 代码检查，宏不比较类型的写法。
- **生成合同**：保留原有泛型参数与 `where` 约束，实现 `::entity_core::HasBaseModel`。`base(&self)` 和 `base_mut(&mut self)` 分别返回 `self.base` 的共享引用和可变引用；宏不新增字段，也不初始化元数据。
- **接入要求**：消费 crate 必须声明 `entity-core` 依赖。实体的 Serde 派生及需要的 `#[serde(flatten)]` 由消费方显式声明。宏不生成 Repository，也不校验业务字段。

### `id_type!`：以独立类型包装字符串

- **输入**：单个类型标识符，例如 `id_type!(SalesOrderId)`；不接受可见性修饰、类型路径或附加参数。
- **生成合同**：声明公开的新类型，唯一字段是私有的 `String`。提供 `new(value: impl Into<String>)`、`Deref<Target = str>`、`AsRef<str>`、`From<String>` 与 `Display`，并派生 `Debug`、`Clone`、`PartialEq`、`Eq`、`Hash`、`Serialize` 和 `Deserialize`。不同 ID 不可直接混用；Serde 的透明表示仍是字符串，例如 JSON 和 BSON 中都不会多包一层对象。
- **接入要求**：直接调用 `id_type!` 的 crate 必须声明启用 `derive` feature 的 `serde` 依赖。构造和反序列化不分配 ID，也不校验字符串内容或格式。

### 使用与边界要求

| 协作 crate | 职责边界 |
| --- | --- |
| [entity-core](../entity-core/src/lib.rs) | 定义 `BaseModel`、`HasBaseModel` 及其默认方法，负责 ID、版本和时间等元数据的结构与初始化；`Entity` 只生成对现有 `base` 的访问实现。 |
| [erp-core](../../foundation/erp-core/src/ids.rs) | 通过 `ids` 模块声明和公开 ERP 共享 ID（如 `SalesOrderId`、`SalesOrderLineId`）；`id_type!` 只提供类型模板，消费方应复用已声明的共享 ID。 |
| [id-generator](../id-generator/src/lib.rs) | `next_id()` 生成内部主键字符串，[`DocumentNumberGenerator`](../id-generator/src/document_number.rs) 分配可展示的业务编号。调用方可以把已生成或取得的 ID 字符串交给 `new`；宏不负责取号，也不区分字符串的业务来源。 |

### 协作示例

[销售建单服务](../../domains/erp-sales/src/service/sales_order/lifecycle.rs) 使用 `SalesOrderId::new(id_generator::next_id())` 生成并包装内部主键。[`SalesOrder` 实体](../../domains/erp-sales/src/entity/sales_order/entity/order.rs) 负责业务校验，把 ID 转回字符串交给 `BaseModel::new`，并显式标注 `#[serde(flatten)]`；`Entity` 只提供仓储可复用的元数据访问。[`erp-core` 的 ID 单元测试](../../foundation/erp-core/src/ids.rs) 覆盖了 JSON 和 BSON 的字符串表示。

## 验证要求

仅修改本文档时，在 `backend/` 运行 `git diff --check -- crates/platform/entity-macros/README.md`，不运行构建或测试。修改宏代码时，在 `backend/` 执行库单元测试：

```bash
env -u ERP_TEST_MONGO_URI cargo test -p entity-macros --lib --locked
```

更改展开代码后，还须检查实际消费 crate（例如 `erp-core` 和 `erp-sales`）；仅编译宏 crate 或检查展开出来的 token，不能验证生成代码的类型约束。按[后端开发规则](../../../AGENTS.md#验证)执行受影响 crate 的定向检查，CI 执行[统一质量门禁](../../README.md#质量门禁)。测试仅限库单元测试，不运行集成测试或依赖真实 MongoDB 等外部服务的测试。
