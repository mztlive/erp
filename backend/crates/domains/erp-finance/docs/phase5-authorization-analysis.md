## FSY-7 第五阶段授权检查：第一轮分析交接

本轮发现两项需要开发核验并修复的仓储合同缺口，均为 P2。指定的 `erp-finance` 库单元测试通过。本报告不是独立 Review 通过结论，业务实现尚未修改。

### 版本、范围与证据口径

| 项目 | 记录 |
| --- | --- |
| 日期 / 环节 / 轮次 | 2026-11-16，后端分析，第一轮 |
| 事项 / 父事项 | FSY-7 / FSY-2 |
| 实际分析代码 SHA | `82706febcc048fe250772eb8d3a047ad017ec3d6` |
| 分支 | `agent/agent/fsy-7` |
| 事项中的历史基线 | `cc248977e7d6b2b73cc7a935f3897b93ea389a4f`；它是本次 HEAD 的祖先，本次实际检查以上述 SHA 为准 |
| 历史基线差异核对 | 两个版本间，finance crate 与 `wiring/authorization/{finance,suppliers}.rs` 无差异；不据此推定其他目录无差异 |
| 本轮变更 | 仅增加分析文档；文档交付提交 SHA 在事项交接评论记录 |
| 历史交接 | 本事项无已有评论；已展开父事项建单、闭环工作流及 FSY-3 / FSY-8 协作说明；相关 FSY-9、FSY-28、FSY-36、FSY-38 首次评论扫描均为空 |
| 依据 | `backend/AGENTS.md`、finance README、权限重写方案 §3.1–3.3、§3.5–3.6、§3.7.4、§4.1.5 |

检查覆盖本 crate 的授权凭证消费者、组织事实端口、授权错误转换，以及其公开财务步骤的实际上游授权位置。跨域 Facts、Mapping 和装配仅做必要追踪；HTTP、worker、Application、旧服务工厂切换及 server 全量编译属于第六阶段，不列为本事项缺陷。

以下“静态核对”指读取生产代码及调用链；“单测通过”仅指本轮实际执行的 finance `--lib` 命令。未运行其他 crate 的单测，未运行数据库行为或应用端到端验证。

路径缩写：`F` = `backend/crates/domains/erp-finance/src`；`R` = `backend/crates/read-models/erp-read-models/src/finance`；`P` = `backend/crates/orchestration/erp-finance-posting/src/finance_posting`；`D` = `backend/crates/orchestration/erp-supplier-orders/src/supplier_deposits`；`W` = `backend/apps/erp-server/src/wiring/authorization`。下述行号均对应实际分析代码 SHA。

### 已证实问题与最小修复方案

#### FIN-AUTH-01 · P2 · 预存款分页未检查凭证使用边界和有效期

- **位置**：`backend/crates/domains/erp-finance/src/repository/supplier_deposit_query.rs:52`，特别是第 71–81 行。
- **触发条件**：仓储收到集合名仍匹配的旧 `CompiledAccess`，但执行器已换事务、发生写入、进入下一读取阶段、退出原作用域，或授权已到期。
- **预期**：在任何聚合 I/O 前调用 `access.ensure_current(executor)`；读取后再次检查，再返回仓储结果。跨阶段返回 identity 的 `ValidationError`，到期返回带 `DATA_SCOPE_CHANGED` 的 `ConflictError`，经财务错误转换保留类别。
- **实际**：入口只检查集合名，复制 `access.stages()` 后直接执行 `account_rows`，没有任何 `ensure_current`。`CompiledAccess::stages()` 本身不检查执行器或时间，底层聚合也不持有该凭证，因此不能代替这项检查。依据：identity `repository/authorization/query.rs:52`、`:282`、`:308`。
- **影响与限制**：这是仓储执行前的明确合同缺口。现有唯一生产调用 `D/management/reads.rs:122` 在同一快照事务内生成凭证，展示补齐后于第 142 行调用统一 `validate`；本轮没有证明现有 HTTP 路径已将失效凭证的数据交付给用户，不能将仓储缺口夸大为已复现的数据泄漏。
- **最小修复**：在 `account_page` 聚合前及成功读取后分别检查原凭证；保留调用方最终 `validate`，不在仓储重复解析权限、重读身份依赖或创建授权服务。
- **验证依据**：静态调用链已确认；现有 finance 单测没有直接覆盖这个入口的失效凭证场景，需按后文补行为测试。

#### FIN-AUTH-02 · P2 · 完整对象仓储的目标模型核对不完整

- **位置 A**：`backend/crates/domains/erp-finance/src/repository/supplier_deposit_query.rs:71`。只检查 `collection == supplier_deposit_accounts`，未检查资源、主键或 `QueryOutput::Object`。
- **位置 B**：`backend/crates/domains/erp-finance/src/repository/payable/payment_request.rs:122`。检查集合和 `Object` 输出，但未核对资源为 `purchase_payment_request`、主键为 `id`。
- **触发条件**：传入由其他固定入口取得、或未来装配变化后得到的同集合但错误资源、错误主键、错误输出粒度的凭证。A 返回完整 `SupplierDepositAccount`，尤其不能将份额或存在性计划当成完整账户授权。
- **预期**：仓储使用前核对与实际读取一致的模型描述：A 为 `(supplier_deposit, supplier_deposit_accounts, id, Object)`；B 为 `(purchase_payment_request, purchase_payment_requests, id, Object)`。非法模型在 I/O 前返回接入校验错误。
- **实际**：上述部分错误模型可通过现有局部条件；`CompiledAccess` 的构造权属于 identity，但这不能替代方案 §3.6.4 要求的消费者目标核验。作为对照，`F/repository/cost/read_scope.rs:232` 已核对资源、集合、主键并调用模型自校验。
- **影响与限制**：当前已定位的生产映射分别由 `DepositMapping` 和 `FundsMapping` 提供，描述符合预期；本轮未发现当前装配可直接获取一个同集合、错误资源的生产凭证。问题属于消费者合同和后续接线防错，未证实可利用的现行外部越权路径，故定为 P2。
- **最小修复**：分别抽出本模块私有模型核对函数，覆盖资源、集合、主键、输出粒度，并在生产入口使用；调用 `QueryModel::validate` 保留基础描述校验。优先复用已有集合常量，不增加第二套权限政策或通用工厂。
- **测试**：对实际生产模型核对函数验证合法完整对象、错误资源、错误集合、错误主键、合法 `Existence`、合法 `Share` 及描述非法场景。另验证成本的存在性会员能力继续有效，不能把成本专用入口一律改为 `Object`。

### 逐入口检查表

表中 M1–M11 对应后文绑定表；“委托”表示该入口是拥有领域的事实或事务步骤，授权由列明的实际业务调用方执行，不要求在每个步骤重新引入身份解析。

| 入口及位置 | 权限 / 目标 | 授权与交付位置 | Facts / Mapping / 装配 | 测试与结论 |
| --- | --- | --- | --- | --- |
| `CostEntryReadScopeExt::scope_candidates`，`F/repository/cost/read_scope.rs:132` | `cost::ENTRY_LIST`、`ENTRY_DETAIL`、`ALLOCATION_LIST` 的成本根会员 | 本地模型校验、聚合前后 `ensure_current`；`R/cost/snapshot.rs:182` query，`:212` validate | M8、M9；根为 `cost_entries`，份额或整账会员先于候选上限 | 模型及会员关联管道单测通过；未验证 Mongo 执行效果；静态符合 |
| `scope_candidate_versions`，同文件 `:161` | 与候选查询相同根凭证；输出身份、版本 | 同一 `authorized_membership`，前后检查；`R/cost/snapshot/versions.rs:85`–`:94` 交付 | M8、M9 | 共用模型检查；版本读取数据库行为未执行；静态符合 |
| `scope_entries_by_ids`，同文件 `:190` | 已收敛候选中的成本身份，内部计算数据 | 委托；唯一生产调用 `R/cost/snapshot.rs:246`–`:266` 先取得授权候选，再用匹配分配收窄 | M8、M9；不是独立成本对象授权 | 空集返回空；金额投影仍由 `Snapshot::project` 裁剪；静态符合委托边界 |
| `scope_whole_ids`，同文件 `:114` | 整账读取计划，输出成本身份、版本 | 委托；`R/cost/snapshot/versions.rs:139` 与 `:153` 对共享 `WholeAccess` 前后检查 | M8、M9；同次根查询 `whole_stages`，不是另签完整对象凭证 | 仅接受当前固定内部调用链；本轮未把裸 stages 当作独立授权证明，见后文边界说明 |
| `CostAllocationReadScopeExt::scope_allocations`，同文件 `:319` | 授权候选对应的全部分配事实 | 委托；`R/cost/snapshot.rs:272` 起批量读取，完整来源投影后统一交付 | M8、M9；不得先丢弃不可见来源后宣称整账可读 | 业务上限与筛选纯函数单测通过；来源完整性数据库行为未验证 |
| `scope_allocation_versions`，同文件 `:336` | 同候选分配的身份、版本、真实销售来源 | 委托；`R/cost/snapshot/versions.rs:123` 使用同一 executor | M8、M9 | 固定轻量投影；保留缺失销售来源；静态符合 |
| `scope_matching_allocations`，同文件 `:353` | 候选身份与业务筛选交集 | 委托；`R/cost/snapshot.rs:248`，完整候选 `$in` 与筛选取交集 | M8、M9 | `read_scope::tests::matching_allocations_intersect_sales_and_cost_filters` 通过；版本投影仅静态核对 |
| `PurchasePaymentRequestRepositoryExt::scope_candidates`，`F/repository/payable/payment_request.rs:116` | `purchase_payment_request::{LIST,DETAIL,CREDIT_LIST}`；完整申请根 | 前后 `ensure_current`，授权阶段先于 `10_001` 上限；`R/payment_request/query.rs:141`、`:165` query/deliver | M4；当前列表调用传 `LIST`，不接受仅部分来源交付整单 | 筛选组合单测通过；**FIN-AUTH-02 待修** |
| `linked_for_account`，同文件 `:141` | 已授权采购上下文中的关联申请份额 | 委托；仅返回候选，调用方不得交付其他采购来源 | M4；与独立申请完整详情不同 | 财务源码列明份额合同；未验证跨域份额读取行为，不声明独立申请授权成立 |
| 付款申请 `find_for_task / reserved_for_account / page`，同文件 `:152/:180/:160` | 精确任务定位、额度事实或已收敛单域读取 | 下层仓储；普通列表实际使用上述 `scope_candidates`，不是 `page` | M4；任务事实由 `P/task_access` 与 W runtime 绑定 | page 本身不是带身份的受保护业务入口；未为通用仓储强加额外 identity 调用 |
| `SalesInvoiceRequestRepositoryExt::scope_candidates`，`F/repository/receivable/invoice_request.rs:129` | `receivable::INVOICE_REQUEST_INVOICE_REQUEST_LIST`；完整开票申请 | 委托；`R/funds_scope/repository/request.rs:20` 前后检查，固定根 guard 后读取；`R/funds_scope/request.rs:161` deliver | M2 | 候选上限与业务筛选单测通过；裸 stages 不可对外作为可重用凭证，见边界说明 |
| 开票申请 `find_for_task / reserved_for_account / page`，同文件 `:140/:168/:148` | 任务定位、额度事实、通用单域查询 | 下层事实；由开票申请业务用例 / 固定 query 入口保护 | M2 | 本轮未以直接调用通用 page 作为用户授权路径 |
| `supplier_deposit_query::account_page`，`F/repository/supplier_deposit_query.rs:52` | `supplier_deposit::LIST`，完整账户对象 | `D/management/reads.rs:122` query，`:142` validate；筛选在授权阶段后、分页前 | M11 | **FIN-AUTH-01、02 待修**；现有 lib 无直接拒绝行为测试 |
| `latest_account_records`，同文件 `:27` | 已授权账户的资金证据 | 委托；`D/management/reads.rs:74` account_access 后 `account_detail` 水合 | M11 | 账户精确筛选且限 100 条；本轮静态核对，不声明每个证据查询独立授权 |
| 同模块 `allocatable_batches / reservation_for_execution / usages_for_reservation / eligible_accounts / bank_source_exists / reservations_for_manual_command / automatic_authorizations / ledger_balance_at` | 已授权账户、执行组、付款来源或对账事务的内部事实 | 预存款领域步骤与 `D` 编排消费；不是公开列表授权的替代入口 | M11；`SupplierDepositAuthorization` 是业务资金执行额度，不能代替用户 RBAC | 有界候选及真实资金事实；未访问数据库；归属核对与执行限额的完整业务复审由 FSY-36 负责 |
| `FundsDataScopePort::expand_org_units`，`F/ports/funds_scope.rs:31`；未接线实现 `:53` | 请求组织筛选事实；不解释权限 | 未接线实现直接 `Internal("资金组织事实端口未接线")`；调用方错误传播 | `wiring/funds_data_scope.rs:57` 为真实组织 adapter，非权限 Mapping | `unwired_organization_reads_fail_closed` 本轮通过 |
| `From<IdentityError> for Error`，`F/error.rs:63` | 拒绝、接入失败、期限冲突、设施错误 | 逐变体转换；不返回空集或授权成功 | 无独立绑定 | `authorization_errors_preserve_their_original_classification` 通过；主要错误类别得到行为验证 |
| `CostService::cost_entry_detail_with_executor`，`F/service/cost.rs:69` | 原创建命令结果；`cost::ENTRY_CREATE` 的服务端候选 | `P/cost/mod.rs:87`、`:119` require 先于 persist，同 executor 回读；本地拒绝非快照 | M8，真实销售来源 | 拒绝非快照执行器的 finance 单测通过；不追加独立详情菜单资格 |
| `PayableService::purchase_invoice_registered_view`，`F/service/payable/invoice.rs:28` | 原进项发票命令结果；`payable::PURCHASE_INVOICE_ALLOCATION_POST` / Operation | `P/payable/invoice.rs:419`–`:438` 原快照内 require → 回读 → 边界核验 | Function 操作不需要无用 Mapping；M10 提供对应查询绑定 | 本地为下层装配；未运行编排单测；静态确认前置授权，不强加详情权限 |
| `ReceivableService::invoice_detail_with_executor`，`F/service/receivable/invoice_query.rs:27` | 原发票命令结果；如 `internal::INVOICE_POSTED_RESULT_READ` / record | `P/receivable/invoice_result.rs:40`–`:61` 同事务 require → 回读 → require；另有原提交事务调用 | M7，根及真实来源 Facts | 金额方向纯函数单测通过；授权行为由调用方及 identity 覆盖，本轮未执行其测试 |
| `FinanceCommandReceiptService::{committed_purchase_invoice_id,save_purchase_invoice,committed_resource_id,save_resource}`，`F/service/command_receipt.rs:39/:63/:84/:112` | 已授权命令的结构化幂等定位与结果 | 财务 Process 的 require 在回执读取/写入前；例如 `P/payable/invoice.rs:302`–`:310` | 不独立签发业务授权；复用原命令与 executor | 回执、类型化结果及冲突单测通过；真实事务原子性未验证 |

### 其余公开领域步骤的责任边界

finance 没有持有 `AuthorizationService` 的业务入口，也没有本地 `require/query`、公共权限常量重声明、政策构造、独立 compile 或授权主体选择。它对 identity 的生产依赖用于三个模块的四个 `CompiledAccess` 消费入口和错误转换。用户级用例主要位于 FSY-28、FSY-36、FSY-38 对应编排 / 读模型；以下公开 API 作为下层步骤，不因签名是 `pub` 就新增一遍 RBAC。

| 领域步骤 | 已定位调用方与前置条件 | 本轮结论 / 未验证项 |
| --- | --- | --- |
| 成本 `prepare_cost_entry / persist_cost_entry`；`service/cost/{purchase_initial,fulfillment_actual,supplier_settlement}` 的 prepare/persist | `P/cost/mod.rs` 显式 Candidate require；采购 / 履约 / 结算过账提供已验证来源和同 executor | 保留金额守恒、状态、来源存在性职责；跨域业务的全部权限矩阵不在本轮重新审查 |
| 应付 `prepare_payable_account`、`purchase_initial::{prepare,persist}`、`PurchaseChangePayableWrite::persist / sync_purchase_subject`、结算应付构造与写入 | 采购、变更、供应商结算编排组织事务；下层读取和写入只使用传入 executor | 不是独立财务列表；lib 的来源、版本及金额单测通过，真实并发未验证 |
| 进项票 `prepare_purchase_invoice / prepare_purchase_invoice_allocations / persist_purchase_invoice` | `P/payable/invoice.rs:293` 的 register 同事务先 require(Operation) | 不在仓储增加新的数据政策或重复主体选择 |
| 应付付款 `settle_supplier_payment / settle_supplier_payment_with_facts / finish_supplier_payment / finish_credit_allocation`、`load_payable_offset_facts` | `P/payable/posting.rs` 消费任务 / 付款申请授权的冻结事实和同事务执行器 | 保留额度、付款状态及核销不变式；FSY-28 独立检查任务与权限组合 |
| 应收 `create_initial_receivable / prepare_sales_change_receivable / SalesChangeReceivableWrite::persist`、回款提交和核销、发票 / 红票过账、回款容量预留 | 销售、finance-posting、reverse-flow 编排；结果回读仍走原命令授权 | 本地 lib 覆盖资金规则，未将这些单测计为上游授权通过 |
| 客户退款、供应商退款、收付款冲正的 load/prepare/persist 函数 | `erp-reverse-flow/src/reverse_flow/{customer_refund,supplier_refund,payment_posting}.rs`；相应 require / 原业务来源在编排中 | 原错误和 executor 继续传递；跨域复审协作方为 FSY-34 |
| `SupplierDepositService` 的开户、充值申请 / 批准 / 付款 / 到账、authorize / revoke、reserve、record_debit、settle / return、release、resolution、funds lease、funds scan / evidence 方法 | `D/management/{reads,recharge,authorization,execution,resolution}.rs` 先经 `account_access / identity_access`；后续执行还依赖持久化资金额度及执行组 | 写方法调用本地 `require_transaction`，不接外部 HTTP；账户读取、coverage、恢复查询为下层事实，不能作为新的用户授权证明；自动路径完整业务检查归 FSY-36 |
| `repository` 的标准 CRUD、search/page、汇总与来源 / 版本事实读取，及 `entity/dto/indexes` | 需要拥有领域上游授权或固定查询 wrapper；无身份的通用仓储不自行创建授权服务 | 没有把未被普通受保护读取使用的旧 search/page 当作当前用户可绕过路径；后续如新增业务调用，必须重新核对其授权前提 |

README 中“不得依赖其他业务领域”措辞比当前 `backend/AGENTS.md` 的 identity 例外更旧；以更高优先级后端规则为准，不将现有 identity 依赖列为架构缺陷。

### 权限 → 事实 / 映射 → 装配

公共常量均定义于 `backend/crates/foundation/erp-permissions/src/{cost,payable,purchase_payment_request,receivable,supplier_deposit,internal}.rs`。finance 不直接声明权限，因此不需要为“有常量引用”额外增加 `erp-permissions` 依赖。

| 编号 | 公共权限 | 真实根 / 输出 | Facts、来源与装配证据 |
| --- | --- | --- | --- |
| M1 | `receivable::{ACCOUNT_LIST,ACCOUNT_DETAIL}` | `receivable_account / receivable_accounts / Object` | FinancialFacts；实际销售来源；`W/finance.rs:48` |
| M2 | `receivable::{INVOICE_REQUEST_INVOICE_REQUEST_LIST,INVOICE_REQUEST_INVOICE_REQUEST_DETAIL}` | `sales_invoice_request / sales_invoice_requests / Object` | 应收引用及销售来源一致性；`W/finance.rs:49`–`:52` |
| M3 | `payable::{ACCOUNT_LIST,ACCOUNT_DETAIL}` | `payable_account / payable_accounts / Object` | 实际 `source_type` 区分采购与供应商结算；`W/finance.rs:53` |
| M4 | `purchase_payment_request::{LIST,DETAIL,CREDIT_LIST}` | `purchase_payment_request / purchase_payment_requests / Object` | 全部原始应付及采购引用；`W/finance.rs:54`–`:58`；任务 Facts 另由 `W/mod.rs:87` 注入共享财务 Facts |
| M5 | `receivable::{CUSTOMER_RECEIPT_LIST,CUSTOMER_RECEIPT_DETAIL}` | `customer_receipt / customer_receipts / Existence(id)` | 回款分配、应收分录、应收账户、实际销售来源；`W/finance.rs:59` |
| M6 | `payable::{SUPPLIER_PAYMENT_LIST,SUPPLIER_PAYMENT_DETAIL}` | `supplier_payment / supplier_payments / Existence(id)` | 付款分配 / 申请预分配、应付、采购或结算来源；`W/finance.rs:60` |
| M7 | `receivable::{INVOICE_LIST,INVOICE_DETAIL}`；`internal::INVOICE_POSTED_RESULT_READ` 走对象 require | `invoice / invoices`；列表为 `Existence(id)` | 销项 / 进项分配分别连接应收 / 应付；登记结果校验真实票据状态；`W/finance.rs:61` 与 FinancialFacts |
| M8 | `cost::{ENTRY_LIST,ENTRY_DETAIL}`；`ENTRY_CREATE` 走 Candidate require | `cost_entry / cost_entries / Existence(id)` | 全部分配保留为真实来源；`W/finance.rs:62` |
| M9 | `cost::ALLOCATION_LIST` | `cost_allocation / cost_entries / Existence(id)` | 查询根故意为成本，不是 `cost_allocations`；份额另由投影裁剪；`W/finance.rs:63` |
| M10 | `payable::PURCHASE_INVOICE_ALLOCATION_LIST`；`PURCHASE_INVOICE_ALLOCATION_POST` 是 Operation | `purchase_invoice_allocation / purchase_invoice_allocations / Existence(id)` | 实际应付、采购 / 结算来源；`W/finance.rs:64`；post 不要求无用 query Mapping |
| M11 | `supplier_deposit::{LIST,DETAIL}`，开户与资金动作的 require 复用同根 Facts | `supplier_deposit / supplier_deposit_accounts / Object` | `D/repository/authorization.rs:28` 创建真实账户 Facts 与静态 Mapping；`W/suppliers.rs:27`、`:31`、`:39`–`:40` 绑定 |

其他财务根 `customer_refund / supplier_refund / receipt_reversal / payment_reversal` 在 `W/finance.rs:25`–`:28` 同样有 FinancialFacts 绑定；它们由对象 / 任务入口消费，不要求添加无用列表 Mapping。

FinancialFacts 的实现位于 `R/funds_scope/repository/management.rs:65`，从未删除的真实根记录读取版本，拒绝来源标记不完整，并保存每个 relation 的 `expected_ids` 与实际来源对象。资金 Mapping 位于同目录 `mapping.rs:81`，通过 `relations()` 和 `relation()` 暴露同一静态来源，`records()` 同时检查软删除与来源完整性。关联准备使用静态 `stages()`；没有空 prepare 或伪造的 Mapping 修订。

`W/finance.rs:66`–`:72` 同一根的 list/detail/pay 复用 Mapping 实例，`with_sources` 复用组合根已有销售、采购、结算实例；FinancialFacts 的共享 map 也传入任务适配器。M11 的 Facts 依赖真实付款公司和供应商，负责人 / 组织来自供应商，结算主体来自账户，Mapping 使用相同外键与有效性条件；该 adapter 无需反向注入授权 Service。

`W/mod.rs:34` 的 build 遍历全部绑定并调用 identity Builder。identity `service/authorization/builder.rs:86` 校验完整性，`:110`–`:135` 对缺少根 Facts、任务 Facts 或 query Mapping 返回错误，未见缺少绑定降级为允许。server 中已有 `complete_bindings_construct_shared_service_without_database_reads` 单测源码，但本轮未运行 server，因此只记录静态装配证据，不声称本轮完整构造或 server 编译通过。

### 组织筛选、来源输出和授权时序的结论

1. **组织筛选只收窄**：`R/funds_scope/linked.rs:26` 未提供筛选才返回 `None`；已提供但展开为空仍保留 `Some([])`，错误继续传播。`repository/source.rs:93` 将组织写成附加 `$in`，`:51` 将该业务条件追加在授权来源阶段之后。`repository/accounts.rs:230`–`:247` 的根授权、真实来源及筛选均先于 page/count/summary 的共同 `$facet`。这是静态代码结论，未执行 Mongo 管道。
2. **未接线失败关闭**：`F/ports/funds_scope.rs:53` 返回 Internal；本轮单测实际验证。真实组织展开 adapter 位于 `wiring/funds_data_scope.rs:57`。旧 factory 的 `SharedRbacService` 注入尚未第六阶段切换，不作为第五阶段 finance 缺陷。
3. **成本 / 资金份额不等于整对象**：成本只用存在性取得内部候选，`Snapshot::project` 按来源和 whole membership 裁剪后才分页；版本与分配上限保留完整候选。FundsMapping 中多来源根只输出身份，不能将其直接拿来交付完整金额；FIN-AUTH-02 专门约束返回完整对象的两个入口，不修改成本的合法会员模型。
4. **固定内部 stages 的边界**：开票申请候选与成本 whole IDs 仓储仍收取裸 `Document` 阶段，但现有唯一生产 wrapper 持有原 `CompiledAccess` 并执行前后边界检查、最后统一 validate。当前证据支持把它们视为受控的下层执行步骤，本轮未证明存在外部可传任意 stages 的路径，未直接列为第三个越权缺陷。FSY-38 Review 如认定其公开合同仍需统一为携带根凭证的接口，应连同唯一调用方一起调整，不能仅删除 guard 或把裸 stages 宣称为凭证。
5. **查询 / 写入阶段**：成本、付款申请、预存款读取均由上游快照事务包住 query、读取、展示补齐、validate；本地只有 FIN-AUTH-01 缺少仓储自检。财务写入由编排先 require，再经同 executor 执行事实写入、回执及审计。命令结果步骤不额外要求菜单详情权限，保留原业务规则。
6. **清理检查**：本 crate 未发现旧授权政策构造、动作授权白名单、临时 AuthorizationService 工厂、调用方选择授权主体或独立 compile。`D/access.rs` 的动作到公共常量映射属于上游调用定位；其进一步清理由 FSY-36 判断，不能据此在 finance 再实现一套规则。

### 已执行验证及覆盖限制

在 `backend/` 执行：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-finance --lib --locked
exit code: 0
test result: ok. 521 passed; 0 failed; 13 ignored; 0 measured; 0 filtered out
```

共运行 534 个登记测试中的 521 个；13 个需要真实 MongoDB 的历史测试保持 ignored，未启用 `--include-ignored`。本轮未新增、修改或执行集成测试、examples、真实 MongoDB / S3 / 外部服务测试；lib 成功后未重复 cargo check，也未执行 workspace 或 server 检查。

与本事项直接相关且本轮通过的测试包括：

- `ports::funds_scope::tests::unwired_organization_reads_fail_closed`。
- `error::tests::authorization_errors_preserve_their_original_classification`。
- `repository::cost::read_scope::membership_tests::{id_only_existence_is_accepted_as_membership_and_rejects_amount_or_wrong_collection,allocation_membership_uses_cost_roots_without_accepting_other_resources,same_root_semijoin_keeps_share_or_whole_membership_before_outer_limit}`。
- `repository::receivable::invoice_request::tests::{scope_candidates_options_are_bounded_sorted_and_do_not_skip,scope_candidates_filters_keep_business_constraints_independent_of_paging}`。
- `repository::payable::payment_request::tests` 的三个筛选组合测试。
- `service::cost::tests::command_result_rejects_non_snapshot_executors_before_reading_cost_facts`。

这些测试覆盖实际模型检查、筛选 / 管道构造、错误转换与执行器拒绝，不等于真实数据库的权限过滤验证。FIN-AUTH-01 的失效凭证、FIN-AUTH-02 的两个实际入口模型拒绝均缺直接行为证据；允许 / 拒绝 / 缺 Facts / 缺绑定、非事务依赖修订变化、有效期和跨事务的完整公共 API 场景，本轮没有在 finance 中重演。identity 与其他 crate 测试源码只能用来设计后续验证，不能计入本轮通过数量。

### 开发实施顺序、影响范围与可执行验收

1. 先读取本报告对应的基准 SHA 与文档交付提交，保留两个问题编号。仅在 `F/repository/supplier_deposit_query.rs` 和 `F/repository/payable/payment_request.rs` 修复 FIN-AUTH-01、02；无须修改数据库、权限常量、资金计算或 Application 工厂。
2. 提取并使用两个私有目标模型校验函数；在 account_page 聚合前及成功读取后调用 `ensure_current`。保持现有分页上限、`size + 1`、稳定排序和授权前缀不变。
3. 只加内联库单测。模型矩阵直接执行上述生产校验函数；不得以源码字符串 / 方法名断言证明接入正确。原成本合法存在性和完整对象模型继续通过，份额不能冒充完整对象。
4. 为 FIN-AUTH-01 增加无需数据库的凭证行为回归：同阶段接受；换 executor、`next_read`、写入导致阶段推进、原阶段释放后拒绝；过期返回原冲突。需要断言拒绝发生在仓储 I/O 前。可以抽出实际生产管道构造 / 校验函数，让测试执行真实守卫和管道逻辑，避免为测试新增通用 Repository 接口层。
5. **测试设施实际限制**：finance 目前只有普通 identity 依赖；identity 现有公开 `test-support` 只有 unavailable / unbound 授权设施，不能签发合法 `CompiledAccess`，构造字段私有。若要在 finance 内证明上述真实凭证行为，应与 FSY-9 / FSY-39 核对后增加最小测试专用签发夹具，限定 feature，并由 identity 持有构造权或通过真实 query 签发；不得开放生产构造接口、放宽 Builder 或用自造布尔值替代凭证。该协作需求在父事项交接中记录；本轮没有修改共享代码。缺少这类证据时须明确保留未验证项，不能以模型单测冒充跨阶段验证。
6. 运行 `env -u ERP_TEST_MONGO_URI cargo test -p erp-finance --lib --locked`，记录准确结果。若确需改共享测试支持，仅对受影响的允许库测试补验，并在交接中列明命令与范围。保持本事项的 lib-only 限制，不以 backend 的一般全量门禁覆盖本任务特殊限制。
7. 提交可读取的实现版本，将修复编号、分支、完整 SHA、测试及未验证项写入本事项，按工作流交 `in_review`。独立审查必须读取实现的准确 SHA；如认为某项不构成缺陷，给出规范与生产调用证据交审查复核，不能静默删项。

| 验收编号 | 可验证结果 |
| --- | --- |
| AC-01 | `account_page` 使用失效边界时，在进入 `account_rows / aggregate` 前返回原身份校验错误；过期保留 `DATA_SCOPE_CHANGED` 冲突。 |
| AC-02 | `account_page` 成功读取后再次检查原凭证；调用方仍在完整展示补齐后执行原根统一 validate。 |
| AC-03 | 预存款只接受 `supplier_deposit / supplier_deposit_accounts / id / Object`；错误资源、集合、主键、Share、Existence 及非法描述均在 I/O 前拒绝。 |
| AC-04 | 付款申请只接受 `purchase_payment_request / purchase_payment_requests / id / Object`；其原前后边界检查保留。 |
| AC-05 | 模型测试与凭证测试执行生产校验 / 管道构造逻辑；不伪造集成通过，不开放生产凭证构造。未覆盖的真实数据库行为如实列出。 |
| AC-06 | 合法账户 / 申请路径仍使用原授权前缀、业务交集筛选、稳定排序和原候选 / 分页上限；成本合法 `Existence(id)` 会员规则不受影响。 |
| AC-07 | 未接线组织端口仍失败；显式空组织展开仍是空筛选；没有 catch-error 转公司范围或全量结果。 |
| AC-08 | finance 指定 lib 命令通过，13 个现有外部服务测试保持 ignored；不执行集成 / examples / 真实外部服务或冗余 cargo check。 |
| AC-09 | 提供实现准确 SHA 与问题逐项处置，由后端审查独立复验通过才置 done。 |

### 协作、兼容性与风险

- **本地业务修复所有者**：FSY-7；目标两个仓储文件及其内联测试。共享授权装配本轮仅核对，无须修改。
- **FSY-9 / FSY-39**：最小测试支持的潜在协作方；父事项已有 FSY8-03 的同类测试设施需求，应集中核对 FSY-9 的已有方案或提交，避免各 crate 重复添加签发夹具。测试辅助接口只能随专用 feature 暴露，不进入生产配置。
- **FSY-3 / FSY-8 已有协作**：已阅读 AUD-02 的错误类型机械适配和 FSY8-01 的共享执行链计划；本任务不修改相关 `errors.rs`、通用恢复合同或履约调用链，保留其交付。
- **FSY-36**：预存款 Mapping、Facts 与上游 Process 的所有者。本轮已核对其当前绑定和交付 validate，不并行修改这些文件。
- **FSY-38**：资金 / 成本读模型与固定 stages wrapper 的所有者；本轮不另行修改其政策、投影或汇总算法。若审查需要调整裸 stages 合同，先在父事项确定单一修改方。
- **FSY-28 / FSY-34**：财务过账、退款冲正调用方；本轮提供其授权前置证据，不接管同族事项。
- **兼容性**：建议修复不改公开业务方法签名、Mongo schema、索引、权限码、DTO 或金额语义，无数据迁移。旧错误模型 / 旧阶段调用会更早失败，这是合同要求；错误类别沿现有转换保留。
- **本轮限制**：完整生产装配成功、真实 Mongo 聚合成员 / 计数一致性、并发冲突和事务回滚未动态验证；server 工厂切换及全量编译归第六阶段。上述限制不等于失败关闭可以省略，也不等于已通过独立 Review。
