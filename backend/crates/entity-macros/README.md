# entity-macros：实体与 ID 过程宏

在编译时生成重复的实体基础字段访问代码，以及具有独立 Rust 类型的字符串 ID。

多个实体都要实现相同的元数据访问，多种 ID 都需要字符串转换和序列化。过程宏将这些重复实现集中生成。

## 使用场景

- 给带 base 字段的实体派生 Entity，生成 HasBaseModel 实现。
- 使用 id_type! 声明 ID 类型，避免不同对象的 ID 在类型层面混用。

## 协作示例

id_type!(ExampleId) 生成字符串包装类型及转换代码；真正的 ID 值由 [id-generator](../id-generator/README.md) 或调用方提供。Entity 宏使用 [entity-core](../entity-core/README.md) 的 BaseModel，实体的业务验证仍由领域代码实现。

## 负责的数据与能力

- Entity 派生宏，为带 base 字段的类型实现 entity_core::HasBaseModel。
- id_type! 生成透明字符串 ID newtype 及访问、转换和序列化实现。

## 使用与边界要求

1. Entity 要求目标类型含有名为 base、类型为 entity_core::BaseModel 的字段；消费 crate 必须声明 entity-core 依赖。
2. id_type! 接受单个类型标识符，消费 crate 必须声明 serde 依赖；生成类型不负责 ID 分配或格式校验。
3. 宏不生成 Repository、不校验业务字段，也不自动插入 serde(flatten)；这些合同由消费方显式声明。
4. 修改展开代码后须检查实际消费 crate；只编译过程宏本身不能验证生成代码的类型约束。

## 代码入口

| 入口 | 用途 |
| --- | --- |
| [src/lib.rs](src/lib.rs) | derive_entity 与 id_type 宏实现 |

## 验证要求

以下命令在 `backend/` 目录执行：

```bash
cargo check -p entity-macros --locked
env -u ERP_TEST_MONGO_URI cargo test -p entity-macros --lib --locked
```

代码变更还须执行[统一质量门禁](../README.md#质量门禁)。测试范围限定为库单元测试，不执行集成测试或真实外部服务测试。
