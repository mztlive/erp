# erp-import：历史数据导入事实

保存历史业务数据导入的批次、逐行处理状态、业务确认记录和最终应用结果。

历史数据从文件进入正式业务前，需要保留解析、身份映射、确认和应用的过程事实，以便查询每一行当前状态和处理结果。

## 使用场景

- 修改历史导入批次、行状态、确认规则或结果查询。
- 维护导入应用结果及导入命令的身份、重放规则。

## 协作示例

一批历史数据解析后，需要逐行确认并应用到正式业务。导入记录归本 crate，正式写入由 [erp-processes](../erp-processes/README.md) 的 import_apply 协调，后台任务状态归 [erp-support](../erp-support/README.md)。商品和供应商模板导入分别有 product_import、supplier_import 流程，应按具体入口定位。

## 负责的数据与能力

- 历史导入批次、明细行、确认记录与应用结果事实。
- 导入批次和行查询，以及依赖后台任务事实的查询合同。

## 依赖与协作边界

术语与统一分工见[总索引](../README.md#代码中常见名称的含义)。

- 本领域的数据结构、业务规则、服务和数据库仓储在此维护。常规、构建和测试依赖均不得指向其他业务领域或组合层。
- 正式应用导入结果由 erp-processes::import_apply 编排，不得直接依赖被导入的业务领域。
- 任务登记属于 erp-support，通过 BulkJobFactsPort 取得所需任务事实。
- 需要其他领域的能力时，由组合层或应用将本域接口接到实际提供方；公开入口见 [src/lib.rs](src/lib.rs)。

## 代码入口

| 入口 | 用途 |
| --- | --- |
| [src/service/legacy_import/mod.rs](src/service/legacy_import/mod.rs) | LegacyImportService |
| [src/ports/mod.rs](src/ports/mod.rs) | BulkJobFactsPort |
| [src/entity/legacy_import/mod.rs](src/entity/legacy_import/mod.rs) | 导入实体与结果合同 |
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
cargo check -p erp-import --locked
env -u ERP_TEST_MONGO_URI cargo test -p erp-import --lib --locked
```

代码变更还须执行[统一质量门禁](../README.md#质量门禁)。测试范围限定为库单元测试，不执行集成测试或真实外部服务测试。
