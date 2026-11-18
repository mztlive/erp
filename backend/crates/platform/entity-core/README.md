# entity-core：实体基础元数据

`entity-core` 是后端的通用技术组件。它定义持久化实体共用的元数据，以及仓储读写这些元数据的接口，不包含任何业务实体。

<a name="使用场景"></a>
<a name="负责的数据与能力"></a>

## 职责

- 提供 `BaseModel`，统一内部主键、乐观锁版本及创建、更新、软删除时间的表示和初始化规则。
- 提供 `HasBaseModel`，让仓储无需了解业务实体的字段布局即可读取或更新元数据；提供内存判定和 BSON 查询共用的未删除零值约定。
- 新增需要持久化的业务实体时，可复用这套契约。本 crate 不分配 ID 或单号，不实现领域校验、状态迁移、CRUD 或事务。

<a name="代码入口"></a>

## 目录结构

| 入口 | 用途 |
| --- | --- |
| [Cargo.toml](Cargo.toml) | crate 清单；生产依赖仅有启用派生的 `serde` 和用于取时的 `chrono`。 |
| [src/lib.rs](src/lib.rs) | `BaseModel`、`HasBaseModel`、未删除常量及内联库单元测试；本 crate 的生产实现均在此文件。 |

<a name="使用与边界要求"></a>

## 架构设计

### 元数据的表示与初始化

| 字段 | Rust 类型 | 含义 |
| --- | --- | --- |
| `id` | `String` | 调用方分配的内部主键；不是业务展示编号，也不是 MongoDB 的 `_id`。 |
| `version` | `u64` | 持久化写入的乐观锁版本，不是单据的业务修订号。 |
| `created_at`、`updated_at` | `u64` | 创建、更新时刻，单位为 Unix 秒。 |
| `deleted_at` | `u64` | 软删除时刻，单位为 Unix 秒。`0` 表示未删除，非零表示已删除。 |

- `BaseModel::new(id)` 接收已分配的 ID（例如由 [id-generator](../id-generator/README.md) 生成），取同一当前 Unix 秒设置 `created_at` 和 `updated_at`（早于 Unix 纪元的值按 `0` 处理），从版本 `1` 开始，`deleted_at` 为 `0`。
- `BaseModel::fake()` 供测试使用，采用固定 ID `fake` 和固定时间，版本与删除状态沿用正式构造的初始约定。`Default` 只产生空 ID 和数字零值，不能代替正式构造；生产路径需要确定性时间时，由调用方显式构造元数据并维护同样的初始约定。
- `BaseModel::is_deleted()` 只判断 `deleted_at != NOT_DELETED_TIMESTAMP`。`NOT_DELETED_TIMESTAMP` 是内存模型使用的 `u64` 零值；`NOT_DELETED_TIMESTAMP_BSON` 是 MongoDB 过滤条件使用的 `i64` 零值，两者表达同一未删除状态。

### 业务实体、访问 trait 与派生宏

- 业务实体持有自己的业务字段和规则，并显式在 `base: BaseModel` 上标记 `#[serde(flatten)]`。因此持久化文档使用顶层 `id`、`version`、`created_at`、`updated_at`、`deleted_at`，而不是嵌套的 `base` 文档；仓储按这些顶层字段匹配和更新。
- `HasBaseModel` 要求实现 `base(&self) -> &BaseModel` 和 `base_mut(&mut self) -> &mut BaseModel`，并提供基于 `base()` 的默认方法 `id()`、`version()` 和 `is_deleted()`。它只是访问接口，不执行并发校验、删除操作或数据写入。
- 消费方可手写 trait 实现，也可用 [entity-macros 的 `Entity` 派生宏](../entity-macros/src/lib.rs)。派生要求带 `base: BaseModel` 的具名结构体，消费 crate 需依赖 `entity-core`。宏只生成两个访问方法，不添加字段或 `serde(flatten)`，也不初始化元数据或生成仓储。

### 仓储如何落实契约

[persistence-core 的通用 `Repository`](../../foundation/persistence-core/src/repository/base.rs) 插入调用方已构造的实体，不替调用方初始化元数据。`find_by_id` 等活跃记录读取入口按 `deleted_at == NOT_DELETED_TIMESTAMP_BSON` 过滤；显式包含已删除记录的入口不加此过滤，通用 `search` 则使用领域筛选器提供的条件，不自动排除软删除记录。

`update`、`soft_delete` 和 `restore` 要求实体实现 `HasBaseModel`。它们沿用调用方的 `Executor`，以顶层 `id`、实体当前 `version` 和预期的软删除状态发起比较写入。更新和删除要求记录未删除，恢复要求记录已删除；状态不符或未匹配到记录时报乐观锁冲突。写入匹配后才同步实体内存中的新版本和更新时间：版本加一，软删除同时把 `deleted_at` 设为本次更新时间，恢复则将其清零。

<a name="协作示例"></a>

### 协作示例：销售单

[erp-sales](../../domains/erp-sales/README.md) 的 [`SalesOrder`](../../domains/erp-sales/src/entity/sales_order/entity/order.rs) 使用 `#[derive(Entity)]` 和显式展平的 `base` 字段。其构造函数将调用方提供的 `SalesOrderId` 转成字符串交给 `BaseModel::new`，销售单号校验和销售状态规则仍由销售域实现。[销售服务的保存入口](../../domains/erp-sales/src/service/sales_order/lifecycle.rs) 在领域状态迁移后，使用原执行器调用销售单仓储的通用 `update`，由仓储执行版本比较。

## 验证要求

仅改文档时，按[后端开发规则](../../../AGENTS.md#验证)只对改动文件执行 `git diff --check`。代码变更按[统一质量门禁](../../README.md#质量门禁)执行相应检查；本 crate 的库单元测试命令在 `backend/` 目录执行：

```bash
env -u ERP_TEST_MONGO_URI cargo test -p entity-core --lib --locked
```

不执行集成测试或依赖真实外部服务的测试。
