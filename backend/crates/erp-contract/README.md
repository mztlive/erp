# erp-contract：合同与归档修订

管理与客户签订的合同，以及每次归档时固定下来的合同内容和 PDF 关联。

合同需要保留稳定身份和历史版本。归档修订记录当时的客户、结算和开票资料，后续主数据变化不得改写已归档内容。

## 使用场景

- 调整合同创建、查询、终止和归档规则。
- 维护合同修订、客户与结算快照，或合同 PDF 的业务关联。

## 协作示例

客户后来更换开票资料时，新资料由 [erp-party](../erp-party/README.md) 维护；已归档合同保留原快照。PDF 字节交给 [storage](../storage/README.md)，上传与合同归档的跨域步骤由 Process 协调。

## 负责的数据与能力

- 合同稳定身份、合同状态及不可变归档修订。
- 客户和结算资料快照、合同 PDF 关联及首次归档和上传归档计划。

## 依赖与协作边界

术语与统一分工见[总索引](../README.md#代码中常见名称的含义)。

- 本领域的数据结构、业务规则、服务和数据库仓储在此维护。常规、构建和测试依赖均不得指向其他业务领域或组合层。
- 客户、账号、分配和文件资产事实通过 Port 取得；跨域上传关联由 erp-processes::contract 编排。
- 合同修订必须保留既有快照和归档语义；文件字节写入使用 storage。
- 需要其他领域的能力时，由组合层或应用将本域接口接到实际提供方；公开入口见 [src/lib.rs](src/lib.rs)。

## 代码入口

| 入口 | 用途 |
| --- | --- |
| [src/service/contract/mod.rs](src/service/contract/mod.rs) | ContractService 与归档计划 |
| [src/entity/contract/snapshot.rs](src/entity/contract/snapshot.rs) | ContractSnapshot |
| [src/ports/mod.rs](src/ports/mod.rs) | 客户、账号、附件和审计合同 |
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
cargo check -p erp-contract --locked
env -u ERP_TEST_MONGO_URI cargo test -p erp-contract --lib --locked
```

代码变更还须执行[统一质量门禁](../README.md#质量门禁)。测试范围限定为库单元测试，不执行集成测试或真实外部服务测试。
