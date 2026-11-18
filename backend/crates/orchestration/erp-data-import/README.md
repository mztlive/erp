# erp-data-import

## 职责

本 crate 是导入与导出的跨域编排层。它负责把商品报价表、品牌与供应商批量资料、合同 PDF 和客户选品工作簿交给相应业务用例；同时提供持久化文件任务（报表、列表、商品池和报价包导出，以及导入预览和批量办理）与历史数据的批次确认、应用编排。它决定何时登记、认领、推进或恢复任务，以及哪些业务写入必须共用事务，不取代各领域的实体、规则与仓储。

- [erp-import](../../domains/erp-import/README.md) 保存历史导入的批次、来源行、确认、应用结果和命令回执，定义这些记录的状态规则；`import_apply` 协调它们与工作项、后台任务及审计的写入。商品、品牌、供应商等模板导入不是 `erp-import` 的历史批次。
- [erp-support](../../domains/erp-support/README.md) 管理共享后台任务、逐项状态和文件资产元数据；对象字节由 [storage](../../platform/storage/README.md) 读写。合同识别任务归 `erp-contract`，询价预览和确认回执归 `erp-sales`，并非都由共享后台任务承载。
- `erp-server` 的 [wiring](../../../apps/erp-server/src/wiring/application/imports.rs) 注入配置好领域 Port 的服务、共享授权、存储与识别提供者，并为历史确认注入工作项授权能力。HTTP 负责协议适配，应用 worker 负责调度；本 crate 不反向依赖应用入口或旧 `erp-processes`。

## 目录结构

公开模块和根级再导出以 [src/lib.rs](src/lib.rs) 为准。商品、品牌、供应商、合同和历史应用的 Process 可从 crate 根导入；`DataTransferProcess` 和 `SalesInquiryImportProcess` 从各自的模块导入。`Error`、`Result` 也从根导出。

| 模块 | 公开入口与内部划分 |
| --- | --- |
| [`product_import/`](src/product_import/) | `ProductImportProcess`：`submit` 或直传系列入口登记报价表，`job_list`、`job_detail`、`job_items` 读进度，`execute_job` 执行一个切片。内部拆分模板解析、行清单、媒体、联合写入和进度保存。 |
| [`brand_import/`](src/brand_import/) | `BrandImportProcess::submit` 登记完整输入与任务，`failures` 交付待处理行；内部拆分来源、执行和逐行结果。 |
| [`supplier_import/`](src/supplier_import/) | `SupplierImportProcess::submit`、`failures` 负责提交与待处理行读取；执行通过 `SupplierProfileProcess` 复用原供应商建档命令。 |
| [`contract_import/`](src/contract_import/) | `ContractImportProcess` 用 `create` 登记，用 `claim`、`execute`、`source_failed` 处理识别，用 `list`、`detail`、`source` 查询，用 `confirm` 归档。识别管线、主数据匹配和归档分开。 |
| [`sales_inquiry_import/`](src/sales_inquiry_import/) | `SalesInquiryImportProcess` 提供 `preview` / `confirm`、`preview_creation` / `create_from_preview`；模板解析、SKU 事实校验、预览授权与提交分开。 |
| [`data_transfer/`](src/data_transfer/) | `DataTransferProcess` 提供 `submit`、`status`、`context`、`result`、`recover` 和 worker 的 `run_due`；任务生命周期与 `TransferHandler` 业务分派分开，默认 `BusinessTransfers` 通过 `TransferReader` 复用业务查询，并按类型生成 CSV、工作簿、报价包或导入结果。 |
| [`import_apply/`](src/import_apply/) | `ImportApplyProcess` 的 `create_batch`、`apply_batch` 管理批次与应用结果；`create_confirmation`、`confirmation_list`、`complete_import_business_confirmation` 处理确认；`execute_import_command` 处理应用阶段命令。各命令分别组织事务，通过 `ImportWorkItems` 获取当前工作项授权。 |
| [`background/`](src/background/)、[`repository/`](src/repository/)、[`xlsx/`](src/xlsx/)、[`errors.rs`](src/errors.rs) | 私有实现：导入来源缓存与切片预算、任务授权和版本读取、有界 XLSX 技术读取，以及编排错误归一。模板列、公式准入和业务解释留在具体导入模块。 |

## 架构设计

### 模板导入：从登记到逐行恢复

[商品报价表 HTTP 入口](../../../apps/erp-server/src/http/handler/catalog/import.rs) 将表单上传交给 `ProductImportProcess::submit`；浏览器直传则由 `submit_direct_upload` 合并分片后复用登记流程。解析、图片预提和对象读写在事务外；登记事务复验提交人的商品创建资格，保存文件资产、任务、行明细及可选的私有行清单。执行优先读私有行清单，缺失或损坏时回退到对象清单，再回退到归档 XLSX；私有清单的数据库读取失败不当作缺失。商品与供给联合导入先准备外部媒体，再用同一行事务重验供应商资格、变更商品和 SKU、建立缺失供给并写审计；worker 另用事务原子保存行结果和任务进度。

[统一 worker](../../../apps/erp-server/src/worker/mod.rs) 把商品、供应商、品牌流程注册为 `BackgroundTaskAdapter`。各适配器每轮最多扫描 4 个任务，逐行检查持久化版本和状态；每任务每个切片最多 10 行、软时限 1 秒，当前行保存后才让出。`ImportSourceCache` 按任务及来源身份复用已验证的不可变输入，有任务数和输入字节预算，不缓存任务状态或授权结论；续办以数据库中的任务和行结果为准，只执行尚无结果的行。

- **品牌**：先一次性加密保存完整输入，再原子登记 `BRAND_IMPORT` 和全部行。每行在同一事务中重验原提交人的品牌创建权限，并写入品牌、审计、行结果和任务进度。中断后从已保存行恢复，不重放已完成行；取消在行事务边界生效。任务结束后，仅仍具品牌创建权限的原提交人可下载待处理源行。
- **供应商**：从加密来源读取原始行，沿用供应商建档的逐行幂等身份；业务写入和任务进度不共用一个事务，行结果与任务进度一同保存。超时或结果未知记为待核对，不声称业务已回滚；核对或重导沿用原行身份。

### 合同导入：识别与归档分离

上传 PDF 只登记识别任务，不创建正式合同。[HTTP 进程内的受控后台任务](../../../apps/erp-server/src/http/handler/contract/import_execution.rs) 领取识别任务并读取原 PDF，不走统一导入 worker。OCR 和字段提取在归档事务之外，只生成待核对结果；人工 `confirm` 时在归档事务内重读任务，复验源文件、客户和合同范围，原子写入合同、修订、导入结果及审计。

### 持久化文件任务：商品池导出与询价导入

1. [文件任务 HTTP 入口](../../../apps/erp-server/src/http/handler/data_transfer.rs) 调用 `DataTransferProcess::submit`，只接受固定 `TransferKind`、业务参数和稳定操作号，不接受任意查询或文件路径。输入在事务外加密写入对象存储；登记事务重验资格并写入文件资产、`DATA_TRANSFER` 任务、明细、私有上下文和审计。同一提交人用原操作号重试时，同载荷返回原任务，异载荷拒绝。
2. [独立文件任务轮询](../../../apps/erp-server/src/worker/data_transfer.rs) 调用 `run_due`，以任务版本竞争认领、用心跳维持执行权。以 `ProductPoolExcel` 为例，[应用装配的 `TransferReader`](../../../apps/erp-server/src/wiring/data_transfer/reads.rs) 在当前授权快照内沿原商品查询口径取数；[工作簿生成](src/data_transfer/workbook.rs) 在事务外读取商品图片并生成 XLSX。结果先加密写入对象存储，再在同一数据库事务登记结果资产、行结果及任务状态。
3. 导出的商品池工作簿可作为询价导入源。`InquiryPreview` / `InquiryCreationPreview` 交给 `SalesInquiryImportProcess` 解析并保存预览，不直接改询价；随后由询价业务入口调用 `confirm` 或 `create_from_preview`，在写入事务内复验当前权限、预览指纹和商品资格；已有询价还须核对当前版本。业务变更与回执一并提交。
4. `status` 供本人轮询，`context` 提供受控续办信息。读取结果时先在快照内核验读取资格，事务外下载解密，交付前复验任务和当前业务权限。缺少分类的旧任务只允许本人从源文件恢复分类，再检查业务资格。导入结果仅向本人交付；导出允许仍有相应范围的非提交人下载，但数据或授权依据变化时须重新导出。`recover` 只用于原供给或门户批量任务的未知结果核对，依据原业务回执更新可确认的项，不重新提交命令。

### 历史数据应用：批次、工作项与回执

[历史导入 HTTP 入口](../../../apps/erp-server/src/http/handler/legacy_import/mod.rs) 复用 `erp-import` 的领域 DTO；批次和行的读取走 ReadModel 或领域服务，涉及跨域状态变更则调用 `ImportApplyProcess`。`create_batch` 用一个事务登记批次、来源行、关联后台任务和审计。`create_confirmation` 建立确认事实与工作项；`complete_import_business_confirmation` 重验当前责任资格，将确认、批次、工作项、流程动作、结构化回执和成功事件一同提交。同命令恢复查 `import_command_receipts` 并复验原决定、任务身份和当前资格，不从审计消息还原结果。

`execute_import_command` 是 W18 执行命令入口。当前历史期初导入执行器尚未接通，`START_APPLY` 和 `RETRY_FAILED` 会明确拒绝；`CANCEL_PENDING` 仍按批次与任务状态校验，并原子保存状态、回执和成功事件。`apply_batch` 接收已在应用阶段的批次逐行结果，只持久化合法迁移的行，并同步更新批次计数、后台进度和审计；它不是文件解析或启动正式业务写入的 worker。

## 验证要求

测试随业务实现维护，执行验收仅限库内单元测试；不运行 ignored 测试、集成测试或依赖真实数据库、供应商服务的测试。仅修改文档时按[后端开发规则](../../../AGENTS.md#验证)执行 `git diff --check`。
