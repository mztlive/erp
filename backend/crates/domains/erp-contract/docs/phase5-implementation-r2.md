FSY-5 第 2 轮返工实现交付，2026-11-16。FSY5-04 已修复创建前置资格顺序，FSY5-03 已补实际模板写步骤的必要行为证据，交独立 Review 复验。FSY5-01、FSY5-02 保持 R1 Review 已认可的修复；本报告替代 R1 对 FSY5-03 已补齐的过宽结论，不抹去上轮缺口。

## 准确版本

| 内容 | 版本 |
| --- | --- |
| 本轮起点 / 上轮完整交付 | `2cf6c23f668d8a421d068e02df424bb41ce0d7c0` |
| 实现分支 | `agent/agent/fsy-5-fdeb3208bf15` |
| 已读取的 R1 Review | `79c68a45badf022ba0d6e1267750924ac5dbb036` |
| 已读取的 R2 分析 | `b2ad279bcee0db24f5e187c97d83a599e56ebe60` |
| FSY5-04 修复及正式回归 | `3825db2ed49973df9422468745b89aa13ea22de2` |
| FSY5-03 实际写步骤及回归 | `ae960f9d01911065cab949a7bef5aef99c033379` |

本轮所有代码均在上述起点之上追加，没有切换分支、回退 R1 修复或重写历史。完整交付另含本报告，其准确 SHA 记录在事项最终评论。未修改其他任务工作树或接收分支，不进行合入、部署或重复派发。

## FSY5-04：完整创建资格先于候选和客户事实

R1 的 require_create 先以 CONTRACT_IMPORT 检查 CREATE，然后候选 Port 预读客户，之后才由 Candidate require 核对独立 customer:DETAIL。独立审查及 R2 分析已重放“只有 CREATE 仍读客户一次、NotFound 抢先”的缺陷；本轮将该边界转为正式回归。

新增内部 `CONTRACT_CREATE_PREPARE`，编码为 `contract:create_prepare`，引用既有公共 CONTRACT 资源常量。它只登记在 identity 的 Function 入口，不进入公共 ENTRIES、角色菜单或前端生成权限。固定 requirements 和 independent 分支直接复用 contract::CREATE 的既有资格定义，没有在业务层维护第二份权限列表，也没有把独立 DETAIL 强行改成与 CREATE 同角色。

现在的真实顺序：

1. ContractAccess::require_create 在原 executor 调用统一 require 的完整准备入口。缺 CREATE 或 DETAIL、只有内部编码、非快照执行器均在候选 Port 之前拒绝。
2. creation_facts 只校验精确 customer_id 引用并读取操作人的主属组织/下一关系边界；根 owner/org 继续与实际新合同一致。移除客户 Facts 预读和无用客户参数，RelatedObjects.objects 保持空。
3. 原 contract::CREATE Candidate require 仍必须执行。identity 使用已绑定的真实 CustomerFacts，正式读取一个精确客户来源，再检查独立当前客户范围、完整性、版本和期限。准备 Operation 不代替 Candidate 授权。
4. require_import 保持原本只要求 CREATE 的任务功能预检。业务创建/归档必须进入 require_create，未扩大识别任务职责。

| 代码位置 | 实现责任 |
| --- | --- |
| `src/authorization.rs::ContractAccess::require_create` | 完整准备 require → 候选 Port → 最终 Candidate require。 |
| `src/repository/authorization/creation.rs` | 仅组织事实与精确客户引用；组织读取晚于完整资格。候选不填充现存来源对象或沿用客户归属。 |
| identity `scope_spec/catalog/functions.rs`、`rules/records.rs` | 固定准备 Operation 及既有完整资格的复用，保留独立角色语义。 |
| `erp-permissions/src/internal.rs` | 内部常量，复用公共资源标识，不增加可分配编码。 |
| server `wiring/authorization/master.rs::contract_access` | 唯一第五阶段工厂的机械参数适配；customer 根 Facts 仍绑定给 identity，没有移除真实来源绑定。 |

正式入口测试覆盖：缺相关权限、只有 CREATE、只有 DETAIL、只有内部准备编码；各自配合客户存在、不存在或设施故障三种事实，均 Forbidden、候选工厂和客户事实零调用。该测试包装实际 creation_facts，因候选未进入，组织读取也未执行。NoTransaction、ReadExecutor 同样在候选/来源前返回 ValidationError。CREATE 与 DETAIL 分属两个角色可以进入候选并完成正式来源读取；一次正式来源读取使用原 executor。

完整资格后的空来源、重复、外资源、错配、过期和设施故障分别保持 identity 的 Forbidden、ValidationError、ConflictError 或原 Internal 分类，不降级成功。候选已不抢先返回预读 NotFound。客户期限由正式来源事实检查；根候选继续保留操作人组织的有效期。R1 的多负责人、独立角色/组织、当前来源及 no_scope 回归均保留。

## FSY5-03：实际生产写步骤与原事务共用

新增私有 TemplateWriter 和一个私有 TemplateStore 存储边界，服务的四个公开事务入口直接调用对应命令步骤。授权、版本、启停、编号组约束、幂等键判断、流水分配及审计顺序全部留在生产命令代码；存储替身只提供原始事实和持久化结果，没有复制这些业务规则或返回授权结论。

SigningCompanyPort、ContractAuditPort 和本分支既有的 AuthorizationFixture 继续复用。没有为每个 Repository 新建公共接口，也未修改或放宽生产 AuthorizationBuilder。prepare、目录读取、下载、只读 replay 及纯实体规则测试保持原职责，不用它们替代首次写入证据。

| 公开入口 → 实际事务步骤 | 权限与执行合同 | 新行为证据 |
| --- | --- | --- |
| create → TemplateWriter::create | TEMPLATES_CREATE Operation → 审计事实准备 → 启用公司 → 编号组 → 模板写入 → 成功审计，均原 executor。 | 无权限及只有 LIST/APPLICATIONS/DOWNLOAD/APPLY 均 Forbidden、零业务效果；允许分支断言返回模板、编号组和审计对象/actor/id。 |
| apply → TemplateWriter::apply → persist_application | TEMPLATES_APPLY Operation → 本人同键查证 → 模板及公司 → 实际流水分配 → 申请写入及成功审计。公开入口仍完成 command_id/purpose 校验，错误后的 finish_apply/replay 合同不变。 | 缺权和其他入口权限均在查证/读取/分配/写入前停止；首次成功按本人身份及原 executor 分配一次；同键同载荷返回原 id/号码且零新分配/写入/审计；异载荷 Conflict。 |
| set_status → TemplateWriter::set_status | TEMPLATES_CREATE Operation → 审计事实准备 → 原模板/版本 → 更新 → 成功审计。 | 错误资格/非快照零读取写入；允许更新 enabled；旧版本 Conflict 且不更新/审计。 |
| configure_counter → TemplateWriter::configure_counter | TEMPLATES_CREATE Operation → 初始流水/审计准备 → 版本 → 只向前校准 → 写入/审计。 | 错误资格/非快照零读取写入；首次校准成功；旧版本及回退失败不产生后续写入。 |

四个实际写步骤都以 NoTransaction、ReadExecutor 验证 ValidationError 与零业务效果。prepare audit 的纯构造不算审计落库；空资源审计参数在业务读取/写入前拒绝。允许路径检查同一 executor 和具体返回/保存内容。公司、模板、申请查证、流水及申请/模板写入的特定故障保持原 Internal，并在失败点停止，后续成功审计零次。停用模板、停用公司、编号组冲突保留原错误。

审计登记只迁移本任务四条真实执行位置：apply_step → write.rs::persist_application，create、set_status、configure_counter → write.rs 对应方法。动作、资源、次数、原 executor 和一次成功事件不变；其他登记、FSY-3 的共享错误恢复合同和其他事项源文件均未覆盖。

## 验证结果及缓存修正

所有测试均在当前精确代码树的 backend/ 下执行，仅库目标。

| 命令 | 最终结果 |
| --- | --- |
| `env -u ERP_TEST_MONGO_URI cargo test -p erp-contract --lib --locked` | 135 passed，0 failed，0 ignored，退出 0。 |
| `env -u ERP_TEST_MONGO_URI cargo test -p erp-identity --lib --locked` | 585 passed，0 failed，0 ignored，退出 0。 |
| `env -u ERP_TEST_MONGO_URI cargo test -p erp-permissions --lib --locked` | 5 passed，0 failed，0 ignored，退出 0。 |
| `cargo clippy -p erp-contract -p erp-identity -p erp-permissions --lib --locked -- -D warnings` | 退出 0。 |
| `cargo fmt --all -- --check` | 通过。 |
| `PYTHONDONTWRITEBYTECODE=1 ./scripts/check-domain-boundaries.sh --cutover` | 领域及审计边界通过，退出 0。 |
| `PYTHONDONTWRITEBYTECODE=1 ./scripts/check-permissions-drift.sh` | 权限及审计目录生成物一致，退出 0；前端生成物无改动。 |
| 统一 rust_size_gate.scan_workspace 对本轮变更 Rust 文件 | 13 个文件，0 问题；未放宽门禁或改动全仓旧超限位置。 |
| `git diff --check`、提交前暂存差异检查 | 通过。 |

本轮先执行创建入口定向测试：5 passed、121 filtered out。模板初版完整库为 134 passed；补充审计参数前置验证后，以表中 135 项最终结果为准。

共享全局构建目录曾出现 E0425：本工作树源码已包含 CONTRACT_CREATE_PREPARE，而编译到的权限依赖不包含它。该次 identity/contract 编译失败不计为通过。随后使用本任务独占的 CARGO_TARGET_DIR：只克隆第三方依赖缓存，排除全部 53 个 workspace 包，重新编译当前工作树包；metadata 确认 target_directory 与 build_directory 都指向该独占目录。表中三个库及 Clippy 的最终结果均来自该隔离目录。附件保留失败日志和最终日志，不以旧缓存产物报告成功。

第三方 proc-macro-error2 v2.0.1 的 future-incompatibility 提示未导致失败，没有扩大修改依赖。R1 全仓体积门禁的 59 个旧超限仅作为既有记录，本轮没有重跑或宣称全仓体积通过。

## 协作、限制与下一步

已读取父事项 FSY-5 线程、FSY-30 的 MD-X01 阻塞、FSY-9 的 R2 分析及 FSY-23 的实际串行合入记录。FSY-9 的 IDN-03、S-01/S-02 归属不变，本轮保留本分支 R1 共享设施，没有叠加另一套同名工厂；FSY-23 已合入的公共资源/同码规范保留，新内部常量引用已有 CONTRACT。父事项交接会提供 FSY5-04 独立提交，供 FSY-30 按准确版本消费，未另行 mention/wakeup 或改动其任务。master-data 未发现直接 creation_facts 调用，require_create 的公开签名保持不变。

本轮不修改数据字段、索引、合同负责人、历史修订或响应 DTO，无数据迁移。prepare Operation 不是可分配菜单权限，不为原本缺 DETAIL 的账号增加业务能力。错误优先级修复为缺完整资格先 Forbidden；缺客户等来源错误在正式取证阶段按 identity 既有分类返回。

未新增、修改或执行集成测试、examples、真实 MongoDB/S3/供应商服务测试，未运行 cargo check、server/workspace 编译或第六阶段切换。替身验证真正生产命令及原 executor 的调用合同，不能证明 MongoDB 回滚、并发唯一性/CAS、审计实际落库或文件交付。真实组织目录读取保持静态追踪；缺资格测试证明不会进入该读取，未假称成功组织查询已在数据库执行。失败后替身内已有写入不代表真实事务提交或回滚结论。

下一位须基于事项最终评论的准确完整 SHA 独立复核 FSY5-03、FSY5-04，并确认已关闭的 FSY5-01、FSY5-02 没有回退。按当前“分析→实现→Review→合入→完成”工作流，本轮只交 in_review；Review 通过进入 started_2，只有 FSY-40 的唯一串行执行者完成本地接收分支合入及组合验证后才置来源任务 done，不代表远端 PR 合并或部署。
