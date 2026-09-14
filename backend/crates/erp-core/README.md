# erp-core：ERP 共享内核

提供整个 ERP 共用的基础业务值类型，例如金额、数量、税率、业务日期、ID 类型和字段更新方式。

销售、采购、库存和财务必须以一致方式表达数量、金额和时间。本 crate 统一这些值的含义、计算及序列化规则。

## 使用场景

- 使用或调整 Amount、数量、单价、税率等定点数类型。
- 维护共享 ID 类型、业务时间、基础校验和字段补丁语义。

## 协作示例

修改客户字段时，FieldUpdate 区分“未传入”“清空”“设置新值”；计算订单金额时使用 money 的定点类型。客户实体归 [erp-customer](../erp-customer/README.md)，内部主键值和业务编号的生成归 [id-generator](../id-generator/README.md)。

## 负责的数据与能力

- 金额、单价、数量、税率、稳定 ID、业务时间与通用校验原语。
- 字段更新语义、操作人类别和共享错误。

## 使用与边界要求

1. 不得放入账号、角色、审计日志、工作项或订单等业务实体。
2. 金额计算和持久化须复用 money 的定点类型与舍入规则，禁止使用浮点金额。
3. 字段补丁复用 FieldUpdate，保持未提供、清空与赋值的既有语义。
4. 不得依赖业务领域、组合层、HTTP 或 MongoDB 客户端；BSON 序列化按现有值对象合同维护。

## 代码入口

| 入口 | 用途 |
| --- | --- |
| [src/money.rs](src/money.rs) | 金额类型及金额计算 |
| [src/common/mod.rs](src/common/mod.rs) | 业务时间及通用值对象 |
| [src/ids.rs](src/ids.rs) | 共享稳定 ID |
| [src/field_update.rs](src/field_update.rs) | FieldUpdate |
| [src/identity.rs](src/identity.rs) | AccountKind |
| [src/validation.rs](src/validation.rs) | 校验原语 |

## 验证要求

以下命令在 `backend/` 目录执行：

```bash
cargo check -p erp-core --locked
env -u ERP_TEST_MONGO_URI cargo test -p erp-core --lib --locked
```

代码变更还须执行[统一质量门禁](../README.md#质量门禁)。测试范围限定为库单元测试，不执行集成测试或真实外部服务测试。
