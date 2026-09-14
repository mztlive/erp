# erp-customer：客户账户与分配

管理“哪些往来对象是我们的客户”，以及客户账户状态、负责人和客户分配记录。

客户账户表达与公司的客户关系；联系人、地址和税务资料属于往来主体。将客户职责单独维护，可以独立管理客户启停、负责人和可访问客户范围。

## 使用场景

- 调整客户创建、列表、详情、状态及客户数据范围查询。
- 分配或结束客户负责人关系，维护客户资料命令的重放结果。

## 协作示例

将客户分配给业务员时，由本 crate 维护分配事实；保存客户联系人时，由 [erp-processes](../erp-processes/README.md) 协调客户与 [erp-party](../erp-party/README.md)；客户中心展示合同、销售和应收汇总时使用 [erp-read-models](../erp-read-models/README.md)。

## 负责的数据与能力

- 客户账户、状态、客户负责人分配与分配命令规则。
- 客户资料命令事实、请求指纹、结果记录和重放校验。

## 依赖与协作边界

术语与统一分工见[总索引](../README.md#代码中常见名称的含义)。

- 本领域的数据结构、业务规则、服务和数据库仓储在此维护。常规、构建和测试依赖均不得指向其他业务领域或组合层。
- 主体身份通过 PartyFactPort 获取；账号资格和审计通过各自 Port 获取。
- 跨主体资料写入由 erp-processes::customer_profile 编排；客户中心聚合查询属于 erp-read-models。
- 需要其他领域的能力时，由组合层或应用将本域接口接到实际提供方；公开入口见 [src/lib.rs](src/lib.rs)。

## 代码入口

| 入口 | 用途 |
| --- | --- |
| [src/service/customer/mod.rs](src/service/customer/mod.rs) | CustomerService 与 CustomerAssignmentService |
| [src/ports/mod.rs](src/ports/mod.rs) | 主体、账号和审计事实合同 |
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
cargo check -p erp-customer --locked
env -u ERP_TEST_MONGO_URI cargo test -p erp-customer --lib --locked
```

代码变更还须执行[统一质量门禁](../README.md#质量门禁)。测试范围限定为库单元测试，不执行集成测试或真实外部服务测试。
