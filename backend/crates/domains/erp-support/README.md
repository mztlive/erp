# erp-support：来源登记、批量任务与文件资产

本 crate 维护多个 ERP 流程共用的业务记录：外部身份如何对应内部对象、批量操作选中了什么及执行到哪一步、文件如何治理及被哪张单据引用。它不拥有被引用的业务对象，不执行各领域的正式业务操作，也不读写对象存储中的文件字节。

## 职责

### 负责的数据与能力

| 模块 | 本域维护的记录和规则 | 不属于本域的职责 |
| --- | --- | --- |
| `source_registry` | `SourceSystem` 登记来源系统及其启停状态；`ExternalIdentityMap` 登记来源侧的稳定 ID 或单号；`ExternalIdentityTarget` 记录内部对象指向、关系角色、有效期和变更历史。 | 外部系统协议、ERP 内部编号生成、目标对象的主数据和业务状态。 |
| `bulk_job` | `BulkSelectionSnapshot` 与 `BulkSelectionItem` 冻结预览目标、数据截止时间及可选的逐项版本和摘要；`BackgroundJob` 与 `BackgroundJobItem` 登记发起人、任务状态、进度和逐项结果。 | 冻结目标不等于授权或锁定业务对象；执行时由对应业务 Process 逐项重验权限、范围、状态及版本，并执行实际业务。 |
| `file_asset` | `FileAsset` 记录对象键、文件名、MIME、大小、内容指纹、敏感级别及扫描、保留和销毁状态；`DocumentAttachment` 记录已注册单据与资产的关联、用途及记录人。 | 对象字节读写、HTTP 上传解析、实际安全扫描，以及按具体单据或业务场景决定文件能否读取。 |
| `scheduled_execution` | `scheduled_runs` 保存定时调度的接收、执行结果和待回报回执；结果回报重试不重跑业务。 | 调度协议和循环由应用 worker 接入；固定业务能力经端口注入，调度回执不替代任务中心或逐项业务结果。 |

### 使用场景

- 需要查找外部单号对应的 ERP 对象、维护来源系统，或追溯映射目标的变化时，使用来源登记。
- 需要把预览时选中的目标固定下来时，使用批量选择快照；需要登记异步任务、查询进度或逐项结果、取消任务时，使用后台任务记录。两者不要求一一对应。
- 需要登记已上传文件、记录治理状态或将文件按用途挂到单据时，使用文件资产与附件记录。通用字符串工具等不持有业务记录的能力应放在相应的基础 crate。

## 目录结构

### 代码入口

| 入口 | 用途 |
| --- | --- |
| [Cargo.toml](Cargo.toml)、[src/lib.rs](src/lib.rs) | 依赖声明与 crate 公开入口，导出实体、DTO、服务、端口和常用仓储接口。 |
| [src/entity/mod.rs](src/entity/mod.rs) | 实体、值对象及无 I/O 的校验和状态规则，包含 [`source_registry`](src/entity/source_registry/mod.rs)、[`bulk_job`](src/entity/bulk_job/mod.rs) 和 [`file_asset`](src/entity/file_asset/mod.rs)。 |
| [src/dto/mod.rs](src/dto/mod.rs) | 三个业务模块的请求、分页参数及响应视图。 |
| [src/service/source_registry/mod.rs](src/service/source_registry/mod.rs) | `SourceRegistryService`：来源系统和外部身份映射用例。 |
| [src/service/bulk_job/mod.rs](src/service/bulk_job/mod.rs) | `BulkJobService`：选择快照、任务登记、查询与取消用例。 |
| [src/service/file_asset/mod.rs](src/service/file_asset/mod.rs) | `FileAssetService`：资产、附件管理和已获业务授权的读取步骤；登记与关联、上传预检、治理分别在 `command`、`upload`、`governance` 子模块。 |
| [src/service/scheduled_execution/mod.rs](src/service/scheduled_execution/mod.rs)、[src/repository/scheduled_runs.rs](src/repository/scheduled_runs.rs) | 定时调度回执的接收、认领、恢复和回报，以及 MongoDB 回执存储。 |
| [src/repository/mod.rs](src/repository/mod.rs) | 本域 MongoDB 集合访问器、过滤分页和跨集合写入步骤；后台任务私有输入实现在 [`bulk_job/input.rs`](src/repository/bulk_job/input.rs)。 |
| [src/authorization/mod.rs](src/authorization/mod.rs)、[src/ports/mod.rs](src/ports/mod.rs) | 后台任务、文件资产的授权事实与查询映射；审计、单据注册事实、待处理附件和调度回执的端口。 |
| [src/indexes/mod.rs](src/indexes/mod.rs) | 公开索引注册入口，供应用按既有逐集合顺序调用。 |

## 架构设计

### 依赖与协作边界

术语见[总索引](../../README.md#代码中常见名称的含义)。本域可以直接使用 `erp-identity` 的公开身份与授权服务；常规、构建和测试依赖均不得指向其他业务领域或组合层。`SupportAuditPort`、`BusinessDocumentPort` 等由本域声明，实际提供方由组合层或应用接入，参见 [Crate 依赖与接入要求](../../README.md#依赖与接入要求)。

- **事务与授权。** Entity 维护不变式，Service 组织本域用例，Process 控制跨域事务。需要原子提交的写入（如映射及其初始目标、快照及逐项目标、任务及逐项明细）连同对应审计复用同一个 `persistence_core::Executor`，Repository 不另开事务。后台任务的对象授权以实际发起人为所有者，由 `erp-identity` 按入口权限和角色判断访问资格；单据型选择目标的注册事实通过端口读取，不等同于业务执行授权。
- **来源身份与目标历史分开。** 映射以「来源系统 + 对象类型 + 外部编号比较键」唯一。比较键只去除首尾空白，按 UTF-8 二进制比较，区分大小写，不做 Unicode 兼容折叠或数值化。变更指向时维护目标关系角色、有效期和历史，不覆盖旧目标来改写来源身份。
- **任务登记与执行分开。** 快照确认后目标、截止时间和预览时的版本信息不可更改。后台任务按 `request_id` 建唯一索引；通用登记入口以请求指纹区分相同请求重试和异载荷冲突。应用 [worker](../../../apps/erp-server/src/worker/mod.rs) 装配并驱动对应 Process；任务中心记录不是领域业务结果的替代品。
- **文件资产与附件关联分开。** 管理入口在事务内核对单据已注册且资产记录存在。附件是只追加的业务关联，不复制文件字节；建立关联不代表文件已通过扫描或可供读取。直接登记元数据时扫描状态为待检查；受控上传可标记为基础内容已检查，不代表安全扫描通过。标记资产销毁只更新治理记录和审计，不删除对象字节，也不移除附件关系。读取时仍须按当前单据或其他明确的业务关系授权，并按场景检查资产治理状态。
- **文件上传、存储和跨域批次分开。** [web-api 上传层](../../../apps/erp-server/src/http/upload.rs) 负责 Multipart 解析和上传大小、类型等检查；[storage](../../platform/storage/README.md) 负责 S3 或兼容服务的对象字节读写、删除、公开 URL 与分片上传，S3 I/O 在数据库事务外执行。跨域业务需要登记一批已上传文件时，由 [erp-file-assets](../../orchestration/erp-file-assets/README.md) 的 `PendingFileAssets` 准备批次，把临时引用解析为正式资产 ID，检查每个引用恰好消费一次，并沿原业务 `Executor` 先写资产再写审计。它不是资产或附件记录的拥有方，不替所属业务用例写单据或另开事务。

### 协作示例

商品导入先由 `storage` 保存源 XLSX，再由 [erp-data-import](../../orchestration/erp-data-import/README.md) 的商品导入 Process 使用本域仓储登记文件资产、任务、行明细及可选私有行清单。应用 worker 驱动 Process 解析或读取来源并执行业务；本域保存任务进度和逐项结果，不负责商品写入规则。

#### 后台任务私有输入

- `background_job_inputs` 只保存任务执行所需的有界私有输入，不进入公开任务列表、详情、明细或导出。格式标识非空且最多 128 字节；二进制内容必须小于 1 MiB，达到上限不截断，使用原文件存储链路。任务 ID 用作 `_id`，私有输入沿登记任务及明细的同一 `Executor` 原子写入。
- 该集合为可选存储，MongoDB 首次写入自动创建集合及 `_id` 唯一索引；部署无需修改已有文档或回填旧任务，部署账号须具备该集合的读写和创建权限。
- 商品导入必须保留源 XLSX 归档。[执行端](../../orchestration/erp-data-import/src/product_import/execute.rs) 优先读取私有行清单；未登记或格式、内容损坏时，依次回退对象存储中的旧行清单和源文件。数据库读取失败须原样传播，不能当作缺记录。回滚程序可沿旧源文件链路执行新任务；回滚不得删除私有输入、源文件、任务、明细或已持久化的行结果。

## 修改执行要求

1. 无 I/O 的校验、状态迁移和不变式放入实体或值对象；本域用例由 Service 组织，跨域操作由对应 Process 协调。
2. 多集合原子写入由用例或 Process 控制事务，仓储步骤沿用调用方的 `Executor`。
3. 集合查询变更时同步评估索引；应用组合根复用本域公开索引入口，保持既有逐集合注册顺序。
4. HTTP 请求和响应优先复用本域 DTO；扩展公开合同时检查 Process、ReadModel 和应用调用方。
5. 业务代码改动补充本域库单元测试，覆盖成功、失败、边界及相关幂等或版本冲突路径。

## 验证要求

仅修改文档时，按[后端开发规则](../../../AGENTS.md#验证)只运行针对改动文件的 `git diff --check`。业务代码变更时，在 `backend/` 执行本域库单测及[统一质量门禁](../../README.md#质量门禁)要求的检查：

```bash
env -u ERP_TEST_MONGO_URI cargo test -p erp-support --lib --locked
```

测试范围限定为库单元测试，不执行集成测试或真实外部服务测试。
