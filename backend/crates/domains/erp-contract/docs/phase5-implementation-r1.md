FSY-5 第五阶段实现交付，第 1 轮，2026-11-16。FSY5-01、FSY5-02 已修复，FSY5-03 已补本轮必要的库单测证据，提交独立 Review；本文不代表审查通过、合并或部署。

## 版本与责任边界

- 本轮起点：`55d240803df94a18f716e60941b941d496734d58`。
- 分支：`agent/agent/fsy-5-fdeb3208bf15`，始终保留该起点，未切换分支、重写历史或改动其他任务的工作树。
- 分析交接：`c794bb84b783a819a39a0f883631a04619178622`，`backend/crates/domains/erp-contract/docs/phase5-analysis-r1.md`；原分析基准 `9a6d5c78e0eb75426339a4c5547e7b428d27292e`。已用 git diff 确认本轮起点与分析基准的合同、identity 源码及第五阶段授权装配目录内容一致。
- 共享能力提交：`12b1cf081bc7e0c42e478d290e40c7ec17f79ce4`，含只读范围摘要与仅测试支持的真实授权设施。
- 实现及回归提交：`c8ab54ed7a2d5ae1c3cdbbcb3462d4366fa805fc`。本报告为后续单独文档提交，完整交付 SHA 见事项最终评论。
- 未收到独立 Review 的既有问题；本轮处理分析交接的三个编号。分析中的完整入口清单和装配追踪仍适用，下面记录实现后核对结果及证据变化。

只处理第五阶段业务授权和本 crate 所需绑定。未切换 HTTP、worker、Application 或旧服务工厂，未编译 server/workspace。保留合同持久化负责人、历史修订及快照，未调整权限编码、集合字段或索引，无数据库迁移。

## 问题处置

| 编号 | 实现结论 | 代码与行为证据 |
| --- | --- | --- |
| FSY5-01 / P1 风险核实 | 原候选确实使用客户归属检查合同目标。本轮按方案的候选合同及本 crate README 的独立负责销售规则修复；目标与来源分别满足条件，拒绝历史参与开启新合同业务。 | `src/repository/authorization/creation.rs:37` 从原 executor 取得客户及操作人主属组织；`:74` 构造操作人 owner/org 和精确 customer 引用，未携带客户的 owner/org 或自行填充来源对象授权事实。identity `service/authorization/contract_creation.rs:14` 分别解析合同创建范围及独立客户详情角色的当前范围。 |
| FSY5-02 / P2 | 恢复已有 DTO 的 `no_scope` 语义；没有正向角色范围且总数为零时返回该原因。 | `src/authorization.rs:167` 只转发 identity 已维护的摘要；`src/service/contract/scope.rs:247` 在视图转换时使用摘要。本人限制令结果为空、有范围但筛选为空、只有历史读取结果等场景不误标。 |
| FSY5-03 / P2 证据缺口 | 增加真正执行 require/query/validate 的必要行为测试。模板三个读取权限分别授权，本人身份由用例传给存储边界；实际查询凭证验证跨阶段、跨事务、到期和动态版本失效。 | `src/service/template/query.rs` 的生产读取函数使用一个私有存储边界，公开入口继续在原事务调用相同函数。`src/authorization.rs:393/:423` 从实际 query 取得凭证；到期测试只收紧凭证期限，未构造允许凭证。 |

创建规则的完整顺序：本域 `require_create` 先检查创建任务的功能资格，再由候选 Port 读取精确客户及操作人主属组织，随后调用统一 `require(contract::CREATE, Candidate)`。identity 在读取现存来源前证明 CREATE 与独立的 customer:detail 功能资格，使用创建角色的 customer:list 配置解释新合同归属，使用客户详情角色的当前范围解释真实客户，全部沿用调用方 executor。来源必须恰好一个、实际存在且身份完整；客户历史参与不参与此创建条件。候选有效期取客户事实及操作人组织关系的最早边界。

这会收紧“只有合同创建权限”“只有历史客户参与”“客户组织满足但拟写入合同组织不满足”等原先可能通过的组合；拥有独立客户详情资格、当前来源范围及合同目标范围的组合允许，两个资格可以来自不同启用角色。没有为迎合旧候选而改变 `Contract::new` 的负责销售规则。

## 逐入口复核

| 入口 | 权限与目标 | 授权、效果和装配结论 | 本轮验证边界 |
| --- | --- | --- | --- |
| create_contract / require_create | CREATE，Candidate；任务预检为 internal::CONTRACT_IMPORT Operation | 原事务写入前执行完整目标与来源授权。`apply_create` 仍是授权后复用的内部业务步骤。 | 候选组装、来源完整性、功能拒绝先于事实、不同角色及组织范围已测；真实数据库写入未执行。 |
| contract_list / list_snapshot / list_scope_version | LIST，完整合同查询 | 原事务同一凭证过滤版本、列表、计数及指标，补齐后 validate，后续独立事务校对跨页版本。新增 no_scope 摘要输出。 | 正反空结果、真实查询凭证及既有分页/指纹测试通过；未执行真实数据库聚合。 |
| contract_detail / require_attachment | DETAIL，完整对象查询 | 保留 query → 完整补齐 → validate，附件仍须属于本次详情修订。 | 既有顺序和拒绝/到期测试通过；真实附件交付未测。 |
| archive_contract_revision / terminate_contract | ARCHIVE_REVISION / TERMINATE，Record | 保留原事务内先 require 后业务版本、状态、修订及写入。 | 既有拒绝顺序、非快照及实体规则测试通过；数据库 CAS/回滚未测。 |
| set_sales_access | internal::CONTRACT_SALES_ACCESS，Record | 保留独立负责销售写入事实及 identity 固定附加资格，原 executor 校验有效账号和更新。 | 本域拒绝先于原对象读取、identity 独立资格测试通过。 |
| ContractAccess::resolve / require / require_with / require_import | 合同固定权限或导入 Operation | 继续委托共享服务；写入调用方使用 require_with 原事务；独立 require 成功不签发跨阶段凭证。 | 外资源拒绝、任务预检与候选不能互替、非快照拒绝已测。 |
| templates | TEMPLATES_LIST，Operation | 原事务固定资格后完整读取页及总数。 | 允许、缺权、其他模板权限不能替代、非快照及存储错误传播已测。 |
| applications | TEMPLATES_APPLICATIONS，Operation | 用例固定 actor.id，存储以申请人过滤页及总数，没有请求参数可以改成他人。 | 真实授权函数传入本人身份，替身返回本人记录和总数；Mongo 筛选执行未测。 |
| download_source | TEMPLATES_DOWNLOAD，Operation | 原事务授权后按申请 id + actor.id 读取，随后模板读取和成功审计；保持原审计入口登记。 | 本人允许、他人/不存在统一 NotFound、拒绝后不加载模板、权限隔离及错误传播已测；审计实际落库与对象存储未测。 |
| prepare / create / set_status | TEMPLATES_CREATE，Operation | 原快照授权后处理主体、编号组或版本；上传后登记仍重新授权，未复用 prepare 结果作为凭证。 | 本轮静态复核及原有实体测试；未将数据库写事务标成已验证。 |
| apply / application_replay | TEMPLATES_APPLY，Operation | 原事务先授权再按本人和幂等键查证、分配；冲突后新事务仍重新检查原资格。 | 既有回读拒绝顺序及幂等纯规则通过；真实并发分配未测。 |
| sample_source / counters / configure_counter | TEMPLATES_CREATE，Operation，公共别名保持不变 | 原事务固定功能资格后读取或写入；不制造无用 Facts/Mapping。 | 静态复核和原有流水规则测试；完整数据库用例未执行。 |
| plan_upload / plan_first_archive / plan_upload_archive / render、原始事实 Port | 纯规划、数据转换或拥有域事实读取 | 不把这些步骤当成独立终端授权入口。创建/下载的真实上层继续承担授权。 | 原有纯规则测试通过；规划不被视为访问或写入授权。 |
| master-data / data-import 创建调用方、ReadModel 合同读取 | 调用本域统一 require_create 或新查询阶段 | 静态核对两条编排在原写事务先 require_create 再 apply_create。data-import 的额外 DETAIL 检查保留，不能绕过新的当前来源检查。ReadModel 的 query/validate 分工保持原分析结果。 | 不运行这些 crate 的测试或接管其工作项。 |

## Facts、Mapping 与共享协作

| 用途 | Facts / Mapping | 绑定及复用 |
| --- | --- | --- |
| 合同 LIST / DETAIL | 本域 ContractFacts / ContractMapping，合同根、独立负责销售、主属组织、明确共享及合法历史参与 | `apps/erp-server/src/wiring/authorization/master.rs` 原绑定不变，两个查询继续复用同一 Mapping 实例。 |
| 合同 CREATE 的拟写入目标 | 本域 CreationFacts，从同一 executor 读取真实客户及操作人的主属组织与下一关系边界，构造无落库版本的 Candidate | 唯一业务装配增加 db 参数，使用原有同一客户 Facts 实例，未创建临时授权服务。 |
| 合同 CREATE 的客户来源 | identity 固定 customer 关系、ExactlyOne 与独立 customer:detail 当前范围；实际来源由已绑定的 CustomerFacts 在原事务读取 | 复用已有 customer 根 Facts 绑定；候选只携带引用，不能提供现存来源的授权事实，不新增无用查询 Mapping。 |
| 合同写动作 | 本域 ContractFacts 的写责任模型 | 保持独立负责销售，历史/共享不提供写权。 |
| 模板、申请、流水功能操作 | Operation | 不需要领域 Facts/Mapping，保留本人申请不变式与原快照内读取/写入。 |

已读取父事项和 FSY-9 的 S-01/S-02 共享归属，以及 FSY-7 新交付 `6e25bc4c532e9bc478e363531c46f1a2df539f69`。FSY-7 的恒拒绝凭证工厂不能证明实际 require/query 的允许场景，本轮未复制该工厂。`12b1cf081bc7e0c42e478d290e40c7ec17f79ce4` 提供本轮已完成并验证的真实执行设施及只读摘要，已在父事项原 FSY-5 线程记录，供 FSY-9 整合复用；未修改其 IDN-01/02 或 FSY-39 种子职责，未重复指派或唤醒。

`AuthorizationFixture` 只替换测试身份设施，按测试明确的角色权限过滤请求资格；本轮一般场景使用固定的测试范围事实。生产的独立角色范围选择另用已有完整 Builder 和 Identity 替身执行 `contract_creation_uses_distinct_qualified_roles_for_target_and_current_customer_scope` 证明，未用统一测试范围代替同角色证明。`expire_access` 仅用于把真实 query 结果缩短为已过期，避免依赖 sleep 或真实时钟竞态。生产 Builder 的完整绑定及冻结校验未变。

## 验证结果

以下命令在 `backend/` 执行，均已收集前台最终退出结果：

| 命令 | 结果 |
| --- | --- |
| `env -u ERP_TEST_MONGO_URI cargo test -p erp-contract --lib --locked` | 124 passed，0 failed，0 ignored，退出 0。 |
| `env -u ERP_TEST_MONGO_URI cargo test -p erp-identity --lib --locked` | 584 passed，0 failed，0 ignored，退出 0。 |
| `cargo clippy -p erp-contract -p erp-identity --lib --locked -- -D warnings` | 退出 0。仅两个库的 lib 目标。 |
| `cargo fmt --all -- --check` | 退出 0。 |
| `PYTHONDONTWRITEBYTECODE=1 ./scripts/check-domain-boundaries.sh --cutover` | 领域及审计边界均通过，退出 0。 |
| `./scripts/check-rust-size.sh` | 全仓退出 1：59 个既有超长方法。使用同一 `scan_workspace` 对本轮 16 个变更 Rust 文件局部检查：0 问题；未放宽门禁、扩大基线或修改范围外方法。 |
| `git diff --check`、提交前 `git diff --cached --check` | 退出 0。 |

首轮编译曾发现新增测试 Mapping 缺少 relation 方法、页总数使用错误整数类型，均已修正。动态依赖测试最初复用成功 validate 后的旧凭证，真实服务按执行阶段拒绝；已改为先断言旧阶段失效，再用新查询凭证单独验证动态修订变化。独立角色范围测试最初错误地把客户详情条款放在 customer:list 配置，改为其真实 customer:detail 配置后通过。这些失败不计为通过证据，以表中最终完整结果为准。

依赖 `proc-macro-error2 v2.0.1` 的 future-incompatibility 提示没有导致命令失败；没有为处理第三方提示扩大本轮范围。门禁产生的临时 Python 字节码已恢复，不纳入交付。

## 未验证行为与接续方式

本轮没有新增、修改或运行集成测试、examples、真实 MongoDB/S3/供应商服务测试，未执行 server/workspace 编译，lib 通过后没有叠加 cargo check。Clippy 限 lib，避免 `--all-targets` 纳入禁止的目标。

替身验证实际生产编排及授权执行合同，不能证明数据库原子提交、并发撤权/CAS、真实主属组织读取、真实聚合/索引计划、审计落库或文件交付。创建 Port 中实际组织仓储读取的同 executor 传递和 server 绑定为静态证据；没有将候选纯组装测试冒充完整数据库路径。模板写事务、应用启动及第六阶段工厂尚未验证。

下一位须读取最终评论给出的完整交付 SHA，包含上述两个代码提交和本报告，核对 FSY5-01～03 的处置及表中证据。待独立 Review 明确通过后才可由审查环节置 done；本轮只交接 in_review，不自动合并或部署。
