# erp-party：往来主体与敏感资料

## 职责

`erp-party` 管理企业组织的稳定身份、法定名称修订和按有效期维护的附属资料。客户账户、供应商账户和历史单据通过 `PartyId` 引用主体，不因名称变化而换 ID。当前 `PartyKind` 只支持企业组织，不支持个人主体。

### 负责的数据与能力

- 往来主体编号、统一社会信用代码、启停状态和法定名称修订，以及联系人、地址、银行账户和税务资料。
- 地址正文、联系人手机号和银行账号的加解密、精确查询指纹和短时查看令牌；明文交付由对应业务入口授权并审计。
- 我方公司资料（公司角色、简称、别名、可选开户行）、独立管理用例、结算主体轻量目录和内建清单同步。

### 使用场景

修改通用往来主体及其附属资料、维护我方公司、调整敏感字段处理时，从本 crate 查找规则；保存完整客户或供应商档案时，从对应 Process 进入，不逐个调用通用资料接口。

## 目录结构

### 代码入口

| 入口 | 用途 |
| --- | --- |
| [Cargo.toml](Cargo.toml)、[src/lib.rs](src/lib.rs) | 依赖声明和对外公开的实体、DTO、服务、仓储及端口 |
| [src/entity/mod.rs](src/entity/mod.rs)、[src/entity/party/](src/entity/party/) | 主体、修订、附属资料、值对象和无 I/O 的业务规则 |
| [src/dto/mod.rs](src/dto/mod.rs) | 主体及附属资料、我方公司和结算目录的请求与视图 |
| [src/service/party/mod.rs](src/service/party/mod.rs) | `PartyService` 和联系人、地址、银行账户、税务资料服务入口 |
| [src/service/party/sensitive.rs](src/service/party/sensitive.rs) | `SensitiveDataCodec`、敏感字段种类与查看令牌范围 |
| [我方公司维护](src/service/party/company.rs)、[结算主体目录](src/service/directory.rs)、[内建同步](src/service/party/builtin/mod.rs) | 手工维护、业务表单查询、启动同步三个独立入口 |
| [src/ports/mod.rs](src/ports/mod.rs) | `PartyAuditPort` 写入审计，`SupplierRolePort` 查询供应商角色事实 |
| [src/repository/mod.rs](src/repository/mod.rs) | `PartyExt` 集合访问器、MongoDB 筛选与分页、带调用方执行器的仓储操作 |
| [src/indexes/mod.rs](src/indexes/mod.rs) | 组合根调用的 `indexes::ensure`，索引定义留在本域 |

## 架构设计

### 稳定身份、名称修订与附属资料

- **主体与修订分开存储。** `parties` 保存稳定 ID、创建后不可变的 `party_no` 和 `party_kind`、可选信用代码、状态及 `current_revision_id`。`party_revisions` 按同一 `party_id` 追加法定名称、简称、变更原因和修订号，不改写旧版。通用创建写入主体、首版修订和审计；更新校验期望版本后追加修订，以 `id + version` 更新当前指针，变更与审计同事务提交。当前名称查询只匹配当前指针指向的修订。主体编号和非空信用代码全局唯一，软删除不释放占用；[身份校验](src/service/party/identity.rs)在保存前用同一事务核对名称和信用代码候选，防止重复建档。
- **附属资料是独立的有效期事实。** `party_contacts`、`party_addresses`、`party_bank_accounts` 和 `party_tax_profiles` 各有自己的记录 ID、版本和 `party_id`，不是名称修订的内嵌字段。按业务日期读取启用且落在 `[valid_from, valid_to)` 的记录，未设结束日表示无上限。业务内容变化原则上追加新事实，保留历史引用；通用更新只允许调整启停、有效期结束日、默认标记。设置默认时在同一事务清理同主体同类资料的其他默认标记。银行账户编号和同主体下的账号指纹分别唯一；账号、户名、银行或支行不能靠改写旧账户变更，供应商档案对默认收款账户的[联行号维护](../../orchestration/erp-master-data/src/supplier_profile/update/bank_routing.rs)走独立校验。
- **我方公司是主体上的可选角色。** [`CompanyProfile`](src/entity/party/company.rs) 保存全称、简称、别名和规范化名称键，以及可选的 `bank_name`；没有独立的公司身份 ID。通用主体更新不修改我方公司资料；公司管理入口使用独立的公司权限、复用主体与修订写入机制，并保留已有开户行。后台登录即可使用[结算主体轻量目录](../../../apps/erp-server/src/http/routes/party.rs)，无需公司管理权限或人员数据范围：只投影未删除我方企业的 ID、编号、名称和状态，保留停用项供历史回显，不返回附属资料。

### 授权与敏感字段

- **管理授权在用例内完成。** 主体、修订、各类附属资料和公司管理分别使用 [erp-permissions 的操作权限](../../foundation/erp-permissions/src/party.rs)。后台路由负责认证和登记权限元数据，Service 通过 [erp-identity 的 `AuthorizationService`](../erp-identity/README.md#业务授权-api) 校验对应操作，在同一快照中完成授权和取数；写入事务内再校验归属或供应商角色边界、版本等条件。Repository 和 Port 复用调用方的 `Executor`，不另开事务、不替代用例授权。
- **敏感值与普通资料分开处理。** 地址正文、手机号、银行账号分别保存版本化 AES-256-GCM 密文与带密钥的 HMAC-SHA256 查询指纹。指纹用于精确匹配、复用或查重，不用明文或裸哈希代替；手机号和账号另存末四位用于掩码。普通主体资料视图不返回这三类明文、密文或指纹：联系人和银行账户只展示手机号或账号掩码，地址视图省略正文。税号是受权限控制的普通文本，不在这三类加密字段之列。应用在启动时装配编解码器，分别派生加密、指纹和令牌签名密钥。
- **令牌不是授权。** 客户、供应商详情可按各自规则签发短时签名令牌，限定字段类型、事实行 ID、所属客户或供应商账户及有效期。[客户明文查看](../../orchestration/erp-master-data/src/customer_profile/sensitive.rs)复核客户对象详情范围、客户揭示权限、字段详情权限及对应揭示权限（银行账号使用收款人揭示权限）；[供应商明文查看](../../orchestration/erp-master-data/src/supplier_profile/sensitive.rs)复核供应商揭示权限及该供应商对象的详情范围。两者均校验令牌、确认事实行归属该账户引用的 `PartyId`，并在原事务中完成解密和成功审计，只返回请求的单个字段。

### 依赖与协作边界

术语与统一分工见[总索引](../../README.md#代码中常见名称的含义)。本域拥有实体、集合、索引和单域用例。常规、构建和测试依赖均遵循领域边界：可使用共享基础库及 `erp-identity` 的公开身份与授权服务，不直接依赖其他业务领域、编排或应用。[服务端接线](../../../apps/erp-server/src/wiring/party.rs)把 `PartyAuditPort` 接到审计域，把 `SupplierRolePort` 接到供应商账户事实；不要在本域直接访问外域集合，也不要在客户、供应商流程复制敏感资料编解码。

| 边界 | 分工 |
| --- | --- |
| [erp-customer](../erp-customer/README.md) | 拥有客户账户、销售负责人和分配事实；通过 `PartyId` 引用共享企业资料。客户详情的跨域拼装由 [erp-read-models](../../read-models/erp-read-models/src/customer_center/profile/query.rs) 负责。 |
| [erp-supplier](../erp-supplier/README.md) | 拥有供应商账户、维护人、商务资料、能力和资质。供应商详情先校验对象范围，再通过 Port 读取主体事实。通用主体服务的更新、软删除和四类附属资料的列表、新增、更新会检查供应商角色；已挂角色时改走供应商档案。 |
| [erp-master-data](../../orchestration/erp-master-data/README.md) | `CustomerProfileProcess`、`SupplierProfileProcess` 负责根级建档和修订，在同一事务核验权限及更新时的主体/账户双版本，保存主体侧和各自领域的资料、命令回执及成功审计。通用主体软删除也由[命名流程](../../orchestration/erp-master-data/src/party.rs)在同一事务封装 `PartyService` 和成功审计。 |
| 服务端启动与本域同步 | `web-api` 决定何时执行启动同步并以失败阻断启动，`PartyService` 负责清单的身份核对、资料合并和审计。同步不调用客户或供应商档案流程，不创建客户或供应商账户。 |

### 协作示例

一家已有供应商档案的公司也要成为客户时，客户建档可在查重后[显式确认复用已有企业](../../orchestration/erp-master-data/src/customer_profile/identity.rs)；满足复用条件时，客户账户引用同一个 `PartyId`，不会改写既有名称、信用代码或附属资料。新增客户负责人留在客户侧，维护供应商能力或资质走供应商档案。涉及共享资料的跨域保存由相应 Process 组织，不能用通用主体入口绕过供应商资料限制。

## 内建我方公司同步

1. [`web-api` 启动](../../../apps/erp-server/src/bootstrap.rs)完成索引初始化后、开放 HTTP 前调用 `PartyService::sync_builtin_companies`；独立 Worker 不执行。[builtin_companies.json](src/entity/party/builtin_companies.json) 是编译时嵌入的六家真实我方企业的权威清单，不属于开发演示种子或清库流程。
2. 按统一社会信用代码和当前完整法定名称查找候选，不匹配简称或别名。命中同一企业时保留原 ID、编号、简称、别名和启停状态；无匹配时创建企业并登记我方公司角色。候选指向不同主体、命中已删除记录或无法读成当前支持的企业类型时，同步失败并阻断启动；先修复档案，不猜测合并、不自动恢复。
3. 同步法定名称、信用代码、注册地址、公司电话和开户行。名称变化时追加修订；电话以“公司电话”联系资料保存，电话值走手机号加密字段。注册地址、电话、税务资料优先复用相同的当前事实，缺少时追加；同步将选中的资料设为默认，保留原事实。公司资料的可选 `bank_name` 保存开户行全称，与账号是否存在无关。
4. 广州福尚云科技研发有限公司的 `bank_account` 保持 `null`：同步仍保存开户行，不创建占位账户，也不清除已有账户。其他公司新增账号时追加账户并切换默认；同一账号若已删除、停用、不在有效期，或户名、银行、支行不一致，同步拒绝改写，须先核对原档案。
5. 六家公司的业务变更及 `company.bootstrap` 审计在同一事务提交。相同清单重跑不会新增业务修订、推进资料版本或新增业务审计。启动成功日志为“内建公司主体同步完成”，`changed` 是发生资料变更的公司数；资料未变时应为 `0`。
6. 修正内建资料须修改清单并重新发布。发布前按[验证要求](#验证要求)执行工作区编译检查、本域库单元测试、相关 crate 的 Clippy 和领域边界检查。发布验收核对启动日志并回读数据库，库单元测试不能替代回读。
7. 旧记录缺少 `bank_name` 时按 `None` 读取，无需批量迁移。回退只停止同步，不删除已写入的主体、修订和资料；若回退版本的公司维护会丢弃开户行，暂停该入口编辑，恢复新版后重新同步。

## 修改执行要求

1. 无 I/O 的校验、状态迁移和不变式放在实体或值对象；Service 组织单域用例，Process 协调跨域原子写入。授权、Repository、Port 和审计使用同一个业务阶段的执行器。
2. 调整集合查询时同步评估索引。组合根调用本域公开索引入口，保留既有逐集合注册顺序。
3. HTTP 优先复用用例 DTO，不把持久化实体直接作为响应；调整公开合同须检查对应 Process、ReadModel 和应用调用方。
4. 业务变更补充库单元测试，覆盖成功、失败、边界及相关幂等或版本冲突路径。

## 验证要求

在 `backend/` 按[后端开发规则](../../../AGENTS.md#验证)执行。仅修改文档时只对改动文件运行 `git diff --check`。业务改动执行受影响 crate 的检查；本域库单元测试命令为 `env -u ERP_TEST_MONGO_URI cargo test -p erp-party --lib --locked`。全工作区检查、Clippy 和领域边界检查见[统一质量门禁](../../README.md#质量门禁)。不运行集成测试或依赖真实 MongoDB、外部服务的测试。
