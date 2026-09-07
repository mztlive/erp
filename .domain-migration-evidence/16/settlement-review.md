# 阶段 16 供应商结算迁移交付合同

## 1. 接受输入与验证边界

- 唯一实施树：`/private/tmp/erp-domain-crate-16-supply`。
- 业务输入：`ed8015e28d36e5b9323b87e1a1fd5f63c122eb70`；after 固定为源码提交 `72a0c79a2261d33699b869329376e534edfb1ef4`；清单 65 个文件逐一读取该提交 blob 并核对当前文件 SHA 相等，不冒认输入提交已包含新实现。
- 原始源快照与 SHA：`/private/tmp/settlement16-before/manifest.json`。只接受该输入的规则、读写顺序、错误、DTO 和测试合同。
- 本片已实施真实本域准备/持久化、跨域根事务、组合读模型、财务实际过账 provider，并删除旧结算服务的 15 个原源文件。每个旧服务文件删除前逐字节核对与输入相等。
- 本片只运行定向 rustfmt、`git diff --check`、词法/符号/测试/注释/指纹静态检查；不运行 Cargo、MongoDB 或历史 `tests/**`。workspace check、strict Clippy、lib tests、边界检查由 root 统一执行。root 已确认 clippy5、domain checker、permissions 均 exit 0，lib tests 在执行中；最终通过状态以 root 对完整阶段树的日志为准。
- 当前源清单、SHA、测试定义和 DTO 比较见同名 JSON；可用 `/private/tmp/audit-settlement16-delivery.py` 重采，不触发 Cargo。

## 2. 唯一公开入口与归属

| 原能力 | 新唯一入口 | 合同 |
| --- | --- | --- |
| 原实体及值对象 | `erp_supply::entity::supplier_settlement::*` | 原状态、金额恒等、差异结论、来源事实和 ID 类型保持。 |
| 原聚合/owned 仓储、索引 | `erp_supply::repository::SupplierSettlementExt`、`repository::supplier_settlement::*`、5 个 `repository::owned::*Repository`、`indexes::supplier_settlement` | 原集合名、过滤/排序、CAS、聚合查询和索引保持。 |
| 本域服务 | `erp_supply::service::supplier_settlement::SupplierSettlementService::new(Database)` | 拥有 native prepare、复验、持久化和单域 GET；不引用外域或旧三层。 |
| 八个命令 | `erp_processes::supply_settlement::SupplierSettlementProcess::new(Database)` | 保留 `create_statement`、`refresh_statement`、`record_source_evidence`、`append_difference_evidence`、`decide_difference`、`submit_review`、`decide_review`、`void_statement` 原签名。 |
| 单域 GET | `SupplierSettlementService::{supplier_settlement_statement_list,supplier_settlement_item_list,supplier_settlement_difference_list,latest_source_evidence}` | 直接归本域；Process 不保留查询转发 facade。 |
| 含正式任务的详情 | `erp_read_models::supplier_center::settlement::SupplierSettlementReadService::new(Database)` → `supplier_settlement_statement_detail(id, &AuditActor)` | 保留原快照查询和工作项映射、任务唯一性、当前责任与岗位分离；不提前授权或新增资格判断。 |
| 单域 DTO | `erp_supply::dto::supplier_settlement::*` | 原 41 个 struct/enum 定义（38 个 pub、3 个 pub(crate)）均有唯一新落点；包含 derive、字段属性与字段顺序的词法 token 全相等。 |
| 3 个跨域 DTO | `erp_read_models::supplier_center::settlement::dto::{SupplierSettlementStatementDetailView,SettlementReviewWorkItemView,SettlementReviewDecisionResult}` | 正式 WorkItem 枚举只在组合层引用，原 HTTP 字段和枚举不复制、不改名。 |
| 应付真实 provider | `erp_finance::service::payable::supplier_settlement::{build_settlement_payable,persist_settlement_payable}` | 财务独占应付实体构造、账户后分录的真实仓储写入。 |
| 成本真实 provider | `erp_finance::service::cost::supplier_settlement::{build_settlement_cost_delta,persist_settlement_costs}` | 仅全零三元组返回空计划；非零保持原错误，禁止伪造 CostEntry。 |

`SupplierSettlementService` 下的 prepare 方法分别执行创建、刷新、差异决定、补证、作废的真实校验与构造；persist 方法分别执行本域 CAS、聚合写入与补证复验。Process 保留审计身份、回执编码/恢复、正式任务、根事务及财务步骤。该划分不是原 Service 的整类搬迁。

## 3. 消费方事实及真实提供方

- `erp_finance::service::payable::supplier_settlement::SettlementPayableSource` 只含 `statement_no: String`、`supplier_id: SupplierAccountId`、`subject_hash: String`、`period_end: BusinessDate`；Process 从当前结算单显式投影。金额、actor 和冻结时间仍为原调用参数。
- `erp_finance::service::cost::supplier_settlement::SettlementCostDeltaFact` 只含 `gross/net/tax: Amount`；Process 从结算实体计算的 `SettlementCostDelta` 投影，不构造未知原成本、税率或分摊链。
- 来源证据消费的履约事实已同属 `erp-supply`，直接复用原仓储查询，不新造跨域中间实体，不增加聚合或过滤。
- 本域 `settlement_review_access(owned, eligible, separation_satisfied)` 消费 WorkItem 所有权布尔事实，RM 在原位置用实际 `is_owned_by(actor.id())` 投影；保留原详情 `eligible = true` 行为，资格授权仍在正式命令事务内执行。

## 4. 生产顺序合同

### 4.1 创建、刷新、来源与差异

- 创建：请求/动作/期间 → 按确定性结算号重放 → 最新来源读取 → Statement ID → 快照 Item/Difference ID → `Instant::now` → 结算实体构造/刷新/主题 → 审计准备 → 根事务本域单头/明细/差异 → 审计。失败恢复仍查原结算号，不增加重试或新幂等键。
- 刷新：请求/动作/路径/指纹/审计重放 → 原责任/版本/来源/旧明细差异读取 → 同来源 no-op 分支。no-op 不生成新快照/ID/时间、不写单头，仍在原位置独立写刷新审计并返回 UNCHANGED。变化分支在原时点构造快照、刷新主题，在根事务重新读当前结算版本/hash，再调用真实仓储替换与审计。
- **草稿替换以真实输入为准**：原 `replace_draft_snapshot` 没有 editable guard。实际物理顺序是旧 difference IDs 非空时删证据 → 旧 item IDs 非空时删差异 → 无条件按 statement ID 删明细 → statement CAS → 无条件 insert_many 新明细 → 新差异非空时 insert_many。不得按先前准备文档误加状态 guard、条件或调整删除顺序。
- 来源证据：原请求校验/摘要/重放 → 本域逐行完整核验和证据构造 → 审计 → 根事务证据 create → 审计；首错与原失败恢复查询不变。
- 差异决定：先规范化结论，再指纹/重放；本域保留归属、责任、版本、正式证据检查，原 `now` 后记录结论并重算单头主题；根事务 statement CAS → difference CAS → 审计回执。
- 补证：外层仍按原顺序准备 ID/时间；事务真实 `evidence_posting::persist_with_store` 再读 difference → 校验版本 → 读 item → 校验归属 → 读 statement → 校验三种允许状态 → statement CAS → evidence create。审计仍在其后，失败按原类型直接传播。
- 作废：原已 Voided 的重放分支先于责任/版本校验；其余仍先准备状态，再根事务 statement CAS → audit；成功后才回填外层实体。

### 4.2 提交复核与决定

- 提交：原校验与审计重放 → 读结算单 → 经办责任 → 单头版本 → 冻结主题/截止策略 → WorkItem ID/new → 根事务重新读 statement → 当前版本/主题 → 本域完整差异复验 → submit_review → statement CAS → 新 work_item → receipt/audit。任务 ID 与构造时间不提前、不延迟。
- 决定外层：原请求/原因/任务版本解析 → action/指纹/审计重放 → statement 读取与版本 → work_item 读取/版本/归属/主题 → items → differences → 当前 resolved subject → `Instant::now`。
- 随后生产 `review_preparation::prepare` 执行：Confirm 时先原 `ensure_confirmable` 和 payable amount → **PayableAccount ID/new → PayableEntry ID/new → 成本 fail-closed 检查 → record_review**；Reject 时不构造财务实体，按是否有差异选择 Draft/HasDifference 并保留原因校验。两分支之后才 WorkItem activity → complete。非零成本错误仍发生在任何复核/任务变更及根事务之前。
- 根事务生产 `review_posting::post` 的顺序固定为 **Authorize → Separation → Items 重新读取 → Differences 重新读取 → Subject 复验 → Statement CAS → Task CAS → Payable → Costs → Audit**。不复用事务前快照替代事务内读取。
- Payable 实际到 `PayableRepository::create_payable_with_entry` → 私有 `command/initial.rs` Store → `mongo_ops::insert_one(account)` → `mongo_ops::insert_one(entry)`。各步复用同一传入 Executor，账户失败不写分录。
- Costs 仍逐条调用原 `create_cost_entry_with_allocations(entry, Vec::new(), executor)`；全零真实计划为空，不伪造成本写入。

## 5. 幂等回执的版本比较

| 回执 | 当前事实要求 | 响应 |
| --- | --- | --- |
| Refresh | statement version **==** receipt，source hash **==** receipt | 原刷新结果；后续快照替代时冲突。 |
| Difference decision | difference version **==** receipt，差异已解决且归属一致；statement version **>=** receipt | 响应 `statement_lock_version` 仍取原 receipt，允许单头继续推进。 |
| Review submission | statement version **>=** receipt；task version **>=** receipt 且原 type/object/subject/role/org 一致 | 恢复原提交结果，单头版本检查仍发生在任务读取前。 |
| Review decision | statement/task versions 均 **==** receipt；task Completed；Confirm/Reject 的正式业务状态和 payable 关系一致 | 精确恢复决定结果，禁止把后续状态误认为该次决定。 |

新增 helper 均由生产 replay 方法在原位置调用；没有只测未接线的副本。原错误类型、中文错误、回执版本来源及单次失败恢复分支不变。

## 6. 测试证据及边界

- 原测试：86 个名字一对一保留，7 个原 ignore 保留；完整 before/after 路径见 `/private/tmp/supply16-settlement-test-comment-audit.json`。其中原实体/仓储 59 个函数体 token 比较复用持久化证据；其余 27 个保留真实断言，适配仅限新路径、消费方事实和服务类。
- 新增测试：26 个。草稿 Store 4、raw BSON/JSON 5、补证 Store 5、财务账户/分录 Store 1、应付字段 1、成本三分量 fail-closed 1、复核准备 3、复核事务顺序/每步失败 2、四类回执边界 4。
- 结算迁移范围共 112 个定义，其中 7 ignored。同名 JSON 扫描另包含被窄编辑的 `finance/repository/payable/command.rs` 里 1 个既存、非本迁移测试，故原始清单为 113；不得将该额外测试算入新增数量。
- 所有执行器身份测试使用非零 `TestExecutor { _identity: u8 }`，真实生产 runner 被测试调用；每步注入首错后断言精确停止前缀。
- 非零成本测试通过真实财务 provider 验证：应付实体已构造后报原业务错误，statement 与 WorkItem 的完整 JSON 在失败前后相等；零成本 Confirm 和两种 Reject 状态均有实际准备路径测试。
- 5 个新 BSON/JSON wire tests 位于 `erp-supply` 仓储测试叶；实体生产和测试文件不引用 BSON。5 个原 Mongo ignored 测试位于 `erp-processes/src/supply_settlement/repository/persistence_tests.rs` 的真实仓储测试边界；不修改历史测试、不放宽检查器。
- 62 个原服务模块/函数注释块均有实际新落点；旧架构描述仅按新责任归属更新，原业务条款保留。
- 以上为测试定义和静态核对证据。实际执行数、通过数与阶段门禁以 root 最终日志回填；不据此声明真实 MongoDB 回滚、并发或线上接口验证已完成。

## 7. 关联证据与接收条件

- 实体/仓储/索引：`/private/tmp/supply16-settlement-persistence-result.md` 与同名 JSON。
- 补证真实 Store：`/private/tmp/supply16-settlement-evidence-posting-result.md` 与同名 JSON。
- 原测试/注释：`/private/tmp/supply16-settlement-test-comment-audit.md` 与同名 JSON。
- 本片源清单与 41 个 DTO 字段/derive token 同一性：`/private/tmp/supply16-settlement-result.json`。
- 独立生产语义审查由 C 提供，接受 before 输入与本片真实 after provider/runner；纯源对照不升级为运行证明。
- 接收阶段 16 时必须将上述指纹绑定 root 门禁所使用的最终源码；后续编译修复改动必须重采本片 JSON，不用过期 hash 冒认最终图。
