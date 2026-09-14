# entity-core：实体基础元数据

提供持久化实体共用的基础字段：ID、版本号、创建时间、更新时间和软删除时间。

业务实体通过 BaseModel 使用相同的元数据结构，仓储通过 HasBaseModel 统一访问这些字段，减少重复的字段与访问实现。

## 使用场景

- 定义需要持久化的新业务实体，嵌入 BaseModel。
- 调整基础元数据访问或软删除状态判断。

## 协作示例

一个销售单实体包含业务字段和 base 字段；base 记录 ID、版本及时间，销售状态规则仍由 [erp-sales](../erp-sales/README.md) 实现。[entity-macros](../entity-macros/README.md) 可以生成 HasBaseModel 的访问代码。

## 负责的数据与能力

- BaseModel 和 HasBaseModel，提供实体持久化元数据的统一访问。
- 软删除的零值常量与删除状态判定。

## 使用与边界要求

1. BaseModel 字段为 id、version、created_at、updated_at、deleted_at；调用方通过 serde(flatten) 嵌入实体。
2. BaseModel::new 接收调用方生成的 ID，使用当前 Unix 秒初始化创建和更新时间，version 从 1 开始，deleted_at 为 0。
3. BaseModel::fake 仅用于测试；Default 的零值不能代替正式实体构造。
4. 本 crate 不生成 ID，不提供 CRUD，不执行领域校验；确定性时间场景应由调用方显式构造元数据。

## 代码入口

| 入口 | 用途 |
| --- | --- |
| [src/lib.rs](src/lib.rs) | BaseModel、HasBaseModel 与软删除常量 |

## 验证要求

以下命令在 `backend/` 目录执行：

```bash
cargo check -p entity-core --locked
env -u ERP_TEST_MONGO_URI cargo test -p entity-core --lib --locked
```

代码变更还须执行[统一质量门禁](../README.md#质量门禁)。测试范围限定为库单元测试，不执行集成测试或真实外部服务测试。
