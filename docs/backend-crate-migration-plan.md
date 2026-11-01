# 后端 crate 分层与编排拆分迁移方案

## 1. 目标与范围

将 `backend/crates/` 按职责分层，将 `erp-processes` 拆为独立业务编排库，缩小修改后的重编译和库测试编译范围。

- 父目录表示架构层次，crate 名表示业务职责；已有领域 crate 保留包名。
- 保留 `backend/Cargo.toml` 为唯一 workspace。分组目录不建立 crate 或嵌套 workspace。
- 保持 HTTP/CLI 合同、权限、序列化、数据库集合与索引、任务身份、审计事件和业务行为不变。
- 保持原事务边界、同一 `Executor`、授权重验、写入顺序和幂等恢复语义；外部 I/O 继续位于事务之外。
- 本次不拆分 `erp-read-models`，不更换存储或部署架构，不新增业务数据迁移。
- 最终删除 `erp-processes`，不保留全量转发 crate、旧路径别名或兼容 façade。

## 2. 目标目录

```text
backend/
├── Cargo.toml
├── apps/                              # 可执行入口、协议处理、依赖装配、worker 生命周期
│   ├── web-api/
│   └── cli/
├── crates/
│   ├── domains/                       # 19 个现有领域 crate，保留各自实体、服务和仓储
│   │   ├── erp-sales/
│   │   ├── erp-procurement/
│   │   ├── erp-finance/
│   │   ├── erp-supply/
│   │   └── ...
│   ├── orchestration/                 # 跨域命令与事务协调；具体成员见第 3 节
│   ├── read-models/
│   │   └── erp-read-models/
│   ├── adapters/                      # 外部协议与供应商技术实现
│   │   ├── erp-connector-jd-vop/
│   │   ├── erp-connector-dangaoshushu/
│   │   └── erp-contract-recognition/
│   ├── foundation/
│   │   ├── erp-core/
│   │   ├── application-core/
│   │   └── persistence-core/
│   ├── platform/
│   │   ├── bpm/
│   │   ├── storage/
│   │   ├── aliyun-ocr/
│   │   ├── entity-core/
│   │   ├── entity-macros/
│   │   ├── id-generator/
│   │   └── background-runtime/        # 不依赖 ERP 领域的轮询、退出与执行预算机制
│   └── testing/
│       └── test-support/
├── config/                            # 保持独立配置 crate
└── scripts/
```

`domains/` 接收现有 `erp-identity`、`erp-audit`、`erp-workflow`、`erp-support`、`erp-party`、`erp-customer`、`erp-supplier`、`erp-catalog`、`erp-warehouse`、`erp-contract`、`erp-import`、`erp-inventory`、`erp-finance`、`erp-sales`、`erp-procurement`、`erp-fulfillment`、`erp-returns`、`erp-integration`、`erp-supply`。

更新 workspace 成员和 `[workspace.dependencies]` 中的路径。依赖方继续使用 `workspace = true`，不重复声明跨层相对路径。新 crate 只声明实际使用的依赖与 features，不复制旧 crate 的全量依赖。

## 3. 编排与公共能力归属

下表中的源模块均相对于 `erp-processes/src/`。公开类型及其固有 `impl` 一起迁移，测试随被测实现迁移。

| 目标编排 crate | 接收的主要模块 |
| --- | --- |
| `erp-master-data` | `party`、`customer`、`customer_profile`、`supplier`、`supplier_profile`、`catalog`、`warehouse`、`contract`、`product_handover`、`offering_handover` |
| `erp-data-import` | `product_import`、`supplier_import`、`contract_import`、`import_apply`、`sales_inquiry_import`、`data_transfer`、`xlsx` |
| `erp-supplier-catalog` | `supply_governance`、`supplier_catalog_adoption`、`supplier_connection_execution`、`supplier_portal`、`supplier_catalog_callbacks`、`source_media`、`product_source_sync`、`supplier_delivery_template`、`sellable_projection_refresh`、`delivery_coverage_refresh`、`source_registry`、`integration_resolution` |
| `erp-supplier-orders` | `supplier_api_orders`、`supplier_deposits`、`supplier_order_tracking`、`supplier_after_sales`、`supplier_messages`、`supplier_callbacks`；供应商凭据与授权持久化用例 |
| `erp-order-to-cash` | `order_to_cash`、`sales_change`、`sales_inquiry`、`sales_selection`、`sales_inquiry_expense` |
| `erp-procure-to-pay` | `procure_to_pay`、`purchase_correspondence` |
| `erp-fulfillment-execution` | `fulfillment_execution`、`inventory_adjustment`、`supply_execution` |
| `erp-finance-posting` | `finance_posting` |
| `erp-reverse-flow` | `reverse_flow` |
| `erp-supply-settlement` | `supply_settlement` |
| `erp-demo-data` | `demo_master_data`，仅由现有演示数据入口和 CLI 按需调用 |
| `erp-approval-runtime` | 审批启动、材料冻结、精确回执恢复所需的共有跨域步骤；不包含业务动作分派 |
| `erp-file-assets` | `attachments/pending` 的跨域文件资产批次与审计协调；不包含商品、付款或履约命令 |

`erp-finance-posting` 仅承接财务过账编排；逆向处理和供应结算使用各自的业务库。`order-to-cash`、`procure-to-pay` 表示业务链路协调，不改变销售、采购、履约和财务事实的领域归属。

其余公共代码按以下规则拆分，不建立统一的 `process-core` 或全域 adapter 库：

| 源代码 | 迁移规则 |
| --- | --- |
| `adapters` | 单一消费方的实现随对应编排库；跨库共享的具体接线由应用装配并通过 Port 注入；Process 工厂移至所属编排库或应用装配处 |
| `connectors` | 纯协议实现进入两个 connector crate；凭据落库、权限重验和接收回执持久化进入所属编排库；技术通信与业务持久化之间通过 Port 连接 |
| `connectors/registry` | 协议无关的运行时注册与分派移至 `erp-supply`，只持有 `SupplierRuntime` 等接口，不导入具体 connector |
| `audit` | 审计持久化与通用执行机制归 `erp-audit`；通用执行接口使用泛型错误或关联错误，不依赖业务流程错误类型；跨域审计转换留在消费方 |
| `errors` | 每个编排库拥有本地错误类型，只转换实际依赖的领域错误；HTTP 映射由 `web-api` 统一承接 |
| `approval_start`、`approval_recovery`、`approval_materials` | 共有跨域步骤归 `erp-approval-runtime`；纯工作流状态与回执操作归 `erp-workflow` |
| `approval_dispatch` | 动作分派归 `web-api` 装配；跨域只读事实归 `erp-read-models`；单域规则归拥有领域 |
| `attachments` | 商品与供应商命令归 `erp-master-data`，付款命令归 `erp-finance-posting`，履约命令归 `erp-fulfillment-execution`；共用批次能力归 `erp-file-assets` |
| `background` | runner、任务执行接口、periodic、drain 和通用预算归 `background-runtime`；具体任务 adapter 随业务库；来源缓存归 `erp-data-import`；任务注册与生命周期归应用 |
| `scheduled_execution` | 调度回执状态与执行合同归 `erp-support`；XXL HTTP 回报实现及业务任务接线归 `web-api`；回报重试不得重放业务 |
| `business_ownership`、`handover_common`、`document_object` | 单域资格与规则归拥有领域；跨域事实查询归读模型；调用方负责注入实际提供者 |
| `test_indexes` | 索引定义归拥有领域；测试辅助随测试归属；普通业务库的测试不得反向依赖 `erp-demo-data` |

拆分边界只公开必要的调用合同，不批量将 `pub(crate)` 改为 `pub`，不以字符串化错误代替跨库转换。

## 4. 依赖与解耦规则

| 层次 | 依赖规则 |
| --- | --- |
| `apps` | 装配领域、编排、读模型和具体外部适配器；CLI 不依赖 web-api |
| `orchestration` | 可依赖所需领域、读模型和明确的下层能力；不依赖应用入口或具体外部协议实现；编排库间只允许登记的单向依赖 |
| `read-models` | 可读取多个领域；不依赖编排库，不执行业务写入 |
| `domains` | 不直接依赖其他领域、编排、读模型或具体外部适配器；外域能力由本域 Port 接入 |
| `adapters` | 依赖消费方 Port 合同及必要技术组件；不依赖编排或应用入口 |
| `foundation`、`platform` | 不依赖领域、编排、读模型、ERP 外部适配器或应用入口；各技术组件继续执行原有专属约束 |
| `testing` | 仅作允许的 dev-dependency；保留现有领域禁用共享测试夹具等约束，不以测试依赖绕过业务边界 |

普通、构建和开发依赖均纳入边界检查。外部业务 Port 由对应领域定义，具体适配器不导入编排层类型。保持 `bpm` 无 ERP、数据库、HTTP 和配置依赖，保持合同识别适配器的现有专属限制。各编排库的直接依赖写入显式清单，未登记的跨编排依赖必须使门禁失败。

迁移交易主链前，必须消除以下回边：

| 回边 | 必须完成的调整 |
| --- | --- |
| 采购与 API 订单互调直发草稿准备 | 将共同准备步骤归履约编排；采购和供应商订单消费窄接口，不调用对方完整 Process |
| 销售询价与采购责任分配互调资格校验 | 资格规则归拥有领域，身份与范围事实经 Port 注入；责任分配不依赖销售编排 |
| 财务、逆向、履约调用销售进度 helper | 复用销售域已有进度操作和财务事实 Port，移除对完整销售编排库的依赖 |
| 付款、预存款与 API 执行互调预算和任务更新 | 冻结预算读取、资金保障、任务同步分别提供窄合同；保留原执行身份、版本与同事务校验 |
| 供应商消息同时触发目录和订单处理 | 消息接收方依赖处理 Port，应用注入目录与订单实现；两个业务库不通过消息 worker 形成反向依赖 |

每项调整必须执行真实编排的库单元测试，覆盖成功、拒绝或失败停止，以及涉及的幂等和未知结果恢复。不得把同事务步骤改为异步消息以消除编译依赖。

## 5. 执行顺序

各阶段必须保持 workspace 可构建、边界可检查。新 crate 不得依赖旧 `erp-processes`；旧 crate 只保留未迁移实现，不重导出已迁移入口。

| 阶段 | 执行动作 | 完成条件 |
| --- | --- | --- |
| M0：建立基线 | 固定源码版本、工具链、构建入口、profile、features；登记模块归属、现有测试与编译耗时；改造门禁路径解析 | 所有活动 crate 均有层次归属；嵌套目录、缺失路径及非法依赖的门禁正反例通过 |
| M1：目录分层 | 移动现有领域、读模型、适配器、基础和技术 crate；同步 workspace、构建与脚本路径；旧 `erp-processes` 暂留原路径 | 原包名和功能保持；新旧过渡路径均受门禁覆盖；无源码或仓储扫描漏项 |
| M2：公共能力与协议 | 拆开错误转换、审计执行、审批支撑、文件批次和后台运行机制；将具体协议实现迁入 adapters | 公共能力不反向依赖业务流程；协议内部修改不触发无关业务库重编译 |
| M3：外围业务 | 迁移主数据、供应目录和可独立切换的导入；逐一迁移应用调用方 | 对应模块退出旧 crate；依赖未迁移销售链路的询价导入与数据搬运留待 M4 完成 |
| M4：交易主链 | 按第 4 节完成解环，再按依赖拓扑迁移销售、采购、履约、供应商订单、财务、逆向和结算；补齐导入迁移 | 新编排依赖图无环；原事务、授权、错误及恢复合同的库测试通过 |
| M5：清退与交付 | 迁移演示数据、剩余应用装配和测试辅助；删除旧 crate、依赖声明和导出；更新开发导航 | 旧入口清零，完整门禁通过，模块归属核对与编译对比完成 |

每个迁移单元同时修改实现、调用方、测试、审计登记和有效指导文档。保留可单独回退的提交边界；发生回归时回退该迁移单元，不新增兼容 façade。

路径迁移必须覆盖以下消费方：

- `backend/Cargo.toml`、`Cargo.lock`、各 crate manifest、`include_*`、`#[path]`、build.rs 与测试数据引用。
- `domain_boundaries.py`：使用 Cargo metadata 的 `manifest_path` 定位 crate；按层次登记校验依赖；缺失源码或未分类成员直接失败，禁止静默跳过。
- BPM、Rust 体积与审计门禁，以及 `audit-entrypoints.json`、`audit-boundaries.json` 和相关正反例。路径变化不得删除规则或扩大豁免。
- Docker、Jenkins、启动/构建脚本中的复制路径、缓存和构建上下文。
- `backend/AGENTS.md`、`backend/crates/README.md`、各 crate README 及当前有效合同中的入口引用。历史归档保持只读。

## 6. 验收与退出条件

**结构与行为验收**

1. 每个活动 crate 的目录、层次登记和实际依赖一致；新编排库与外部适配器均受门禁检查。
2. 原模块逐项有明确归属；代码、测试和公开调用方完成迁移；活动源码与 Cargo 图中不存在旧 `erp-processes` 入口。
3. 保留权限与数据范围、HTTP 错误映射、字段校验明细、审计身份、冻结快照、命令回执、未知提交分类及原写入顺序。
4. 保留后台任务认领、切片预算、取消、恢复和 drain 语义；供应商请求及回调保持原持久化与应答顺序。
5. 沿用内联库单元测试和现有替身；不得删除行为测试或以源码文本断言替代行为验证。真实数据库、外部服务和浏览器验收不由库测试结果代替。

开发阶段执行受影响 crate 的库测试与 Clippy；公开合同变化追加 workspace check；依赖与路径变化执行边界门禁。每个提交单元按 `backend/AGENTS.md` 执行提交门禁，最终交付在 `backend/` 执行：

```bash
cargo fmt --all -- --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
env -u ERP_TEST_MONGO_URI cargo test --workspace --lib --locked
./scripts/check-domain-boundaries.sh --cutover
./scripts/check-bpm-boundaries.sh
./scripts/check-audit-boundaries.sh
./scripts/check-permissions-drift.sh
git diff --check
```

迁移前后分别记录 `check-rust-size.sh` 结果；既有超限与新增超限分开报告，不得通过修改路径排除生产源码。禁止执行集成测试、`--include-ignored`、验收 examples 或依赖真实 MongoDB/外部服务的命令。

**编译验收**

在独立构建目录中固定工具链、构建入口、profile、features、并发和缓存条件；使用 `--timings` 记录迁移前后结果。测试场景固定业务内容，不混用普通 Cargo 与 Blaze 的耗时。

| 场景 | 验收要求 |
| --- | --- |
| web-api 完整构建 | 记录总耗时、主要编译单元与关键依赖路径；将完整构建和增量构建分别比较 |
| 京东协议内部实现修改 | 销售、采购、财务编排库保持 `Fresh`；应用重新编译与链接耗时单独记录 |
| 商品导入内部实现修改 | 无关的销售、采购、财务编排库保持 `Fresh`；依赖方重编译符合登记的依赖图 |
| 同一业务的定向库测试 | 比较旧 `erp-processes` 与对应新 crate 的测试编译及执行耗时，保留同一业务测试覆盖 |

每项计时重复三次，报告中位数、实际重编译的 crate 及测量配置。上述两个局部修改场景和定向测试编译耗时必须低于基线；完整构建若变慢，必须定位并记录原因，不以目录整理完成代替性能验收。

交付物为迁移代码、更新后的开发约束与导航、逐模块归属核对、门禁结果及编译对比。实施验收记录与本方案分开保存。
