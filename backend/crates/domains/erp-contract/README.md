# erp-contract：合同与归档修订

本领域维护与客户签订的合同、每次归档形成的不可变修订，以及签约前的 Word 模板与领号记录。ERP 不起草合同正文；业务约定见[合同与销售单](../../../../docs/erp-phase-1.md#43-合同与销售单)。

## 职责

### 负责的数据与能力

- **稳定合同**：`Contract` 保存全局唯一的合同编号、客户、默认结算主体、合同状态、独立负责销售、明确共享销售及当前修订指针。本域提供列表、详情、终止和共享名单维护，不创建空合同草稿。共享不转移合同负责人或客户归属。
- **归档修订**：`ContractRevision` 每版关联一份已签署正文 PDF 的文件资产，冻结合同编号、客户名称、结算主体、付款条件、开票类型、税率和日期；通过识别导入的修订另存识别依据和人工确认值。首次归档创建合同及首修订；追加只新增递增修订并切换当前指针，不覆盖旧版。终止不改变旧版和历史销售引用。
- **识别任务**：`ContractImport` 是按创建人隔离的持久任务，不是合同草稿，也不是有效合同。它记录源 PDF 的资产 ID、摘要和页数，以及识别阶段、OCR、字段提取结果、失败原因、确认命令和归档结果。待确认任务只提供可编辑预填和证据，不自动建合同或客户。
- **签约前模板与领号**：管理不可覆盖的 DOCX 模板、稳定的公司编号组、按上海业务年度共享的组内流水和本人申请记录。换正文须上传新模板；停用不影响历史申请按原模板、原号码下载。ERP 只在 DOCX 首页右上角写入编号；样张及重复下载不占号。同一申请人用同键、同模板及同用途重试返回原编号，异内容拒绝；流水、申请与成功审计原子提交。领号不是签署、审批、有效合同或销售资格。

### 使用场景

修改合同状态、快照、PDF 关联或销售共享时，从合同实体与服务入口查找；修改任务校验、进度和预填时，从识别实体与导入 Process 查找；修改签约前模板、编号和本人申请时，从模板服务与 DOCX 处理入口查找。

### 协作示例

客户更换开票资料时，当前主体资料由 [erp-party](../erp-party/README.md) 维护，客户账户与负责人分配由 [erp-customer](../erp-customer/README.md) 维护。已归档的合同仍读取当次开票快照；销售历史仍引用原合同修订，不能改读当前资料或最新修订。

## 目录结构

### 代码入口

| 入口 | 职责 |
| --- | --- |
| [Cargo.toml](Cargo.toml)、[src/lib.rs](src/lib.rs) | Crate 依赖与公开 API；识别类型和仓储通过公开模块访问。 |
| [src/entity/mod.rs](src/entity/mod.rs)、[合同实体](src/entity/contract/entity.rs)、[修订](src/entity/contract/contract_revision.rs)、[快照](src/entity/contract/snapshot.rs) | 稳定身份、状态、不可变修订与结构化快照。 |
| [src/entity/recognition/mod.rs](src/entity/recognition/mod.rs)、[任务](src/entity/recognition/task.rs)、[预填与确认](src/entity/recognition/draft.rs)、[PDF 校验](src/entity/recognition/pdf.rs) | 页数和证据校验、任务状态与重放契约、可编辑预填及确认字段规则。 |
| [模板实体](src/entity/template.rs)、[DOCX 处理](src/entity/template_docx.rs)、[src/service/template/mod.rs](src/service/template/mod.rs) | 模板不可覆盖、公司编号组、流水、申请和首页页眉编号。 |
| [src/service/contract/mod.rs](src/service/contract/mod.rs)、[归档计划](src/service/contract/archive.rs)、[DTO 入口](src/dto/mod.rs) | 合同查询与命令，首次归档和上传归档计划，对外请求及视图。 |
| [src/ports/mod.rs](src/ports/mod.rs)、[src/authorization.rs](src/authorization.rs)、[责任 Facts 与 Mapping](src/repository/authorization.rs) | 本域消费的事实和能力、授权入口及固定责任事实。 |
| [src/repository/mod.rs](src/repository/mod.rs)、[任务仓储](src/repository/recognition.rs)、[src/indexes/mod.rs](src/indexes/mod.rs) | 本域 MongoDB 仓储和索引，含合同、修订、识别任务、模板、主体编号绑定、年度流水与申请。 |

## 架构设计

### 依赖与协作边界

本领域定义所需的最小事实和能力，由组合层或应用接入实际提供方。账号资格与姓名直接使用 `erp-identity::AccountDirectoryService`，组织事实使用身份域公开目录；权限判定只由 `erp-identity::AuthorizationService` 执行，权限常量归 `erp-permissions`。除 `erp-identity` 公开 API 外，常规、构建和测试依赖均不得指向其他业务领域，也不得反向依赖 Process、ReadModel、应用或识别适配器。术语见[后端 crate 总索引](../../README.md#代码中常见名称的含义)。

| 消费方 Port | 生产接线与边界 |
| --- | --- |
| `CustomerFactsPort`、`ContractCreationPort` | [合同服务装配](../../../apps/erp-server/src/wiring/contract.rs)读取客户是否启用、客户编号等事实；[创建候选](src/repository/authorization/creation.rs)由组合根注入真实客户事实，按当前负责人和组织建立候选，不能用合同负责人或请求填报代替来源。 |
| `SigningCompanyPort`、`ContractAuditPort` | [公司事实装配](../../../apps/erp-server/src/wiring/contract_template.rs)从 `erp-party` 读取公司资格；[审计装配](../../../apps/erp-server/src/wiring/contract.rs)接入 `erp-audit`，使用原写入执行器。 |
| `ContractParticipantPort` | [授权装配](../../../apps/erp-server/src/wiring/authorization/master/contract.rs)接入 `erp-workflow` 的历史参与事实，只补充读取，不提供写入责任。 |
| `FileAssetFactsPort` | 本域公开的只读资产存在性接口，不负责上传、登记或读取字节。当前导入 Process 直接核验 `erp-support` 的源资产；正式合同附件由[附件读模型](../../read-models/erp-read-models/src/contract_attachment/mod.rs)核对合同权限、修订关联和资产后交付元数据，不经此 Port。 |
| [`ContractOcr`、`ContractExtractor`](src/ports/recognition.rs) | OCR 接受 PDF 与独立校验的页数，按原顺序返回页面并保留空白页信息；缺页或不可读页不能当作成功。字段提取读取完整 OCR，输出带页码、引文的类型化字段及冲突，不匹配 ERP 身份，不访问主数据或写库。 |

### 合同识别导入执行合同

1. [HTTP 上传与任务入口](../../../apps/erp-server/src/http/handler/contract/imports.rs)只接受 PDF 和控制命令（请求键、可选预期客户及追加目标），不接收手填业务字段。服务端独立解析 1 至 200 页并计算 SHA-256；对象存储写入在事务外。[`erp-data-import::contract_import`](../../orchestration/erp-data-import/src/contract_import/mod.rs) 的[登记步骤](../../orchestration/erp-data-import/src/contract_import/source.rs)在同一事务登记 `erp-support` 文件资产和本域任务。同一创建人、同一请求键仅在文件摘要及控制上下文一致时重放；异载荷拒绝，提交结果未知不得删除对象。
2. HTTP `run` 只等待任务领取，[应用后台任务](../../../apps/erp-server/src/http/handler/contract/import_execution.rs)继续执行。任务从 `ready` 进入 `processing`，依次记录读文件、OCR、字段提取、准备核对；过期处理可在原任务恢复，版本比较防止并发覆盖。供应商调用在事务外；[应用装配](../../../apps/erp-server/src/wiring/application/imports.rs)按配置注入 [erp-contract-recognition](../../adapters/erp-contract-recognition/src/lib.rs) 的 Poppler／阿里云逐页 OCR 和 Rig／OpenAI 兼容字段提取。Process 只持有领域 Port；未配置或供应商失败记为 `failed`，保留原任务和文件，不伪造结果。识别成功进入 `review`，缺失、冲突或无可靠证据的字段留空，用户可核对修正。
3. 只有人工确认才能进入[归档事务](../../orchestration/erp-data-import/src/contract_import/archive.rs)。事务内重验本人任务与版本、源文件、我方签约主体、实际客户及相关权限。复用已有启用客户；没有客户身份时，仅在用户明确提交 `create_customer` 且具备建档权限时补建，不覆盖已有资料。指定预期客户或追加目标时不得借建档换客户；新建企业须有有效统一社会信用代码。
4. 新合同以我方签约主体为默认结算主体；追加须核对原合同版本、有效状态、编号、客户、结算主体，以及原修订有证据时的我方签约主体。合同、修订中的 PDF 关联、识别及确认依据、任务 `succeeded` 结果与成功审计原子提交；需要补建的客户也在同一事务。相同成功确认返回原结果，异内容或过期版本拒绝。

[erp-master-data::contract::upload_contract](../../orchestration/erp-master-data/src/contract.rs) 是独立的登记入口：接收已写入对象存储的 PDF 元数据和合同业务字段，同一事务登记文件资产、合同、首修订及成功审计。它不负责识别、主数据匹配或人工确认，也不是当前 `/admin/contracts/upload` 调用链；旧手填创建和追加 [HTTP 入口](../../../apps/erp-server/src/http/handler/contract/mod.rs)均拒绝。文件字节由 [storage](../../platform/storage/README.md) 保存和读取，本域只维护业务关联。

### 合同模板与申请执行合同

合同号按 `编号组-S-两位年度四位流水` 分配。公司改名不换编号组，同组共用年度流水；校准须核对版本、不得回退或回收号码。2026 年承接历史流水，其他年度从 `0001` 起，具体起点、号段与上限见[编号实体](src/entity/template.rs)和[流水服务](src/service/template/counter.rs)。模板维护与本人申请分别授权；[应用入口](../../../apps/erp-server/src/http/handler/contract/templates.rs)在事务外上传、读取和处理 DOCX，提交结果未知不得清理新对象或自动重放写入。开放领号前的[迁移命令](../../../apps/cli/src/migrate_contract_templates.rs)复用 [`ensure_templates`](src/indexes/mod.rs)，不改写历史合同或已用流水。

## 修改执行要求

1. 无 I/O 的校验、状态迁移和不变式放在实体或值对象；Service 组织单域用例，Process 持有跨域根事务。Repository、Port 和事务内步骤复用调用方的 `persistence_core::Executor`，不得另开事务；对象存储和供应商 I/O 在数据库事务外。
2. 合同责任采用固定 Facts 与 Mapping：读取以独立负责销售和明确共享销售提供责任事实，历史参与只补充读取；写入责任只认独立负责销售。两种责任口径使用同一合同主键及根记录，不按账号角色改变业务事实。本域不得解释角色条款、组合权限范围或自行构造授权结果；创建合同另验实际客户来源资格。
3. 查询必须使用当前读取阶段的编译凭证，在原执行器上检查凭证后取数，并在交付前复验账号、权限、事实版本和有效期；凭证不得跨请求或事务复用。新增或调整查询须同步评估索引，组合根复用本域公开索引入口并保持注册顺序。
4. HTTP 优先复用本域 DTO；公开契约变更须核对 Process、ReadModel 和应用调用方。业务改动须补充库单元测试，覆盖成功、失败、边界以及相关幂等或版本冲突。

## 验证要求

仅修改文档时，只对变更文件执行 `git diff --check`。业务代码改动在 `backend/` 执行 `env -u ERP_TEST_MONGO_URI cargo test -p erp-contract --lib --locked`，并按[后端开发规则](../../../AGENTS.md#验证)及[统一质量门禁](../../README.md#质量门禁)执行对应检查，不运行集成测试或真实外部服务测试。
