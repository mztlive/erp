# erp-supply：供给、供应商连接、履约与结算

## 职责合同

- 供应商供给及修订、可用性和资格合同。
- 供应商 API 连接能力、命令意图、确认与引用登记。
- 供应商履约、结果回执、退款结果及结算单和差异规则。

## 依赖与协作边界

- 本 crate 拥有本域实体、DTO、规则、Service、Repository、集合访问器和索引。normal、build、dev 依赖均不得指向其他业务领域或组合层。
- 供应商账户、资质与商业档案由 erp-supplier 提供；通过 Port 获取资格和引用事实。
- 外部网关调用与跨域事务由 erp-processes 的 supply_* 和 supplier_connection_execution 模块编排，不得把外部 I/O 放入数据库事务。
- 外域适配器在组合层或应用组合根装配；公开入口以 [src/lib.rs](src/lib.rs) 为准。

## 代码入口

| 入口 | 用途 |
| --- | --- |
| [src/service/mod.rs](src/service/mod.rs) | 四组供应业务服务 |
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
