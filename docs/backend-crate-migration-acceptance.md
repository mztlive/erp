# 后端 crate 迁移实施验收记录

验收状态：第 10 节代码差异已修复，日志定位及 CLI 依赖收窄按已确认的新结构合同保留。第 11 节全量离线门禁通过，workspace 库测试为 7,394 通过、0 失败、63 ignored；源码仍使用第 10 节清单。第 12 节已执行本地 ERP 数据的真实数据库及 HTTP 验收：数据库回滚、提交重试、版本竞争和销售到采购生效链路通过；终态审批原样重放返回 404，确认为迁移前已有的 ARP-01 缺陷。不得声明真实业务验收全项通过，也不得将该缺陷归因为本次 crate 迁移。第 5 节性能结果仅适用于初始迁移测量快照；既有债务按第 4 节保留登记。

## 1. 验收对象

- 执行方案：[后端 crate 分层与编排拆分迁移方案](backend-crate-migration-plan.md)。原方案保持原字节，本记录登记实施结果。
- 固定基线：`bf53f397dcc70cbe1f0d4a254ccdf0f2f14e0fc8`。
- 编译测量源码快照：`06e2f4ca7a7228e334ddaada30e384b4376b4418`；初始迁移目录的 3,880 个 backend 文件及种子共享脚本逐一核对；该阶段 Rust 源码与测量快照字节一致。4 份 manifest 仅规范化末尾空行，TOML 解析结构与测量快照逐项相同。该快照是测量用 detached 工作树，不表示主分支已提交。当前交付包含第 7、8、10 节改动，不得继续声明当前 Rust 源码与该快照字节一致。
- 模块归属：[backend-crate-migration-modules.json](backend-crate-migration-modules.json)。原 `lib.rs` 的 64 个模块声明全部登记；同名文件与目录分别计入，共 66 个原始路径。
- 分层与依赖：[crate-layers.json](../backend/scripts/crate-layers.json)。活动成员与 normal/build/dev 直接依赖必须同时符合 Cargo metadata 和登记清单。
- 机器可读结果：[backend-crate-migration-validation.json](backend-crate-migration-validation.json)。原始日志、timings HTML、测试映射及债务逐项对照保存在 `/tmp/erp-crate-migration-evidence/`。

## 2. 结构交付

| 验收项 | 交付结果 |
| --- | --- |
| 单一 workspace | 保留 `backend/Cargo.toml`；50 个活动成员，无分组 crate 或嵌套 workspace |
| 目录分层 | 原 31 个 crate 迁入 domains、read-models、adapters、foundation、platform、testing；原包名保留；apps 与 config 原位保留 |
| 领域归属 | 19 个领域保留各自实体、服务、仓储、集合和索引定义 |
| 业务编排 | 13 个独立编排库；具体模块及拆分支持合同按模块台账执行 |
| 协议与运行机制 | 京东 VOP、蛋糕叔叔进入各自 connector；通用轮询、预算、取消和 drain 进入 `background-runtime` |
| 应用装配 | 跨库具体接线归 `web-api::wiring`；CLI 使用自己的 adapter，不依赖 web-api |
| 依赖核对 | 1,021 条直接边：normal 990、dev 29、build 2；必需成员、manifest 路径、层次归属及非法依赖错误均为 0 |
| 旧入口清退 | 删除 `erp-processes` 实现及 workspace/lock 入口；活动 Rust 源码不保留旧导入、路径别名或全量转发层 |
| 构建路径 | Docker、Jenkins 与启动构建入口兼容分层目录；75 处 Rust 字面量包含路径的目标存在；种子共享 JSON 路径已同步并离线导入成功 |
| 外部依赖 | 原 585 个第三方包的版本、来源、checksum 与锁文件条目保持一致，零新增、零删除、零升级 |
| 历史保护 | 原迁移方案、历史权限验收记录及 archive 保持原字节 |

`sales_inquiry_expense` 的实际归属为 `erp-procure-to-pay::procure_to_pay::inquiry_expense`，负责采购费用占用、发生与财务来源核验。销售事实继续归销售领域；财务编排消费窄合同。该归属按实际写入责任登记，不建立销售与采购的相互依赖。

## 3. 行为与测试证据

必须保持以下合同：

1. 跨域写入沿用原事务入口、同一 `Executor`、授权重验、版本检查和数据库写入顺序；外部 I/O 继续位于事务之外。
2. 调用方共享身份与每次独立创建的身份保持原来源。应收嵌套流程、财务责任解析、指定任务所有权校验和普通履约不扩大身份实例共享范围。
3. 审批材料的两次独立授权按原顺序执行，中间保留不可变快照写入；取消抄送、附件转授、绑定和候选重验保留各自原授权来源。
4. 编排错误使用本地类型与明确转换；HTTP 对所有 13 个编排库的状态码、错误消息、校验明细和未知提交分类保持合同。
5. 供应商请求与回调保留执行身份、冻结预算、接收身份、回执及应答顺序；调度回报重试不得重放业务步骤。
6. 审计登记保留原入口政策和审计调用次数；CLI 的共享身份入口以相同政策单独登记，不扩大豁免。

| 测试证据 | 结果与边界 |
| --- | --- |
| 基线 workspace 库测试 | 7,160 通过、0 失败、63 ignored；共 7,223 条实际测试结果 |
| 初始迁移 workspace 库测试 | 7,360 通过、0 失败、63 ignored；50 个结果组，共 7,423 条实际测试结果 |
| 原测试逐项映射 | 7,223 条基线结果均有归属或明确拆分/重命名映射；缺失 0，状态及 `should_panic` 合同变化 0 |
| 商品导入定向测试 | 原 `erp-processes` 与新 `erp-data-import` 使用完全相同的 52 个测试，全部通过 |
| 蛋糕叔叔协议覆盖 | 初始迁移分别验证真实 adapter 的协议与编排 Port 消费合同；当前协议到编排的组合执行见第 8 节 |
| 测试夹具 | 迁移所需图片与离线授权夹具保持原字节；测试不反向依赖演示库或具体协议库 |

上述结果只核销库单元测试与所列静态对照。不得据此声明真实 MongoDB、供应商、对象存储或浏览器验收通过。

## 4. 最终门禁

下表登记初始迁移快照的门禁；当前交付的全量验证见第 11 节。所有命令在 `backend/` 执行。Rust 构建使用独立 `CARGO_TARGET_DIR`，库测试显式移除 `ERP_TEST_MONGO_URI`。

| 命令或检查 | 结果 | 原始证据 |
| --- | --- | --- |
| `cargo fmt --all -- --check` | 通过，退出 0 | `candidate-static/fmt-verified.log` |
| `cargo check --workspace --locked` | 通过，退出 0 | `candidate-check-final.log` |
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | 通过，退出 0；实际增加 JSON 输出和 keep-going，不改变检查范围 | `candidate-clippy-8.log`、对应 JSONL |
| `env -u ERP_TEST_MONGO_URI cargo test --workspace --lib --locked` | 通过，退出 0；实际增加 JSON 输出和 no-fail-fast | `candidate-lib-tests-4.log`、对应 JSONL |
| `./scripts/check-domain-boundaries.sh --cutover` | 通过；由最终组织数据范围检查调用，领域及审计子命令均退出 0 | `candidate-static/org-domain-final-2.log` |
| `./scripts/check-bpm-boundaries.sh` | 通过，退出 0 | `candidate-static/bpm-final-1.log` |
| `./scripts/check-audit-boundaries.sh` | 通过，退出 0 | `candidate-static/audit-final-2.log` |
| `./scripts/check-permissions-drift.sh` | 通过，退出 0；权限和审计动作生成物均无漂移 | `candidate-permissions-final.log` |
| `git diff --check` | 通过，退出 0；新增文件同时通过独立临时索引检查 | `candidate-static/diff-final.log` |
| 分层、门禁工具夹具 | 切换工具 22 项、组织数据范围 33 项、Rust 体积 19 项、审计 9 项通过 | `candidate-static/` 对应自检日志 |

不得将以下既有债务登记为全局通过：

| 附加检查 | 基线 | 交付 | 对照结论 |
| --- | --- | --- | --- |
| `check-rust-size.sh` | 6 个超限文件、93 个超限方法 | 6 个超限文件、93 个超限方法，解析失败 0 | 按迁移路径规范化后，99 项身份和实际行数逐项相同；新增、加重均为 0 |
| `check-org-data-scope.sh` | BLOCKED 3 项 | BLOCKED 3 项 | 规则、诊断、条款及源码归属逐项相同；新增 0 |
| 当前导航缺失链接 | 11 项 | 11 项 | 原目标集合不变，无迁移新增缺失 |

组织数据范围的 3 项为身份解析生产样例结构、资金范围 adapter 的公共解析调用识别、演示规格中的原始 scope 字段。源码和门禁不通过删除规则、扩大排除项或提高阈值消除这些债务。

## 5. 编译性能验收

本节结果仅适用于 `06e2f4ca7a7228e334ddaada30e384b4376b4418`。第 7、8 节改动后未重新计时，不得把以下比例表述为当前源码的实测性能。

测量固定为 `rustc 1.101.0-nightly (41089d07a 2026-10-27)`、`aarch64-apple-darwin`、普通 Cargo、dev profile、default features、jobs=4。完整构建前逐一清理 workspace 包产物，保留已预热第三方依赖；完整构建与无改动增量分别计时。局部探针仅修改函数内部等价表达式，不修改公开签名，完成后恢复原字节。

基线与交付各场景均重复三次，表中使用中位数。预热、恢复构建、清理和测试首次编译不计入正式样本。正式窗口排除手工启动的竞争 Cargo；IDE 原有自动后台活动保持同等条件。

| 场景 | 基线三次（秒） | 交付三次（秒） | 基线中位数 | 交付中位数 | 变化 |
| --- | --- | --- | --- | --- | --- |
| web-api 完整构建 | 109.816 / 108.156 / 105.553 | 93.278 / 91.889 / 93.104 | 108.156s | 93.104s | -13.92% |
| 无改动增量构建 | 0.743 / 0.239 / 0.243 | 0.663 / 0.237 / 0.238 | 0.243s | 0.238s | -2.06% |
| 京东协议内部修改 | 14.682 / 15.370 / 15.660 | 8.249 / 7.851 / 7.833 | 15.370s | 7.851s | -48.92% |
| 商品导入内部修改 | 15.334 / 12.323 / 19.495 | 8.238 / 12.835 / 8.132 | 15.334s | 8.238s | -46.28% |
| 同 52 项商品导入测试：编译及执行 | 10.976 / 6.289 / 11.306 | 2.317 / 2.173 / 2.182 | 10.976s | 2.182s | -80.13% |

京东内部修改每轮仅重编译 `erp-connector-jd-vop` 与 `web-api`；商品导入内部修改每轮仅重编译 `erp-data-import` 与 `web-api`。两类场景的销售、采购、财务编排库及对应领域库在全部样本中均为 `Fresh`。定向测试每轮仅重编译 `erp-data-import`，52 项业务测试集合与基线完全一致且全部通过；纯执行时间每轮约 0.10 秒，表中数据包含编译和启动开销。

完整构建的基线最大单元为 `erp-processes`，中位数 40.43 秒；交付最大单元为 `web-api` 35.74 秒，其次为 `erp-read-models` 19.31 秒、`erp-supply` 16.83 秒。新编排库中最大单元为 `erp-supplier-catalog` 7.83 秒。

Cargo 实际依赖释放记录显示，基线末端链为 `erp-read-models → erp-processes → web-api 库 → 应用二进制`；交付末端链为 `erp-read-models → erp-approval-runtime → erp-fulfillment-execution → erp-supplier-orders → erp-procure-to-pay → erp-finance-posting → erp-order-to-cash → erp-data-import → web-api 库 → 应用二进制`。各轮完整链及 rmeta 释放时点按机器可读结果核对，不相加重叠单元时间，不将排队间隔归因为未测量的原因。

应用库开始编译的时点中位数为 70.20 → 54.93 秒；完整构建的应用库/二进制单元跨度为 37.89 → 38.25 秒，基本持平。局部修改的应用跨度分别为京东 10.14 → 7.20 秒、导入 9.71 → 7.23 秒。整体完整构建降低 13.92%；两个局部修改与同业务测试均低于基线，满足编译退出条件。

Cargo timings 的应用单元覆盖编译与链接，不提供独立 linker 用时；不得将其单元跨度写为纯链接耗时。

## 6. 交付边界与继续开发要求

1. 后续修改必须按 [crate 导航](../backend/crates/README.md) 选择领域、编排、读模型、协议或应用落点；新增成员与跨编排依赖同步修改层次登记和门禁正反例。
2. 不得恢复旧 `erp-processes`、兼容 façade 或跨领域直接依赖，不得以异步消息改变原同事务业务步骤。
3. 离线门禁不得执行集成测试、ignored 测试或真实服务验收例程。9 个已有 examples 仅按迁移授权同步导入路径，all-targets Clippy 仅作编译检查，例程未执行。
4. 真实业务验收按第 11、12 节核对环境、运行版本和已授权操作范围。数据库迁移、服务重启、部署、外部业务操作、真实数据库或浏览器验收必须分别登记执行证据，不由离线门禁代替。
5. 本次正式交付提交包含迁移实现、行为差异修复及最终验收文档，范围限于当前本地分支；不包含推送或部署。测量快照提交只服务独立验收，不代替正式交付提交。


## 7. 身份生命周期与错误合同

当前源码必须保持下列身份生命周期与失败语义；应用组合层负责工厂装配，业务编排仅在原调用点创建身份，不得在 Process 构造时创建一次后长期复用。

| 范围 | 当前执行合同 |
| --- | --- |
| 供应商结算 | 提交复核、决定重放和复核决定各自创建独立授权实例；候选查询与数据范围继续使用调用方实例。结算提交结果未知不得污染应用共享 RBAC。 |
| 供给异常、销售交接、导入确认 | 恢复原异常任务、交接目标资格、确认完成及重放的独立身份；列表、命令预检和原调用方范围不变。同次目标资格与后续对象可读性使用同一局部实例。 |
| 普通履约与客户验收 | 新任务在既有任务检查和责任解析之后创建身份；活动、完成及验收执行在原领域守卫之后创建另一份身份。既有任务幂等重入不创建新任务身份；采购原直接资格校验仍使用调用方身份。 |
| 供应商履约 | 完成、权限核验、交接、候选及调查前后阶段分别创建身份。交接的目标资格、重放和事务内 DataScope 必须使用同一局部身份；普通查询保留调用方范围。 |
| 集成治理 | 无任务直接决定、正式任务动作、完成及回执重放各自创建身份；原数据范围继续使用调用方实例。 |
| 京东授权 | 本地连接绑定与 HTTP 初始化结果独立。缺配置仍返回 `Unavailable`，错误绑定仍返回 `BindingMismatch`；授权引擎按加密配置、通信可用性的原顺序校验，通信不可用不得发起远程授权。 |
| 京东消息组合 | 真实协议解析经实际消息编排执行；原订单事实未核实不得 ACK，发送前必须保存 `AckSending`。删除结果为 false 时保留 `AckUnknown`，同消息重拉不得重复应用或再次删除。 |

所有修复保留原事务入口、同一 `Executor`、授权和写入顺序、错误载荷及原命令恢复身份。不得通过清空共享 RBAC 的未知状态、重试未知提交或扩大权限例外实现隔离。

| 修复后证据 | 结果与边界 |
| --- | --- |
| workspace 库测试 | 7,373 通过、0 失败、63 ignored；50 个结果组，共 7,436 条结果。 |
| 初始迁移测试保留 | 原 7,423 条测试身份全部保留，状态变化 0；新增 13 条内联离线测试。 |
| 身份隔离 | 实际应用工厂验证调用间及与 caller 的实例隔离；身份域既有未知提交状态隔离单测通过。导入能力测试验证失败停止、后续独立身份、原 actor、任务及 Executor 透传。 |
| 京东失败路径 | 真实 adapter 验证 HTTP 初始化失败时本地绑定和参数预检可用、通信拒绝；生产授权用例验证未就绪引擎与本地绑定分离、缺配置及错误绑定分类保持。 |
| 京东跨层组合 | 两项应用层测试执行真实 runtime、registry、协议解析和消息编排，覆盖 10/14/15/18 消息、精确原消息 ID、加密证据、失败零 ACK、确认顺序及未知 ACK 不重放。 |

修复后 `fmt --check`、workspace check、全 targets/features 严格 Clippy、领域与审计边界、BPM 边界均退出 0；权限和审计动作生成物对照无漂移。Rust 体积仍为 6 个超限文件、93 个超限方法，99 项身份及有效行数逐项相同；组织数据范围仍为原 3 项阻断，规则与诊断相同。不得将这两项既有债务表述为全局通过。

测试映射仅核销身份与执行状态，不能替代断言及组合路径审查。初始迁移多出的 200 条结果中，153 条来自旧错误合同在不同 crate 中重复执行；本节新增 13 条为身份隔离、京东故障路径和消息组合回归测试。

本节验证日志及源码哈希清单保存在 `/tmp/erp-crate-behavior-fix/`，机器可读状态为 `backend-crate-migration-validation.json` 的 `behavior_fix_validation`。该清单只对应本节验证时的源码；当前交付须使用第 10 节清单和第 11 节全量验证。构建使用独立 target、jobs=4、`CARGO_INCREMENTAL=0`；库测试移除 `ERP_TEST_MONGO_URI`，不执行 ignored 测试。

验收边界：身份工厂测试与调用链审查不等于真实 MongoDB 未知提交故障注入；京东消息组合测试替换网络、持久化和业务事实刷新，不证明真实数据库回滚或并发。蛋糕叔叔与 W01 的执行合同按第 8 节核销。

## 8. 离线组合与 W01 执行验收合同

蛋糕叔叔跨库组合测试必须放在应用层。测试执行真实 `DangaoshushuRuntime`、连接注册表、回调验签、回执构造、商品协议解析和目录补查；仅通信及回执持久化使用替身。编排库不得为测试反向依赖具体 connector。

| 范围 | 必须保持的行为 | 执行证据入口 |
| --- | --- | --- |
| 回调接收 | 按服务端连接绑定协议；固定签名向量通过；保存加密原文及原订单提示后返回原成功应答。重复接收核验同一回执，保留原密文与指纹。 | `web-api::wiring::dangaoshushu_composition_tests` |
| 回调失败 | 错误签名、绑定失配、回执冲突和写入结果未知均拒绝成功应答；未知写入保留原错误，不自动重放。 | 同上 |
| 商品补查 | 商品、价格、可售状态和城市提示调用真实查询协议；业务值取自补查响应。保留精确价格、规格、库存、父级可售性及显式空地区；缺失报价/状态、身份不符、观察过早和地区映射不全返回原错误。 | 同上 |
| 通信注入 | 注入通信实现仍执行默认构造入口的配置校验，保留关闭及无效配置的分类和消息；校验失败不发送请求。 | `erp-connector-dangaoshushu::runtime::tests` |
| W01 创建 | 先查既有任务，再解析责任，随后创建本次身份并授权，最后构造及保存任务。匹配任务幂等重入不解析新责任、不创建身份、不写入；重复或冻结身份失配立即停止。 | `erp-fulfillment-execution::fulfillment_execution::task::creation::tests` |
| W01 活动与完成 | 原任务读取和实体守卫先于身份创建及授权。读取、责任、操作人或授权失败停止后续步骤；下一次调用创建另一份身份。所有读写沿用调用方同一 `Executor`，保留任务身份、责任、版本和原错误。 | `erp-fulfillment-execution::fulfillment_execution::task::command::tests` |

| 当前交付证据 | 结果 |
| --- | --- |
| 受影响的 5 个库 | 927 通过、0 失败；包括本次新增的 12 项内联离线测试 |
| workspace 库测试 | 7,385 通过、0 失败、63 ignored；50 个结果组，共 7,448 条结果 |
| 测试保留 | 第 7 节的 7,436 条测试身份全部保留；状态及 `should_panic` 合同变化 0；新增 12 条 |
| 构建与静态门禁 | `fmt --check`、workspace check、全 targets/features 严格 Clippy、领域/审计边界及 BPM 边界通过；权限与审计动作生成物无漂移 |
| 既有债务 | 体积仍为 6 个超限文件、93 个超限方法，99 项身份及有效行数相同；组织数据范围仍为原 3 项阻断，规则与诊断相同；新增均为 0 |
| 源码范围 | 相对第 7 节修改 9 个 Rust 文件，新增 4 个 Rust 文件，无删除；Cargo manifest、lock、路由、权限目录、实体和索引未变 |

当前源码哈希清单和原始日志位于 `/tmp/erp-crate-composition-fix/`；机器可读状态使用 `backend-crate-migration-validation.json` 的 `composition_validation` 与 `source_freeze`。清单覆盖 3,882 个 backend 文件，排除 Python 自动字节码缓存；不得以旧清单替代。构建使用独立 target、jobs=4、`CARGO_INCREMENTAL=0`、locked/offline 模式；库测试移除 `ERP_TEST_MONGO_URI`，不执行 ignored 或集成目标。

离线证据只核销上述实际生产步骤。目录补查测试不核销 worker 的真实 MongoDB 租约、游标保存及业务写入；替身不能证明数据库回滚、并发或真实未知提交恢复。不得据此声明其他身份工厂均已有同等执行覆盖，也不得声明供应商网络、对象存储或浏览器验收通过。编译性能仍仅适用第 5 节快照。

## 9. 静态语义审查与已知差异

详细执行合同、触发条件、新旧代码位置与处置要求见 [静态语义验收合同](backend-crate-migration-semantic-review.md)。基线 3,553 个 Rust 文件、当前 3,633 个 Rust 文件均有映射或审查归属；原 `erp-processes` 的 795 个 Rust 文件无未登记去向。覆盖计数不代替调用链与错误路径的语义证据。

| 已确认项 | 影响范围与当前状态 |
| --- | --- |
| RP-01 后台轮询错误日志 | 修复前两处查询直接装箱，日志缺原流程前缀；第 10 节恢复业务转换，保留计数、调度及重试策略。 |
| COM-01 非法动作首错 | 修复前先解析 action 再查账号；第 10 节恢复一次账号读取及校验先于解析。 |
| COM-02 采购构造前提 | 修复前 ports 可缺失并新增运行时 Internal；第 10 节改为必需构造依赖，消除该状态并同步全部生产调用者。 |
| APP-LOG 日志定位字段 | target、file、line 使用新 crate 及真实位置；已明确保留该结构，外部定向规则按部署日志合同核对。 |

workspace 的 585 个第三方解析节点相同，单选 web-api 的 459 个第三方包及 feature 相同。单选 CLI 的包数由 432 减少为 418，7 个共享依赖 feature 收窄；实际编解码与 Once 实现差异已登记，当前 CLI 命令未确认业务输出偏移。不得把锁文件相同扩大为所有目标构建输入相同。

本节原静态证据对应第 8 节快照，登记在 `semantic_review`；修复后的代码、验证和合同状态以第 10 节及 `semantic_correction_validation` 为准。不得将原审查快照误作修复后源码清单。

## 10. 保持新架构的差异修复验收

禁止恢复旧 `erp-processes`、旧路径别名、兼容 façade 或反向业务依赖。身份资格由 identity 拥有领域执行，采购能力由应用层显式装配，轮询业务错误在供应目录编排库转换。

| 项目 | 当前执行合同 |
| --- | --- |
| RP-01 | 两处查询使用同一私有结果转换：持久化错误 → 本业务 Error → 通用 TaskError；保留原正文、错误分类和 typed source。3 项离线测试执行该生产 helper。 |
| COM-01 | 一次账号查询及有效性检查 → 动作解析 → 原角色快照、策略重验和角色检查；typed 权限入口保持。6 项离线测试核验首错、权限顺序及错误/版本传递。 |
| COM-02 | 两个 Process 必需持有 `Arc<PurchaseOrderPorts>`；3 个构造签名及 20 个构造点同步更新。删除缺装配错误和内部检查器构造的虚假 Result，保留真正授权失败。 |
| 日志新合同 | 保留新 target/file/line；45 个事件宏及 23 个 instrument 属性的级别、消息和参数保持。仓内默认 `info` 适用，外部定向规则按 `backend/DEPLOY.md` 核对。 |
| CLI 新合同 | 保留 418 个第三方包及合理 feature 收窄，不补回未使用 RSA/XLSX 依赖；当前和上轮依赖树一致，未发现现有命令触达受影响编解码路径。 |

| 当前验证 | 结果与范围 |
| --- | --- |
| 库测试 | `erp-identity`、`erp-procure-to-pay`、`erp-order-to-cash`、`erp-supplier-catalog`、`web-api`、`cli`：1,220 通过、0 失败、0 ignored；新增 9 项，原 1,211 项身份和状态全部保留 |
| 构建门禁 | `fmt --check`、`cargo check --workspace`、上述 6 个包的全 targets/features 严格 Clippy 通过；使用 locked/offline，移除 `ERP_TEST_MONGO_URI` |
| 架构与既有债务 | 领域/审计边界通过；体积仍为原 99 项，规则身份及有效行数相同；组织范围仍为原 3 项，规则与诊断相同。新增或恶化均为 0 |
| 源码边界 | 修改 33 个 Rust 文件及部署说明，无新增/删除 backend 文件；Cargo manifests/lock、通用 background-runtime、路由、权限目录、实体和索引未改变 |
| 独立复核 | 采购修复 27 个文件、20 个构造点及 12 个 command_access 调用独立核对通过；身份工厂时点、同一 Executor 和写入顺序保持 |

当前 backend 清单覆盖 3,882 个文件，SHA-256 为 `01e2bdcd4745185ee0b30fa32f3807b04c31ad8460800eb8f88b5f835852be55`。精确补丁、日志和清单位于 `/tmp/erp-crate-semantic-fix/`；机器结果以 `source_freeze` 和 `semantic_correction_validation` 为准。体积与组织范围门禁须继续对照原债务，不能将原有阻断登记为整体通过。

本节不承诺旧 Rust 构造签名、旧日志定位值及全部目标构建输入相同。本节修复验证只执行离线门禁；另行授权的真实数据库与定向 ignored 测试结果以第 12 节为准。

## 11. 修复后最终全量验证与真实验收准入

当前 backend 文件必须匹配第 10 节的 3,882 文件清单及 SHA-256。下列证据对应同一修复后版本；权限生成物与核验前及 Git 中的内容一致。

| 完整门禁 | 当前结果 |
| --- | --- |
| `cargo fmt --all -- --check` | 通过 |
| `cargo check --workspace --locked --offline` | 通过 |
| `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings` | 通过 |
| `cargo test --workspace --lib --locked --offline --no-fail-fast` | 7,394 通过、0 失败、63 ignored；50 个结果组，共 7,457 条结果 |
| 测试保留 | 第 8 节 7,448 条测试身份及状态全部保留；仅增加第 10 节的 9 项回归测试 |
| `check-bpm-boundaries.sh` | 通过 |
| `check-domain-boundaries.sh --cutover` | 领域及审计子门禁均通过 |
| `check-permissions-drift.sh` | 官方脚本重新构建成功，权限及审计动作生成物均无漂移 |

构建复用独立 target，jobs=4、`CARGO_INCREMENTAL=0`、`CARGO_NET_OFFLINE=true`，库测试移除 `ERP_TEST_MONGO_URI`；不执行 ignored 或集成目标。Cargo.lock 和全部 backend 文件核验前后均相同。原 99 项体积债务、3 项组织范围阻断沿用同源码版本的第 10 节逐项对照，不重复登记为通过。

真实验收必须使用已明确授权的 ERP 隔离环境，支持测试部署或本机数据副本；必须明确候选镜像/源码版本、验收账号、可写业务数据范围和供应商测试边界。不得把其他项目的 MongoDB、共享开发库清理脚本或供应商生产账号默认为验收资源。用户授权使用本地 ERP 库的执行范围与结果按第 12 节核销。

2026-11-01 15:21 UTC 的只读核对表明：仓库登记的 `test` 命名空间存在，但当前配置集群中没有 ERP Deployment、ERP Pod、ERP Service 或两个测试域名对应的 Ingress；`erp-api-test.fushangyunfu.com/health` 与 `erp-test.fushangyunfu.com/` 的 HTTPS 握手失败。该结果仅证明当前目标未具备可用 ERP 验收入口，不作为业务缺陷或迁移回归。实际测试地址或集群归属确认后，须先核对运行版本，再执行真实链路。

全量门禁、测试身份对照、源码核验及环境只读证据位于 `/tmp/erp-crate-final-validation/`，机器状态使用 `full_workspace_validation` 与 `runtime_acceptance_readiness`。不得将只读环境检查计作真实业务验收通过。

## 12. 本地 ERP 数据真实验收合同

本节使用第 10 节相同的 3,882 文件源码清单，生产源码无新增修改。API 二进制 SHA-256 为 `4850c74155edbf3373745eb9fb99dd29d9a0afb810798c885b184c64191626b3`；原始结果位于 `/tmp/erp-crate-local-acceptance/`，持久摘要使用 `local_runtime_acceptance`。本节不能替代第 11 节全量离线门禁，也不能把其中 63 个 ignored 统一改记为通过。

| 环境约束 | 实际执行范围 |
| --- | --- |
| 数据来源 | 已授权的本机 `127.0.0.1:27017`、`rs0`、逻辑库 `erp`；277 个集合，301,098 条记录。备份包含数据和索引元数据，压缩大小 28,973,642 字节。 |
| 原实例限制 | Mongo 数据盘可用空间约 255 MiB，低于索引创建要求的 500 MiB。原实例上的导入回滚用例通过，下一项在创建测试索引时因 `OutOfDiskSpace` 中止；不得将该环境错误登记为迁移回归。 |
| 业务验收实例 | 备份恢复到外置盘上的独占本机副本集 `erp_crate_acceptance`、端口 `29017`；301,098 条恢复成功，0 条失败。候选 API 使用端口 `19001`，保留原加密密钥及存量角色、审批和责任规则。 |
| 外部边界 | 供应商、OCR、AI、邮件及完整 Worker 均关闭。测试附件通过真实上传接口建立文件资产，字节保存到本机 S3 HTTP 替身；这不证明真实对象存储、签名校验或供应商网络可用。 |
| 故障注入 | 另建独占副本集 `erp_crate_faults`、端口 `29018`，只对 `appName=erp-crate-fault-probe` 注入两次提交错误；不对原实例或业务验收实例设置 failpoint。机制采用 [MongoDB failCommand 合同](https://github.com/mongodb/mongo/wiki/The-failCommand-fail-point)。 |

| 执行合同 | 结果与证据 |
| --- | --- |
| 定向真实 Mongo 库测试 | 5/5 通过：导入后行 CAS 失败整体回滚；采购读模型沿用原 Executor 读到未提交写入；应收唯一键冲突回滚；应付批量失败回滚；并发结算不超额。最后一项验证单文档条件更新，不冒充多事务竞争。见 `mongo-test-results.json`。 |
| 真实生产事务入口 | 3/3 探针通过：正常提交；两次真实驱动 `UnknownTransactionCommitResult` 后恢复；两个真实事务竞争同一期望版本。故障场景回调 1 次、insert 1 次、commit 3 次，保持同一 `lsid`/`txnNumber`；竞争场景各回调 1 次，仅一方成功，最终 version=1。见 `tx-harness-result.json`。 |
| HTTP 与身份 | 健康、匿名 401、坏 token 401、错误密码 401、销售越权 403、缺失单据 404 均符合合同；13 个业务查询入口通过。新建专用账号停用后，原 JWT 和重新登录均被拒绝。见 `http-smoke-results.json`、`http-query-results.json` 与 identity 执行报告。 |
| 销售到采购生效 | 新建客户和服务销售单，销售审批后生效；采购创建并提交、内部审批后生效，销售金额 200.00、采购金额 120.00、应付未结 120.00。真实数据库中销售正式版本 1 个、采购正式版本 1 个、服务履约草稿 1 个，供应商 API 执行记录 0 个。草稿采购 submission 已 superseded，正式 submission 为 approved，不将两种快照误计为重复业务事实。 |
| 采购创建重放 | 审批前与生效后均以原请求、原幂等键重放，返回原采购 ID、`replayed=true`；快照、单一服务草稿和审批历史保持。独立收尾的 13 项断言通过，同时保留审批重放失败结论。 |
| 终态审批重放 | **未通过，ARP-01**：同账号、同任务、同版本、同幂等键和相同请求字节，首次返回 `200/APPLIED/APPROVED`，原样重放返回 `404/NOT_FOUND`。不得修改请求身份或将 404 改为预期结果来核销该项。 |

ARP-01 的收据使用 V3 编码 scope；[终态恢复校验](../backend/crates/domains/erp-workflow/src/service/approval/execution/runtime_service/decision_apply.rs) 第 410 行却要求 `receipt.scope_id` 等于原始 execution ID，合法收据因此被拒绝。实际数据库确认：任务已完成、execution 已批准、唯一收据指向原审批实例，scope 为 `v3:<hash>`。写入、编码、恢复和权限相关实现与基线 `bf53f397dcc70cbe1f0d4a254ccdf0f2f14e0fc8` 相同，主文件 Git blob 为 `ee7f79d35857129aaaa974d667814487c31644da`。该问题登记为**既有审批幂等缺陷，非迁移新增差异**；修复必须作为独立业务变更验证，不恢复旧 crate、旧 façade 或绕过当前权限校验。

本节仅覆盖表中执行合同。以下项目不得登记为通过：持续未知提交达到生产 120 秒上限的终止及业务回执恢复、完整 Worker 生命周期和供应商 Mongo 租约/游标、真实供应商协议、真实 S3、外部日志采集、部署及浏览器全流程。现存空体 Mongo ignored 测试和使用内存替身的供应商测试不计入真实数据库验收。

清理要求已核销：原实例初次尝试的 2 个临时库、隔离实例的 5 个库测试随机库和 1 个故障探针库均已删除；候选 API、S3 替身及两个临时 Mongo 进程已停止，4 个专用端口关闭，业务副本和故障实例的数据目录已删除。原实例启动准备期间新增的 4 个空索引集合，经核验不在备份集合清单且记录数为 0 后定向清理；原 ERP 的 277 个集合身份及记录数与验收前相同。该核对不扩大为全库字节一致性证明。原库未清空，既有服务未重启，生产环境未部署；原始私有备份和脱敏证据保留，临时凭据文件已删除。清理证据使用 `cleanup-result.json`。
