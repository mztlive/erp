# 供应商门户交付与运行验收执行合同

版本：1.1

修订日期：2026-10-05

状态：代码实施完成；静态及库单元测试完成，前端全量格式存在未改动基线失败；运行验收未执行

本合同依据 [供应商门户建设与执行合同](supplier-portal-contract.md) 的 SP01—SP30，规定交付核对、环境准备、验收操作、通过标准和证据登记。接口及字段执行 [供应商门户接口执行合同](supplier-portal-api-contract.md)。代码落点用于定位实现，不构成运行通过证据。

## 1 交付与验收边界

1. 第一版同时交付已有商品报价、新品提报、独立账号、供给读取与可供维护、合作条款申请、内部审核任务及四类批量入口。任何一条业务线未完成，不得登记第一版整体验收通过。
2. 静态与构建、库单元测试、页面实际检查、真实接口与跨账号、真实事务、对象存储分别登记。某类证据不得替代其他类别。
3. 本次代码实施不授权启动服务、清库、重建种子、写入开发或生产数据、访问真实 MongoDB 或 S3，亦不授权执行集成测试或 E2E。
4. 第 5 节为后续运行验收规程。执行前须取得当次环境授权，明确环境、允许使用的账号、允许写入的业务数据、对象存储位置和验收后保留或清理范围。未取得授权时保持“未验证”。
5. 后端仅使用仓库允许的库单元测试；前端不新增单元测试。质量门禁按 [仓库指导](../AGENTS.md)、[后端指导](../backend/AGENTS.md) 和 [前端指导](../erp-client/AGENTS.md) 执行。
6. 所有证据登记须注明实际代码版本或工作区差异标识。不得将某次较早检查的结果直接用于后续改动后的交付。

## 2 代码落点登记

下表登记主实现入口。验收项引用落点编号时，须沿对应 Service、Process、Repository、Port、Handler 和页面检查完整调用链。

| 落点 | 责任与入口 |
| --- | --- |
| L01 身份 | [门户身份服务](../backend/crates/erp-identity/src/service/portal/mod.rs)、[账号绑定仓储](../backend/crates/erp-identity/src/repository/portal.rs)、[门户鉴权](../backend/apps/web-api/src/core/middleware/supplier_portal.rs)、[登录与密码接口](../backend/apps/web-api/src/core/handler/supplier_portal/auth.rs) |
| L02 账号与开放目录 | [账号跨域用例](../backend/crates/erp-processes/src/supplier_portal/accounts.rs)、[目录授权用例](../backend/crates/erp-processes/src/supplier_portal/grants.rs)、[供给领域目录授权](../backend/crates/erp-supply/src/portal/grants.rs)、[内部管理接口](../backend/apps/web-api/src/core/handler/supplier_portal/admin.rs)、[账号页面](../erp-client/features/supplier-portal-admin/pages/accounts-page.tsx)、[目录页面](../erp-client/features/supplier-portal-admin/pages/catalog-page.tsx) |
| L03 外部安全投影 | [门户读模型](../backend/crates/erp-read-models/src/supplier_portal/mod.rs)、[自己的供给投影](../backend/crates/erp-read-models/src/supplier_center/offering/portal.rs)、[定向目录投影](../backend/crates/erp-read-models/src/supplier_portal/catalog.rs)、[外部申请投影](../backend/crates/erp-read-models/src/supplier_portal/applications.rs)、[门户读取接口](../backend/apps/web-api/src/core/handler/supplier_portal/read.rs) |
| L04 供给与可供 | [供给门户规则](../backend/crates/erp-supply/src/portal/service.rs)、[申请规则](../backend/crates/erp-supply/src/portal/application.rs)、[正式供给确认](../backend/crates/erp-supply/src/portal/confirm.rs)、[供给跨域用例](../backend/crates/erp-processes/src/supplier_portal/offerings.rs)、[供给详情页面](../erp-client/features/supplier-portal/pages/offering-detail-page.tsx) |
| L05 事务、回执与授权 | [命令事务编排](../backend/crates/erp-processes/src/supplier_portal/command.rs)、[原命令拒绝终态](../backend/crates/erp-processes/src/supplier_portal/command_recovery.rs)、[当前身份与内部权限重验](../backend/crates/erp-processes/src/supplier_portal/authorization.rs)、[领域对象访问](../backend/crates/erp-processes/src/supplier_portal/access.rs)、[申请统一分派](../backend/crates/erp-processes/src/supplier_portal/dispatch.rs) |
| L06 专项任务 | [门户任务编排](../backend/crates/erp-processes/src/supplier_portal/tasks.rs)、[任务实体合同](../backend/crates/erp-workflow/src/entity/work_item/entity/supplier_portal.rs)、[任务 Service](../backend/crates/erp-workflow/src/service/work_item/supplier_portal.rs)、[工作台申请摘要](../backend/crates/erp-read-models/src/workbench/supplier_portal_brief.rs) |
| L07 新品原稿与正式建档 | [新品模型](../backend/crates/erp-catalog/src/portal/model.rs)、[新品校验](../backend/crates/erp-catalog/src/portal/validation.rs)、[建档及复用](../backend/crates/erp-catalog/src/portal/materialize.rs)、[唯一条码占用](../backend/crates/erp-catalog/src/repository/catalog/barcode_claim.rs)、[新品跨域事务](../backend/crates/erp-processes/src/supplier_portal/new_products.rs)、[重复匹配上下文](../backend/crates/erp-read-models/src/supplier_portal/review_context.rs) |
| L08 字典、包装与规格 | [独立字典候选](../backend/crates/erp-catalog/src/portal/dictionaries.rs)、[字典及目标校验](../backend/crates/erp-catalog/src/portal/service.rs)、[供应商分类映射事实](../backend/crates/erp-catalog/src/portal/category_mapping.rs)、[分类映射保存及建议](../backend/crates/erp-catalog/src/portal/category_mapping_service.rs)、[包装声明](../backend/crates/erp-catalog/src/portal/packaging.rs)、[新品输入页面](../erp-client/features/supplier-portal/pages/new-product-page.tsx)、[内部匹配与审核页面](../erp-client/features/supplier-portal-admin/pages/review-page.tsx)、[审核内字典补建](../erp-client/features/supplier-portal-admin/components/dictionary-create-dialog.tsx)、[勾选 SKU 复用单位映射](../erp-client/features/supplier-portal-admin/components/unit-mapping-bulk.tsx)、[跨申请辅助映射](../erp-client/features/supplier-portal-admin/components/review-mapping-batch.tsx) |
| L09 合作条款 | [合作条款领域](../backend/crates/erp-supplier/src/portal/mod.rs)、[合作条款跨域用例](../backend/crates/erp-processes/src/supplier_portal/commercial.rs)、[合作资料页面](../erp-client/features/supplier-portal/pages/cooperation-page.tsx) |
| L10 批量 | [整批预检及逐单元执行](../backend/crates/erp-processes/src/supplier_portal/batch.rs)、[批量对话框](../erp-client/features/supplier-portal/components/batch-dialog.tsx)、[表格读取](../erp-client/features/supplier-portal/lib/batch-import.ts)、[逐行状态模型](../erp-client/features/supplier-portal/lib/batch-model.ts) |
| L11 素材 | [素材授权与绑定](../backend/crates/erp-processes/src/supplier_portal/assets.rs)、[图片与资料上传及受控下载](../backend/apps/web-api/src/core/handler/supplier_portal/assets.rs)、[静态 PDF 内容检查](../backend/apps/web-api/src/core/handler/supplier_portal/asset_pdf.rs)、[门户素材页面组件](../erp-client/features/supplier-portal/components/attachments.tsx)、[内部审核素材组件](../erp-client/features/supplier-portal-admin/components/review-images.tsx) |
| L12 采购选源与影响 | [采购行选源事实](../backend/crates/erp-procurement/src/entity/purchase_order/offering_source.rs)、[创建时冻结选源](../backend/crates/erp-procurement/src/service/purchase_order/creation_basis/create.rs)、[提交时重验选源](../backend/crates/erp-processes/src/procure_to_pay/start_approval/supply_selection.rs)、[当前关联采购仓储](../backend/crates/erp-procurement/src/repository/purchase_order/offering_usage.rs)、[内部影响读模型](../backend/crates/erp-read-models/src/supplier_portal/impacts.rs)、[工作台缺货提示](../backend/crates/erp-read-models/src/workbench/supply_warnings.rs)、[审核影响组件](../erp-client/features/supplier-portal-admin/components/offering-impacts.tsx) |
| L13 上架与价格边界 | [新品建档](../backend/crates/erp-catalog/src/portal/materialize.rs)、[上架领域服务](../backend/crates/erp-catalog/src/service/catalog/listing.rs)、[商品列表及待上架入口](../erp-client/features/master-data/pages/products-list-page.tsx)、[第一期业务基线](erp-phase-1.md) |
| L14 门户入口与会话 | [门户路由](../backend/apps/web-api/src/core/routes/supplier_portal.rs)、[独立门户会话](../erp-client/features/supplier-portal/components/portal-session.tsx)、[已有商品报价页面](../erp-client/features/supplier-portal/pages/quotes-page.tsx)、[新品页面](../erp-client/features/supplier-portal/pages/new-product-page.tsx)、[我的申请页面](../erp-client/features/supplier-portal/pages/applications-page.tsx)、[内部审核差异组件](../erp-client/features/supplier-portal-admin/components/review-diff.tsx) |

## 3 必须遵守的配置与数据规则

### 3.1 内部权限配置

1. 现存内部角色不因服务启动自动取得门户专项权限。管理员须按岗位责任显式授权；新模板包含专项权限不表示既有角色已获授权。
2. 按操作授予下列权限，不得为便于验收将所有参与人员统一配置为管理员：

| 操作 | 专项权限 |
| --- | --- |
| 查询账号 | `supplier_portal_account:list` |
| 开通账号 | `supplier_portal_account:create` |
| 停用及调整账号 | `supplier_portal_account:update` |
| 查看目录开放 | `supplier_portal_catalog:list` |
| 开放或撤销 SKU | `supplier_portal_catalog:update` |
| 查看申请列表 | `supplier_portal_request:list` |
| 查看申请、审核素材及关联影响 | `supplier_portal_request:detail` |
| 通过或退回申请、读取匹配候选 | `supplier_portal_request:review` |

3. 专项权限之外，仍须配置实际供应商、供给、商品和采购的 DataScope、对象处理资格、内部维护责任及任务处理资格。字典补建采用原字典维护权限，审核资格不得替代字典创建权。
4. 普通供应商、SKU、采购人员选择器采用各自查询权限；无权限时不得在后台发送相应请求。有权读取候选不表示有权修改候选对象。
5. 每条待确认申请必须有启用且具备实际处理资格的内部人员。缺少合法默认维护人时先完成责任配置或合法转交，不创建无人负责的任务。

### 3.2 精确选源与冻结事实

1. 新创建的采购商品行从已验证的选源依据冻结 `supplier_offering_id`、`supplier_offering_revision_id`、`offering_version`、`revision_version`、`availability_version`；客户端不得自行改写这组来源。
2. 提交、正式版本、采购变更及撤回恢复须保留原选源事实，不能从供应商和 SKU 重新猜测。恢复草稿再次提交时，沿原 source 在最终写入事务重验当前供给、条款、可供版本、期限、关系状态和数量资格；旧依据必须拒绝，冻结价格不得自动替换。
3. 历史行缺少 `supplier_offering_source` 时保留“关联未知”。影响列表只能表示已证明的精确关联，不得宣称覆盖所有历史采购。
4. 影响读取重验当前采购范围、责任人、采购正式指针、未完成履约对象及其当前开放任务。提示不自动取消单据，不回改冻结金额、付款条件、库存或财务事实。

### 3.3 单位、包装与可供时间

1. 正式供给价格、起订量和可供数量采用公司 SKU 基础单位。已有 SKU 的基础单位保持只读。
2. 包装换算由供应商明确填写已按基础单位核对的价格和数量，并确认原单位、基础单位、每包数量和原单位报价。当前流程不自动执行数学换算或四舍五入。审核人不得直接改写供应商实质报价或单位含义。
3. 缺少包装依据、未作供应商确认、基础单位不一致或数量精度超限时阻断提交或确认；不得默认补为“件”。单位原文与正式名称不同时，审核人须逐行显式确认同义含义并填写非空依据；不得据此改变数量、价格、包装或将箱与瓶视为同义。
4. 初始可供时间保留供应商实际报送的 Unix 秒整数。审核不得将通过时间写成报送时间；未来或非法报送时间须拒绝。
5. 未配置自动过期时限时不擅设阈值。首次报价和新品审核须显式确认已核对实际报送时间；需要更新资料时退回供应商，保留原历史。

### 3.4 素材上传与保留

1. 素材采用独立上传，不随 Excel 或 CSV 嵌入后直接落库。先取得有权维护的新品草稿，再按草稿当前版本和原操作号上传，后续编辑采用服务端返回的申请版本。
2. 批量新品分为准备及最终提交。准备阶段按商品组保存原草稿，供应商在原批次补齐独立上传图片并核对资料；最终批量以各原申请 ID 和当前版本进行整批预检，逐商品组事务提交为待确认并创建具体审核任务。准备成功不得显示为已提交、已建档或已生效。
3. 受控上传接受 JPEG、PNG、WebP 图片及静态 PDF 资料，单个文件不超过 5 MiB。图片执行完整解码和尺寸、内存限制；PDF 执行严格结构、对象、页面、引用和有界解压检查，具体预算执行接口合同。PDF 不得用作商品或 SKU 图片。
4. PDF 仅接受可完整检查的经典 xref 静态文件；加密、主动行为、嵌入或外部文件、对象流、xref 流、未知过滤器、结构损坏及预算超限须拒绝。Office 和其他格式未开放。内容检查登记 ContentChecked，不得冒充病毒扫描 Passed；后续扫描、隔离及拒绝须继续生效。
5. 下载只允许一个明确来源：自己的新品原稿或历史提交、自己的供给当前图片、当前仍开放 SKU 的当前图片。文件 ID 不构成访问权，不返回公开对象存储地址。
6. 数据库事务外完成对象存储 I/O；事务内完成受控文件记录和申请或正式素材引用绑定。图片上传恢复须保留同一内容和原操作号。
7. 正式素材与有效历史引用不能被临时文件清理删除。运行验收须检查真实存储字节、受控下载和保留行为，库单元测试不能替代该证据。

## 4 当前证据与结果登记

### 4.1 证据类别

| 类别 | 有效证据 | 当前状态 |
| --- | --- | --- |
| S 静态与构建 | 格式、类型、编译、Clippy、权限漂移、领域、BPM 及审计边界、差异检查的命令、退出码、日志与检查版本 | 后端格式、workspace 编译、严格 Clippy、BPM、领域 cutover、审计边界及权限漂移通过，本次 Rust 体积未新增或扩大超限；前端 lint、类型、构建及 77 个改动文件定向格式通过；前端全量格式存在 6 个未改动基线失败 |
| L 库单元测试 | 仓库允许的库测试命令、通过/失败/忽略数量、日志；标明纯规则及模拟范围 | 最终全库 33 个 target：4992 passed、0 failed、63 ignored；不包含真实 MongoDB、S3 或运行验收 |
| P 页面实际检查 | 实际浏览器操作、页面反馈、冲突输入保留、只读状态、权限外后台请求检查及可追溯截图 | 未验证 |
| I 真实接口与跨账号 | 授权及拒绝请求的实际状态、响应、目标归属、账号角色与版本；不保存密码或完整凭证 | 未验证 |
| D 真实事务与采购影响 | 授权副本集环境中的事实前后值、原子提交/回滚、并发结果、当前任务和冻结采购证据 | 未验证 |
| F 真实对象存储 | 实际上传及下载字节、内容指纹、跨来源拒绝、存储失败处理、正式素材保留证据 | 未验证 |

### 4.2 统一门禁登记表

实施负责人按已确认结果填写下表。代码再次变更且影响结果时重新执行对应门禁。证据目录为 `logs/supplier-portal-review-fix-20261004/`，后端和前端日志均已归档。代码版本及各文件指纹由该目录内的 [工作区清单](../logs/supplier-portal-review-fix-20261004/worktree-manifest.json) 登记，不以本次门禁结果证明后续改动。

| 检查 | 实际命令或执行入口 | 结果与数量 | 证据位置及版本 |
| --- | --- | --- | --- |
| 后端格式 | `cargo fmt --all -- --check` | 通过，退出码 0 | [格式日志](../logs/supplier-portal-review-fix-20261004/backend/fmt.log) |
| 后端 workspace 编译 | `cargo check --workspace --locked` | 通过，退出码 0 | [workspace 编译日志](../logs/supplier-portal-review-fix-20261004/backend/workspace-check.log) |
| 后端严格 Clippy | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | 通过，退出码 0 | [严格 Clippy 日志](../logs/supplier-portal-review-fix-20261004/backend/clippy.log) |
| 后端库单元测试 | `env -u ERP_TEST_MONGO_URI cargo test --workspace --lib --locked` | 最终复跑通过，退出码 0；33 个 target，4992 passed、0 failed、63 ignored | [库单测日志](../logs/supplier-portal-review-fix-20261004/backend/library-tests.log)、[数量汇总](../logs/supplier-portal-review-fix-20261004/backend/library-summary.json) |
| BPM 与领域边界 | `./scripts/check-bpm-boundaries.sh`、`./scripts/check-domain-boundaries.sh --cutover` | 两项通过 | [BPM 日志](../logs/supplier-portal-review-fix-20261004/backend/bpm-gate.log)、[领域 cutover 日志](../logs/supplier-portal-review-fix-20261004/backend/domain-gate.log) |
| 审计边界 | `./scripts/check-audit-boundaries.sh` | 通过，退出码 0；不替代业务运行覆盖证据 | [审计边界日志](../logs/supplier-portal-review-fix-20261004/backend/audit-gate.log) |
| 权限生成物与漂移 | `./scripts/check-permissions-drift.sh`，使用临时 Git 索引比较重建前后 | 通过，退出码 0；权限及审计动作两份生成物重建前后相同 | [权限漂移日志](../logs/supplier-portal-review-fix-20261004/backend/permission-drift.log) |
| 前端 lint 与类型 | `npm run lint`、`npx tsc --noEmit` | 两项通过，退出码 0 | [lint 日志](../logs/supplier-portal-review-fix-20261004/frontend/lint.log)、[串行类型检查日志](../logs/supplier-portal-review-fix-20261004/frontend/typecheck.log) |
| 前端本次改动格式 | `npx oxfmt --check` 最终定向检查 77 个改动文件，包含权限生成物 | 通过，退出码 0 | [最终定向格式日志](../logs/supplier-portal-review-fix-20261004/frontend/format-scoped.log) |
| 前端全量格式 | `npm run format:check` | 退出码 1；仅 6 个未改动基线文件失败 | [全量格式日志](../logs/supplier-portal-review-fix-20261004/frontend/format-full.log)、[HEAD 基线核对](../logs/supplier-portal-review-fix-20261004/frontend/format-baseline.json) |
| 前端生产构建 | `npm run build` | 通过，退出码 0 | [生产构建日志](../logs/supplier-portal-review-fix-20261004/frontend/build.log) |
| 全部改动差异检查 | 临时 Git 索引执行 `git diff --cached --check`，包含未跟踪新文件 | 通过，退出码 0；实际 Git 索引保持未暂存 | [差异检查日志](../logs/supplier-portal-review-fix-20261004/diff-check.log) |
| 本次 Rust 源文件与方法体积 | 改动范围尺寸扫描 | 210 个文件；0 新增超限，7 个原有超限均未扩大；不表示全库历史超限清零 | [本次体积日志](../logs/supplier-portal-review-fix-20261004/backend/size-check.log) |

全库基线失败、本次范围失败和已通过检查分别登记。部分检查通过不得写成“所有门禁通过”。忽略的测试不得计入通过数。统一库测试通过不表示每个 SP 项已取得相应真实运行证据。

前端全量格式检查仅报告下列未改动基线文件；本次不将它们计为门户改动失败，也不将全量格式结果登记为通过：

- `features/access-audit/components/access-list-toolbar.tsx`
- `features/access-audit/pages/access-audit-page.tsx`
- `features/customer-receivables/components/receivable-counterparty-search-combobox.tsx`
- `features/data-scope/cache.ts`
- `features/invoice-requests/scoped-view.ts`
- `lib/selector-list.ts`

前端各项退出码及上述基线范围执行 [前端门禁结果登记](../logs/supplier-portal-review-fix-20261004/frontend/gate-results.json)，该登记保留原 67 文件检查；最终数量及结果以 [68 文件定向格式日志](../logs/supplier-portal-review-fix-20261004/frontend/format-scoped.log) 为准。后端各项退出码执行 [后端门禁结果登记](../logs/supplier-portal-review-fix-20261004/backend/gate-results.json)。权限漂移采用仅暂存本次两份生成物的临时 `GIT_INDEX_FILE`，比较重建前后的内容，不修改真实索引、不创建提交；新增专项权限相对 HEAD 的已授权差异不作为生成漂移。

### 4.3 必须具备的实施能力

下列能力已完成代码落地；运行结果按第 5 节分别取得证据。实施缺失与运行未验证分别记录，不得以“待运行验收”替代代码能力交付。

| 关联验收项 | 已实施能力 | 当前状态 |
| --- | --- | --- |
| SP12、SP17、SP23 | 批量新品准备原草稿、在批次补图，最终以原申请 ID 和版本统一预检并逐组进入待确认及创建具体任务；原操作号恢复保持原申请身份 | 已落代码；运行未验证 |
| SP25、SP29 | 有字典创建权者在审核页完成必要补建并将新正式值关联回匹配；无权者可合法转交，不能仅以普通列表跳转代替补建闭环 | 已落代码；运行未验证 |
| SP26、SP30 | 生效决定同事务保存供应商、原始完整分类路径及商品类型下的独立分类映射与确认历史；后续仅提供需再次确认的建议，完整路径或版本变化要求重新核对 | 已落代码；运行未验证 |
| SP30 | 审核人选择已确认单位来源后显式勾选相同语义行；跨申请按内部账号和目标申请保存辅助映射，采用前重读申请及完整字典路径版本。只复用品牌、分类、单位及同义确认，不携带复用决定、维护人或审核决定 | 已落代码；运行未验证 |
| SP04、SP10 | 首次报价冻结 Product、SKU、精确当前修订及基础单位全部版本；保存、提交及通过同事务重验当前资格，不用通过时的最新值替换冻结依据 | 已落代码；运行未验证 |
| SP03、SP08、SP10 | 密码长度在哈希前执行 6 至 32 字符校验；任务执行与转交候选重验真实业务写权限；指定维护人须为启用内部账号并具备对应维护资格 | 已落代码；运行未验证 |
| SP08、SP09、SP12 | 结果未知保留原命令；当前前提已拒绝时，通过同一唯一回执争用写拒绝终态，确定封存后允许重新核对并建立新操作号；不在实际业务写入错误后提交拒绝事实 | 已落代码；真实并发未验证 |
| SP13 | 恢复采购草稿提交沿冻结选源重验当前资格及版本；供给关系停止独立于可供有货形成真实关联提示 | 已落代码；运行未验证 |
| SP18、SP20 | 不同 SKU 的建档及修订事务争用同一条码唯一占用；同 SKU 历次修订允许复用；历史多归属明确拒绝 | 已落代码；真实并发未验证 |
| SP23、SP27、SP30 | 严格静态 PDF 资料检查及可继续治理的内容检查态；公共商品主图受控回退；正式规格采用稳定签名；根至叶分类链冻结及原文保留的同义确认 | 已落代码；运行未验证 |

## 5 SP01—SP30 运行验收规程

### 5.1 运行准备与证据采集

1. 仅在第 1 节规定的当次环境授权完成后执行本节。
2. 准备供应商 A、B，各自的维护员与只读账号，以及启用、停用、已撤销绑定的账号状态。准备具有专项权限和对应范围的内部处理人、范围外处理人、无字典创建权处理人及合法转交接收人。
3. 准备精确 SKU 定向开放、未开放、开放后撤销，以及供给正常、暂停、停止、临时缺货、API 来源和条款过期样本。准备新商品、已有商品新规格、已有 SKU 无本供应商供给、已有同一供给、订货编码冲突及字典未匹配样本。
4. 准备冻结了真实选源的新采购、缺少历史选源的旧采购、不同维护责任和履约状态的样本。验收不得修改现有历史数据来伪造选源关联。
5. 每个写入场景先记录申请、供给条款、可供、相关正式事实和任务的版本；执行后读取实际结果及相应证据。网络成功反馈或页面提示不能替代事实核验。
6. 页面发起的操作同时登记真实响应，确认业务成功、对象标识和版本后再核验后续读取。素材须核验浏览器取得的实际下载字节。
7. 以下“所需证据”只表示验收要求。当前各项 P、I、D、F 均为未验证；S、L 结果由第 4 节单独登记。

### 5.2 身份、供给、任务与批量

| 编号及落点 | 必须执行的操作 | 通过标准与所需证据 | 当前运行状态 |
| --- | --- | --- | --- |
| SP01；L01、L03、L11 | A、B 分别读取列表、详情、历史、统计和素材；互换 URL、请求体目标及文件来源；检查批量和页面可达的导出路径 | 对方数据和未知对象均不能取得，不泄露是否存在；所有实际暴露的入口采用同一归属边界。I、P、F | 未验证 |
| SP02；L01、L14 | 将门户凭证用于内部管理接口，将内部凭证用于门户；检查门户页面的后台请求 | 服务端拒绝错误主体；供应商不能取得内部组织或角色授权；门户不发出越权内部查询。I、P | 未验证 |
| SP03；L01、L02、L05 | 用只读账号执行全部写入口；登录后分别停用账号、停用供应商和撤销绑定，再用旧凭证访问 | 只读写入被拒绝；失效后续请求立即拒绝，不能等待凭证自然过期；无正式事实变化。I、D、P | 未验证 |
| SP04；L02、L03、L04 | A、B 查看各自目录；提交首次报价后分别撤销 SKU 开放、停用 Product、更新 Product/SKU 当前修订或单位，再执行确认 | 只显示各自开放的精确 SKU；失效开放、停用对象及任一冻结版本变化在最终确认时阻断；不读取销售价或其他供应商供给。I、D、P | 未验证 |
| SP05；L04、L05 | 分别更新数量为空、为零、有货和缺货；读取供给、条款及公司库存；使用旧可供版本再次更新 | 空白保留未提供语义，零明确无货；立即更新可供，不增加条款修订、不修改库存；旧版本拒绝且页面保留输入。I、D、P | 未验证 |
| SP06；L04、L12 | 对暂停、停止、资格失效和条款过期供给报告恢复有货，再尝试新选源 | 可供事实可独立展示；内部管控及其他资格仍阻断选源，恢复不修改关系状态。I、D、P | 未验证 |
| SP07；L03、L04、L14 | 保存并提交调价，读取门户待确认差异和正式供给，尝试采购选源 | 待确认内容与当前正式值分别展示；确认前正式选源仍使用原有效条款，不消费申请价格。I、D、P | 未验证 |
| SP08；L04、L05、L06 | 同一命令重复确认；同号改内容；正式版本变更后确认旧申请；并发执行通过和撤回；验证明确拒绝可修改、409 重新读取并人工确认、未知结果原号恢复 | 同号同内容仅一个结果，同号不同内容拒绝；旧版本不能覆盖；通过与撤回最多一个有效结果，页面保留待核对内容。I、D、P | 未验证 |
| SP09；L05、L06、L07 | 在授权环境对申请确认执行可控失败验证，逐项核验正式事实、申请、任务、审计及回执；按原命令恢复 | 所有事务内写入同时提交或回滚；结果未知先恢复原结果，无部分生效、重复正式版本或重复任务完成。D、I | 未验证 |
| SP10；L02、L04、L05、L14 | 完成首次报价、同供给修订、订货编码占用其他 SKU、无合法维护责任、停用或非内部维护人、无供给维护权维护人和目标状态变化场景 | 首次只关联合法开放 SKU；同一供给追加条款；冲突拒绝；责任明确且原维护人、组织及 SKU 上架状态保留。I、D、P | 未验证 |
| SP11；L09、L05 | 保存、提交并确认合作条款，比较供应商商务档案、供给条款及已冻结采购付款条件 | 只追加供应商商务档案；供给价格修订及冻结采购不变；审核页说明后续采购影响。I、D、P | 未验证 |
| SP12；L10、L05、L11 | 四种批量模式分别预检；包含非法行和重复业务单元；制造执行部分失败及网络结果未知后按原号恢复；对原请求未到达且目标版本已变化的样本封存拒绝终态，同时发送迟到原请求；新品在原批次补齐原草稿素材后最终批量提交 | 预检失败阻断新写入；执行成功单元保留且锁定，失败输入保留；恢复不重复创建，拒绝终态与迟到原请求最多一方提交，封存后原号不能写业务；新品最终批量使用原草稿进入待确认并同事务创建具体任务，准备成功不得称已提交。I、D、F、P | 未验证 |
| SP13；L12、L04 | 新选源前报告缺货或停供；查看真实关联未完成履约采购、当前负责人及任务；检查历史无 source、范围外和已完成对象 | 新选源采用最新状态；仅提示精确来源、当前责任及开放实际履约任务；历史关联未知；不自动取消或改写采购与财务。I、D、P | 未验证 |
| SP14；L03、L04、L05 | 检查门户各响应允许字段；维护 Excel 来源供给后读取原来源；对 API 来源供给执行单条和批量写入 | 无其他报价、销售价、毛利、客户或内部备注；原登记来源保留、本次渠道独立；API 写入拒绝。I、D、P | 未验证 |
| SP15；L03、L04、L05、L06 | 完成提交、退回、修改重提、撤回及确认，读取实际审计、提交历史、正式修订及可供 | 可区分真实供应商提交人与内部确认人、各次快照和决定；正式条款与实时可供分开；无内部身份冒用。I、D、P | 未验证 |

### 5.3 两条业务线、建档及素材

| 编号及落点 | 必须执行的操作 | 通过标准与所需证据 | 当前运行状态 |
| --- | --- | --- | --- |
| SP16；L07、L14 | 从门户直接进入已有商品报价和新品提报并完成各自审核 | 已有 SKU 无需重复录入商品主数据；新品无需内部先建档；两线均返回可读取的实际供给结果。I、D、P | 未验证 |
| SP17；L07、L11 | 新品保存草稿、提交、退回和撤回；分别读取正式目录与素材；尝试跨供应商引用申请和素材 | 非生效状态不产生正式商品、SKU 或供给；原稿和历史保留；素材访问和绑定不越权。I、D、F、P | 未验证 |
| SP18；L05、L06、L07、L11 | 对一个含多个 SKU 的新品执行一次通过、可控失败和原命令重试；并发通过两个包含同条码的新 SKU 申请 | 缺少对象、全部本次 SKU、供给、初始可供、责任、素材引用、结果、任务与审计原子落地；失败无部分建档，重试无重复；同条码不同 SKU 最多一方完整建档，失败方无部分事实。D、I、F | 未验证 |
| SP19；L07、L04、L08 | 分别审核全新商品、已有商品新增规格、已有 SKU 新增供给、已有同供给转修订及歧义冲突 | 显式匹配所选对象及版本；实际结果类型正确；不自动按名称合并，不重复建档，不覆盖共享现行资料。I、D、P | 未验证 |
| SP20；L07、L13 | 审核新品后读取每个新 SKU 上架字段；查询待上架、销售商品池、开单和选品候选及相关导出；直接提交使用未上架 SKU 的销售写命令 | 新建 SKU 明确为 Unlisted 且商品库可见；所有新增销售候选、相关导出及服务端销售资格均阻断；不自动生成销售价。I、D、P | 未验证 |
| SP21；L07、L13 | 对已有在售 SKU 新增供给，对已有商品新增一个规格；比较全部相关 SKU 前后状态 | 复用 SKU 原状态不变；已有商品原 SKU 不下架；仅本次新建 SKU 未上架，审核页说明已有在售对象的影响。I、D、P | 未验证 |
| SP22；L07、L08、L13 | 比较供应商原稿和内部规范化快照；尝试改变价格、规格或单位含义后通过；检查正式商品写入口及销售价 | 原稿不被覆盖；实质变化须供应商重提，不能格式整理后直接通过；供货价不变为销售价；门户不能修改正式主数据。I、D、P | 未验证 |
| SP23；L07、L11、L04 | 上传合法与伪造/截断图片、合法经典 xref PDF、含主动内容或超预算 PDF；检查 ContentChecked 后续扫描、隔离和拒绝；核验浏览器实际下载；用其他来源及撤销目录读取；审核后检查正式素材保留和初始报送时间 | 格式、内容预算与归属真实校验，PDF 不用作主图；ContentChecked 不代替病毒扫描，后续隔离或拒绝阻断使用；下载不越权，有效正式素材不会被临时清理删除；审核须显式核对时间并保留原实际报送时间。I、D、F、P | 未验证 |

### 5.4 字典、包装、规格与快照

| 编号及落点 | 必须执行的操作 | 通过标准与所需证据 | 当前运行状态 |
| --- | --- | --- | --- |
| SP24；L07、L08 | 使用资料充分但未匹配的品牌、分类或单位提交；尝试不完成正式映射即通过；再提交缺失和非法输入 | 充分原始资料允许进入审核；无正式映射不能建档；缺失及非法输入定位到字段或 SKU，不能与待内部匹配混同。I、D、P | 未验证 |
| SP25；L08、L07 | 提交品牌建议，读取正式字典；审核疑似重复；由有权人员完成必要补建并重试，检查无品牌标准项 | 建议不自动创建字典；复用经内部确认；补建及重试不重复创建；未知、未匹配与正式无品牌项不混用。I、D、P | 未验证 |
| SP26；L08、L07 | 用同名不同父级分类、不同商品类型及不同供应商原路径执行匹配；复核保存的映射及后续建议；改变目标状态、叶或任一祖先名称/父级/类型/版本后再确认 | 使用稳定 ID、完整路径与适用类型；任一分类链变化拒绝旧冻结映射；映射按供应商、原路径和类型隔离；建议不代替确认，不自动回改已建档历史。I、D、P | 未验证 |
| SP27；L08、L04 | 对已有 SKU 尝试改单位；新品填箱/瓶包装关系并由供应商填写基础单位价格和数量；分别缺依据、不确认和精度超限 | 既有单位只读；包装与基础单位一致且供应商显式确认；不自动数学换算；不可靠依据和精度超限阻断，不默认件。I、D、P | 未验证 |
| SP28；L07、L08 | 提报新的属性和值、重复规格组合和规范化后重复组合；尝试以名称整理改变单位或规格含义 | 新规格可提报；重复组合阻断；格式整理不改变 SKU 身份，实质变化退回供应商确认。I、D、P | 未验证 |
| SP29；L08、L05 | 无字典维护权审核人尝试补建；匹配后停用字典或改变关键引用，再通过；内部补建后继续原申请 | 无权补建拒绝且无越权后台请求；最终确认重验存在、启用、版本及适用条件；必要内部补建不要求供应商重复录入。I、D、P | 未验证 |
| SP30；L03、L07、L08、L10 | 搜索有效但尚无商品使用的字典；检查门户候选字段；读取原稿与映射记录；显式勾选同申请行及同供应商跨申请目标应用映射，再改变原稿、目标申请版本或字典链后尝试采用已保存计划 | 独立候选可读取且不泄露关联业务；原始值与最终映射、确认人、时间、理由分别可追溯；只作用于明确选择且适用的记录。I、D、P | 未验证 |

## 6 结果登记与完成条件

1. 每个 SP 项登记：代码版本、执行人、环境授权范围、账号角色及对象范围、样本标识、步骤、预期、实际 HTTP/页面/业务事实结果、证据位置和结论。
2. 结论仅使用“通过”“失败”“未验证”“不适用”。“不适用”须说明接口或业务路径确实不存在的依据，并由验收负责人确认；不得以没有测试环境或未执行代替“不适用”。
3. 涉及身份和权限的项目同时登记允许与拒绝结果；仅按钮隐藏、仅管理员成功、仅静态类型通过不能证明权限正确。
4. 涉及事务和并发的项目必须有真实副本集证据；涉及素材的项目必须有真实对象存储和实际下载字节证据；涉及交互反馈的项目必须有页面实际检查证据。
5. 跨账号、历史无来源、对象状态变化、供应商或账号失效、版本冲突、结果未知等边界未取得证据时，对应 SP 项保持未验证。
6. 完成状态分别登记为“代码实施完成”“静态与库单元测试完成”“页面检查完成”“运行验收完成”。只有 SP01—SP30 全部取得适用类别的通过证据后，才能登记“供应商门户第一版整体验收通过”。

| 交付状态 | 当前结论 | 更新责任 |
| --- | --- | --- |
| 代码实施与落点登记 | 已实施并登记；不代表运行验收通过 | 实施负责人 |
| 最终静态与构建 | 检查已完成，具体通过范围见第 4.2 节；前端全量格式存在 6 个未改动基线失败 | 实施负责人 |
| 最终库单元测试 | 33 个 target；4992 passed、0 failed、63 ignored | 实施负责人 |
| 页面实际检查 | 未验证 | 运行验收负责人 |
| 真实接口与跨账号 | 未验证 | 运行验收负责人 |
| 真实事务、并发与采购影响 | 未验证 | 运行验收负责人 |
| 真实素材上传、下载与保留 | 未验证 | 运行验收负责人 |
| 第一版整体验收 | 未通过；运行证据尚未取得 | 运行验收负责人 |
