# UI 文案术语表

> **状态**：基线 v1.2（第 5 轮复核后）
> **依据**：`erp-client` 全量扫描（2026-08-03）；`erp-ui-design.md` 的语言原则
> **适用范围**：所有用户可见文案——按钮、状态条、提示、Alert、描述、列名、空态、toast、错误恢复指引
> **不适用**：代码注释、数据模型字段名、内部错误对象 key、文档中的架构描述
> **验收挂钩**：W 系列工作面文档 §12「页面文案不出现实现术语」

## 1. 目的与原则

### 1.1 目的

本系统围绕内部工作流架构构建（work item、租约、投影、事实、幂等键）。这些概念**只在代码与文档中使用**，用户界面一律翻译成业务语言。

### 1.2 判断原则

| 问题 | 答案 |
| --- | --- |
| 这个词用户（采购/财务/运营）在职场上会这么说吗？ | 会 → 可用；不会 → 替换 |
| 去掉/换词后业务含义会丢失吗？ | 会 → 保留但换说法；不会 → 删掉 |
| 这个词解释的是"系统内部怎么实现"还是"业务发生了什么"？ | 实现 → 必须替换 |

### 1.3 通用规则

1. **按钮说动作，不说机制**：用户点的是"去确认采购计划"，不是"打开专用处理器"。
2. **状态说结果，不说锁**：用户看到的是"正在处理中"，不是"租约 v1 有效"。
3. **错误说下一步，不说原理**：用户看到的是"请刷新后重新处理"，不是"幂等键冲突"。
4. **"正式"默认删除**：除非与"草稿/预览"形成必要对比，否则"正式提交"就是"提交"。
5. **内部词只出现在代码注释里**：租约、投影、幂等键、work_item、指纹、水位等词禁止进入任何用户可见字符串。
6. **不把工作面编号当导航**：用户提示写「客户往来」「接口错误中心」，不写「W11」「W29」。
7. **不把命令名/字段名当文案**：禁止 `Complete*Command`、`subject_hash`、`mappingTaskStatus`、`fail-closed` 等进入界面。

---

## 2. 禁用词表（用户界面禁止出现）

> 状态：🔴 必须替换（P0，按钮与主操作）／🟠 必须替换（P1，状态与提示）／🟡 尽快替换（P2，长尾）

### P0 · 按钮 / 主操作

| 禁用词 | 出现位置 | 替换为 |
| --- | --- | --- |
| 打开专用处理器 | `mock/work-items.ts` ×6、`mock/workspace-pages.ts` ×3、`queue-workspace-page.tsx:333/336/439`、`unified-task-queue-page.tsx:1175/1456` | 按任务类型给动作词（见 §3.1），兜底「前往处理」 |
| 正式处理 / 正式完成 / 正式确认 | `queue-workspace-page.tsx:439`、`unified-task-queue-page.tsx:1175/1456/1597`、`mock/work-items.ts:236/489` | 处理 / 完成 / 确认（去掉"正式"） |
| 提交正式动作 | `procurement-rejection-card.tsx:537` | 提交处理结果 |
| Complete*Command / *Envelope（用户可见） | 确认对话框 description | 将提交「…」结论（业务说法） |
| work_item / work_item_type（用户可见） | 结算、权限审计、导入阻断 | 任务 / （类型码不出现） |
| 任务信封 | 结算确认文案 | （不出现）→ 领取任务后提交 |

### P1 · 状态与提示条

| 禁用词 | 出现位置 | 替换为 |
| --- | --- | --- |
| 处理租约有效 · v1 | `workflow.tsx:355`、`queue-workspace-page.tsx:330`、`unified-task-queue-page.tsx:1167` | 正在处理中 · 请勿重复打开 |
| 处理租约续期中 | `workflow.tsx:360` | 处理权限已延期 |
| 处理租约已丢失 | `workflow.tsx:365`、`unified-task-queue-page.tsx:1171` | 操作已失效，请刷新后重新处理 |
| 处理租约已释放 | `workflow.tsx:370` | 本次处理已结束 |
| 租约令牌已清除 / 令牌已清除 | `unified-task-queue-page.tsx:373/982/1165`、`session-state.ts:455` | 临时信息已清除 |
| 租约令牌仅存于当前会话内存 | `unified-task-queue-page.tsx:1211` | 处理进度仅保存在当前页面 |
| 领取后取得租约方可正式处理 | `unified-task-queue-page.tsx:1252` | 领取任务后即可开始处理 |
| 角色池任务待领取 | `unified-task-queue-page.tsx:1250` | 团队任务待认领 |
| 角色池待领取 | `fulfillment-operations/api.ts:123`、`card-funds-review/api.ts:194`、`procurement-confirmation/api.ts:176`、`mock/workspace.ts:592` | 团队待认领 |
| 无法取得编辑租约 / 正在领取编辑租约 | `purchase-order-detail-page.tsx:198/1391` | 无法进入编辑 / 正在进入编辑 |
| 编辑租约有效 · lockVersion | `purchase-order-detail-page.tsx:1398` | 正在编辑中 |
| 任务仍在有效队列（PENDING） | `integration-errors/api.ts:799` | 任务仍在待处理列表，可稍后继续 |
| 租约无效，请重新领取 | `fulfillment-operations/api.ts:944`、`procurement-confirmation/api.ts:322` | 操作已失效，请重新领取 |
| fail-closed（用户可见） | 商品发布、权限审计等 | 结果未确认前禁止… / 按保守策略拒绝 |
| subject_hash / 当前 subject_hash（UI 标签） | 卡券复核、供应商商品 | 数据版本 |
| mappingTaskStatus / supplier_catalog_intake_batch 等字段名 | 商城同步、供应商商品库 | 业务中文描述 |
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

### 第 5 轮新增（2026-08-03，架构词与英文原值）

> 本轮覆盖四轮之外的新家族：架构层词汇、重放/发送语义、版本冲突、字段名/枚举原值上屏、Q 代号、接口术语残留、散词。行号以复核时为准，验收以扫描命令为准。

#### P0 · 字段名 / 枚举原值上屏（违反规则 7，必须清零）

| 禁用词 | 出现位置（修复前） | 替换为 |
| --- | --- | --- |
| scopeHash / 当前 scopeHash（UI 标签） | `ownership-migration-page.tsx` | 数据版本 / 当前数据版本 |
| subjectHash（UI 标签） | `ownership-migration-page.tsx`（复核卡） | 数据版本 |
| cutoverId / rangeStart / T / rangeEnd（UI 标签） | `history-backfill-page.tsx`（任务身份与范围卡） | 切换编号 / 范围起点 / 截止时点 |
| IN_PROGRESS / PENDING / COMPLETED（事实值） | `integration-errors/api.ts`（任务状态事实） | 处理中 / 待处理 / 已完成 |
| REPLAY_ACCEPTED · 任务仍非终态 | `integration-errors/api.ts` | 已受理重新提交 · 任务尚未完成 |
| RECOVERY_RESPONSIBILITY_UNCONFIRMED /（Q3） | `supplier-catalog/types.ts` 消息 | 恢复责任尚未确认 |
| SKU 修订 ID / 商城类目 ID / 唯一固定供给修订 ID | `publication-center-page.tsx` | …编号 |
| 提交正式流程 | `sales-order-create-page.tsx` | 提交 |

#### P1 · 架构层词汇（服务端 / 客户端 / 前端 / 本地 / 浏览器）

| 禁用词 | 出现位置 | 替换为 |
| --- | --- | --- |
| 服务端 | 全仓 60+ 处：`customer-form-sheet.tsx`、`workflow.tsx`、`feedback.tsx`、`domain.tsx`、`integration-errors/api.ts`、`execution-projections*`、`purchase-order-detail-page.tsx`、`supplier-orders*`、`mall-sync`、`inventory`、`acceptance-workspace.tsx`、mock 各文件 | 系统 / 删除（如「服务端合计」→「系统合计」；「服务端筛选结果」→「系统筛选结果」） |
| 服务端法定名称 / 服务端简称 / 服务端信用代码 | `customer-form-sheet.tsx`（冲突解决框） | 系统现有法定名称 / 简称 / 信用代码 |
| 本地输入 / 本地内容 / 本地提交时间 / 本地保持 / 本地覆盖 | `workflow.tsx`、`customer-form-sheet.tsx`、`safety-pause-panel.tsx`、`actual-profit-loss*`、mock | 你输入的内容 / 本页 / 提交时间 |
| 前端 | `customer-receivables/api.ts`、`history-backfill*`、`import-opening*`、`mall-sync/api.ts`、`sales-orders-list-page.tsx` | 删除或「系统统一判定 / 本页」 |
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
| Q1–Q5 策略代号（Q1 复核策略 / Q4 策略配置 / Q5 未决 /（Q3）） | `access-audit*`、`supplier-settlements/api.ts`、`card-funds-review/types.ts`、`supplier-catalog/types.ts` | 写策略业务名（如「复核策略」「策略配置」） |
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
| 掩码（用户可见） | `customer-detail-page.tsx`、`access-audit*`、`ownership-migration*`、`purchase-order-preview-panel.tsx`、`supplier-order-center-page.tsx`、`mall-consumption-orders*`、`supplier-catalog*`、`contract-detail-page.tsx`、mock | 打码 / 隐藏 |
| 复核策略未固化 | `access-audit*` | 复核策略未确定 |
| 基线已过期 / 本地内容基线 | `workflow.tsx` | 数据已过期 / 你输入的内容版本 |
| 同一事务落地 / 同事务 / 本事务 / 原子事务回滚 | `ownership-migration*`、`supplier-settlements*`、`card-funds-review*`、`procurement-confirmation*`、`unified-task-queue-page.tsx`、mock | 同一次提交 / 本次提交 / 同时生效 |
| 异步（用户可见） | `customer-detail-page.tsx`、`mall-sync*`、`history-backfill*`、mock | 后台 / 系统（删除「异步」） |
| 对象级 AccessChange 预览 | `access-audit/session.ts` | 按对象变更预览 |
| 正式决策 / 正式单 | `mock/workspace-pages.ts`、`sales-order-create-page.tsx` | 决定 / 生效单 |
| 与 wi_pc_01 指向同一确认事项 | `mock/work-items.ts` | 与已有任务指向同一确认事项 |

> **「基线」保留场景**：商城同步（期初基线）、供应商结算（成本基线）、商品发布（基线修订）为业务领域词，保留。
> **「核销 / 对账 / 差异 / 试算 / 冻结 / 回填 / 期初」保留**：财务与运营通用业务词。

> **「正式」可保留的对比场景**：与「草稿 / 会话草稿 / 预览」对照时，可用「已生效记录 / 已入账记录」；避免「正式操作」「正式待办」等无对照刷屏。

---

## 3. 场景推荐文案

### 3.1 打开专用处理器 → 按任务类型

`handlerHref` 指向的任务页各有明确业务动作，按钮文案应按 `workItemTypeLabel` 生成：

| 任务类型 | 推荐按钮文案 |
| --- | --- |
| 采购二次确认 | 去确认采购计划 |
| 卡券票款复核 | 去复核卡券票款 |
| 映射异常处理 | 去处理映射异常 |
| 回款事实复核 | 去核对回款 |
| 收货与发货 / 交付与代发 / 电子履约 / 实物履约 / 服务履约 | 去处理 |
| 任意未登记类型 | 前往处理（兜底） |

对应文案模板：`去${工作动词}${对象}`，避免出现"处理器""页面""打开"等机制词。

履约那一行是模板的**唯一例外**：五类作业的对象名各不相同，而这个按钮永远紧挨着
任务类型标签出现，对象已由相邻文字给出，再拼一次会变成「履约作业 · 去处理履约」。
脱离任务列表、独立出现的入口不适用本行，要给全称（如验收空态用「打开履约处理」）。

### 3.2 连续处理条

| 场景 | 推荐文案 |
| --- | --- |
| 有跳转页 | 前往处理 · 处理完返回队列 |
| 无跳转页，仅当前项 | 完成当前项 |
| 无跳转页，可连下一条 | 完成并处理下一条 |
| 提交中 | 正在提交… |
| 任务待领取 | 任务待认领 |

### 3.3 处理权限（租约）提示

| 场景 | 推荐文案 |
| --- | --- |
| 持有处理权 | 正在处理中 · 请勿重复打开 |
| 处理权丢失 | 操作已失效，请刷新后重新处理 |
| 处理权收回 | 权限已收回，不能提交 |
| 与别人冲突 | 此任务已被其他人领取，请稍后再试 |
| 领取提示 | 领取任务后即可开始处理 |

### 3.4 结果反馈

| 场景 | 推荐文案 |
| --- | --- |
| 提交成功但结果未回 | 处理结果待确认，请勿重复提交 |
| 提交成功 | 处理结果已记录 |
| 重试提示 | 未查到处理结果，请使用原任务号重试 |

### 3.5 数据新鲜度（原「水位」）

| 场景 | 推荐文案 |
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
| W18 | 导入与期初 |
| W23 | 执行信息 |
| W26 | 供应商订单 |
| W29 | 接口错误与对账中心 |

---

## 4. 内部词保留清单

以下词**只允许出现在代码注释、字段名、错误对象、文档架构章节**，禁止出现在用户可见字符串：

| 内部词 | 代码位置（示意） | 用户界面替代 |
| --- | --- | --- |
| lease / 租约 | `components/business/workflow.tsx`、`features/*/api.ts` | 处理权限 / 正在处理中 |
| claimToken / 令牌 | `features/*/session.ts` | （不出现） |
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

## 5. 实施方式

### 5.1 分轮次推进

| 轮次 | 范围 | 验收 |
| --- | --- | --- |
| 第 1 轮 | P0 按钮文案（约 28 处 + 兜底逻辑） | 界面无「打开专用处理器」「正式处理」 |
| 第 2 轮 | P1 状态条与提示（租约/令牌/角色池） | 界面无「租约」「令牌」「角色池」 |
| 第 3 轮 | P2 长尾（正式/投影/幂等键/快照/事实） | 导航与主路径无「投影」「正式结果」 |
| 第 4 轮 | 幂等/指纹/水位/work_item/fail-closed/W 编号/正式操作* | 用户可见串清零（本版） |
| 第 5 轮 | 架构词（服务端/客户端/前端/本地/浏览器）、重放/投递、版本冲突/锁版本/重载、字段名与枚举原值上屏（scopeHash/subjectHash/cutoverId/IN_PROGRESS 等）、Q 代号、接口术语（限流/回调/退避/报文）、散词（终态/轮询/批处理/缓存/会话/工作面/掩码/固化/事务/异步/基线过期） | 用户可见串清零（本版） |

### 5.2 扫描验收

每轮完成后执行全局扫描确认清零：

```bash
rg -n "专用处理器|租约|令牌|角色池|幂等|投影|正式结果|正式提交|正式操作|正式待办|正式终态|正式水位|内容指纹|对象指纹|事实复核|快照|work_item|任务信封|fail-closed|subject_hash|水位|对象版本" \
  erp-client --glob '*.{tsx,ts}' \
  -g '!**/node_modules/**' -g '!**/.next/**'
```

第 5 轮补充扫描（架构词 / 重放 / 版本冲突 / 字段名上屏 / Q 代号 / 散词）：

```bash
rg -n "服务端|客户端|前端|浏览器|本地|重放|投递|对象级|并发|版本冲突|锁版本|重载|接口限流|重复回调|乱序回调|退避|报文|终态|轮询|批处理|缓存|工作面|掩码|固化|scopeHash|subjectHash|cutoverId|IN_PROGRESS|REPLAY_ACCEPTED|RECOVERY_RESPONSIBILITY_UNCONFIRMED|Q[1-5] |锁版本|call.*接口" \
  erp-client --glob '*.{tsx,ts}' \
  -g '!**/node_modules/**' -g '!**/.next/**'
```

剩余命中必须逐条确认：属于注释 / 字段名 / 错误对象 key / 类型名的放行（`DISABLED_Q1`、`subject_hash` 数据字段、内部枚举值属性等），**用户可见字符串**（JSX 文本、title、description、label、message、header、placeholder、Alert、toast、事实表的 label/value）全部清零。验收注意区分「业务保留词」：核销、对账、差异、试算、冻结、回填、期初、基线（商城同步/成本基线）为业务词，不替换。

### 5.3 新增文案守则

- 新写用户可见文案前查本表；命中禁用词一律改写。
- **跨页复用文案**优先从 `erp-client/lib/ui-text.ts` 引用（`leaseText` / `sequentialText` / `resultText` / `versionText` / `freshnessText` / `workspaceLabel` / `actionLabelForWorkItemType`），禁止各页手写「正在处理中 · 请勿重复打开」等同义变体。
- 页面专属业务说明可继续写在组件内；一旦第二处复用，抽到 `ui-text.ts`。
- W 系列工作面文档 §12 验收清单已有对应条目，评审时逐页核对。
- 内部概念如需在界面表达，先找业务等价词，找不到就**不表达**（用户不需要知道）。

### 5.4 `lib/ui-text.ts` 使用说明

| 导出 | 用途 |
| --- | --- |
| `leaseText` | 处理权状态条、领取/失效提示 |
| `sequentialText` | 连续处理条按钮与提交中 |
| `resultText` | 操作结果 / 结果未知 / 按原任务号查询 |
| `versionText` | 数据版本标签与变更提示 |
| `freshnessText` | 数据更新时间 / 同步进度 |
| `workspaceLabel(id)` | 跨页跳转中文名（禁止 Wxx） |
| `openWorkspaceLabel` / `goToWorkspaceLabel` | 「打开…」「前往…」 |
| `actionLabelForWorkItemType` | 任务类型 → 主按钮文案 |
| `documentText` | 打印件页脚 |
| `uiText` | 上述分组的聚合导出 |

完整路由显示名仍以 `lib/workspace-registry.ts` 为准；`workspaceLabel` 提供**提示语短名**（如「待办队列」而非「待办队列（统一）」）。

## 6. 待确认事项

| ID | 问题 | 影响 | 决策人 | 结论 |
| --- | --- | --- | --- | --- |
| G1 | 「暂挂」是否保留？（业务半术语，意为"先跳过稍后处理"） | 20+ 处按钮与提示 | 产品负责人（2026-08-03） | **不保留**：改「先跳过」。W09 已落地；其余页面见 §7 待跟进 |
| G2 | 「核销」是否保留？（财务专业词，目标用户为财务） | 付款/回款核销界面 | — | 建议保留 |
| G3 | 「履约」是否保留？（电商通用词） | 多处状态列 | — | 建议保留 |
| G4 | 术语表是否需要落在代码层（`lib/ui-text.ts` 常量） | 防止再次漂移 | — | **已落地** `erp-client/lib/ui-text.ts`；共享组件与队列页已接入，新跨页文案优先引用 |
| G5 | 「过账」是否保留？（仓储/财务半术语） | 履约、付款 | 产品负责人（2026-08-03） | **不保留**：主动作按业务类型分别命名（确认入库/确认发货/确认交付/确认完成）。W09 已落地；其余页面见 §7 待跟进 |
| G6 | 「演示」标记（演示结果/演示角色/演示空态/演示批次）是否保留 | 客户表单模拟控件、各页演示徽标 | — | **保留**：本项目为演示/模拟环境，明示数据来源；生产环境上线时按 §5.4 统一隐藏或改「示例模式」 |

---

## 7. W09 收货与发货 / 交付与代发 口语化（2026-08-03）

W09 目标用户确认为一线仓储/采购经办，本轮按「全面口语化」口径改写。已落地的替换：

| 原词 | 改为 | 说明 |
| --- | --- | --- |
| 过账 | 确认入库 / 确认发货 / 确认交付 / 确认完成 | 见 G5；常量 `OPERATION_ACTION_LABEL` |
| 已过账 | 已入库 / 已发货 / 已交付 / 已完成 | 常量 `OPERATION_DONE_LABEL` |
| 暂挂 | 先跳过 | 见 G1 |
| 付款门禁 / 仓发门禁 | 先款条件 / 发货条件 | |
| 门禁阻塞 / 门禁已满足 | 先款未到，暂时不能收货 / 货款已到，可以收货 | |
| 预占 | 已为这单留的货 / 留货 | |
| 预占 ID `rsv_*`、采购销售分配 `pla_*`、来源版本 `sv_*` | 品名 + 数量 + 销售单号 | 内部 ID 不再进入界面 |
| 领取 / 处理权 | 接手 / 你正在处理这一条 | 与既有「禁止说租约」一致 |
| 作业队列 / 作业类型 | 待办 / 任务类型 | |
| 乐观修改 | （删除）没确认成功之前，库存和留货都不会动 | |

| 原始枚举 `POSTED` / `SHIPPED` / `CONFIRMED` | 已入库 / 已发出 / 已确认（`FORMAL_STATUS_LABEL`） | 枚举原值不得漏到界面 |
| 已暂挂 | 已跳过 | 队列状态徽章 |

### 侧栏入口命名（2026-08-03 补）

菜单名同样是用户可见字符串，同样过这张表。「履约作业」是领域上位概念，
不是一线每天要干的活，且一个名字服务不了仓储和采购两个岗位 —— 已按岗位拆成两个入口：

| 位置 | 名称 | 覆盖 |
| --- | --- | --- |
| 侧栏「仓储」组 | **收货与发货** | 入库 + 公司仓发 |
| 侧栏「采购与履约」组 | **交付与代发** | 供应商直发 + 电子交付 + 线下服务 |
| 跨页提示 / 只读进入 / 无岗位深链 | **履约处理**（中性短名，`lib/ui-text.ts` 的 `W09`） | 五类 |

「履约」保留给**状态与进度**（销售单的履约/回款/开票多轨），不用作干活入口。
只读角色和未声明岗位的深链一律用中性短名，**不要**回落到「收货与发货」——
那会在页头对着一条电子交付任务写「收货与发货」。

### 只读角色文案（第 3 期）

销售/财务进 W09 只能查看。**不要摆一排禁用按钮** —— 禁用态不解释原因。动作区整体替换为：

> 👁 你只能查看。这条由 仓储 · 周航 处理，预计 今天 15:00 前完成。　[打开销售单 →]

超期时后半句变「原定 今天 11:00，已超期」。状态徽章显示「只能查看」，不显示「待领取」。

### 共享组件：加 props，不要改默认值

面向不同角色的措辞差异**必须通过 props 表达**，改共享组件默认文案会波及其它工作面。

| 组件 | 扩展点 | 默认 |
| --- | --- | --- |
| `PrepaymentGate`（`components/business/domain.tsx`） | `copy?: Partial<PrepaymentGateCopy>` | 面向采购/财务的原措辞，W08 不受影响 |
| `PrepaymentGate` | `presentation?: "panel" \| "badge"` | `panel` 完整卡片；W09 传 `badge`：顶栏结果徽章，悬停展开详情 |
| `SequentialProcessBar`（`components/business/workflow.tsx`） | `showProcess?: boolean` | `true`；只读角色传 `false` |
| `SequentialProcessBar` | `statusExtras?: ReactNode` | 无；W09 挂先款条件徽章（位置/租约之后） |

### 待跟进（本轮未做）

- 「过账」仍出现在 **W06 销售验收、W10 库存、W11 客户往来、W12 供应商往来**（`features/` 下 8 个目录）。W09 已改，这些页面暂未同步，存在术语漂移。
- 「暂挂」在 W09 之外的页面同样未同步。
- 两项都需要按 G1/G5 的决议做一次跨页对齐。

### 如何防止再次漂移

1. **新增枚举必须同时给中文映射**。`POSTED`、`BLOCKED` 这类值漏到界面，禁用词表扫不出来 —— 它不在词表里。
2. **内部 ID 不得进界面**。`rsv_*`、`pla_*`、`sv_*`、`wi_*` 一律换成「品名 + 数量 + 单号」。
3. **共享组件加 props，不改默认值**（见上表）。
4. 建议把文案扫描固化为回归脚本：除禁用词外，还要正则拦截内部 ID 与原始枚举。
   本轮验证脚本已验证过 W09 的全部用户可见字符串，但未落库。
