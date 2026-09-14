# erp-support：来源登记、批量任务与文件资产

保存多个业务流程共用的业务支撑记录：外部编号映射、批量选择与后台任务、文件资产及附件关系。

批量处理和文件上传都需要可查询的业务记录。这里集中维护任务状态、文件元数据及归属关系，使各领域能够复用同一套记录能力。

## 使用场景

- 修改来源系统、外部标识与内部对象的映射。
- 修改后台任务和逐项状态、批量选择快照、文件资产及单据附件。

## 协作示例

导入文件的字节由 [storage](../storage/README.md) 保存，本 crate 登记文件资产、后台任务和逐项结果；对应 Process 处理业务数据，应用 worker 驱动执行。新增通用字符串工具等无业务记录的能力应按基础 crate 职责归属。

## 负责的数据与能力

- 来源系统、外部身份映射和映射目标事实。
- 批量选择快照、后台任务及任务明细。
- 文件资产元数据、扫描与保留状态、业务单据附件关联。

## 依赖与协作边界

术语与统一分工见[总索引](../README.md#代码中常见名称的含义)。

- 本领域的数据结构、业务规则、服务和数据库仓储在此维护。常规、构建和测试依赖均不得指向其他业务领域或组合层。
- S3 对象字节读写使用 storage；HTTP 上传解析和文件类型校验由 web-api 负责。
- 后台任务在本域登记，实际业务执行由对应 Process 和应用 worker 装配。
- 需要其他领域的能力时，由组合层或应用将本域接口接到实际提供方；公开入口见 [src/lib.rs](src/lib.rs)。

## 代码入口

| 入口 | 用途 |
| --- | --- |
| [src/service/source_registry/mod.rs](src/service/source_registry/mod.rs) | 来源系统及映射服务 |
| [src/service/bulk_job/mod.rs](src/service/bulk_job/mod.rs) | 批量任务服务 |
| [src/service/file_asset/mod.rs](src/service/file_asset/mod.rs) | 文件资产及附件服务 |
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
cargo check -p erp-support --locked
env -u ERP_TEST_MONGO_URI cargo test -p erp-support --lib --locked
```

代码变更还须执行[统一质量门禁](../README.md#质量门禁)。测试范围限定为库单元测试，不执行集成测试或真实外部服务测试。
