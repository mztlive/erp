# 后端 crate 用途与开发导航

本目录的 crate 按业务职责和技术能力组织。开发时先确定“修改哪类业务事实”或“组合哪些业务步骤”，再选择对应入口。每个 crate 的 README 说明用途、使用场景、协作示例、代码入口及修改要求。

当前 [workspace](../Cargo.toml) 在本目录登记 31 个 crate：19 个业务领域、2 个跨域组合、3 个共享基础、7 个技术及测试支持。HTTP 应用见 [web-api](../apps/web-api/README.md)，运维命令见 [cli](../apps/cli/README.md)，全局开发要求见 [backend/AGENTS.md](../AGENTS.md)。

## 先按工作内容选择位置

| 要完成的工作 | 实现位置 | 具体例子 |
| --- | --- | --- |
| 管理一种业务对象，维护它的规则、查询和数据 | 对应业务领域 crate | 客户分配放 erp-customer，库存预占放 erp-inventory |
| 一个操作需要多个领域共同完成 | [erp-processes](erp-processes/README.md) | 提交销售单并启动审批；收货过账并更新库存 |
| 一个查询需要组合多个领域的信息 | [erp-read-models](erp-read-models/README.md) | 客户中心展示合同、销售和应收；采购列表补齐供应商名称 |
| 多个领域需要相同的金额、数量、时间类型 | [erp-core](erp-core/README.md) | 使用 Amount 计算金额，使用 BusinessDate 表达到期日 |
| 多个用例需要相同的请求处理约定 | [application-core](application-core/README.md) | 分页结构、调用人上下文、命令重试匹配 |
| 多个仓储需要相同的数据库操作机制 | [persistence-core](persistence-core/README.md) | 通用 CRUD、分页、共享事务执行器 |
| 接收 HTTP 请求、返回响应、装配后台 worker | [web-api](../apps/web-api/README.md) | 路由、JWT 中间件、请求参数提取、任务启停 |

普通的单领域查询仍放在该领域的 Service/Repository。新增列表接口时，先判断它是否真的需要跨领域组合，再决定是否使用 ReadModel。

## 容易混淆的职责

| 相关 crate | 分工规则 |
| --- | --- |
| party / customer / supplier | party 维护往来主体及联系、银行、税务资料；customer 维护客户账户与负责人；supplier 维护供应商账户、能力与资质 |
| catalog / supply | catalog 定义公司商品与 SKU；supply 定义哪个供应商供应该 SKU、按什么条款供应以及当前是否可供 |
| supplier / procurement / supply | supplier 管合作对象与资格；procurement 管采购订单及责任；supply 管供给、供应商接口执行及结算单 |
| warehouse / inventory / fulfillment | warehouse 管仓库资料和策略；inventory 管余额、预占和流水；fulfillment 管实际收货、交付及验收 |
| finance / returns | finance 管应收应付、实际收付款、发票和成本；returns 管退货、退款及收付款冲正单据 |
| bpm / workflow / processes | bpm 计算流程状态；workflow 接入 ERP 审批政策、工作项和持久化；processes 协调审批与业务单据动作 |
| processes / read-models | processes 组织业务命令与写入；read-models 组合查询结果和只读事实，供页面及用例读取 |
| support / storage | support 保存文件资产和附件业务关系；storage 保存、读取和删除文件字节 |
| erp-core / entity-core / entity-macros / id-generator | erp-core 定义共享业务值；entity-core 提供持久化元数据；entity-macros 生成重复访问代码和 ID 类型；id-generator 生成实际 ID 值及单号 |

## 业务领域：按业务对象查找

| Crate | 用途 |
| --- | --- |
| [erp-party](erp-party/README.md) | 管理往来公司或个人的基础身份，以及联系人、地址、银行账户、税务资料和敏感字段。 |
| [erp-customer](erp-customer/README.md) | 管理“哪些往来对象是我们的客户”，以及客户账户状态、负责人和客户分配记录。 |
| [erp-supplier](erp-supplier/README.md) | 管理“哪些往来对象可以作为供应商”，以及供应商能力、资质、评级和商业结算档案。 |
| [erp-catalog](erp-catalog/README.md) | 管理公司卖的商品是什么，以及每种商品有哪些可区分的 SKU、规格和上下架状态。 |
| [erp-warehouse](erp-warehouse/README.md) | 管理仓库在哪里、处于什么状态、由谁处理履约，以及仓库对各 SKU 的处理策略和有效期。 |
| [erp-sales](erp-sales/README.md) | 管理卖给客户的订单及后续变更，保存可编辑草稿、每次提交内容和正式生效版本。 |
| [erp-procurement](erp-procurement/README.md) | 管理向供应商采购的订单：采购内容、数量、提交版本、变更，以及由谁负责采购。 |
| [erp-fulfillment](erp-fulfillment/README.md) | 记录货物或服务实际如何完成交付：采购收货、实物发货、电子交付、服务履约和客户验收。 |
| [erp-inventory](erp-inventory/README.md) | 管理每个仓库中 SKU 的库存余额、增减流水、预占和库存调整。 |
| [erp-finance](erp-finance/README.md) | 记录客户欠公司多少钱、公司欠供应商多少钱、实际收付了多少，以及发票、成本和分摊结果。 |
| [erp-returns](erp-returns/README.md) | 管理交易完成后的逆向单据，包括销售退货、采购退货、客户退款、供应商退款和收付款冲正。 |
| [erp-contract](erp-contract/README.md) | 管理与客户签订的合同，以及每次归档时固定下来的合同内容和 PDF 关联。 |
| [erp-supply](erp-supply/README.md) | 管理公司 SKU 的供应来源，以及供应商接口连接、供应商侧订单执行结果和结算单。 |
| [erp-integration](erp-integration/README.md) | 记录外部系统入站消息、处理失败任务和对账差异，并管理处理决定及其证据。 |
| [erp-import](erp-import/README.md) | 保存历史业务数据导入的批次、逐行处理状态、业务确认记录和最终应用结果。 |
| [erp-identity](erp-identity/README.md) | 管理登录账号、组织与成员关系、角色权限和数据范围，提供后台身份认证与访问控制能力。 |
| [erp-workflow](erp-workflow/README.md) | 将通用流程引擎接入 ERP，管理业务审批、审批参与者、业务单据关联和人工待办任务。 |
| [erp-audit](erp-audit/README.md) | 记录和查询“谁在什么时候对哪个业务对象执行了什么操作”，并提供已执行命令的回执查询。 |
| [erp-support](erp-support/README.md) | 保存多个业务流程共用的业务支撑记录：外部编号映射、批量选择与后台任务、文件资产及附件关系。 |

## 跨域组合：按完整用例或查询查找

| Crate | 用途 |
| --- | --- |
| [erp-processes](erp-processes/README.md) | 执行需要多个业务领域共同完成的操作，协调调用顺序、事务、审计、审批动作和外部调用。 |
| [erp-read-models](erp-read-models/README.md) | 组合多个业务领域的数据，返回客户中心、采购列表、工作台和财务报表等场景需要的查询结果。 |

## 共享基础：按复用能力查找

| Crate | 用途 |
| --- | --- |
| [erp-core](erp-core/README.md) | 提供整个 ERP 共用的基础业务值类型，例如金额、数量、税率、业务日期、ID 类型和字段更新方式。 |
| [application-core](application-core/README.md) | 提供多个业务用例共同使用的应用层约定：分页结果、调用人上下文、命令身份、重试匹配和错误分类。 |
| [persistence-core](persistence-core/README.md) | 提供各业务仓储共用的 MongoDB 连接、通用数据操作、分页与事务执行机制。 |

## 技术与测试支持

| Crate | 用途 |
| --- | --- |
| [bpm](bpm/README.md) | 根据流程定义和当前状态，计算审批下一步应该进入哪个节点、产生哪些事件和任务意图。 |
| [entity-core](entity-core/README.md) | 提供持久化实体共用的基础字段：ID、版本号、创建时间、更新时间和软删除时间。 |
| [entity-macros](entity-macros/README.md) | 在编译时生成重复的实体基础字段访问代码，以及具有独立 Rust 类型的字符串 ID。 |
| [id-generator](id-generator/README.md) | 为业务对象生成内部主键，并为正式单据分配可展示的业务编号。 |
| [permission-macros](permission-macros/README.md) | 为 HTTP Handler 声明权限标识，并在编译时生成取得该权限键的函数。 |
| [storage](storage/README.md) | 将文件字节保存到 S3 或兼容服务，提供读取、删除、公开 URL 和大文件分片上传能力。 |
| [test-support](test-support/README.md) | 提供后端测试共用的请求调用、测试 JWT、账号种子和索引断言辅助，并保留历史数据库测试夹具。 |

## 代码中常见名称的含义

| 名称 | 本项目中的含义 | 放置要求 |
| --- | --- | --- |
| Entity / 值对象 | 业务对象及其自身必须成立的规则，例如订单状态、金额 | 具体业务归所属领域，共享基础值归 erp-core |
| DTO / View | 调用参数或返回给调用方的数据结构 | 与提供该用例或查询的领域、组合层放在一起 |
| Service | 组织本领域的用例步骤，调用规则和仓储 | 放在所属领域；跨域命令由命名 Process 协调 |
| Repository | 封装集合查询和数据写入 | 业务集合及规则由所属领域拥有；跨域只读组装见 ReadModel |
| Port | 消费方声明的接口，说明自己需要什么外部能力或事实 | 接口由消费方定义，实际实现由组合层或应用装配 |
| Adapter | 将 Port 接到实际提供方的实现 | 组合层负责跨领域接线，避免业务领域直接相互依赖 |
| 事实 | 已保存且可作为判断依据的业务记录或其必要字段 | 从拥有领域获取，遵守其权限与版本语义 |
| 投影 / ReadModel | 从业务事实中选择、组合或汇总出的读取结果 | 复杂跨域查询放 erp-read-models；不要求另建读数据库 |
| Executor | 仓储操作使用的执行上下文，决定是否加入调用方事务 | 同一原子用例沿用同一执行器 |
| 幂等 / 命令回执 | 同一操作重试时识别原请求并返回已执行结果，防止重复产生业务效果 | 通用匹配规则归 application-core，实际记录由拥有领域保存 |

## 依赖与接入要求

1. 业务领域的 normal、build、dev 依赖均不得指向其他业务领域或组合层；需要外域事实时，声明窄 Port 或输入事实，由组合层接入实际提供方。
2. erp-processes 允许依赖领域和 erp-read-models；erp-read-models 允许读取多个领域，但不得反向依赖 erp-processes。共享基础 crate 不得持有业务实体或依赖业务领域。
3. HTTP、CLI 等应用负责装配具体服务和适配器；CLI 不得依赖 web-api。test-support 仅作为 dev-dependency 使用。
4. bpm 只接收调用方提供的模型、人员资格、ID 和时间；ERP 政策与持久化由 erp-workflow 接入。
5. 不得恢复 entities、database、services 旧三层 crate 或兼容转发层，历史归档不得接入活动 Cargo target。
6. 新增 crate 时同步更新 workspace 成员、依赖、边界检查配置、本索引及该 crate 的 README。

## 修改执行顺序

1. 阅读目标 crate README 的使用场景和协作示例，再通过代码入口定位实现。
2. 将无 I/O 的业务校验、状态迁移和不变式放在所属领域实体或值对象；由 Service 组织本域用例，Process 协调跨域业务操作。
3. 由本域 Repository 封装数据操作。Repository 接收调用方的 Executor；多集合原子写入由用例或 Process 控制事务。外部 HTTP、S3 和供应商调用置于数据库事务之外。
4. 查询变更同步评估索引；应用组合根复用各领域公开索引入口，保持既有逐集合注册顺序。列表、统计和导出保持相同的数据授权及筛选口径。
5. HTTP 请求和响应优先复用领域或组合层 DTO。扩展公共类型或方法时检查 Process、ReadModel、应用调用方及宏消费方。
6. 业务代码改动补充库单元测试，覆盖成功、失败、边界及相关幂等、版本冲突和金额数量规则；执行目标 README 的定向检查及下列统一质量门禁。
7. 仅修改 README 时，核对 crate 覆盖、相对链接、公开入口、命令包名和 git diff --check；无需因文档编辑执行编译或业务回归。

## 质量门禁

以下命令均在 `backend/` 目录执行：

```bash
cargo fmt --all -- --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
env -u ERP_TEST_MONGO_URI cargo test --workspace --lib --locked
./scripts/check-bpm-boundaries.sh
./scripts/check-domain-boundaries.sh --cutover
./scripts/check-permissions-drift.sh
git diff --check
```

测试仅运行库单元测试。不得新增、修改或执行集成测试，不运行 `--test`、`--include-ignored` 或真实 MongoDB、S3 等外部服务测试。上述命令是执行要求，不代表当前工作区已完成验证。
