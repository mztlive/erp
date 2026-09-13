# 组织与数据范围 workflow 执行合同

状态：workflow 本地实施与验证控制；不授予正式业务开放资格。

## 1. 执行范围

1. 执行脚本为 `complete-org-data-scope.rhai`；权威需求为 `docs/organization-data-scope-contract.md`、S1/S2/S3 分阶段合同及关联业务合同。
2. 默认选择 S2/S3 核心接入、基础设施、初始化代码、跨资源审计和上线准入清单。销售责任交接须以 `only=s3-handover` 独立运行；S4 须显式设置 `include_s4=true`。
3. 任务目录必须登记全部 S2-01—S2-11、S3-01—S3-07、M01—M18 和 A01—A32。登记覆盖不等于实施完成或业务验收通过。
4. `ORG-BASE` 对应主合同第 4、5 章基础能力；`ATTRIBUTION-CORRECTION` 对应第 6.2.2 条；`RELEASE-READINESS` 对应第 10、12.1 章只读准入核查。三者属于补充检查单位，不增加业务功能组计数。
5. `S4-M13`—`S4-M18` 是 S4 的逐功能子任务。S4 仅在全部适用子任务及真实验收完成后核销。不得使用 `S2-03-rest`、`S2-11-part` 等未定义剩余量别名替代原合同 ID。
6. S1 既有实现须在客户、合同、销售、采购接入及 A01/A07/A10/A15/A21 中回归核对。S1 尚未执行的业务验收继续保留。
7. 不重建已移除的商城或 ERP 二期业务。适用入口缺失、来源映射规则不明或责任交接政策未确定时登记阻塞与正式开放限制，不推测补齐。

## 2. 参数与执行方式

| 参数 | 默认值 | 强制行为 |
| --- | --- | --- |
| `apply` | `true` | `false` 仅盘点、计划和写临时报告；不得调用实施、修复、合入、文档写入或质量执行 agent |
| `only` | 不指定 | 指定一个下表中的精确组名；未选依赖只读核对，不自动扩大实施范围 |
| `include_s4` | `false` | 即使 `only` 指向 S4 组，也必须显式设为 `true` |
| `max_repairs` | `1` | 每批次修复和全局门禁修复分别最多 0—2 轮；修复后重新审查和检查 |
| `max_parallel` | `3` | 每个并发面板最多 1—8 个 agent；设为 1 可顺序执行同一调度流程 |
| `agent_budget` | 宿主默认 128 | Grok 子 agent 调用上限；全范围含 S4 可显式设为 256；预算不足不得判通过 |
| `effort` | 宿主设置 | 由 Grok 宿主解释；不改变任务或验收边界 |

未知参数、参数类型错误、无效组名、缺少 S4 选择条件必须在启动 agent 前拒绝。并发数限制与 agent_budget 调用总数限制分别生效。

```text
/workflow complete-org-data-scope {"apply":false}
/workflow complete-org-data-scope {"only":"scope-foundation"}
/workflow complete-org-data-scope {"max_parallel":3}
/workflow complete-org-data-scope {"only":"s2-sales-purchase"}
/workflow complete-org-data-scope {"only":"s3-handover"}
/workflow complete-org-data-scope --agent-budget 256 {"include_s4":true}
```

`infra-handover` 已拆分为基础设施和独立交接批次，不作为兼容组名继续执行。

## 3. 任务与需求覆盖矩阵

表中“默认”表示默认执行集合；“独立”表示销售责任交接专批；“显式”表示 S4 扩展；“只读”表示准入清单。

| 执行组 | 合同任务 | 功能组 | 验收行 | 选择方式 |
| --- | --- | --- | --- | --- |
| `scope-foundation` | ORG-BASE | 公共约束 | A02、A03、A04、A05、A06、A09、A18、A19、A27、A28、A32 | 默认 |
| `s2-customer-contract` | S2-01、S2-02 | M01、M02 | A01、A07、A08、A15、A16、A21、A27、A28 | 默认 |
| `s2-sales-purchase` | S2-03、S2-04、ATTRIBUTION-CORRECTION | M04、M05 | A01、A07、A09、A10、A12、A14、A15、A16、A17、A21、A24、A27、A28、A30 | 默认 |
| `org-pages` | S2-08 | 公共约束 | A02、A03、A06、A09、A15、A16、A19、A32 | 默认 |
| `entry-mapping` | S2-09 | 公共约束 | A17、A30、A31 | 默认 |
| `s3-selection` | S3-02 | M03 | A01、A07、A15、A16、A17、A28 | 默认 |
| `s3-funds` | S3-03 | M07、M08、M09 | A11、A12、A13、A15、A16、A29 | 默认 |
| `s3-fulfill-workbench` | S2-10、S3-04 | M06、M12 | A13、A14、A21、A22、A23、A24、A25 | 默认 |
| `s3-quality-profit` | S3-05、S3-06 | M10、M11 | A09、A11、A12、A15、A16、A20、A29、A30 | 默认 |
| `initialization` | S2-11 | 公共约束 | A18、A19、A30、A31、A32 | 默认 |
| `cross-resource-audit` | S2-05、S2-06、S2-07、S3-01 | 公共约束 | A01、A04、A05、A06、A07、A15、A16、A21、A27、A28、A32 | 默认 |
| `s3-handover` | S3-07 | 公共约束 | A14、A19、A24、A25、A26、A30 | 独立 |
| `s4-m13` | S4-M13 | M13 | A01、A07、A13、A15、A16、A17、A19、A31、A32 | 显式 |
| `s4-m14` | S4-M14 | M14 | A01、A07、A13、A15、A16、A17、A19、A31、A32 | 显式 |
| `s4-m15` | S4-M15 | M15 | A01、A07、A13、A15、A16、A17、A19、A31、A32 | 显式 |
| `s4-m16` | S4-M16 | M16 | A01、A07、A13、A15、A16、A17、A19、A31、A32 | 显式 |
| `s4-m17` | S4-M17 | M17 | A01、A07、A13、A15、A16、A17、A19、A31、A32 | 显式 |
| `s4-m18` | S4-M18 | M18 | A01、A07、A13、A15、A16、A17、A19、A31、A32 | 显式 |
| `release-readiness` | RELEASE-READINESS | 公共约束 | A18、A19、A26、A30、A31 | 只读 |

每个功能组必须分别登记适用的列表、详情、汇总、趋势、下钻、候选、导出、打印、附件、写命令和关联任务；不适用项必须给出业务依据。变更、退货、退款、冲正沿来源业务接入，不另建平行管理入口。

## 4. 条款归属

| 主合同范围 | 必须覆盖的规则 | 主责执行组 |
| --- | --- | --- |
| 第 3 章 | 18 功能组；公共字典、库存余额不虚构负责人；扩展资源先建立责任事实 | 各 M 组、cross-resource-audit |
| 第 4 章 | 组织树、有效期、单一主属组织、跨团队管理、下级开关、影响预览、乐观锁与审计 | scope-foundation、org-pages |
| 第 5 章 | v2 类型与资源动作注册、同角色完整授权、动态范围、多维求交、个人上限、合法历史参与、错误语义 | scope-foundation、所有消费者、cross-resource-audit |
| 第 6.1 节 | 当前责任来源、单据业务组织、派生关系、创建/首次生效必填、客户交接展示、显式变更 | 客户合同、销售采购、选品、entry-mapping |
| 第 6.2 节 | 原子归属快照、历史祖先路径、不可漂移、版本化纠正、历史参与撤权、M10 两种口径 | 销售采购、entry-mapping、s3-quality-profit |
| 第 6.3 节 | 查看/执行/改派分离、任务专用候选、采购全任务原子级联、审批受阻与恢复、销售验收联动 | s3-fulfill-workbench、s3-handover |
| 第 7 章 | 正反向分配、不重复金额、余额独立授权、部分字段空值、打印附件限制、未知分区、主动筛选与授权范围区分 | s3-funds、s3-quality-profit、所有资金消费者 |
| 第 8 章 | ID 归一化、每字段 100 上限、OR/AND、参数拒绝、三类候选、URL/Query/已应用条件、摘要元信息、无权/空/失败 | 各资源前后端、org-pages、cross-resource-audit |
| 第 9 章 | 分层与 Port、同一查询快照、详情重验、导出撤权、缓存有效期、事务内重验、批量展开、索引、并发审计 | scope-foundation、全部消费者、initialization、cross-resource-audit |
| 第 10 章 | 首次初始化清单、模型/索引一致性、责任来源清单、第一单原子归属、首发接口与任务、账号及业务验收记录 | initialization、entry-mapping、release-readiness |
| 第 11 章 | 分批交付、状态层级、S2/S3/S4 退出、交接独立记录、首发范围独立登记 | cross-resource-audit、Docs、release-readiness |
| 第 12/12.1 节 | A01—A32 逐行主责及证据；静态/单元/HTTP浏览器/数据库/业务验收区分 | 上表对应组、最终 Verify、release-readiness |

Inventory 必须逐条核对当前完整合同。发现未登记需求时填入 `unmapped_requirements` 并终止写入，先修订任务目录后重新执行。

## 5. 并发与串行执行规则

### 5.1 阶段安排

| 阶段 | 执行方式 | 等待条件与依据 |
| --- | --- | --- |
| Inventory | 单个只读 agent | 形成统一任务与证据清单，后续计划使用同一基线 |
| Plan | 对当前依赖已就绪的组并发 | 不与写入重叠；只读模式可对全部已选组并发计划 |
| Implement | 文件兼容的组并发 | 所有已选前置组通过批次审查；域写入无冲突且关键读取文件保持稳定 |
| Merge | 每批次一个 agent | 等待该批全部域写入结束；统一处理共享注册、缓存、DTO 胶接和 OpenAPI |
| LocalChecks | 每批次一个 agent，命令依次执行 | 等待所有写入与合入结束，避免编译读到其他 agent 的半成品及权限生成物竞争 |
| Review | 对该批所有组并发只读审查 | 等待 LocalChecks 完成；审查期间无写入、构建、测试或生成命令 |
| 批次 Repair | 域文件按该批原归属并发修复 | 共享修复仍交单一 Merge；修复后重跑 LocalChecks 和全部批次审查 |
| 全局 Review | 所有已选组分片并发 | 所有批次结束，重新检查后续共享修改对前序组的影响 |
| Quality | 单个 agent 顺序执行 11 项门禁 | 不与写入、审查并发；后端 build.rs 会生成前端 permissions.generated.ts |
| 全局 Repair | 单个 agent | 全局错误可能跨批次且多个批次允许先后修改同一文件；修复后重跑全局 Review 和全部门禁 |
| Verify | 所有已选组分片并发 | 全部写入和门禁结束后，对同一最终代码状态独立核验 |
| Docs → 文档核验 | 先写后验，串行 | 更新统一交付状态后再核对；不得与代码验证同时修改文档 |

面板内使用 Grok `parallel()`；面板完成前不得启动下一阶段。超过 `max_parallel` 的只读任务分片执行，缺失/失败输出按原输入位置保留为空，不删除结果槽位或错配任务。

### 5.2 业务依赖与可并发批次

下表为全部默认核心任务待实施、文件清单不冲突且 `max_parallel=3` 时的参考安排。实际调度由前置完成状态和精确文件清单决定；不得机械忽略文件冲突。

| 顺序 | 可同批执行的组 | 必须先完成 |
| --- | --- | --- |
| 1 | scope-foundation | 无；先建立稳定授权接口 |
| 2 | s2-customer-contract、org-pages、initialization | scope-foundation |
| 3 | s2-sales-purchase、s3-selection | 客户/合同接入；选品和销售分别消费其授权入口 |
| 4 | entry-mapping、s3-funds | 销售/采购责任与正式来源事实 |
| 5 | s3-fulfill-workbench、s3-quality-profit | 销售/采购和资金授权事实；工作台实际读取票款事实 |
| 6 | cross-resource-audit | 所有核心消费者和 S2 必需依赖 |
| 7，可选 | S4 各功能组，按文件兼容关系分批 | cross-resource-audit；供给、供应商订单与结算共用文件时必须拆批 |
| 最后 | release-readiness | 本次全部已选实施组，包括显式选中的 S4 |
| 独立运行 | s3-handover | 当前代码的跨资源接入和履约/验收任务资格已经核对；不得混入核心/S4 实施批次 |

已选依赖失败时，其下游保持未完成；可继续其他独立组。`only` 未选择的依赖由 Plan 只读核对，不自动扩大实施范围。只缺外部业务验收的前置项不等于缺少本地实现，但不得因此开放正式业务。

### 5.3 文件冲突与验证边界

1. Plan 必须完整分割本组 ID 为待实施、本地已有证据、阻塞；仅缺业务验收不得触发重新实施。每批实施前重新读取当前代码和已完成前置组证据。
2. `files_to_touch` 登记本组独占域写入；`shared_files` 登记延后统一合入的文件；`read_files` 登记实施时必须保持稳定的外部接口和业务事实文件。读取自己的待改文件不构成跨组稳定依赖。
3. 域写入之间、域写入与其他组共享写入之间不得重叠。任一组的域/共享写入与另一组的稳定读取相交时不得并发。双方只是在同一共享注册文件增加独立声明时，域实现可并发，统一 Merge 负责合并。
4. 不满足兼容条件或超过并发上限的组延后；前一批完成后必须重新计划，不能直接执行已过时的文件清单。遇到无法由文件表达的业务依赖或未知规则须登记阻塞。
5. 并发域实施/修复只写代码与单元测试，不运行 Cargo/npm、格式化、构建或代码生成。Cargo.lock、permissions.generated.ts 等预期生成物须登记在共享清单；LocalChecks 在所有写入结束后统一执行。
6. 每个写入面板结束后，以 `org-data-scope-snapshot.py` 对比实际 tracked/untracked 文件内容、HEAD 和 index，允许范围为该面板的域文件并集；共享合入单独核对共享文件并集。不得把其他并发组的合法修改误判为当前组越界。
7. 超范围变化终止后续写入，不自动回滚用户工作。脚本范围检查是事后核验，不替代宿主沙箱。基础设施错误可能已发生写入，不自动重放失败的写入面板。
8. 每次并发审查结束必须核对工作区未变化。修复后的所有批次组均须重验；全局门禁之后的最终 Verify 使用同一冻结代码状态。
9. Verify 的 verified/held 必须完整、无重复、互斥且仅含本组 ID；缺失、失败、passed=false 或非法输出均将本组保留未完成。
10. Docs 只更新四份组织范围 Markdown 的交付状态，不修改生产代码、OpenAPI、目标条款或退出条件。文档之后独立核对内容与文件变化；源代码变化使验证失效。
11. 不执行 git add/commit，不修改用户原有暂存内容，不部署或开放业务。销售责任交接本地验证通过仍不得开放改派入口。报告必须输出实际并发批次、最大并发数及保留未完成项。

## 6. 质量与完成判定

后端必须执行 fmt、workspace check、全目标全特性 Clippy、`env -u ERP_TEST_MONGO_URI cargo test --workspace --lib`、BPM/领域边界及权限漂移检查；前端必须执行 TypeScript、lint、完整 Vitest 和 Node 单元测试。11 个命令必须逐个记录真实退出码与证据。

权限生成物合法变更可能触发当前脚本的未暂存漂移失败；必须如实保留失败，不得自动暂存、提交、删除变更或改弱门禁。

| 结果字段/状态 | 判定要求 |
| --- | --- |
| `verified_ids` | 最终独立 Verify 确认本地实现与测试证据，且验证期间代码未变化 |
| `held_ids` | 本次已选但验证失败、缺证据、阻塞或未执行的 ID |
| `excluded_ids` | 本次未选任务；不得并入已完成数 |
| `checks_passed` | 全局审查有效、11 项质量门禁全部通过且检查前后工作区一致 |
| `local_scope_closed` | 本次所有 ID 已验证，无 held，质量与文档审计通过，无工作区异常 |
| `contract_closed` | 本 workflow 固定为 false；不代理真实业务验收 |
| `pending_acceptance` | HTTP/浏览器真实账号、数据库事务与并发、初始化重跑、查询计划、正式业务验收等未执行项目 |

静态和单元测试证明本地实现；不得代替真实数据库、账号或业务验收。仅代码与单元范围已就绪的任务重跑时应重新核对证据，禁止反复实施。受当前仓库约束禁止的验证必须保留未执行，不得为完成合同绕过限制。

## 7. 原生脚本校验

使用 Grok 的 `workflow` 工具，指定脚本并显式设置 `validate_only=true`：

```json
{
  "source": {
    "type": "script_path",
    "script_path": "/Users/huangjiajiang/Development/erp/.grok/workflows/complete-org-data-scope.rhai"
  },
  "validate_only": true,
  "args": {"apply": false}
}
```

分别校验只读参数、默认并发参数、max_parallel=1、独立交接参数和 S4 参数。校验只检查元信息、完整脚本编译及参数对应的模拟宿主路径；不得据此登记真实 agent 执行、所有分支、数据库或业务验收通过。

`apply=false` 属于实际 workflow 的只读执行模式，仍会调用真实盘点/计划 agent；语法校验必须使用宿主的 `validate_only=true`，两者不得混用。
