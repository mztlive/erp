# S3 核心链路实施及验收合同

后续候选改造：普通筛选候选的独立接口、候选自身 DataScope 及跨页面迁移按[列表筛选候选独立查询执行合同](query-selector-decoupling-contract.md)执行。本文随列表返回候选等记录保留 S3 交付基线，不构成后续实现要求；新专项完成度单独登记。

状态：已完成；达到 S3 退出条件（证据见第 3、6、10 章；未登记已验收，非正式上线）

实施日期：2026-09-13

修订日期：2026-09-17（本次只修正合同与代码漂移：S3-01—S3-08 功能／架构本地检查已闭合，阶段改为已完成；真实验收仍转上线准入跟踪。§7—§9 为历史时点，不得当作当前未完成依据）

完成度口径：S3 完成只要求 M03、M06—M12 功能与架构本地检查通过；真实库事务、真实权限、浏览器真实账号、代表性索引计划、业务验收统一转上线准入跟踪，未执行不阻塞 S3 功能完成登记。“已验收”与正式开放仍须单独核销。

上位合同：[组织架构、数据范围与负责人查询执行合同](organization-data-scope-contract.md) v1.3，第 5—12 章；公共解析接入必须执行第 9.2—9.5 节及 A33—A36。

同步基线：S2 以 `3c4ed851` 及 `docs/organization-data-scope-s2.md` §6.15 第 7 条、§8.3 为准（阶段退出已核销）。S3 功能／架构收尾提交为 `730a847c`。`s2_task_acceptance.rs` 已提交并按 S2 §6.11 运行，不得再登记为未提交未运行。

## 1. 交付边界

1. S3 阶段总范围为 M03、M06—M12，并核对 M01、M02、M04、M05 的 S2 接入结果。S2 依赖已按 S2 合同关闭；S3-01—S3-08 适用功能／架构退出条件已按第 3、6 章核销。S4 不纳入本阶段。不得把 S2 核销或任一读取子批次单独视为 S3 退出。
2. 第 2 章登记既有报表能力，第 5 章登记销售及成本读取接入。所有后续消费者必须复用 DataScope v2；销售列表接入不得作为客户、合同、采购、命令或任务已接入的证明。
3. 销售责任交接与客户验收联动已按 S3-07 独立实施并核销本地检查。正式开放销售责任改派仍须 A26 业务验收（转上线准入跟踪），不阻塞 S3「已完成」登记。
4. 正式业务准入继续执行上位合同第 10 章；本地代码检查、内存事实测试及浏览器样本验证不构成真实业务验收。真实业务验收转上线准入跟踪，不阻塞 S3 功能完成判定。
5. M11 独立销售／成本读取、客户端缓存及盈亏索引声明已按第 2、5 章与 S3-06 闭合本地项。数据库实际创建、代表性执行计划与真实权限验收转上线准入跟踪，不阻塞 S3 功能完成登记，也不得据此把 M11 登记为已验收。
6. M03 已按 S3-02 接入选品册显式销售责任、业务组织、方案责任关联、列表／详情／候选／命令 DataScope v2 与前端范围筛选；选品无独立导出入口，按不适用登记。不得再写「缺少选品责任」或仅凭人员条件宣称本功能组完成。
7. 第 2、5 章已实施能力的架构符合性按第 6 章核销，不自动视为已验收。客户整改沿用 S2-12 最新状态（跨域直连已关闭，A34 真实库等价、A36 解析层及四类资源业务验收已按 S2 §6.6—§6.15、§8.3 核销）。工作流／工作台授权基础已迁移到 `WorkflowAuthorizationPort` 及组合层 adapter（S2-05／S2-10／S2-13 已完成，静态零阻断见 S2 §6.15）。S3-04 业务功能已实施，不得再登记为“尚未迁移的旧读取器”或 S3-04 未完成。

## 2. M11 已实施能力

| 能力 | 实现入口 | 执行要求 |
| --- | --- | --- |
| 报表当前访问范围 | `erp-read-models/src/finance/actual_profit_loss/access.rs` | 事务内调用 S2 DataScope v2；同角色提供销售单查询和成本查询权限；当前销售责任、单据业务组织、有效客户协作及合法历史参与与个人上限求交 |
| 数据库授权过滤 | `erp-sales/src/repository/sales_order/{scope,profit_loss}.rs` | 固定字段表达角色并集和个人上限；空范围保持空集；显式客户条件不得覆盖授权；首次生效快照不提供访问权 |
| 历史归属事实 | `erp-read-models/src/finance/actual_profit_loss/{source,calculation,attribution}.rs` | 读取首次生效时冻结的人员、组织与祖先路径；不得按当前人员名称或组织树回填 |
| 历史筛选与分组 | `erp-read-models/src/finance/actual_profit_loss/{dto,query,projection}.rs` | 同字段 OR、不同字段 AND；人员按稳定 ID 分组，组织按冻结归属分组；组织筛选匹配冻结祖先路径；未知归属单列 |
| 金额与完整性 | 原 `calculation.rs` 及财务领域成本规则 | 保持非卡券、不含税、实际成本和冲减方向；成本不足不输出完整利润；新增筛选先于汇总、趋势、覆盖率和分页执行 |
| 候选与页面状态 | `erp-client/features/actual-profit-loss` | 候选来自完整有界授权订单集合；同名人员可区分；URL、查询键、草稿、已生效标签、清除和导出参数一致 |
| 查询版本 | `actual_profit_loss/{access,source,mod}.rs` | 版本覆盖身份授权、组织、有效协作集合及查询内销售单身份和版本；跨页必须提供版本；返回前在新事务重验当前范围与业务责任集合 |
| 同步 CSV | `actual_profit_loss/export.rs` | 导出全部匹配分组及历史归属列；保留覆盖缺口与公式注入防护；不使用客户端金额 |
| 历史分组下钻 | `actual_profit_loss/attribution.rs`、前端 `hooks/columns.tsx` | 已知与未知归属均使用直接归属精确匹配；保留期间及其他筛选，不把祖先路径筛选误作分组下钻 |
| 空结果与撤权恢复 | 报表读模型及前端页面 Hook | 区分无范围、无期间数据和筛选为空；查询、成本或导出撤权后撤下旧数据，清理本功能非活动缓存；导出与成本缓存绑定报表范围版本 |

1. 新增查询参数：`attribution_user_ids`、`attribution_org_unit_ids`、`attribution_group`、`scope_version`。人员和组织参数使用逗号分隔的稳定 ID，各最多 100 项；精确分组条件使用返回的历史分组行 ID。
2. 新增分组：`attribution_user`、`attribution_org`。组织分组按单据冻结的归属组织形成互斥分区，组织条件可包含当时的下级贡献。
3. 报表最多装载 10000 张授权销售单、100000 条成本分配。历史候选先于人员筛选生成；超过上限必须缩小期间或指定客户，不得截断汇总。
4. 报表返回前重验不提供事务快照建立后的墙钟级即时撤权保证；已下载到本地的文件不属于服务端可撤回对象。
5. 报表成本入口要求成本详情动作资格；详情接口必须独立解析成本与销售数据范围。部分读取只返回获授权的分配份额；整笔金额、税额及完整来源引用返回空值，执行第 5 章。
6. 销售单索引声明新增 `idx_sales_orders_profit_owner` 与 `idx_sales_orders_profit_org`，分别覆盖当前负责销售和业务组织的期间查询；沿用领域索引初始化入口。首次上线前必须实际创建并检查执行计划；本批未执行数据库建索引（转上线准入跟踪，不阻塞完成）。回退本批时仅移除这两个普通索引，不改动单据或历史快照。

## 3. 阶段项核销

以下为本次总交付范围的阶段项。S3-01—S3-08 本地检查均已闭合；真实验收转上线准入跟踪，不降低退出条件，也不再阻塞「已完成」登记。

| 编号 | 范围 | 必须完成 | 当前状态 |
| --- | --- | --- | --- |
| S3-01 | S2 必需依赖、M01／M02／M04／M05 | 统一范围消费者、业务组织筛选、详情／附件／打印、写命令和关联任务检查 | S2 依赖已关闭：S2-01—S2-13 适用项、A34 真实库等价、A36 解析层、任务层／审批管线、种子业务账号与浏览器真实账号均按 S2 §6.15、§8.3 核销（含已提交并运行的 `s2_task_acceptance`）。M04 全组本地约束已随 S3-04、S3-07 闭合；不得将 S2 核销单独视为 M04 全组或 S3 退出，也不得再把 S3-04／S3-07 登记为未实施 |
| S3-02 | M03 | 选品册显式销售责任与组织、方案责任关联、列表／汇总／详情／候选／导出和命令范围 | 已接入本地检查通过（选品册显式责任、方案继承、列表／详情／候选／命令 DataScope v2 与前端范围筛选；选品无独立导出入口，按不适用登记）。A33—A36 本地证据随 S3-08 核销；真实权限验收转上线准入跟踪，不阻塞本项功能完成，“已验收”单列 |
| S3-03 | M07／M08／M09 | 回款、付款、票据及正反分配；整单／余额授权依据；部分金额、完整凭证限制、经办人和处理人条件 | 已实施＋本地检查通过（M07 负责销售＋登记／核销经办、M08 负责销售／申请人／当前开票处理人、M09 采购负责人＋付款／收票经办分别查询；份额／未分配单列／部分授权 null；列表／详情／汇总／候选／导出／命令同对象映射与范围版本绑定；三范围页＋URL/QueryKey）。A34 内存等价与 A35 失败关闭见 `funds_data_scope::equivalence_tests`；A33—A36 本地证据随 S3-08 核销。A34 真实库对拍、A36 真实验收及浏览器真实账号转上线准入跟踪，不阻塞功能完成 |
| S3-04 | M06／M12 | 来源销售与采购、处理人查询、管理范围、受阻任务展示、独立改派候选和原有原子级联 | 已实施＋本地检查通过：工作台列表／统计新增 `handler_user_ids`、`sales_order_ids`、`purchase_order_ids` 收窄筛选（授权后内存求交、焦点复核、统计复用同一语义、进队列上下文与范围版本），W09 履约队列同事务快照＋`scope_version` 跨页锚定；管理范围沿 S2 `managed_task_owners` 当前负责人组织条件，受阻保留原负责人＋原因且统计排除可处理数，改派候选与采购级联沿既有专用资格／原子规则；详情同一事务 `detail_page`；OpenAPI 与前端 URL／QueryKey 已同步。A33—A36 本地证据随 S3-08 核销。待验证（转上线准入跟踪，不阻塞功能完成）：新增链路真实验收、真实库事务、浏览器写入 |
| S3-05 | M10 | 当前负责客户经营情况与历史订单贡献分离；列表、汇总、下钻与导出 | 本地检查通过（`org-scope/s3-quality`）：双口径独立路由／查询／导出（`customer-quality/current|history`），历史口径读冻结归属不回填现任；DataScope v2 同快照授权＋筛选、scope_version 跨页、CSV 版本绑定；前端双口径切换＋URL/Query key＋三分空态。A33—A36 本地证据随 S3-08 核销。真实权限验收、代表性索引计划转上线准入跟踪，不阻塞功能完成 |
| S3-06 | M11 剩余项 | 独立销售／成本详情权限、跨功能客户端范围缓存联动、代表性索引检查及真实权限验收 | 本地检查通过：成本详情独立解析 `cost_entry` 动作；盈亏第二页缺 `scope_version` 返回 409 `DATA_SCOPE_CHANGED`；交接 mutation 标注 `affectsDataScope`；盈亏索引已声明落地（`idx_sales_orders_profit_owner/org` 在 `erp-sales/src/indexes/sales_order.rs`，组合根 `web-api/cli` 只调 `erp_sales::indexes::ensure` 公开入口，`profit_owner_and_org_indexes_cover_effective_period_query` 内存断言通过）。代表性执行计划与 M11 真实权限验收转上线准入跟踪，不阻塞功能完成。不得将 S2 规模证据登记为 M11 真实验收 |
| S3-07 | 销售责任交接与验收联动 | 显式交接命令、目标资格、开放验收任务原子处理、新任务来源切换、审计幂等；审批与历史贡献保持 | 已实施＋本地检查通过：显式交接命令 `POST /admin/sales-orders/{id}/handover`（期望版本 CAS、目标账号有效加验收执行资格加岗位分离、显式业务组织留空保留、原因与幂等键必填，异载荷同键拒绝），同事务原子更新负责人与组织并转交全部开放验收任务（`sales_order.handover` 审计收据），开放审批任务、已完成验收与历史经营归属快照保持不变；验收新任务责任来源切换为 `sales_owner_user_id`（`ensure_task_identity` 负责人一致性门控，不一致路径拒绝）；候选 `GET …/handover-candidates` 只含合格有效人员；前端详情交接面列示单据摘要与开放验收随转预览（复用验收面 QueryKey，390px 可用）。待验证（转上线准入跟踪，不阻塞功能完成）：A26 业务验收、真实库事务、浏览器写入 |
| S3-08 | 既有及新增消费者架构接入 | 公共规则复用、业务条件等价、纯判定归属、Port／adapter、事务与资源准入登记；A33—A36 | 本地检查通过。九列登记见 §6.1；A33—A36 本地证据见 §6.2。销售内存等价 `erp-processes/src/adapters/scope_equivalence.rs`；选品／资金 adapter 等价与 FailClosed 见 `selection_data_scope`／`funds_data_scope` 的 `equivalence_tests`。S2 侧 A34 真实库等价（2436 组）与 A36 解析层只作复用引用，不得重复登记为 S3 新增验证。真实库对拍与业务验收转跟踪 |

## 4. 既有独立报表子批次验证记录

以下为 2026-09-13 独立 M11 批次历史记录，交付参考提交为 `a99b0782`；本章“本批”均指该批次。保留原结果及未执行边界，不得把本批计数换算为当前 HEAD 或当作 A33—A36 证据；当前 A33—A36 本地检查见 §6.2。

1. 本批 workspace 编译、全目标全特性 Clippy、格式、BPM 与领域边界检查通过；workspace lib 回归及后续定向复验结果按第 5 条登记。依赖 `proc-macro-error2 v2.0.1` 仍有既有未来兼容性警告。该结果仅核验本批改动，不核销 S3 其他功能组。
2. M11 单元测试必须覆盖同名身份、冻结组织路径、未知归属加总、筛选交集、候选非当前页、业务版本变更、空授权、个人上限、成本缺口及全量导出。
3. 前端必须完成类型检查、功能 lint 和本功能测试；浏览器验证必须标明是否使用样本数据。
4. 浏览器样本已验证：人员选择后待查询、应用后的 URL 与标签同步、刷新保持、清除条件、历史分组切换；390px 窄屏无页面水平溢出。补充验证已知／未知历史组织分组下钻及独立清除，原期间和组织条件保持。该验证使用生产组件及 URL 状态，未连接真实账号或数据库。
5. 本批执行 `env -u ERP_TEST_MONGO_URI cargo test --workspace --lib`：3798 通过、64 忽略、0 失败。随后补充的日期、精确分组和历史参与逻辑已通过盈亏 17 项及销售范围 3 项定向复验；该记录不得换算为新的全量测试计数。
6. 前端类型检查、功能 lint 通过；4 个测试文件共 15 项通过，覆盖下钻条件、撤权后清除旧数据、范围变化关闭明细、重新授权恢复和导出版本绑定。
7. 真实 MongoDB 事务、HTTP 授权、并发、初始化重跑、索引执行计划及 A11—A16、A20、A22—A26、A29 的业务验收转上线准入跟踪，不阻塞本批功能完成。

## 5. 销售及成本读取接入合同

### 5.1 服务端读取

| 范围 | 实现入口 | 必须执行 |
| --- | --- | --- |
| 共享销售读取 | `erp-read-models/src/sales_center/access.rs` | 同角色动作证明；当前销售责任、业务组织、有效协作及读取参与与个人上限求交；历史归属快照不提供访问资格 |
| 销售列表和候选 | `sales_center/order/{query,scope}.rs`、`erp-sales/src/repository/sales_order/{order,scope}.rs` | 列表、总数、候选及业务身份版本同事务读取；完整可见范围内形成负责人候选；返回前新事务复核 |
| 销售详情 | `sales_center/order/query.rs` | HTTP 读取必须提供认证操作人；读取前和返回前各自检查当前详情动作、对象范围与单据版本；内部写命令回读不替代命令自己的授权 |
| 销售主单命令 | `erp-processes/src/order_to_cash/{authorization,command/*,draft_working_copy,start_approval,cancel_approval}.rs` | 创建、保存、提交、撤回、作废按各自动作解析范围；原写入事务重验当前责任及版本；创建并提交分别检查两个动作范围且要求同一角色提供完整权限；幂等重放重验当前操作资格 |
| 成本及分配 | `erp-read-models/src/finance/cost{.rs,/snapshot.rs}`、`erp-finance/src/dto/cost_scope.rs` | 目标成本动作和销售读取动作由同一合格角色提供；成本资源范围与销售对象范围交集；全量裁剪后计算总数、排序和分页 |
| 盈亏收入与成本 | `finance/actual_profit_loss/access.rs` | 销售与成本查询的数据范围独立解析后求交；成本个人上限不得被销售 Company 范围绕过 |
| 初始化 | `erp-identity/src/service/iam/predefined_data_scopes.rs` | 增加 `cost_entry:list/detail` 与 `cost_allocation:list` 显式资源清单；沿用原岗位规则及撤销留痕，不恢复管理员已撤销的范围 |

1. 销售列表、成本列表和成本分配列表第二页起必须携带 `scope_version`；缺失或变化返回 `DATA_SCOPE_CHANGED` 冲突。版本包含资源授权、组织及当前查询业务版本；不返回内部授权集合。
2. 销售查询最多 10000 张匹配订单，成本读取最多 10000 笔候选成本、100000 条分配；超限必须整体拒绝并收窄条件，不得截断统计或导出。当前成本列表使用有界候选装载；大数据量查询计划仍须上线前验收。
3. 整笔成本读取要求成本资源和关联销售资源均具有未被个人上限收窄的 Company 范围。其他范围只提供获授权分配；整笔含税、不含税、税额及完整来源引用返回 `null`，不得使用零值或差额替代。
4. `scope_gross_amount`、`scope_net_amount` 只合计当前返回的分配事实；预计、确认、实际和冲减阶段保持原事实方向，不因授权裁剪改变金额。未分配部分不得由可见分配推导获得。
5. 部分成本读取不接受以完整来源字段探测隐藏单据；带来源单据条件时必须返回校验错误，不以空列表掩盖不适用组合。成本列表金额排序使用授权分配合计，不按隐藏整笔金额排序。
6. 销售主单的历史参与仅用于 `list/detail`，不得用于任何写动作。客户独立接口的功能接入不得作为销售建单所选客户／合同依赖已经完整接入的证明；必须单独核对跨域调用链、对象范围和原事务重验。S2-12 已按 S2 核销；责任交接已按 S3-07 核销本地检查。
7. 销售 CSV 必须绑定首个响应的范围版本，最后一页之后、下载之前再次查询校验；校验失败不得输出部分文件。下载后的本地文件不属于可撤回对象。
8. 增量接口合同：[销售及成本范围读取接口合同](organization-data-scope-s3-openapi.yaml)。销售组织筛选、关联客户与合同范围、变更单及附件／独立打印消费者已接入；S2-10 已按 S2 核销。M04 全组本地约束已随 S3-04／S3-07 闭合，不得把本读取批次单独视为 M04 全组或 S3 退出。

### 5.2 客户端范围变化

1. 不可见详情返回 404 时必须同时撤下旧详情缓存，不得保留先前成功响应。`features/data-scope/cache.ts` 必须统一处理账号权限、组织版本、同查询业务范围版本变化和服务器撤权错误；撤下旧数据、取消未完成请求并清理其他功能缓存，防止旧响应重新写回；已观察到较新权限或组织版本后，拒绝较旧版本响应。
2. 账号资料返回 `policy_version`、`organization_version`；页面聚焦和会话轮询刷新当前资料。轮询不是服务端即时推送，不承诺客户端在下一次通信前感知远端撤权。
3. 账号权限变更、客户归属变更、任务改派及销售责任交接的成功回调必须标注 `affectsDataScope`，触发跨功能失效；普通只读导出不触发全站失效。S3-04 履约／工作台业务功能已按第 3 章核销，不得用本缓存条款回退其状态。
4. 销售列表必须区分无范围、筛选无结果与请求失败。无范围不得提示通过创建第一张销售单解决；成本部分详情必须明确显示权限限制及可见分配金额。

### 5.3 销售及成本接入批次历史验证

以下测试与浏览器结果属于 2026-09-13 销售及成本接入批次，交付参考提交为 `59ce038a`；本节“本批”指该批次。不得沿用本节历史通过结论作为当前 HEAD 计数；当前阶段状态以第 3、6、10 章为准。

1. workspace 编译、全目标全特性 Clippy、BPM／领域边界检查通过；`env -u ERP_TEST_MONGO_URI cargo test --workspace --lib` 为 3811 通过、64 忽略、0 失败；最终成本分页及历史参与调整后，`erp-read-models --lib` 为 203 通过、35 忽略、0 失败。末次定向结果不得换算为新的全量计数。静态、单元及浏览器样本只证明对应层级，不构成数据库或生产验收。
2. 前端类型检查及功能 lint 通过；30 个测试文件共 98 项通过，覆盖销售、盈亏与跨功能缓存。浏览器样本验证销售跨页携带版本、无范围时清除旧列表，以及成本只显示获授权的 60 元分配、整笔金额与完整来源受限；390px 页面无水平溢出。浏览器样本未连接真实账号或数据库。
3. 真实 MongoDB 事务、HTTP 账号矩阵、并发撤权、首次初始化重跑、代表性索引计划和正式业务验收转上线准入跟踪，不阻塞本批功能完成。
4. 本节时点（`59ce038a`）S3-02、S3-03、S3-04、S3-05、S3-07 尚未实施。该句只描述当时批次边界；上述项及 S3-06／S3-08 已按第 3、6 章核销本地检查，不得再当作当前未完成依据。

## 6. 公共解析接入与架构验收

### 6.1 消费方登记与责任边界

本表以 `3c4ed851` 及 S2 §6.15、§8.3 为 S2 静态登记基线（替代原 `d70eb6ca`／`3988ae56`）；S3 新增消费方以 `730a847c` 及下方九列表为准。完整资源动作登记沿用主合同第 9.5 节与 [S2 接入登记](organization-data-scope-s2.md)。实现入口说明代码位置；架构本地检查按 §6.2 核销，真实验收转跟踪。

| 消费链路 | 当前入口或状态 | 必须执行的架构要求 |
| --- | --- | --- |
| 销售列表／详情及主单命令 | `erp-read-models/src/sales_center/access.rs`；`erp-processes/src/order_to_cash/authorization.rs` | 命名组合用例可调用身份域公共解析；销售对象事实与条件映射必须唯一复用；按每个入口动作传递权限、执行器和版本；不得让销售域依赖组合层 |
| 销售数据库条件与创建判定 | `erp-sales/src/repository/sales_order/scope.rs` | 数据库条件编译可保留在 Repository；生产单对象判定已复用公共入口（`SalesAccess::allows` → `ResolvedScope::allows`，`erp-read-models/src/sales_center/access.rs`），`allows_creation` 仅为 `#[cfg(test)]`，不进入生产授权；四类资源 A34／A36 及 S2 业务验收已按 S2 §6.6—§6.15、§8.3 核销；S3 销售内存等价见 `scope_equivalence.rs`，新增入口按 §6.2 核销本地检查 |
| 成本分配与实际盈亏 | `erp-read-models/src/finance/cost/snapshot.rs`、`finance/actual_profit_loss/access.rs` | 分别解析销售与成本资源动作后求交，保留同角色完整权限及个人上限；金额裁剪继续由业务规则处理，不放入身份模型；不得复制 SalesAccess 或形成另一套授权默认值 |
| 客户／合同／采购依赖 | 客户、合同、采购均已建立本域 Port 并由 `erp-processes` 生产 adapter 调用 `DataScopeService`（S2 §4.1、§4.3）；A34 真实库等价已按 S2 §6.6 执行（客户 492、合同 486、采购 486 组），A36 解析层已按 S2 §6.7 执行，S2 业务验收已按 §6.14／§6.15、§8.3 核销 | 客户整改以 S2 登记为准；后续业务 Service 通过本域 Port，生产 adapter 在 erp-processes；S3 关联读取与命令已按各功能组核销本地检查，不以独立接口接入替代真实验收 |
| 选品册、票款、履约、工作台、客户经营质量等剩余链路 | S3-08 已按主合同第 9.5 节补九列登记（见下表）；旧读取器残留已收敛：工作台详情／审批跟踪展示读取沿用调用方同一执行器（`query.rs detail_page`、`access.rs` 删除 `NoTransaction` 快照入口、`party_names.rs` 删除展示层独立执行器、`approval_list.rs` 复用调用方执行器），未发现 `scope_type/scope_targets` 再解析或 Company 回退 | 先登记事实来源、资源动作及必需维度，再接入公共解析；工作流 Port 已按 S2 §8.1 返回已解析事实，生产消费者不得读取原始范围（静态零阻断，S2 §6.10）；当前责任与历史归属各守其口径 |

#### S3-08 新增消费方登记（主合同 §9.5 第 1 条九列；基线 `730a847c`，登记稿自 `c842b4e8`）

| 资源动作 | 拥有领域 | 公共解析入口 | Port／adapter | 适用与必需维度 | 历史参与允许动作 | 对象事实来源 | 条件编译入口 | HTTP／CLI 入口 | 状态 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `sales_selection_booklet:list/get/create/maintain/prepare/publish/copy_link/rotate_link/close/revoke/void` | erp-sales | `DataScopeService::resolve` | `erp-sales::ports::SelectionDataScopePort`／`erp-processes::adapters::selection_data_scope::MongoSelectionDataScope` | 适用 InternalOrg；必需 InternalOrg（结算主体／仓库出现即拒绝） | 不允许（adapter 固定 `historical_read_participant` 输入，册事实不提供参与） | 册显式 `sales_owner_user_id`＋`business_org_unit_id`（`SelectionAccess::resolve`／`require_booklet`） | `erp-sales::repository::sales_selection::scope::SelectionReadScope::document`；等价证据 `selection_data_scope::equivalence_tests` | 选品 HTTP（`selection_access` 装配） | 本地检查通过（A33—A36 本地证据见 §6.2；真实验收转跟踪） |
| `sales_selection_proposal:list/get` | erp-sales | `DataScopeService::resolve` | 同上（`ensure_selection_resource` 限定册／方案） | 同上 | 不允许 | 方案沿所属册责任关联（`require_proposal`，不得以客户提交人兜底） | 同上 | 同上 | 本地检查通过（真实验收转跟踪） |
| `receivable_account:list/detail`、`customer_receipt:list/detail`、`invoice:list/detail`、`sales_invoice_request:list/detail` | erp-finance | `DataScopeService::resolve`（资金资源动作）＋同角色 `sales_order:list` | `erp-finance::ports::funds_scope::FundsDataScopePort`／`erp-processes::adapters::funds_data_scope::MongoFundsDataScope`，对象映射唯一归 `erp-read-models::finance::funds_scope::FundsAccess::allows` | 适用 InternalOrg；必需 InternalOrg（结算主体／仓库出现即拒绝）；关联销售 `sales_order:list` 同角色证明后求交 | 不允许（`allows_history=false`；附属单行仅关联销售／采购责任判定） | 关联销售单当前 `owner_user_id`＋`business_org_unit_id`、经办人（`FundsLinkedFacts`；`collaborating` 按协作客户事实） | `SalesAccess::authorized_sales_ids`＋`SalesReadScope::document`；等价证据 `funds_data_scope::equivalence_tests`（本批新增） | 票款 HTTP（`funds_access_with_rbac` 装配） | 本地检查通过（A33—A36 本地证据见 §6.2；真实验收转跟踪） |
| `payable_account:list/detail`、`supplier_payment:list/detail` | erp-finance | 同上，关联采购 `purchase_order:list` 由 `PurchaseAccess` 同角色证明 | 同上（`FundsAccess::resolve_with_purchase`，`allows_purchase` 复用 `purchase_order` 登记） | 适用 InternalOrg；必需 InternalOrg；关联采购范围求交 | 不允许 | 关联采购单当前负责人＋业务组织（`FundsLinkedFacts`，协作不映射为采购对象） | `FundsAccess::authorized_purchase_ids`＋`PurchaseReadScope::document` | 同上 | 本地检查通过（真实验收转跟踪） |
| `purchase_invoice_allocation:list` | erp-finance | 同上（资金资源动作） | 同上 | 适用 InternalOrg；必需 InternalOrg | 不允许 | 关联单据责任事实（同上） | 同上 | 同上 | 本地检查通过（真实验收转跟踪） |
| `sales_order:handover`（显式交接命令） | erp-sales（命令）＋erp-processes 编排 | `SalesCommandAccess::current/revalidate`（`SalesAccess::require_object`→公共解析，同一事务执行器） | 复用销售 Port／`order_to_cash::authorization`（无新增 Port；`handover.rs apply_handover`） | 必需 InternalOrg（销售 `update` 动作登记） | 不允许（历史参与不授予修改；交接前后均重验） | 当前单据 `sales_owner_user_id`＋`business_org_unit_id`＋版本；目标资格按验收完整执行权限 | 复用 `SalesReadScope::document`（对象级 `SalesAccess::allows`＋版本 CAS） | `POST /admin/sales-orders/{id}/handover`、`GET …/handover-candidates` | 已实施＋本地检查通过（A26 业务验收、真实库事务转跟踪） |
| `work_item:list/detail/stats`（工作台管理查询） | erp-workflow（任务）＋erp-read-models 投影 | `WorkflowAuthorizationPort::queue_scope_version`＋`managed_task_owners`（已解析事实；`workbench/query.rs queue_page` 同一事务） | `erp-workflow::ports::authorization::WorkflowAuthorizationPort`／`erp-processes::adapters::workflow::authorization::WorkflowAuth` | 适用 InternalOrg（`work_item:manage` 登记）；个人上限与正向范围求交，无正向范围禁回退公司 | 已办按实际处理人（`has_personal_history_access`），不构成修改权 | 当前 `owner_user_id`＋业务对象事实（`filter_order_access` 复用命令端对象判定；`owner_organization_id` 保留责任语义） | 仓储 `WorkItemFilter`（`managed_owner_ids`＋对象形状）＋授权后内存求交（`matches_handler/order_sources` 只收窄）；详情同一事务 `detail_page` | 工作台 HTTP（列表／详情／统计） | 本地检查通过（本批收敛详情展示层执行器；真实库事务、浏览器写入转跟踪） |
| 履约队列（`FULFILLMENT_OPERATION` 个人责任队列） | erp-fulfillment（作业）＋erp-read-models 投影 | `queue_scope_version`＋`actor_access_for`（同一事务；`fulfillment_queue.rs fulfillment_queue_page`） | 同上 Port／adapter | 个人责任队列（`owner_user_id`＝本人；管理查询走工作台管理范围） | 不适用（个人责任＋对象范围双重过滤） | 当前处理人＋履约责任事实（来源销售／采购单 ID 只收窄） | `FulfillmentQueueFilter` 服务端聚合＋`scope_version` 跨页锚定 | 履约队列 HTTP | 已实施＋本地检查通过（真实库事务转跟踪） |
| 客户经营质量双口径（`customer-quality/current\|history`） | erp-read-models 投影（现任客户＋销售事实） | 现任口径 `CustomerAccess::resolve(list)`＋`SalesAccess::resolve(list)`；历史口径 `SalesAccess::resolve(list)`（`customer_quality/access.rs QualityAccess`，同一执行器） | 复用客户 Port／`MongoCustomerDataScope`＋销售公共入口（无新增 Port） | 适用 InternalOrg；必需 InternalOrg；现任口径客户与销售双重求交 | 现任口径不补充（`no_current_scope`）；历史口径沿销售 `list` 登记 | 现任负责人所属组织（客户）＋单据业务组织（销售）；冻结归属仅分组不授权 | 复用 `CustomerReadScope::document`＋`SalesReadScope::document` | 客户经营质量 HTTP（双口径） | 本地检查通过（真实权限验收、代表性索引计划转跟踪） |

### 6.2 实施顺序与验收证据

1. 先对当前批次核对 S2-12、S2-13 的适用依赖及资源动作登记，确定 Port／adapter 和唯一对象映射；未满足依赖的消费者不得开放配置或正式业务，不得扩大到 S4 以规避依赖缺口。
2. 既有销售／成本链路已按 S3-08 复核公共判定归属、条件等价、事务和动作边界；新增链路已按主合同第 9.5 节实施并移除自身旧解释。工作台已按 S3-04 接入，状态见第 3 章。
3. 代码接入批次须从 `backend/` 执行领域边界门禁及仓库规定的功能检查，并审查生产调用链。禁止跨业务域 normal／build／dev 依赖、adapter 丢弃维度或版本、内部另开事务以及缺范围补全量；门禁漏检必须补齐，不得增加例外。本同步只改合同，不重跑门禁，不把 `730a847c` 当时计数换算为当前 HEAD 新计数。

| 验收项 | S3 必须提供的证据 | 当前登记 |
| --- | --- | --- |
| A33 | 本域 Port、组合层 adapter、实际公共入口及依赖图；违规依赖拒绝夹具与调用链检查结果 | 本地检查通过。基线 `730a847c`。S3 新增链路：`SelectionDataScopePort`／`MongoSelectionDataScope`，`FundsDataScopePort`／`MongoFundsDataScope`，交接复用销售 Port，工作台／履约复用 `WorkflowAuthorizationPort`，经营质量复用客户 Port＋`SalesAccess`。生产 adapter 调用 `DataScopeService::resolve`，业务域不依赖身份域。S2 侧领域边界／BPM／跨域直连关闭可复用。调用链审查日志 `arch.log` 未入库，不以缺失日志否定源码登记。真实验收转跟踪 |
| A34 | 公共单对象范围判定与数据库条件编译对相同动作、对象事实、业务边界及筛选的集合等价证据，覆盖多角色、空范围、上限和历史参与 | 本地检查通过（内存集合等价，不得登记为真实库执行等价）。选品 `selection_data_scope::equivalence_tests`；资金 `funds_data_scope::equivalence_tests::public_object_decision_matches_compiled_conditions`；销售 `erp-processes/src/adapters/scope_equivalence.rs`。S2 四类资源真实库 2436 组只作复用引用，不得登记为 S3 新增验证。真实库对拍转跟踪 |
| A35 | 未接入资源配置／初始化拒绝、未装配 Port 失败关闭及旧消费者不解释 v2 的验证 | 本地检查通过。`consumers::WIRED_CONSUMERS` 已含选品／资金／成本；未接线动作由 `wired_s3_actions_admit_reads_unwired_actions_fail_closed`、`wired_funds_resources_admit_reads_without_history_or_commands` 拒绝。`FailClosedSelectionDataScopePort`、`FailClosedFundsDataScopePort` 失败关闭。工作台删除 `NoTransaction` 快照入口，详情沿用调用方执行器。S2 初始化拒绝与静态零阻断可复用。真实验收转跟踪 |
| A36 | 列表、详情、候选、汇总、导出和命令复用对象映射，各自动作、事务、版本及撤权重验的覆盖记录 | 本地检查通过。九列表覆盖适用入口；选品无独立导出按不适用。工作台详情 `detail_page` 同一执行器；盈亏／工作台／履约／经营质量跨页绑定 `scope_version`；交接 mutation 标注 `affectsDataScope`。S2 侧解析层与四类资源入口覆盖可复用。真实账号矩阵、浏览器写入与撤权下载转跟踪 |

4. 功能与架构记录必须分别注明代码基线、命令、结果及未执行项；全部适用功能／架构检查满足后才能登记相应批次本地检查通过，真实业务验收继续单列为上线准入跟踪，未执行不阻塞本地检查通过。历史测试计数不得累计为当前计数。
5. 本文件第 10 章关闭 S3 阶段退出的文档漂移：S3-01—S3-08 适用功能／架构退出条件已核销，状态为「已完成」。本同步不执行代码构建、测试或真实业务验收，不新增测试计数，不登记已验收。真实验收转跟踪，不计入退出条件。

## 7. 本次同步说明（2026-09-15）

1. 本节仅修正 S3 与 S2 现状的表述漂移，不降低主合同第 11 章及本文件第 3、6 章的退出条件，不新增任何通过结论。
2. 漂移修正清单：§1.7 工作流／工作台“仍为待接入项”改为授权基础已迁移、剩余任务层与业务验收；§3 S3-04 “尚未迁移的旧读取器”改为 S2 基础已迁移、S3 业务功能未实施；§3 S3-01 补 S2-12 最新状态与 `s2_task_acceptance.rs` 未提交未运行；§3 S3-06、S3-08 补 S2 §6.6—§6.10 可复用证据边界；§6.1 基线由 `3988ae56` 更新为 `d70eb6ca`，销售创建判定、客户／合同／采购、工作流三行按代码与 S2 登记重写；§6.2 A33—A36 按 S2 §6.6、§6.7、§6.3、§6.10 重写为“未核销＋可复用部分”。
3. 本节时点仍未核销项：S3-02、S3-03、S3-04、S3-05、S3-07 未实施；S3-01 任务依赖、S3-06 代表性计划与真实权限、S3-08 A33—A36 仍未核销；浏览器真实账号与现有业务账号验收仍未执行。该清单只描述 2026-09-15 时点；当前状态以第 10 章为准，不得沿用本节时点的未完成结论。

## 8. 本次同步说明（2026-09-16）

1. 本节仅修正 S3 与 S2「已完成」现状的表述漂移，不降低主合同第 11 章及本文件第 3、6 章的退出条件，不新增任何 S3 通过结论。
2. 漂移修正清单：同步基线由 `d70eb6ca` 更新为 `3c4ed851`；§1.1／§1.7／S3-01 改为 S2 依赖已关闭，不再把 S2-05／S2-10／S2-13、`s2_task_acceptance.rs` 未运行或四类资源 A36 入口覆盖写成未完成；S3-06 把已核销的审批全管线／超限／代表性业务数据计划改回 M11 盈亏索引与真实权限剩余项；§5.1.6／§5.1.8／§5.2.3／§5.3.4 去掉「任务命令仍归 S2-10」「S2-12 缺口未关」；§6.1／§6.2 A33—A36 的 S2 可复用边界补 §6.14／§6.15、§8.3。
3. 仍未核销项（本节时点口径，当时把真实验收计入退出）：S3-02、S3-03、S3-04、S3-05、S3-07 未实施；S3-01 的 S2 依赖已关闭，但 M04 全组完成仍受 S3-04／S3-07 约束；S3-06 剩余 M11 盈亏索引落地与真实权限验收；S3-08 A33—A36 对 S3 新增链路仍未核销。该句只描述 2026-09-16 上午时点；当前状态以第 10 章为准。

## 9. 完成度口径调整说明（2026-09-16，主合同 v1.3）

1. 本节按主合同 v1.3 解耦真实验收，不降低功能／架构要求。§7—§8 中的“仍未核销／未实施”包含当时真实验收口径，仅描述该节时点。
2. 真实验收（真实库事务、真实权限、浏览器真实账号、代表性索引计划、A26／业务验收）统一转上线准入跟踪，未执行不阻塞 S3 功能完成登记。该口径继续有效。
3. 本节时点曾以 S3-08 待复核、S3-06 剩余功能项、M04 受 S3-04／S3-07 约束为由保持“执行中”。上述功能／架构项已闭合；当前完成判定以第 10 章为准，不得沿用本节“执行中”结论。

## 10. 本次同步说明（2026-09-17，关闭文档漂移）

1. 本节只修正合同与代码漂移，不新写功能、不重跑门禁、不登记已验收、不开放正式业务。功能／架构收尾提交为 `730a847c`；本同步工作区 HEAD 为 `0d882bce`（其后为非 S3 范围改动，不回退本阶段完成判定）。
2. 漂移原因：`730a847c` 已实施 S3-04／S3-06／S3-07／S3-08 并补九列登记，但未改文首状态、第 3 章 S3-08 总状态、§6.2 A33—A36 证据表，也未删除 §1.6／§1.7／§5.3.4／§9.3 的旧“未实施／执行中”表述。
3. 本次改动：S3-01—S3-08 一律按第 3 章登记本地检查通过；§6.2 A33—A36 改为本地检查通过并写明测试入口与未执行项；主合同第 11 章与 S2 交叉引用同步。§7—§9 保留为历史时点。
4. 未执行（转上线准入跟踪，不阻塞已完成）：真实库事务与 A34 真实 `find` 对拍、真实权限／浏览器真实账号、代表性索引执行计划、A26 业务验收、调用链审查日志入库。`730a847c` 提交说明中的 `arch.log`／`gates.log` 未进入仓库，不以缺失文件否定源码登记，也不得把当时测试计数换算为当前计数。
5. 正式业务开放仍须满足上位合同第 10 章。S4 未开始；未接入资源不得将 v2 配置作为已生效授权。
