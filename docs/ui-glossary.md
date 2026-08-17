# UI 文案术语表

> **状态**：基线 v1.4
> **依据**：`erp-client` 全量扫描（2026-08-03）+ W21 供应商供给模型（2026-08-08）+ `approval-workflow-contract.md`（2026-08-13）；`erp-ui-design.md` 的语言原则
> **适用范围**：所有用户可见文案——按钮、状态条、提示、Alert、描述、列名、空态、toast、错误恢复指引
> **不适用**：代码注释、数据模型字段名、内部错误对象 key、文档中的架构描述
> **验收挂钩**：W 系列工作面文档 §12「页面文案不出现实现术语」

## 1. 目的与原则

### 1.1 目的

本系统围绕内部工作流架构构建（work item、任务责任、审批步骤、动作命令、事实、对象版本）。这些概念**只允许**出现在代码与文档中；用户界面**必须**翻译成业务语言。

### 1.2 判断原则

| 条件 | 必须执行 |
| --- | --- |
| 该词用户（采购/财务/运营）在职场上不会这么说 | **必须**替换为业务说法 |
| 去掉/换词后业务含义会丢失 | **必须**保留业务含义并改用业务说法；否则**必须**删掉 |
| 该词解释的是「系统内部怎么实现」而非「业务发生了什么」 | **必须**替换 |

### 1.3 通用规则

1. **按钮说动作，禁止说机制**：按钮文案**必须**是业务动作（如「去确认采购计划」），**禁止**「打开专用处理器」等机制词。
2. **状态说结果，禁止说锁**：状态文案**必须**是处理结果（如「正在处理中」），**禁止**「租约 v1 有效」等锁/租约说法。
3. **错误说下一步，禁止说原理**：错误文案**必须**给出可执行下一步（如「请刷新后重新处理」），**禁止**「幂等键冲突」等原理词。
4. **「正式」默认删除**：仅当与「草稿/预览」形成必要对比时方可保留对比语义；否则「正式提交」**必须**写为「提交」，「正式」字样**必须**删除。
5. **内部词禁止上屏**：work_item、领取/重新领取、动作命令、审批步骤实例、对象版本、幂等键、水位等实现词**禁止**进入任何用户可见字符串；租约、令牌、信封、指纹等分布式协议概念**禁止**出现在用户界面。
6. **禁止把工作面编号当导航**：用户提示**必须**写页面中文名（如「客户往来」「接口错误中心」），**禁止**写「W11」「W29」。
7. **禁止把命令名/字段名当文案**：**禁止** `Complete*Command`、`subject_hash`、`mappingTaskStatus`、`fail-closed` 等进入界面。

---

## 2. 禁用词表（用户界面禁止出现）

> 严重度：🔴 必须替换（P0，按钮与主操作）／🟠 必须替换（P1，状态与提示）／🟡 必须替换（P2，长尾）

### P0 · 按钮 / 主操作

| 禁用词 | 出现位置 | 替换为 |
| --- | --- | --- |
| 打开专用处理器 / 打开业务对象 | `unified-task-queue-page.tsx` 主按钮 | **必须**按任务类型使用动作词（见 §3.1）；未登记类型**必须**用兜底「前往处理」 |
| 正式处理 / 正式完成 / 正式确认 | `queue-workspace-page.tsx:439`、`unified-task-queue-page.tsx:1175/1456/1597`、`mock/work-items.ts:236/489` | 处理 / 完成 / 确认（去掉"正式"） |
| 提交正式动作 | `procurement-rejection-card.tsx:537` | 提交处理结果 |
| Complete*Command / *Envelope（用户可见） | 确认对话框 description | 将提交「…」结论（业务说法） |
| work_item / work_item_type（用户可见） | 结算、权限审计、导入阻断 | 任务 / （类型码不出现） |
| 任务信封 | 结算确认文案 | （不出现）→ 提交处理结果 |

### P1 · 状态与提示条

| 禁用词 | 出现位置 | 替换为 |
| --- | --- | --- |
| 处理租约有效 · v1 | `workflow.tsx:355`、`queue-workspace-page.tsx:330`、`unified-task-queue-page.tsx:1167` | 你正在处理这一条 |
| 处理租约续期中 | `workflow.tsx:360` | （删除该提示） |
| 处理租约已丢失 | `workflow.tsx:365`、`unified-task-queue-page.tsx:1171` | 处理权已变化，请刷新 |
| 处理租约已释放 | `workflow.tsx:370` | 已退回团队 |
| 租约令牌已清除 / 令牌已清除 | `unified-task-queue-page.tsx:373/982/1165`、`session-state.ts:455` | （删除该提示） |
| 租约令牌仅存于当前会话内存 | `unified-task-queue-page.tsx:1211` | （删除该提示） |
| 领取后取得租约方可正式处理 | `unified-task-queue-page.tsx:1252` | 开始处理后即可提交 |
| 角色池任务待领取 | `unified-task-queue-page.tsx:1250` | 团队待处理 |
| 角色池待领取 | `fulfillment-operations/api.ts:123`、`card-funds-review/api.ts:194`、`procurement-confirmation/api.ts:176`、`mock/workspace.ts:592` | 团队待处理 |
| 无法取得编辑租约 / 正在领取编辑租约 | `purchase-order-detail-page.tsx:198/1391` | 无法进入编辑 / 正在进入编辑 |
| 编辑租约有效 · lockVersion | `purchase-order-detail-page.tsx:1398` | 正在编辑中 |
| 任务仍在有效队列（PENDING） | `integration-errors/api.ts:799` | 任务仍在待处理列表，可稍后继续 |
| 租约无效，请重新领取 | `fulfillment-operations/api.ts:944`、`procurement-confirmation/api.ts:322` | 处理权已变化，请刷新 |
| fail-closed（用户可见） | 商品发布、权限审计等 | 结果未确认前禁止… / 按保守策略拒绝 |
| subject_hash / 当前 subject_hash（UI 标签） | 卡券复核 | 数据版本 |
| supplier_offering_availability / expected_revision_no（用户可见） | 供应商供给 | 当前可供情况 / 当前条款版本 |
| Wxx 工作面编号（面向业务用户的提示） | 各页 mock / api message | 页面中文名（客户往来、接口错误中心…） |

### P2 · 长尾（批量替换）

| 禁用词 | 替换为 |
| --- | --- |
| 正式结果未知 / 正式结果仍未知 / 正式结果不确定 | 处理结果待确认 |
| 正式结果已记录 | 处理结果已记录 |
| 正式提交 | 提交 |
| 正式结果 | 处理结果 / 系统结果 |
| 正式状态 | 当前状态 |
| 正式确认（映射） | 确认映射 |
| 正式操作已完成 / 未通过 / 被阻断 / 正在处理 | 操作已完成 / 未通过 / 被阻断 / 正在处理 |
| 正式决定正在提交 | 决定正在提交 |
| 正式待办 / 正式终态 / 正式水位 | 待办 / 处理结果已确认 / 数据更新时间 |
| 正式过账（非与草稿对比时） | 过账 / 确认入账 |
| 执行投影 / 销售单执行投影 | 执行信息 / 执行摘要 |
| 服务端投影 / 服务端执行投影摘要 | 系统最新数据 |
| 投影已陈旧 / 投影已更新 / 投影重建中 / 投影失败 / 投影重建失败 | 数据已更新 / 数据可能不是最新 / 数据更新中 / 数据加载失败 |
| 工作台投影 / 经营质量投影 / 分析投影 | 工作台汇总 / 经营质量汇总 / 分析汇总 |
| 投影投递 / 投影修订 / 投影版本 | 已发送到商城 / 数据版本 / 版本 |
| 此单据为系统正式事实的打印投影 | 此单据为系统数据的打印件 |
| 合同条款投影 / 纸质投影 / 履约进度投影 | 合同条款 / 打印件 / 履约进度 |
| 回款事实复核 / 事实追溯 / 关键事实 | 回款核对 / 来源追溯 / 关键记录 |
| 按幂等键查询最终结果 / 使用原幂等键重试 / 原幂等键 | 按原任务号查询 / 使用原任务号重试 / 原任务号 |
| 幂等查询确认成功 / 幂等查询已得结果 | 已查到原任务处理结果 / 已查到处理结果 |
| 未找到该幂等操作 | 未找到该次操作记录 |
| 幂等与不可覆盖 | 防重复与不可覆盖 |
| 幂等尾部 / 幂等命名空间 | 任务号尾号 / 原任务标识 |
| 幂等跳过 / 幂等重跑 / 沿原幂等身份 | 已处理跳过 / 按原任务重试 |
| 入账投影 / 入账 | 入账 |
| 敏感快照 | 临时数据 |
| 历史快照 | 历史记录 |
| 当前任务租约 / 任务租约 / 会话租约 / 当前状态租约（锁定字段名） | 当前处理状态 |
| 内容指纹 / 对象指纹 / 指纹（列名/状态） / subjectHash（用户可见处） | 数据版本 / 版本状态 |
| 审计指纹 / 输入包指纹 / 驳回指纹 | 校验码 / 数据包版本 / 驳回时数据版本 |
| 指纹不一致 / 旧指纹失效 | 数据已变更，请刷新后重试 |
| 对象版本（用户可见处） | 版本 |
| 数据水位 / 目录水位 / 队列水位 / 审计水位 / 同步水位 | 数据更新时间 / 同步进度 / 最新同步时间 |

### 架构词与英文原值（必须清零）

> 本表覆盖：架构层词汇、重放/发送语义、版本冲突、字段名/枚举原值上屏、Q 代号、接口术语残留、散词。出现位置仅供定位；验收**必须**以 §5.2 扫描命令为准，用户可见串**必须**清零。

#### P0 · 字段名 / 枚举原值上屏（违反规则 7，必须清零）

| 禁用词 | 出现位置 | 替换为 |
| --- | --- | --- |
| scopeHash / 当前 scopeHash（UI 标签） | `ownership-migration-page.tsx` | 数据版本 / 当前数据版本 |
| subjectHash（UI 标签） | `ownership-migration-page.tsx`（复核卡） | 数据版本 |
| cutoverId / rangeStart / T / rangeEnd（UI 标签） | `history-backfill-page.tsx`（任务身份与范围卡） | 切换编号 / 范围起点 / 截止时点 |
| OPEN / COMPLETED / CLOSED（任务枚举原值） | 待办页面和任务提示 | 待处理 / 已完成 / 已关闭 |
| REPLAY_ACCEPTED · 任务仍非终态 | `integration-errors/api.ts` | 已受理重新提交 · 任务尚未完成 |
| RECOVERY_RESPONSIBILITY_UNCONFIRMED /（Q3） | 错误消息 | 恢复责任尚未确认 |
| SKU 修订 ID / 商城类目 ID / 唯一固定供给修订 ID | `publication-center-page.tsx` | …编号 |
| 提交正式流程 | `sales-order-create-page.tsx` | 提交 |

#### P1 · 架构层词汇（服务端 / 客户端 / 前端 / 本地 / 浏览器）

| 禁用词 | 出现位置 | 替换为 |
| --- | --- | --- |
| 服务端 | 全仓 60+ 处：`customer-form-sheet.tsx`、`workflow.tsx`、`feedback.tsx`、`domain.tsx`、`integration-errors/api.ts`、`execution-projections*`、`purchase-order-detail-page.tsx`、`supplier-orders*`、`mall-sync`、`inventory`、`acceptance-workspace.tsx`、mock 各文件 | 系统 / 删除（如「服务端合计」→「系统合计」；「服务端筛选结果」→「系统筛选结果」） |
| 服务端法定名称 / 服务端简称 / 服务端信用代码 | `customer-form-sheet.tsx`（冲突解决框） | 系统现有法定名称 / 简称 / 信用代码 |
| 本地输入 / 本地内容 / 本地提交时间 / 本地保持 / 本地覆盖 | `workflow.tsx`、`customer-form-sheet.tsx`、`safety-pause-panel.tsx`、`actual-profit-loss*`、mock | 你输入的内容 / 本页 / 提交时间 |
| 前端 | `customer-receivables/api.ts`、`history-backfill*`、`import-opening*`、`mall-sync/api.ts`、`sales-orders-list-page.tsx` | **必须**删除，或改为「系统统一判定 / 本页」 |
| 客户端 | `integration-errors/api.ts`、`supplier-settlements/api.ts`、`history-backfill/api.ts`、`access-audit/session.ts`、mock | 系统 / 删除 |
| 浏览器 | `ownership-migration/api.ts` | 删除（「不可由浏览器忽略」→「不可跳过」） |
| 服务端 safeNow / （服务端按安全同步点计算） | `mall-sync/api.ts` | 系统当前时间 |
| 由服务端自动形成 / 服务端聚合 / 服务端履约数据 / 服务端更新时间 | `execution-projections*`、`customer-detail-page.tsx`、`acceptance-workspace.tsx`、`customer-receivables-page.tsx` | 由系统自动形成 / 系统汇总 / 系统履约数据 / 系统更新时间 |

#### P1 · 重放 / 发送 / 对象级（replay 语义）

| 禁用词 | 出现位置 | 替换为 |
| --- | --- | --- |
| 安全重放 / 重放原动作 / 重放未开放 / 重放已受理 / 重放后待确认 / 重放已接单 | `integration-errors*`、`supplier-orders*` | 安全重发 / 按原任务重新提交 / 暂不可重发 / 重发已受理 / 重发后待确认 / 重发已接单 |
| 原键锁定 / 客户端未传键 / 禁止客户端传入原任务号 | `integration-errors/api.ts` | 按原任务号重发 / 未手动指定 / 禁止自行指定原任务号 |
| 对象级重试 | `execution-projections*` | 按单据重试 |
| 存在进行中的投递，禁止并发重试 | `execution-projections/api.ts`、mock | 正在发送中，请勿重复操作 |
| 投递（W22 发布 / W23 执行信息，用户可见） | `product-publications*`、`execution-projections*`、mock | 发送（发送记录 / 发送状态 / 发送至目标商城） |

#### P1 · 版本冲突 / 锁版本 / 重载 / 并发

| 禁用词 | 出现位置 | 替换为 |
| --- | --- | --- |
| 锁版本冲突 / 差异版本冲突 / 配置版本冲突 / 连接版本冲突 / 能力版本冲突 / 权限版本冲突 / 数据版本冲突 | `supplier-settlements/api.ts`、`supplier-api-connections/api.ts`、`access-audit-page.tsx`、`unified-task-queue-page.tsx`、`consumption-order-center-page.tsx` | 数据已更新 / 差异数据已更新 / 配置已更新 / 连接信息已更新 / 能力信息已更新 / 权限已更新 |
| 版本冲突，请刷新后重试（各变体） | `supplier-orders`、`customer-receivables`、mock/session-state | 数据已更新，请刷新后重试（统一走 `versionText.versionChangedRefresh`） |
| 请重载…后再保存 / 请重载差异后重试 | `supplier-settlements/api.ts`、mock | 请刷新…后再保存 / 请刷新后重试 |
| 并发冲突 / 发现并发版本冲突 / 不可并发作废 | `feedback.tsx`、`workflow.tsx`、`sales-orders/api.ts` | 数据已更新 / 不可同时作废 |
| 发现并发版本冲突（标题） | `workflow.tsx` | 数据已更新 |

#### P2 · Q 策略代号 / 接口术语残留 / 散词

| 禁用词 | 出现位置 | 替换为 |
| --- | --- | --- |
| Q1–Q5 策略代号（Q1 复核策略 / Q4 策略配置 / Q5 未决 /（Q3）） | `access-audit*`、`supplier-settlements/api.ts`、`card-funds-review/types.ts` | 写策略业务名（如「复核策略」「策略配置」） |
| 接口限流 / 重复回调 / 乱序回调 / 等待退避时间 | `domain.tsx`（W29 错误分类） | 调用次数受限 / 重复通知 / 通知顺序异常 / 请稍后重试 |
| 报文 / 调用供应商下单接口 | `supplier-order-center-page.tsx`、`history-backfill-page.tsx`、mock | 消息内容 / 向供应商发起下单 |
| 终态（用户可见） | `integration-errors*`、`supplier-orders*`、`unified-task-queue-page.tsx`、`mall-sync-page.tsx`、mock | 处理结果 / 处理完成 |
| 终态证据 | `domain.tsx` | 完成凭证 |
| 批处理结果 | `audit-import.tsx` | 批量处理结果 |
| 一期轮询封存 / 第一期轮询已封存 | `ownership-migration*`、`mall-sync*`、mock | 一期同步已封存 |
| 缓存（用户可见） | `supplier-settlements-page.tsx`、`supplier-api-connections-page.tsx`、`actual-profit-loss*`、`customer-quality*`、`card-business-analytics*`、`access-audit/session.ts`、`sales-orders/acceptance.ts` | 已显示的数据 / 暂无可用结果 / 立即更新 |
| 会话（用户可见，非「核销」业务词场景） | `fulfillment-operations-page.tsx`、`product-publications*`、`mall-sync-page.tsx`、`card-funds-review-page.tsx`、`inventory-ledger-page.tsx`、mock | 本次操作 / 本次输入 / 本次登记 |
| 核销会话 / 草稿会话（客户往来 / 供应商往来） | `customer-receivables*`、`supplier-payables*` | 本次核销 / 本次草稿 |
| 工作面（用户可见） | `fulfillment-operations*`、`access-audit*`、`supplier-accounts-page.tsx` | 页面 |
| 掩码（用户可见） | `customer-detail-page.tsx`、`access-audit*`、`ownership-migration*`、`purchase-order-preview-panel.tsx`、`supplier-order-center-page.tsx`、`mall-consumption-orders*`、`contract-detail-page.tsx`、mock | 打码 / 隐藏 |
| 复核策略未固化 | `access-audit*` | 复核策略未确定 |
| 基线已过期 / 本地内容基线 | `workflow.tsx` | 数据已过期 / 你输入的内容版本 |
| 同一事务落地 / 同事务 / 本事务 / 原子事务回滚 | `ownership-migration*`、`supplier-settlements*`、`card-funds-review*`、`procurement-confirmation*`、`unified-task-queue-page.tsx`、mock | 同一次提交 / 本次提交 / 同时生效 |
| 异步（用户可见） | `customer-detail-page.tsx`、`mall-sync*`、`history-backfill*`、mock | 后台 / 系统（删除「异步」） |
| 对象级 AccessChange 预览 | `access-audit/session.ts` | 按对象变更预览 |
| 正式决策 / 正式单 | `mock/workspace-pages.ts`、`sales-order-create-page.tsx` | 决定 / 生效单 |
| 与 wi_pc_01 指向同一确认事项 | `mock/work-items.ts` | 与已有任务指向同一确认事项 |

> **「基线」必须保留的场景**：商城同步（期初基线）、供应商结算（成本基线）、商品发布（基线修订）为业务领域词，**必须**保留，**禁止**当作实现术语替换。
> **「核销 / 对账 / 差异 / 试算 / 冻结 / 回填 / 期初」必须保留**：财务与运营通用业务词，**禁止**替换。

> **「正式」仅当对比时保留**：仅当与「草稿 / 会话草稿 / 预览」对照时，**必须**使用「已生效记录 / 已入账记录」表达对比；**禁止**「正式操作」「正式待办」等无对照刷屏。

---

## 3. 场景标准文案

### 3.1 打开专用处理器 → 按任务类型

`handlerHref` 指向的任务页各有明确业务动作；按钮文案**必须**按 `workItemTypeLabel` 生成，且**必须**使用下表标准文案：

| 任务类型 | 标准按钮文案 |
| --- | --- |
| 采购二次确认 | 去确认采购计划 |
| 卡券票款复核 | 去复核卡券票款 |
| 映射异常处理 | 去处理映射异常 |
| 回款事实复核 | 去核对回款 |
| 收货与发货 / 交付与代发 / 电子履约 / 实物履约 / 服务履约 | 去处理 |
| 其他已登记任务类型 | 按「去 + 动词 + 对象」登记，例如「去审核采购单」 |
| 任意未登记类型 | 前往处理（兜底） |

对应文案模板：`去${工作动词}${对象}`；**禁止**出现「处理器」「页面」「打开」等机制词。

履约那一行是模板的**唯一例外**：五类作业的对象名各不相同，且该按钮紧挨任务类型标签出现，对象已由相邻文字给出；再拼一次会变成「履约作业 · 去处理履约」，**禁止**重复对象。脱离任务列表、独立出现的入口**不得**使用本行简写，**必须**给全称（如验收空态用「打开履约处理」）。

### 3.2 连续处理条

| 场景 | 标准文案 |
| --- | --- |
| 有跳转页 | 前往处理 · 处理完返回队列 |
| 无跳转页，仅当前项 | 完成当前项 |
| 无跳转页，可连下一条 | 完成并处理下一条 |
| 提交中 | 正在提交… |
| 团队任务尚无个人责任人 | 团队待处理 |
| 从团队任务建立本人责任 | 开始处理 |
| 主管查看授权范围内全部开放任务 | 团队任务 |
| 查看已完成或已关闭任务 | 处理历史 |
| 处理权已由他人取得或发生变化 | 刷新任务 |

### 3.3 并发编辑与责任冲突提示

| 场景 | 标准文案 |
| --- | --- |
| 数据已被他人更新 | 数据已更新，请刷新后重新校验版本 |
| 处理权变化 | 处理权已变化，请刷新 |
| 与别人冲突 | 此任务已由其他人处理 |
| 开始处理提示 | 开始处理后即可提交 |

### 3.4 结果反馈

| 场景 | 标准文案 |
| --- | --- |
| 提交成功但结果未回 | 处理结果待确认，请勿重复提交 |
| 提交成功 | 处理结果已记录 |
| 重试提示 | 未查到处理结果，请使用原任务号重试 |

### 3.5 数据新鲜度（原「水位」）

| 场景 | 标准文案 |
| --- | --- |
| 列表/指标更新时间 | 数据更新于 {时间} |
| 目录/同步进度 | 最新同步时间 / 同步进度 |
| 数据可能过期 | 数据可能不是最新，请刷新 |

### 3.6 工作面跳转（禁止 W 编号）

| 内部编号 | 用户提示中的名称 |
| --- | --- |
| W02 | 待办队列 |
| W05 | 销售单 |
| W07 | 采购二次确认 |
| W11 | 客户往来 |
| W12 | 供应商往来 |
| W14 | 基础资料 / 公司商品 |
| W18 | 导入与期初 |
| W21 | 供应商供给 |
| W23 | 执行信息 |
| W26 | 供应商订单 |
| W29 | 接口错误与对账中心 |

### 3.7 供应商供给（W21）

> 业务词 **必须保留**：「公司 SKU」「供应商」「订货编码」「供给」「商业条款」「可供」「起订量」。
> 实现词 **禁止上屏**：API 路径、`revision_no`、`availability_version`、幂等键字面量。

| 场景 | 标准文案 |
| --- | --- |
| 页头主按钮 | 添加供给 |
| Dialog 标题 | 添加供给 |
| Dialog 副文案 | 供给直接连接公司 SKU 与供应商；在此维护供应商订货编码、商业条款和当前可供情况。 |
| 条款动作 | 修订条款 |
| 可供动作 | 更新可供 |
| 关系状态 | 启用 / 暂停 / 停止 |
| 可供状态 | 可供 / 不可供 / 停止供应 / 数据已过期 |
| 缺起订量 | 请填写集采起订量 |
| 条款版本冲突 | 供给条款已被更新，请刷新后重试 |
| 可供版本冲突 | 当前可供情况已被更新，请刷新后重试 |
| 进项税率标签 / 占位 | 进项税率 * / 例如 13（右侧 `%`） |
| 进项税率说明 | 填写整数百分比，提交时自动转为小数税率 |
| 添加成功 | 供给已添加 |
| 条款成功 | 供给条款已保存 |
| 可供成功 | 当前可供情况已更新 |
| 查看供给 | 查看供给 |

**用语约定（代码/API → UI）**

| 内部说法 | 用户可见说法 |
| --- | --- |
| company SKU | 公司规格 / 公司 SKU（列表可用「公司规格编号」） |
| sales_visible_price_gross | 销售可见价 |
| market_price | 市场价 |
| dropship/bulk supply price | 代发供给价 / 集采供给价 |
| bulk minimum order quantity | 集采起订量 / 起订量 |
| supplier offering revision | 供给条款版本 |
| supplier offering availability | 当前可供情况 |
| 公司商品池（sellable-items 查询视图） | 公司商品池（**保留**，业务名；禁止写「sellable-items」） |

---

## 4. 内部词保留清单

以下词**只允许**出现在代码注释、字段名、错误对象、文档架构章节；**禁止**出现在用户可见字符串：

| 内部词 | 代码位置（示意） | 用户界面替代 |
| --- | --- | --- |
| claimToken / 令牌 | `features/*/session.ts` | （不出现） |
| claim / 领取 / 重新领取 | `features/*/api.ts`、任务组件状态 | 开始处理 / 刷新任务 |
| projection / 投影 | `features/execution-projections/`、`features/workspace/freshness.ts` | 汇总 / 数据 / 摘要 |
| fact / 事实 | `features/*/types.ts`、`mock/*` | 记录 / 凭证 |
| idempotency key / 幂等键 / 幂等 | `features/*/api.ts` | 原任务号 / 防重复 / 已处理跳过 |
| subjectHash / 内容指纹 / 指纹 | `mock/*`、`features/*` | 数据版本 |
| work item / work_item / 任务项 | `features/workspace-kit/`、`mock/work-items.ts` | 任务 |
| handler / 处理器 | `mock/work-items.ts`、`queue-workspace-page.tsx` | （不出现） |
| snapshot / 快照 | `mock/*`、`components/business/paper-document.tsx` | 历史记录 |
| role pool / 角色池 | `features/*/api.ts`、`mock/workspace.ts` | 团队 / 可认领 |
| watermark / 水位 | 分析页、同步页 | 数据更新时间 / 同步进度 |
| fail-closed | 商品发布、权限 | 结果未确认前禁止… |
| envelope / 任务信封 / Complete*Command | 结算、复核确认框 | 提交 / 确认处理 |
| W01–W30 工作面编号 | 文档与路由注释 | 页面中文名 |
| server / client / 服务端 / 客户端 / 前端 / 本地 / 浏览器 | 实现层、`api.ts`、错误对象 | 系统 / 本页 / 删除 |
| replay / 重放 / REPLAY_ORIGINAL | `integration-errors*`、`supplier-orders*` | 按原任务重发 / 重新提交 |
| lockVersion / 锁版本 / 版本冲突 / VERSION_CONFLICT | 会话层、错误对象 | 数据已更新，请刷新后重试 |
| scopeHash / subjectHash / cutoverId / rangeStart / rangeEnd | 数据字段与 fact key | 数据版本 / 切换编号 / 范围起点 / 截止时点 |
| Q1–Q5 策略代号 / DISABLED_Q1 | 权限审计会话、`workItemSupport` | 复核策略 / 策略确定前关闭 |
| 掩码 / 会话（非业务词场景）/ 工作面 / 缓存 / 终态 / 轮询 / 异步 / 事务 / 批处理 / 退避 | mock 与实现层 | 打码 / 本次操作 / 页面 / 已显示数据 / 处理结果 / 同步 / 后台 / 同一次提交 / 批量处理 / 稍后重试 |

---

## 5. 验收与执行规则

### 5.1 验收范围

| 优先级 | 范围 | 成功标准 |
| --- | --- | --- |
| P0 | 按钮文案与兜底逻辑 | 界面**禁止**出现「打开专用处理器」「正式处理」 |
| P1 | 状态条与提示（领取/租约/令牌/角色池） | 界面**禁止**出现「领取」「重新领取」「团队待认领」「租约」「令牌」「角色池」 |
| P2 | 长尾（正式/投影/幂等键/快照/事实） | 导航与主路径**禁止**出现「投影」「正式结果」 |
| 扩展清零 A | 幂等/指纹/水位/work_item/fail-closed/W 编号/正式操作* | 用户可见串**必须**清零 |
| 扩展清零 B | 架构词（服务端/客户端/前端/本地/浏览器）、重放/投递、版本冲突/锁版本/重载、字段名与枚举原值上屏（scopeHash/subjectHash/cutoverId/IN_PROGRESS 等）、Q 代号、接口术语（限流/回调/退避/报文）、散词（终态/轮询/批处理/缓存/会话/工作面/掩码/固化/事务/异步/基线过期） | 用户可见串**必须**清零 |

### 5.2 扫描验收

交付前**必须**执行全局扫描并确认用户可见串清零：

```bash
rg -n "专用处理器|领取|重新领取|团队待认领|租约|令牌|角色池|幂等|投影|正式结果|正式提交|正式操作|正式待办|正式终态|正式水位|内容指纹|对象指纹|事实复核|快照|work_item|任务信封|fail-closed|subject_hash|水位|对象版本" \
  erp-client --glob '*.{tsx,ts}' \
  -g '!**/node_modules/**' -g '!**/.next/**'
```

架构词 / 重放 / 版本冲突 / 字段名上屏 / Q 代号 / 散词补充扫描（**必须**一并执行）：

```bash
rg -n "服务端|客户端|前端|浏览器|本地|重放|投递|对象级|并发|版本冲突|锁版本|重载|接口限流|重复回调|乱序回调|退避|报文|终态|轮询|批处理|缓存|工作面|掩码|固化|scopeHash|subjectHash|cutoverId|IN_PROGRESS|REPLAY_ACCEPTED|RECOVERY_RESPONSIBILITY_UNCONFIRMED|Q[1-5] |锁版本|call.*接口" \
  erp-client --glob '*.{tsx,ts}' \
  -g '!**/node_modules/**' -g '!**/.next/**'
```

剩余命中**必须**逐条确认：仅当属于注释 / 字段名 / 错误对象 key / 类型名时方可放行（如 `DISABLED_Q1`、`subject_hash` 数据字段、内部枚举值属性）；**用户可见字符串**（JSX 文本、title、description、label、message、header、placeholder、Alert、toast、事实表的 label/value）**必须**全部清零。验收时**必须**区分业务保留词：核销、对账、差异、试算、冻结、回填、期初、基线（商城同步/成本基线）为业务词，**禁止**替换。

### 5.3 新增文案守则

- 新写用户可见文案前**必须**查本表；命中禁用词**必须**改写。
- **跨页复用文案必须**从 `erp-client/lib/ui-text.ts` 引用（目标注册为 `responsibilityText` / `sequentialText` / `resultText` / `versionText` / `freshnessText` / `workspaceLabel` / `actionLabelForWorkItemType`）。旧 `leaseText` 必须删除，禁止各页手写责任提示同义变体。
- 页面专属业务说明**仅当**只在一处使用时方可写在组件内；一旦第二处复用，**必须**抽到 `ui-text.ts`。
- W 系列工作面文档 §12 验收清单**必须**逐页核对通过，否则验收失败。
- 内部概念如需在界面表达，**必须**先找业务等价词；找不到则**禁止**在界面表达。

### 5.4 `lib/ui-text.ts` 使用规则

| 导出 | 用途 |
| --- | --- |
| `responsibilityText` | 当前责任、开始处理、退回团队与处理权变化提示 |
| `sequentialText` | 连续处理条按钮与提交中 |
| `resultText` | 操作结果 / 结果未知 / 按原任务号查询 |
| `versionText` | 数据版本标签与变更提示 |
| `freshnessText` | 数据更新时间 / 同步进度 |
| `workspaceLabel(id)` | 跨页跳转中文名（禁止 Wxx） |
| `openWorkspaceLabel` / `goToWorkspaceLabel` | 「打开…」「前往…」 |
| `actionLabelForWorkItemType` | 任务类型 → 主按钮文案 |
| `documentText` | 打印件页脚 |
| `uiText` | 上述分组的聚合导出 |

完整路由显示名仍以 `lib/workspace-registry.ts` 为准；`workspaceLabel` **必须**提供提示语短名（如「待办队列」而非「待办队列（统一）」）。

## 7. W09 收货与发货 / 交付与代发 口语化

W09 面向一线仓储/采购经办，文案**必须**全面口语化，并**必须**按下表替换：

| 原词 | 改为 | 规则 |
| --- | --- | --- |
| 过账 | 确认入库 / 确认发货 / 确认交付 / 确认完成 | **禁止**保留「过账」；常量 `OPERATION_ACTION_LABEL` |
| 已过账 | 已入库 / 已发货 / 已交付 / 已完成 | 常量 `OPERATION_DONE_LABEL` |
| 暂挂 | 先跳过 | **禁止**保留「暂挂」 |
| 付款门禁 / 仓发门禁 | 先款条件 / 发货条件 | |
| 门禁阻塞 / 门禁已满足 | 先款未到，暂时不能收货 / 货款已到，可以收货 | |
| 预占 | 已为这单留的货 / 留货 | |
| 预占 ID `rsv_*`、采购销售分配 `pla_*`、来源版本 `sv_*` | 品名 + 数量 + 销售单号 | 内部 ID **禁止**进入界面 |
| 领取 / 处理权 | 开始处理 / 你正在处理这一条 | 与审批待办责任合同一致 |
| 作业队列 / 作业类型 | 待办 / 任务类型 | |
| 乐观修改 | （删除）没确认成功之前，库存和留货都不会动 | |

| 原始枚举 `POSTED` / `SHIPPED` / `CONFIRMED` | 已入库 / 已发出 / 已确认（`FORMAL_STATUS_LABEL`） | 枚举原值**禁止**漏到界面 |
| 已暂挂 | 已跳过 | 队列状态徽章 |

### 侧栏入口命名

菜单名是用户可见字符串，**必须**过本表。「履约作业」**禁止**用作干活入口菜单名；侧栏**必须**按岗位拆成两个入口：

| 位置 | 名称 | 覆盖 |
| --- | --- | --- |
| 侧栏「仓储」组 | **收货与发货** | 入库 + 公司仓发 |
| 侧栏「采购与履约」组 | **交付与代发** | 供应商直发 + 电子交付 + 线下服务 |
| 跨页提示 / 只读进入 / 无岗位深链 | **履约处理**（中性短名，`lib/ui-text.ts` 的 `W09`） | 五类 |

「履约」**仅允许**用于**状态与进度**（销售单的履约/回款/开票多轨），**禁止**用作干活入口。
只读角色和未声明岗位的深链**必须**使用中性短名「履约处理」，**禁止**回落到「收货与发货」（否则页头会对着电子交付任务错误写「收货与发货」）。

### 只读角色文案

销售/财务进入 W09 **只能**查看。**禁止**摆一排禁用按钮（禁用态不解释原因）。动作区**必须**整体替换为：

> 👁 你只能查看。这条由 仓储 · 周航 处理，预计 今天 15:00 前完成。　[打开销售单 →]

超期时后半句**必须**变为「原定 今天 11:00，已超期」。状态徽章**必须**显示「只能查看」，**禁止**显示「待领取」。

### 共享组件：必须加 props，禁止改默认值

面向不同角色的措辞差异**必须**通过 props 表达；**禁止**修改共享组件默认文案（否则会波及其他工作面）。

| 组件 | 扩展点 | 默认 |
| --- | --- | --- |
| `PrepaymentGate`（`components/business/domain.tsx`） | `copy?: Partial<PrepaymentGateCopy>` | 面向采购/财务的原措辞，W08 不受影响 |
| `PrepaymentGate` | `presentation?: "panel" \| "badge"` | `panel` 完整卡片；W09 传 `badge`：顶栏结果徽章，悬停展开详情 |
| `SequentialProcessBar`（`components/business/workflow.tsx`） | `showProcess?: boolean` | `true`；只读角色传 `false` |
| `SequentialProcessBar` | `statusExtras?: ReactNode` | 无；W09 挂先款条件徽章（位置/处理状态之后） |

### 防漂移硬性规则

1. **新增枚举必须同时给中文映射**。`POSTED`、`BLOCKED` 等值漏到界面时禁用词表扫不出来（不在词表内），故**必须**有中文映射。
2. **内部 ID 禁止进界面**。`rsv_*`、`pla_*`、`sv_*`、`wi_*` **必须**换成「品名 + 数量 + 单号」。
3. **共享组件必须加 props，禁止改默认值**（见上表）。
4. 文案扫描**必须**固化为回归脚本：除禁用词外，**必须**正则拦截内部 ID 与原始枚举。
