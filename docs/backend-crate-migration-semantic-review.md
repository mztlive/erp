# 后端 crate 迁移静态语义验收合同

验收范围：保持当前生产业务链路的权限、错误、事务和恢复语义。RP-01、COM-01 的原语义已恢复；COM-02 改为构造时必需的能力合同。日志使用新 crate 与真实源码位置，CLI 保留合理依赖收窄，这两项已明确接受。不得将该结论表述为所有 Rust API、日志定位字段或构建输入逐项相同。

## 1. 适用版本与范围

- 基线：`bf53f397dcc70cbe1f0d4a254ccdf0f2f14e0fc8`。
- 候选：本次正式交付提交；精确文件清单与 SHA-256 使用 `backend-crate-migration-validation.json` 的 `source_freeze`，修复验证使用 `semantic_correction_validation`。
- 覆盖：基线 3,553 个 Rust 文件、候选 3,633 个 Rust 文件均有迁移映射、审查归属或明确删除说明；包括原 `erp-processes` 的全部 795 个 Rust 文件。映射包含测试与构建文件，不能将其数量作为生产行为证明。
- 判定依据：实际生产调用者、具体 Port 实现、身份来源与生命周期、首错顺序、错误载荷、同一 Executor、写入与恢复顺序、序列化及依赖配置。
- 审查分工：8 个责任组并行核对，主审汇总基础合同、保留领域及跨组调用。具体源码哈希、差异和行号见机器台账 `semantic_review`。
- 架构约束：禁止恢复旧 `erp-processes`、旧路径别名、转发 façade 或反向业务依赖。身份资格由 identity 执行，业务错误转换由所属编排库执行，跨域能力由应用装配。

## 2. 已核销的生产合同

| 范围 | 当前源码判定 |
| --- | --- |
| 审批与工作流 | 材料授权 A、不可变快照、授权 B 和开始抄送保持原顺序；取消抄送保留独立身份。审批恢复保留原命令身份、8 次只读查证、首次未知错误及分派动作。 |
| 履约、库存与逆向 | W01/W25 的领域守卫、冻结责任、版本、身份创建和写入顺序保持；供应履约移交、退款财务责任和付款冲正沿用原 Executor。删除的付款任务同步调用在基线为无副作用操作。 |
| 销售与采购 | 当前所有生产构造点均已提供实际依赖；销售交接、采购变更生效仍在原资格检查位置创建独立身份。销售进度、采购费用和退货财务 helper 的迁出保持原提供者与错误传播。权限顺序和构造合同见 COM-01、COM-02。 |
| 财务与供应商结算 | 财务责任逐次创建身份；应收嵌套流程、付款恢复、结算提交/决定/重放保持原隔离范围。预算解析、金额、状态、审计与未知提交恢复语义保持。 |
| 供应商订单与目录 | 冻结执行身份和预算、回调保存后 ACK、消息去重、租约/版本门禁、业务应用与 cursor/checkpoint 顺序保持；新目录服务和治理工厂已追踪到实际消费方法。 |
| 导入、主档、附件与演示 | 导入完成和重放使用独立身份，列表使用调用方身份；供应商删除仍显式创建身份。附件持久化与业务写入共享原 Executor，35 个非 Rust 输入资产字节相同。 |
| 协议与后台运行 | JD/DGSS 的实际协议、绑定、发送预算、取消、排空及调度路径已核。DGSS 旧 `reception.rs` 没有基线生产调用，当前接收链路沿用原通用入口；其删除不作为回调行为变化。轮询错误转换按 RP-01 保持。 |
| HTTP、CLI 与错误 | 42 个路由文件、42 个权限目录文件、12 个配置文件字节相同；13 个编排错误类型的实际可达转换、字段错误、事务源错误和未知结果分类保持。DuplicateKey 提示表收窄未发现实际写入路径遗漏。 |
| 保留领域与基础机制 | 496 个原仓储文件字节相同；526 个原实体文件中 525 个相同，另 1 个仅增加模块导出。83 个非应用索引文件相同，两个应用索引入口保持原调用顺序。审计泛化、组织资格和只读恢复 helper 保留原分支及错误载荷。 |

同一方法体的字节或 token 相同只作为定位证据。涉及上下文变化的文件必须同时核对实际依赖装配、错误转换与调用顺序。

## 3. 三项代码修复合同

### RP-01：后台轮询保留业务错误转换

目录采纳批次和目录连接分页查询的持久化结果必须经过 `erp-supplier-catalog::background::polling_result`，先执行本 crate 的 `Error::from`，再装箱交给通用 runner。

- 数据库错误保留 `数据库错误：database error: <原因>` 的原正文及持久化载荷。
- 事务冲突保留原业务分类、消息和类型化 source；成功值原样返回。
- runner 的当前任务计数、后续调度与错误消费方式保持；不得为日志恢复给 `background-runtime` 增加业务依赖。
- 3 个内联离线测试执行两个生产调用点共用的转换 helper，覆盖成功值、数据库错误和事务冲突；不证明真实 Mongo 查询或完整调度。

入口：[轮询转换](../backend/crates/orchestration/erp-supplier-catalog/src/background/mod.rs)、[目录采纳](../backend/crates/orchestration/erp-supplier-catalog/src/background/supplier_catalog_adoption_adapter.rs)、[目录同步](../backend/crates/orchestration/erp-supplier-catalog/src/background/supplier_catalog_adapter.rs)。

### COM-01：账号校验先于动作解析

销售询价和采购资格检查通过 identity 的 `ensure_actor_action_permissions` 执行：

1. 使用调用方同一 Executor 查询一次账号，原样传播读取失败。
2. 校验有效后台账号及身份类型；失败返回原 `Forbidden("账号已失效")`。
3. 按原顺序解析资源和动作，保留权限规范化及重复项。
4. 获取角色权限快照，事务内重验策略版本，读取有效角色，返回原策略版本或错误。

不得在编排层提前解析动作，也不得以重复读取账号实现顺序一致。原 typed `ensure_actor_permissions` 的签名、权限切片和后半段授权顺序保持。原分派流程已有的目标预读与后续授权重验属于不同业务阶段，不合并。

6 个内联离线测试执行实际共用 helper，覆盖账号读取失败优先、账号缺失/失效/类型不符、活跃账号非法动作、合法权限顺序、后续错误和策略版本传递；不以替身证明数据库或 RBAC 持久化事实。

入口：[身份权限](../backend/crates/domains/erp-identity/src/service/account_permissions.rs)、[销售询价](../backend/crates/orchestration/erp-order-to-cash/src/sales_inquiry/access.rs)、[采购资格](../backend/crates/orchestration/erp-procure-to-pay/src/procure_to_pay/responsibility/eligibility.rs)。

### COM-02：采购跨域能力为必需构造依赖

`PurchaseOrderProcess::new/with_rbac` 与 `PurchaseOrderFormalizationProcess::new` 必须显式接收 `Arc<PurchaseOrderPorts>`。结构直接保存该 Arc，不提供缺失 ports 的状态、补装 builder 或运行时缺装配错误。

- 20 个构造点使用原提供者：16 个 handler、1 个审批分派、1 个正式化工厂及 2 个内部正式化调用。
- 内部正式化克隆父流程的同一 ports Arc；能力容器构造不执行 I/O、不创建独立 RBAC、不提前授权。
- `command_access` 直接返回检查器；其 12 个调用点保留后续 `current/revalidate` 的实际错误、顺序及 Executor。
- Rust 调用签名采用本节的新构造合同；不为保留原先不传 ports 的构造方式恢复隐式跨域装配。原 RBAC 与审批对象读取合同保持。
- 必需依赖由类型系统和全工作区编译核验，不增加源码字符串断言或同义构造测试。

入口：[采购流程](../backend/crates/orchestration/erp-procure-to-pay/src/procure_to_pay/mod.rs)、[正式化流程](../backend/crates/orchestration/erp-procure-to-pay/src/procure_to_pay/formalization_root.rs)、[应用工厂](../backend/apps/web-api/src/wiring/purchase.rs)。

## 4. 明确接受的新结构合同

### 日志定位

日志 target 使用新 crate/module；file 和 line 定位真实源码，不要求跨版本固定，不伪造旧命名空间或路径。原 45 个生产事件宏和 23 个 instrument 属性的级别、消息及结构化参数保持；动态错误正文按 RP-01 验证。HTTP 显式 `target=http` 和追踪字段保持。

仓内 Docker、Compose、Helm 的默认 `RUST_LOG=info` 继续有效，没有旧 `erp_processes` 定向过滤。具体新前缀和过滤示例按 [部署日志合同](../backend/DEPLOY.md#日志定位与过滤合同) 执行。Helm 当前模板固定 `info`，不得宣称已提供日志级别 values 开关；外部采集和告警配置另行核对。

### CLI 构建输入

| 检查口径 | 当前合同 |
| --- | --- |
| workspace 解析图 | 585 个第三方节点的版本、source、解析依赖和 resolved features 与迁移基线相同 |
| 单选 web-api 默认依赖树 | 459 个第三方包及 feature 集合保持 |
| 单选 CLI 默认依赖树 | 保留 418 个包，相对基线移除 14 个未使用包，7 个共享依赖 features 收窄；排除开发依赖后的 feature 集合同样成立 |

CLI 不增加未使用的 RSA/XLSX 依赖，也不借旧总库强制统一 features。flate2 使用当前解析出的 miniz_oxide；现有命令不触达受影响的 PDF/ZIP/PNG 编解码。lazy_static 使用标准库 Once，当前唯一消费链为 sharded-slab → tracing-subscriber → CLI；原 spin 实现来自已删除 RSA 依赖的附带 feature 统一。

新 CLI 命令若开始使用这些能力，必须在实际拥有者显式选择必要 features，并验证输出。当前证据限定本机目标及明确的默认包选择，不宣称跨平台输入、压缩字节或初始化调度完全相同。

## 5. 验证与交付边界

1. 当前代码已通过格式检查、全工作区编译、全 targets/features 严格 Clippy 和全量库测试：7,394 通过、0 失败、63 ignored。最终结果以 `full_workspace_validation` 为准；`semantic_correction_validation` 保留定向修复验证，不把历史快照测试替代当前结果。
2. 领域和审计边界必须通过；体积及组织范围门禁分别对照原 99 项、3 项债务，不得以既有失败掩盖新增违规，也不得声明既有债务门禁全通过。
3. Cargo manifests/lock、路由、权限目录、实体和索引不因本修复改变；不得新增旧 crate、路径别名或兼容 façade。
4. 精确源码清单变更后必须重新验证受影响合同；初始编译性能测量不得套用为修复后实测结果。
5. Mongo 实际回滚、并发、未知提交、部署索引、存量数据、供应商真实协议及外部日志配置须独立登记。已执行的本地数据库与 HTTP 范围使用验收记录第 12 节；未执行的事实不得登记为通过。

静态审查原始证据位于 `/tmp/erp-crate-semantic-audit/`；修复补丁与定向验证位于 `/tmp/erp-crate-semantic-fix/`；当前全量门禁和环境只读检查位于 `/tmp/erp-crate-final-validation/`。持久摘要和证据哈希登记在 [迁移验收数据](backend-crate-migration-validation.json)。真实业务验收准入按 [验收记录第 11 节](backend-crate-migration-acceptance.md#11-修复后最终全量验证与真实验收准入) 执行，本地执行证据与边界按第 12 节核销。

## 6. 真实验收发现的既有缺陷

静态语义核销只证明迁移前后的执行合同保持，不保证基线实现不存在业务缺陷。

| 编号 | 实际行为与归因 | 退出条件 |
| --- | --- | --- |
| ARP-01 | 终态审批原样重放返回 404。V3 收据保存编码 scope，恢复前置校验却与原始 execution ID 比较，阻断合法收据恢复。真实 HTTP 请求字节、Mongo 收据及任务状态已核验；相关写入、编码、恢复、权限实现与迁移基线相同，非本次迁移新增。 | 独立修复须使用原命令身份校验 V3/legacy 收据，保留 actor、实例、执行节点及载荷冲突保护；补终态原样重放、异载荷拒绝和越权失败覆盖，并在真实 Mongo/HTTP 上复验。当前不得将审批幂等验收登记为通过。 |

本轮不修改该既有业务行为，也不恢复旧架构。实际请求、收据、源码对照和剩余边界登记在 `local_runtime_acceptance` 及 `/tmp/erp-crate-local-acceptance/`。
