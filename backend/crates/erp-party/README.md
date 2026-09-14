# erp-party：往来主体与敏感资料

管理往来公司或个人的基础身份，以及联系人、地址、银行账户、税务资料和敏感字段。

同一个往来主体可以被客户账户或供应商账户引用。共享身份和资料在此维护，客户关系、供应商关系各自由相应领域管理。

## 使用场景

- 修改往来主体及其修订、联系人、地址、银行和税务资料。
- 调整敏感资料加解密、查询指纹或受限查看令牌。

## 协作示例

某家公司同时是客户和供应商时，往来主体资料由本 crate 提供；客户负责人归 [erp-customer](../erp-customer/README.md)，供应商能力和资质归 [erp-supplier](../erp-supplier/README.md)。跨域资料保存使用对应 Process，并执行主体资料的写入限制。

## 负责的数据与能力

- 往来主体稳定身份、修订，以及联系人、地址、银行账户和税务资料。
- 敏感资料加解密、查询指纹和限域查看令牌。

## 依赖与协作边界

术语与统一分工见[总索引](../README.md#代码中常见名称的含义)。

- 本领域的数据结构、业务规则、服务和数据库仓储在此维护。常规、构建和测试依赖均不得指向其他业务领域或组合层。
- 客户账户及分配属于 erp-customer；供应商账户及资质属于 erp-supplier。
- 共享主体资料必须通过本域规则维护；不得在客户或供应商流程复制敏感数据编解码逻辑。
- 需要其他领域的能力时，由组合层或应用将本域接口接到实际提供方；公开入口见 [src/lib.rs](src/lib.rs)。

## 代码入口

| 入口 | 用途 |
| --- | --- |
| [src/service/party/mod.rs](src/service/party/mod.rs) | 主体及从属资料服务 |
| [src/service/party/sensitive.rs](src/service/party/sensitive.rs) | SensitiveDataCodec 与查看令牌合同 |
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
cargo check -p erp-party --locked
env -u ERP_TEST_MONGO_URI cargo test -p erp-party --lib --locked
```

代码变更还须执行[统一质量门禁](../README.md#质量门禁)。测试范围限定为库单元测试，不执行集成测试或真实外部服务测试。
