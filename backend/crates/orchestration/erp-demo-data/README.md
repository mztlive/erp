# erp-demo-data

## 职责

为受控演示环境准备开单所需的岗位、审批配置和业务资料，提供进度查询、分批生成或恢复，以及显式的全库重置。生成的是供销售、采购、仓储和财务演练使用的资料与配置，不会创建销售单、采购单、库存余额或收付款与发票记录，也不会发起审批实例。普通业务库及其测试不得依赖本库；服务端启动流程和 CLI 调用本库仅为登记索引，不运行演示初始化。

## 目录结构

| 位置 | 职责 |
| --- | --- |
| [src/lib.rs](src/lib.rs)、[src/errors.rs](src/errors.rs) | 导出演示 Process、请求、报告和索引入口；保留字段校验、审批错误码及命令恢复所需的错误分类。 |
| [src/runtime_ports.rs](src/runtime_ports.rs) | `DemoRuntimePorts<A>`：演示入口所需的领域和跨域命令装配合同。 |
| [src/demo_master_data/](src/demo_master_data/) | `service.rs` 负责状态与生成批次，`plan.rs`、`seed.rs` 加载和验证固定清单，`ensure_*.rs` 构造各领域创建或复用步骤。 |
| [foundation.rs](src/demo_master_data/foundation.rs) 同级模块 | `spec.rs`、`foundation_validation.rs` 读取并校验基础规格；`roles.rs`、`accounts.rs`、`organization.rs`、`person_scopes.rs`、`approvals.rs`、`responsibility.rs` 分别准备岗位、组织、范围、审批和责任规则。 |
| [offerings.rs](src/demo_master_data/offerings.rs)、[voucher.rs](src/demo_master_data/voucher.rs)、[images.rs](src/demo_master_data/images.rs) | 供给登记与可售核验、卡券原子创建、商品补图与补价；`prices.rs`、`image_assets.rs`、`image_request.rs` 提供输入校验和构造。 |
| [repository/](src/demo_master_data/repository/)、[lifecycle.rs](src/demo_master_data/lifecycle.rs)、[removal.rs](src/demo_master_data/removal.rs) | 演示登记与商品修订读取、旧软删除资料恢复、显式全库重置。 |
| [master-data.json](src/demo_master_data/master-data.json)、[offerings.json](src/demo_master_data/offerings.json)、[dev-foundation.json](src/demo_master_data/dev-foundation.json)、[images/](src/demo_master_data/images/) | 本库维护的主数据、供给、岗位与审批规格，以及编译时嵌入的商品 JPEG 和[图片清单](src/demo_master_data/images/manifest.json)。开发种子[共享脚本](../../../../scripts/dev-seed-lib.mjs)也读取 `dev-foundation.json`；共享规格不代表脚本与 HTTP 编排的更新策略相同。 |

## 架构设计

### 演示初始化入口

管理端[路由](../../../apps/erp-server/src/http/routes/demo_master_data.rs)统一通过 JWT 认证；[处理器](../../../apps/erp-server/src/http/handler/demo_master_data/mod.rs)将已认证的 `AuditActor` 交给 `DemoMasterDataProcess<A>`。路由权限标记仅用于权限目录，授权由 Process 和正式业务用例执行。

| 请求 | 用例与执行条件 |
| --- | --- |
| `GET /admin/demo-master-data` | `status` 在同一事务快照内校验 `demo_master_data:read` 并读取登记，按种类返回计划、有效登记和旧版已删除登记的数量。开关关闭时仍可授权查询，`enabled` 为 `false`。 |
| `POST /admin/demo-master-data/foundation` | `ensure_foundation` 校验开关、`demo_master_data:apply` 及后续各正式命令的资格，补齐演示岗位与业务配置。 |
| `POST /admin/demo-master-data` | `apply_chunk` 校验同一开关及生成权限，从 `cursor`（缺省为 `0`）起按固定清单每批最多处理 8 项，返回本批计数、`next_cursor` 和 `done`。 |
| `DELETE /admin/demo-master-data` | `reset_database` 校验开关并按下述专门的超管授权重置整个目标数据库，而非按演示登记逐条删除。路由登记 `demo_master_data:remove`，仅具有该权限不足以重置。 |

[系统管理页面的生成流程](../../../../erp-client/features/demo-master-data/hooks/queries.ts)先调用基础准备，再从 `0` 开始按 `next_cursor` 逐批调用，直到 `done`。两个写入口相互独立，`apply_chunk` 不会自动补建基础岗位。应用组合根的[工厂](../../../apps/erp-server/src/wiring/application.rs)读取当前配置快照中的 `demo.master_data`（默认关闭），注入目标数据库、共享 RBAC 与授权服务、S3 存储和图片指纹密钥；构造本身不写入。关闭时禁止基础准备、分批生成和重置，不得以测试或迁移为由绕过。

### 资料构造与执行顺序

- **基础准备。** 先校验岗位引用、工作流政策覆盖与岗位分离，再依次补齐缺失的内建岗位角色和演示专用角色、创建或复用岗位账号、核验岗位权限、准备部门和成员归属、补齐未配置的人员范围、发布审批定义、准备默认责任规则。新账号使用规格中的演示初始密码；已有账号不改密码，但会同步姓名并补齐缺失角色。人员范围保留已配置授权和撤权。审批人须由同一启用角色获得读取和决定权限，并按单据政策校验提交人与审批人；缺少发布版时复用或建立草稿、配置节点后正式发布。已发布节点一致则跳过；有差异时保留原定义，审批阶段结束后报错，不进入责任规则阶段。已有启用的默认采购调度、供应商付款或销售开票规则保留现负责人并复验资格，不合格则停止，不自动替换。
- **主数据样本。** 当前清单按声明顺序处理 113 项：6 个计量单位、8 个品牌、13 个分类、6 个仓库、24 个工会或职工服务类客户、28 个供应商、28 个商品。商品包括 25 个实物商品（含一个双规格福利礼包）、电子兑换码、上门服务和员工福利卡，共 29 个 SKU；卡券走正式原子创建入口，同时写入商品、唯一 SKU 和卡券扩展修订。主数据和供给 JSON 复用领域 DTO，在批次写入前校验字段、重复键及前置引用；生成时从演示登记解析字典和供给引用，使用当前环境的公司主体和岗位账号替换种子中的主键占位值。公司主体优先复用操作人可见的启用公司，缺少时按规格创建。仓库沿用并核验已有收发经办人资格，不覆盖人工配置。
- **商品可售准备。** 创建或恢复商品后，从当前修订补齐缺失的轮播图、已登记且启用的 SKU 主图，以及匹配种子的公司价格和集采起订量，不覆盖已有值。公司一件代发价、集采价、市场价和集采门槛独立于供应商供价与起订量。39 条初始供给覆盖全部 29 个 SKU，其中双规格福利礼包的标准款配置了 11 条供给，覆盖种子声明的全部付款条件。供给模板校验供应商能力及关联合同资质；生成时以实际采购账号调用正式供给命令并上架，再复用销售开单的资格查询，按当日核验所有样本 SKU 的可售性，不能以已有创建回执代替资格检查。

### 授权、事务与重试

- **装配与操作身份。** Process 通过 `DemoRuntimePorts<A>` 接收已装配的领域服务、资料与供给命令、采购责任命令、工作项服务，以及工作流授权和审计 Port。`A` 必须实现 `WorkflowAuthorizationPort`；具体 `WorkflowAuth` 和外域适配器只在[应用组合根](../../../apps/erp-server/src/wiring/application/demo_data.rs)装配。各步骤沿用正式命令的身份与范围：例如新建客户用真实销售岗位，卡券创建、供给及上架用真实采购岗位；其余步骤按命令传递请求操作人，不借演示开关提升权限。
- **恢复与失败停止。** 字典、仓库、商品按演示登记的稳定种子键和实际主键确认归属，拒绝认领撞号的未登记资料；客户、供应商优先按原幂等键恢复创建回执关联的资料与主体。软删除恢复在同一审计事务内重验生成权限、读取当前版本并按版本恢复。供给命令键包含当前 SKU 与供应商主键，避免重建后回放旧关系。基础准备和生成批次都不是一个大事务：各正式命令沿用原有校验及事务，演示登记在正式写入后另存。阻断性校验、授权或写入失败时不继续后续生成，已提交的步骤不会随整批回滚；批次可用原游标重试，沿已有登记和回执继续，不保证自动认领尚未登记的撞号资料。图片字节先上传 S3，再由正式商品用例在同一数据库事务内登记文件资产并提交商品修订；对象上传不随数据库事务回滚。
- **全库重置。** 只允许启用的后台 `admin` 本人，且必须直接持有启用的内建超管角色及其 `*:*` 授权。重置前读取目标数据库的非系统集合（包括未登记的历史集合）；在单个受授权策略版本约束的事务内复验身份与保留记录，按物理主键保留 `admin` 账号（含原密码）、内建超管角色、直接绑定、通配授权及策略版本记录，并对其余文档执行物理删除、检查残留。成功返回删除的文档总数，无游标；策略版本冲突或事务内失败不提交，提交结果未知不等于未执行。此操作会删除非演示业务数据、其他账号、审批配置和文件资产登记等，但不删除集合、索引或 S3 对象。只应在可清空的专用演示库启用。
- **索引归属。** [服务端](../../../apps/erp-server/src/indexes.rs)和 [CLI](../../../apps/cli/src/indexes.rs)调用的 `ensure_indexes` 只登记 `demo_master_records.key` 的唯一索引 `uk_demo_master_records_key`，不受生成开关控制；其他集合和索引继续由各领域的公开索引入口唯一拥有。

### 验证要求

遵循[后端开发规则](../../../AGENTS.md#验证)。仅改文档时只检查目标文件的 `git diff --check`；修改主数据、供给、基础规格或内嵌图片时须运行本库相应单元测试。不得运行集成测试、ignored 测试或依赖真实数据库及外部服务的验收命令。
