# W30 · 历史消费回填

> 状态：草稿
> 页面模式：M7 治理与导入
> 主要路由：`/governance/history-backfill`、`/governance/history-backfill/:jobId`
> 主要角色：系统管理员；报告复核角色由服务端策略配置，运营协同数据来源
> 最后更新：2026-08-01

## 1. 定位与目标

### 1.1 用户目标

- 管理员在消费回流启用时间 `T` 之后，创建并执行覆盖 `[rangeStart, T)` 的正式历史回填任务；`rangeStart` 固定为目标商城/业务范围内最早业务事实或已登记历史边界，持续看到读取、去重、归集、成本评估和失败进度。
- 任务失败或中断时沿原任务、原范围和原业务事实键续跑，不新建重叠批次，也不重复形成支付、取消、退款、完成或余额恢复事实。
- 有报告查看权的财务可审阅总笔数、金额、重叠去重数、`ACTUAL`/`STANDARD`/`NONE` 构成及未归集清单，并从报告下钻到来源和治理任务；是否承担正式确认由尚待配置的报告复核策略决定。

### 1.2 业务目标

- 将 `T` 以前的五类商城不可变关键事实补入与实时回流完全相同的正式实体和追溯链。
- 回填只追加缺失事实、归集结果和成本评估；绝不覆盖已存在实时事实、原消费、原成本、退款或余额记录。
- `T` 前支付只补台账，不触发供应商下单；履约链始终为 `LEGACY_MANUAL`。
- 对成本按逐笔事实使用商城成本快照、消费时点有效供给版本或 `NONE`，禁止用当前价/猜测税率补齐历史。
- 形成长期可审计的回填报告和处理证据，同时遵守文件、敏感数据和失败诊断保留策略。

### 1.3 不在本工作面完成

- 不修改消费回流启用时间 `T`，不在此重新执行主责迁移（一次性运营行为）。
- 不创建 `T` 前支付对应的供应商订单、取消或退款动作；只回填已发生结果事实。
- 不修订来源商城历史记录、实时事件、正式消费事实或既有成本评估。
- 不在回填页面手工指定成本金额、税率或成本口径。
- 不用 Excel 导入作为历史消费主路径；正式来源是商城受控回填契约。

## 2. 用户、权限与数据范围

| 角色 | 默认入口 | 可见范围 | 主要动作 |
| --- | --- | --- | --- |
| 系统管理员 | 回填任务列表 | 授权商城和环境 | 建立范围、执行、续跑、查看进度、下载审计报告 |
| 上线负责人 | 切换后只读核对 | 指定商城及切换记录 | 查看 `T`、前置检查和正式任务范围；不代替执行或策略授权的报告复核 |
| 财务 | 报告与成本覆盖 | 授权客户、期间和成本范围 | 查看统计、成本口径与未归集清单；仅在报告复核策略授权时执行正式确认 |
| 运营 | 来源与映射协同 | 负责商城、类目和商品范围 | 查看来源缺口、协同商品/卡实例归集；不能启动正式任务 |
| 研发运维 | 运行排障 | 授权环境和连接 | 查看脱敏任务日志、性能和失败分类；不能修改业务结果 |

权限规则：

- 无模块权限隐藏入口；无商城数据范围显示专用空态。
- 正式“开始回填”和“续跑”仅系统管理员可执行，并要求服务端确认唯一 `T` 已启用、`BACKFILL_CAPABILITY` 已通过。
- 财务成本金额、商城用户标识、卡实例引用和失败原值使用独立字段权限；报告导出按当前权限裁剪。
- 报告复核策略未配置时，服务端不得返回报告确认动作；技术处理即使为 `COMPLETED`，页面也只能展示和下载标记“未确认”的技术报告，不得宣称最终业务完成或解锁正式下游。
- 原始商城报文不在普通页面展示；失败详情只显示白名单字段和脱敏错误。
- 任务执行过程中权限收回不取消已提交的后台任务，但立即撤销页面、报告和下载访问；任务继续以系统身份执行并保留原发起人审计。

## 3. 入口、路由与任务页签

| 场景 | 入口 | URL / 页签行为 | 返回位置 |
| --- | --- | --- | --- |
| 浏览任务 | 侧栏“历史消费回填” | `/governance/history-backfill?view=active` | 恢复筛选、分页和滚动 |
| 创建正式任务 | 列表页头 | 在当前页打开范围确认 Sheet；确认后创建任务页签 | 取消回列表原位置 |
| 查看任务 | 列表行 / 后台任务中心 | `/governance/history-backfill/:jobId` | 返回列表筛选不丢失 |
| 下钻未归集 | 任务报告 / 明细 | 打开 W29，携带任务、事实键摘要和原因 | 返回当前报告筛选 |
| 查看经营影响 | 成本口径摘要 | 打开 W28，携带回填范围、成本口径和 `reportReviewStatus`；未确认时目标页必须标记为技术结果且正式门禁保持关闭 | 返回报告原指标 |

TaskTab 身份为 `mall-consumption-backfill:{jobId}`。同一正式任务重复打开只聚焦；技术处理状态、报告确认状态、进度和报告更新均不改变页签身份。刷新恢复任务、当前阶段、明细筛选和分页；临时确认对话框不恢复。范围一旦正式执行即只读，失败续跑仍使用同一 TaskTab 和任务 ID。

## 4. 页面布局

### 4.1 桌面布局

任务列表：

```text
┌ PageHeader：历史消费回填                  [数据水位] [创建正式回填任务]
├ MetricStrip：执行中 | 待归集 | 重叠去重 | NONE 消费 | 失败项
├ ListToolbar：商城 | 处理状态 | 报告确认状态 | 数据范围 | 成本口径 | 搜索
├ BusinessTableFrame
│ 任务号 | 商城 | 范围 [start,T) | 处理状态 | 报告确认 | 进度 | 去重 | 未归集 | 成本覆盖 | 操作
└ 分页
```

任务中心：

```text
┌ PageHeader object-chrome：历史回填 › 任务号               [返回列表] ─┐
├ DocumentHeader compact：回填任务 [处理状态] · 任务号 · 商城 · 范围      │
│ 数据来源版本 · 发起人 · 开始/最近进度时间            [续跑/下载报告]     │
├ ImportStageIndicator：范围确认 → 来源校验 → 事实入库 → 归集评估 → 报告
├ BackgroundJobProgress：总数 / 已处理 / 新增 / 去重 / 待归集 / 失败
├ CostCoverageNotice：ACTUAL | STANDARD | NONE 金额、笔数与原因
├ 概览 | 事实结果 | 去重 | 未归集 | 成本口径 | 失败诊断 | 审计报告
├ 当前子区明细
└ FormalActionResult：任务创建/续跑命令结果与下一步
```

### 4.2 区域说明

| 区域 | 目的 | 主组件 | 是否固定 |
| --- | --- | --- | --- |
| 页头 | 锁定商城、范围、`T` 和任务身份 | `PageHeader object-chrome` + `DocumentHeader density="compact"` `DataFreshness` | 中心顶部固定 |
| 阶段条 | 说明当前后台处理阶段，不伪装同步完成 | `ImportStageIndicator` | 页头下固定 |
| 进度区 | 呈现吞吐、心跳、结果分布和预计剩余 | `BackgroundJobProgress` | 执行中保持可见 |
| 成本覆盖 | 显示 ACTUAL/STANDARD/NONE 构成和风险 | `CostCoverageNotice` | 报告前强制出现 |
| 结果明细 | 查询新增、重复、待归集、失败 | `DataTable` `ImportIssueTable` | 表头固定 |
| 报告区 | 提供可审计范围、统计、清单、版本与独立确认状态 | `DocumentSection` `PaperDocument`/受控下载 | 技术处理完成并生成后可见；未确认也可下载但必须标记 |
| 正式结果 | 固定展示创建/续跑命令结果 | `FormalActionResult` | 动作后保持可见 |

## 5. 展示内容与字段

### 5.1 任务身份与范围

| 区域 | 字段 | 用户文案 | 数据来源 | 口径 / 格式 | 权限规则 |
| --- | --- | --- | --- | --- | --- |
| 身份 | `jobNo` | 回填任务号 | `mall_consumption_backfill_job` | 稳定任务编号 | 有任务权限可打开 |
| 商城 | `mallName/environment` | 来源商城 / 环境 | 来源系统与切换记录 | 生产/验证文字明确 | 环境权限 |
| 边界 | `requiredHistoryStart/rangeStart/rangeEnd` | 必须覆盖起点 / 回填范围 | 目标范围历史边界、回填任务 | `rangeStart` 必须等于目标范围最早业务事实或已登记历史边界 `requiredHistoryStart`；半开 `[rangeStart, T)`，`rangeEnd` 必须等于切换 `T` | 全部任务查看者可见 |
| 覆盖 | `sourceCoverageStart/coverageComplete/coverageGaps` | 来源历史覆盖 | 来源校验结果 | 来源可提供起点晚于 `requiredHistoryStart` 时为阻断性覆盖缺口，不得把较晚时间改成 `rangeStart` 后宣称全历史完成 | 管理员、财务可见 |
| 切换 | `cutoverId/enabledAt` | 消费回流启用时间 T | `mall_consumption_cutover` | 不可修改 | 全部任务查看者可见 |
| 处理状态 | `processingStatus` | 待执行 / 校验中 / 可执行 / 运行中 / 部分完成 / 技术处理完成 / 失败 | 回填任务与后台作业 | 只表达技术处理，不等于报告已确认或全业务完成 | 全部任务查看者可见 |
| 报告确认 | `reportReviewStatus` | 未就绪 / 策略未配置 / 待确认 / 已确认 / 已驳回 | 独立报告复核记录 | 与 `processingStatus` 分开；未确认时不得显示“最终完成” | 有报告查看权可见；正式确认另按策略授权 |
| 水位 | `sourceAsOf/lastProgressAt` | 来源水位 / 最近进度 | 商城回填源与后台任务 | 超过阈值标滞留 | 运维可见详情 |
| 发起 | `requestedBy/requestedAt` | 发起人 / 时间 | `background_job` | 审计显示 | 管理员/财务可见 |

### 5.2 结果统计

| 字段 | 用户文案 | 数据来源 | 口径 |
| --- | --- | --- | --- |
| `totalCount/totalAmount` | 来源事实数 / 金额 | 回填任务来源统计 | 五类关键事实分别统计；金额口径说明 |
| `processedCount` | 已处理 | 后台任务进度 | 不等同于已归集 |
| `insertedCount` | 新增正式事实 | 回填明细 | 以业务事实键首次形成 |
| `deduplicatedCount` | 重叠去重 | 回填明细 | 与实时或原任务重跑重叠；不形成第二份事实 |
| `unattributedCount` | 待归集 | 回填明细 / 错误任务 | 原始事实已保存、经营归属尚未完成 |
| `failedCount` | 处理失败 | 回填明细 | 区分可续跑与需要业务修复 |
| `actual/standard/noneCount` | 成本口径笔数 | 成本评估链尾 | 按消费记录，不按卡券分组 |
| `basisConsumptionAmount` | 各口径消费金额（含税） | 成本口径报告 | ACTUAL + STANDARD + NONE 与可归集卡券消费守恒 |
| `costCoverageRate` | 回填成本覆盖率 | 服务端报告 | 有成本消费金额 ÷ 总消费金额；NONE 进分母 |

### 5.3 明细与报告

| 子区 | 必须展示 | 事实来源 | 规则 |
| --- | --- | --- | --- |
| 事实结果 | 事实类型、业务事实键摘要、商城订单、发生时间、结果（新增/重复/待归集/失败） | `mall_consumption_backfill_item`、`mall_order_fact` | 同一订单下不同事实不合并 |
| 去重 | 实时/回填来源、原消息、当前任务项、命中正式事实 | inbox / 业务事实键 | 去重证明同一事实只留一份，不写“跳过数据”含糊文案 |
| 未归集 | 商品、支付来源、卡实例、销售单、税口径或成本缺口 | 归集状态与错误任务 | 原始事实仍已保存；提供 W29/W21 去向 |
| 成本口径 | ACTUAL 来源、STANDARD 供给版本、NONE 原因、税口径 | 成本评估链 | STANDARD 必须命中消费发生时点版本，禁止当前价 |
| 失败诊断 | 错误码、阶段、重试资格、来源记录摘要 | 回填项 / 后台任务项 | 不显示完整敏感原文；失败不删除已成功项目 |
| 审计报告 | 必须覆盖起点、正式范围、`T`、总笔数/金额、去重、三口径占比、未归集和失败清单、规则/Schema 版本、操作者、处理状态与报告确认状态 | 正式报告文件 | `processingStatus=COMPLETED` 后可生成和下载技术报告；`reportReviewStatus` 未确认时必须固定标“未确认”，只有来源覆盖完整且报告按有效策略确认为 `CONFIRMED` 才可标“全历史回填最终完成”并由服务端决定正式下游解锁 |

五类事实必须分别保留：`PAYMENT_SUCCEEDED`、`ORDER_CANCELED`、`REFUND_SUCCEEDED`、`ORDER_COMPLETED`、`CARD_BALANCE_RESTORED`。商城订单号不是唯一幂等键；同一订单的支付、取消、完成、多次部分退款和多次余额恢复均为不同正式事实。

## 6. 搜索、筛选、排序与默认视图

### 6.1 任务列表

| 能力 | 默认值 | URL 状态 | 行为 |
| --- | --- | --- | --- |
| Saved View | `active` | `view=active` | 按技术处理状态展示待执行、运行中、部分完成、失败；可切全部/技术处理完成/报告待确认 |
| 商城 / 环境 | 当前生产商城 | `mallId=` / `environment=` | 环境文字明确，数据隔离 |
| 处理状态 | 活跃集合 | `processingStatus=` | 固定技术处理状态，不混入报告确认 |
| 报告确认状态 | 全部 | `reportReviewStatus=` | 独立筛选策略未配置、待确认、已确认和已驳回 |
| 范围 | 全部 | `rangeFrom=` / `rangeTo=` | 仅查询任务，不允许改已执行范围 |
| 成本风险 | 全部 | `basis=NONE` / `coverage=` | 筛选含 NONE 或覆盖不足任务 |
| 搜索 | 空 | `q=` | 精确匹配任务号、商城、报告号 |
| 排序 | 运行中优先、最近进度升序 | `sort=` | 滞留任务优先处理 |

### 6.2 任务明细

| 能力 | 默认值 | URL 状态 | 行为 |
| --- | --- | --- | --- |
| 结果 | 全部 | `result=inserted/deduplicated/unattributed/failed` | 指标点击同步筛选 |
| 事实类型 | 全部五类 | `factType=` | 多选，不合并多次退款/恢复 |
| 成本口径 | 全部 | `costBasis=` | 仅适用消费条目；非消费事实显示不适用 |
| 原因 | 全部 | `reasonCode=` | 固定错误/未归集原因 |
| 时间 | 任务范围 | `occurredFrom=` / `occurredTo=` | 不得超过 `[rangeStart,T)` |
| 搜索 | 空 | `q=` | 商城订单号、退款/恢复单号、事实键摘要 |
| 分页 | 100 条 | `page=` / `pageSize=` | 服务端分页/排序 |

刷新执行中任务时保留旧进度并轮询/订阅新水位；页面不可用浏览器轮询结果推断后台成功。浏览器后退恢复明细筛选，任务正式范围永远只读。

## 7. 操作契约

| 操作 | 入口 | 权限 / 前置条件 | 确认 | 成功结果 | 失败恢复 |
| --- | --- | --- | --- | --- | --- |
| 创建范围草稿 | 列表页头 | 管理员；唯一 `T` 已启用；服务端已登记目标范围 `requiredHistoryStart`；无另一正式重叠任务 | 展示必须覆盖起点、来源可提供起点、`T` 和预计规模 | 以服务端计算的 `[requiredHistoryStart,T)` 生成待执行任务草稿和范围摘要 | 历史边界未知或来源校验失败不形成正式执行 |
| 校验来源 | 任务范围阶段 | 管理员；来源契约可用 | 无 | 返回五类事实从 `requiredHistoryStart` 至 `T` 的连续覆盖、Schema、金额/分摊/税字段问题和预计结果 | 覆盖起点偏晚或存在区间缺口时保持阻断；修复来源后重验 |
| 开始正式回填 | 页头主动作 | `BACKFILL_CAPABILITY` 通过；`rangeStart=requiredHistoryStart`、`rangeEnd=T`、`coverageComplete=true`；无重叠正式任务 | `FormalActionConfirmDialog` 明示全历史范围、只追加、T 前不下单、不可改范围 | 冻结范围并创建后台任务，固定展示任务号 | 提交超时按 operation ID 查询，不新建第二任务；覆盖不足不得开始 |
| 续跑失败/中断任务 | 页头 | 原任务为部分完成/失败且允许续跑 | 展示已成功、待处理和幂等影响 | 沿原任务、原范围、原事实键续跑 | 再失败保留进度和失败明细 |
| 重新归集待归集项 | 未归集区 | 映射/基线/税口径已通过正式路径补齐 | 展示原事实和修复证据 | 引用原事实重新归集，必要时追加成本评估 | 不复制原事实；失败更新任务/错误记录 |
| 查看 / 处理原因 | 未归集/失败行 | 有目标 Wxx 权限 | 无 | 打开 W29/W21 等目标工作面 | 返回当前行并刷新 |
| 下载报告 | 报告区 | 技术报告已生成、有当前导出权限 | 展示范围、字段、敏感级别、报告确认状态和过期时间 | 下载受控报告并记录审计；未确认时文件和下载结果固定标“技术报告 · 未确认” | 链接过期后重新鉴权生成；不因下载改变确认状态 |
| 正式确认报告 | 报告区 | `processingStatus=COMPLETED`，报告复核策略已配置，当前角色满足策略且服务端返回确认动作 | 按策略展示报告版本、确认责任和影响的正式下游 | 仅迁移 `reportReviewStatus` 并追加独立审计；确认后仍不改写已入库事实或技术处理状态 | 策略缺失时不显示动作；版本冲突、权限或证据不足时保持未确认 |
| 导出明细 | 当前筛选 | 有明细导出权限 | `BatchImpactPreview` | 创建后台导出任务，不影响回填 | 失败报告保留筛选摘要 |

明确禁止：

- 不提供“删除并重跑”“覆盖已存在事实”“把 NONE 改成 0”“改用当前供给价”“修改 T”按钮。
- 不允许新建另一个正式批次覆盖同一 `[rangeStart,T)`；修复只能续跑原任务。
- 已成功写入的正式事实不因任务取消、失败或续跑而回滚；取消仅停止尚未开始的后台项（若实现取消能力，须在 API 契约中显式启用）。

## 8. 数据契约

### 8.1 查询

```ts
type HistoryBackfillProcessingStatus =
  | "DRAFT"
  | "VALIDATING"
  | "READY"
  | "RUNNING"
  | "PARTIAL"
  | "COMPLETED"
  | "FAILED"

type HistoryBackfillReportReviewStatus =
  | "NOT_READY"
  | "POLICY_NOT_CONFIGURED"
  | "PENDING"
  | "CONFIRMED"
  | "REJECTED"

type HistoryBackfillListQuery = {
  view: "active" | "processing_completed" | "report_pending" | "all"
  mallId?: string
  environment?: "production" | "verification"
  processingStatuses?: HistoryBackfillProcessingStatus[]
  reportReviewStatuses?: HistoryBackfillReportReviewStatus[]
  rangeFrom?: string
  rangeTo?: string
  basis?: "ACTUAL" | "STANDARD" | "NONE"
  coverage?: "below_threshold"
  q?: string
  sort: string
  page: number
  pageSize: number
}

type HistoryBackfillDetailQuery = {
  jobId: string
  results?: Array<"INSERTED" | "DEDUPLICATED" | "UNATTRIBUTED" | "FAILED">
  factTypes?: string[]
  costBases?: Array<"ACTUAL" | "STANDARD" | "NONE">
  reasonCodes?: string[]
  occurredFrom?: string
  occurredTo?: string
  q?: string
  page: number
  pageSize: number
  sort: string
}

type HistoryBackfillDetailView = {
  job: {
    id: string
    jobNo: string
    mallId: string
    mallName: string
    cutoverId: string
    requiredHistoryStart: string
    rangeStart: string
    rangeEnd: string
    cutoverAt: string
    sourceCoverageStart?: string
    coverageComplete: boolean
    coverageGaps: Array<{ from: string; to: string; reasonCode: string }>
    processingStatus: HistoryBackfillProcessingStatus
    reportReviewStatus: HistoryBackfillReportReviewStatus
    reportReviewPolicy?: {
      reportReviewPolicyId: string
      reportReviewPolicyVersion: number
    }
    formalDownstreamUnlocked: boolean
    lockVersion: number
    requestedBy: ActorView
  }
  progress: {
    totalCount: number
    processedCount: number
    insertedCount: number
    deduplicatedCount: number
    unattributedCount: number
    failedCount: number
    lastProgressAt?: string
  }
  costBasis: Array<{
    basis: "ACTUAL" | "STANDARD" | "NONE"
    count: number
    consumptionAmountGross: string
    costAmountNet?: string
  }>
  coverageRate: string | null
  itemsPage: Page<HistoryBackfillItemView>
  report?: {
    reportId: string
    reportVersion: number
    file: AuthorizedFileLink
    generatedAt: string
    reviewLabel: "UNCONFIRMED" | "CONFIRMED"
  }
  allowedActions: string[]
  actionBlockers: ActionBlocker[]
  sourceAsOf: string
  permissionVersion: string
  queriedAt: string
}
```

列表指标和任务进度使用服务端统一统计；前端不从当前明细页求和。执行中进度允许异步更新，但正式事实数量、成本口径和报告必须来自任务快照及正式实体核对。`processingStatus` 与 `reportReviewStatus` 是独立状态源；仅当报告复核策略存在且 `reportReviewStatus=CONFIRMED` 时，服务端才可在其它业务门禁也通过后返回 `formalDownstreamUnlocked=true`。策略缺失或报告未确认时该值强制为 `false`，前端不得根据 `COMPLETED` 推导。

### 8.2 提交

```ts
type HistoryBackfillCommand = {
  jobId?: string
  cutoverId: string
  action: "CREATE_DRAFT" | "VALIDATE_SOURCE" | "START" | "RESUME" | "REATTRIBUTE"
  expectedLockVersion?: number
  rangeStart: string
  rangeEnd: string
  operationId: string
  idempotencyKey: string
  itemIds?: string[]
}
```

- `rangeStart` 必须等于服务端登记的目标范围 `requiredHistoryStart`，即最早业务事实或来源历史边界；客户端不能选择较晚起点缩小范围。`rangeEnd` 必须等于 `cutover.enabledAt`；`START` 后范围和切换引用不可修改。
- `START` 前服务端必须验证五类事实在 `[requiredHistoryStart,T)` 连续可取且 `coverageComplete=true`。来源覆盖开始时间更晚、任何区间缺失或边界无法证明均阻断；不得仅在报告记录缺口后仍把任务标为完整历史回填。
- `START` 的 idempotency key 与正式任务唯一绑定；网络超时先查询原 operation，不创建新任务。
- `RESUME` 必须引用原 `jobId`、原范围和原幂等命名空间；逐项仍使用相同业务事实键。
- 所有来源消息先走统一 inbox 消息幂等，再按 `businessFactKey` 做正式事实幂等。
- `REATTRIBUTE` 只更新归集处理状态并追加成本评估/成本差额，不修改 `mall_order_fact`、原消费或旧成本评估。
- `HistoryBackfillCommand` 只推进技术处理，不能写 `reportReviewStatus`。报告复核策略未配置时不存在可调用的报告确认命令；配置后必须使用策略注册的独立、幂等且带报告版本的确认契约，不能复用 `START/RESUME` 或把技术 `COMPLETED` 当确认。

### 8.3 前端边界

- 前端不生成业务事实键、不判断去重、不计算成本口径、不用当前价补历史成本。
- 前端不推断或放宽 `requiredHistoryStart`，也不允许用来源当前最早可取时间覆盖服务端登记的全历史边界。
- 是否为 `T` 前支付、履约链是否 `LEGACY_MANUAL`、是否创建供应商订单由服务端强制；页面只展示结果。
- 前端只格式化进度、金额、时间、口径说明和错误文案，不修改任务范围或正式报告统计。
- 原始报文、完整商城用户信息和敏感履约地址不进入页面状态、URL、日志或埋点。

### 8.4 缓存、新鲜度与失效

- 列表 Query Key 包含用户、角色、权限版本、商城/环境、任务筛选、范围、成本风险、排序和分页。
- 对象 Query Key 包含 `jobId`、`sourceAsOf` 和明细全部筛选/分页；任务进度、正式事实统计和报告链接分区缓存，局部失败不得清空其它分区。
- 执行中按服务端建议轮询或订阅；页面显示来源水位与最近进度时间，后台静默不等于成功。
- 开始、续跑或重新归集得到确定结果后按返回的任务版本和事实 ID 定向失效；结果未知期间不新建任务、不乐观增加成功数。

## 9. 页面状态矩阵

| 状态 | 页面表现 | 可执行动作 | 恢复方式 |
| --- | --- | --- | --- |
| 初载 | 列表/任务中心结构 Skeleton | 应用壳导航 | 原位替换 |
| 刷新 | 保留进度和统计，显示最近心跳 | 可继续查看；正式动作重验 | 成功更新；失败标陈旧 |
| 无任务 | “尚未创建历史回填任务” | 满足前置时创建草稿 | 创建后进入任务中心 |
| 筛选无结果 | 保留筛选摘要 | 清除筛选 | 默认活动视图 |
| 无数据范围 | 专用无范围空态 | 查看角色 / 申请权限 | 范围变化后重查 |
| 查询失败 | 无缓存显示 `BusinessFailureState`；有缓存保留进度/统计并标陈旧 | 重试；正式动作禁用 | 查询恢复 |
| 前置未满足 | 展示缺失的 `T`、能力检查、来源契约或权限 | 去 W29 处理 | 前置满足后重验 |
| 全历史覆盖不足 | 展示 `requiredHistoryStart`、`sourceCoverageStart` 和缺失区间；“开始正式回填”禁用，不允许改晚 `rangeStart` | 修复来源契约、补齐历史数据、重新校验 | `[requiredHistoryStart,T)` 连续覆盖验证通过 |
| 来源校验失败 | 显示五类事实、Schema、金额/税字段问题 | 修复来源、重新校验 | 校验通过后允许开始 |
| 任务版本冲突 | 展示 `lockVersion`、范围或来源水位变化，旧确认失效 | 刷新并重新审阅 | 基于当前任务版本提交 |
| 等待执行 | 范围和影响已冻结 | 开始正式回填 | 后台任务启动 |
| 执行中 | 阶段条、进度、吞吐、最近心跳；旧结果可查 | 查看明细/治理任务 | 正常完成或部分完成 |
| 进度滞留 | 警示最近心跳和任务阶段 | 查看 W29、授权续跑（任务失败后） | 任务恢复或失败终态 |
| 部分完成 | 已成功/去重事实保持，失败与待归集单列 | 续跑原任务、处理原因 | 原任务追平 |
| 失败 | 固定失败摘要和可重试分类 | 续跑原任务、查看 W29 | 不删除已成功事实 |
| 去重命中 | 明确显示“已存在同一业务事实”，关联原正式事实 | 查看原事实 | 无需重新写入 |
| 待归集 | 原事实已保存，显示缺失环节和责任人 | 去修复、重新归集 | 追加归集/成本评估 |
| NONE 成本 | 风险提示和原因；成本字段为空而非 0 | 去 W29/W21 查看原因 | 取得依据后追加新评估 |
| 技术处理完成、报告策略未配置 | 固定技术统计和“报告确认策略未配置”，报告标“未确认” | 下载未确认技术报告、配置策略 | 策略配置后进入独立待确认状态；不得宣称最终完成或解锁正式下游 |
| 技术处理完成、报告待确认 | 固定技术统计、报告版本和待确认责任摘要 | 下载未确认报告；策略授权者可正式确认 | 确认或驳回只改变报告确认状态 |
| 报告已驳回 | 技术处理仍为 `COMPLETED`，报告显示驳回和受控整改入口 | 下载未确认报告、按策略整改/重新生成 | 新报告版本重新进入待确认，不回滚已入库事实 |
| 报告已确认 | 分别展示技术处理完成与报告已确认，满足其它服务端门禁时才显示“全历史回填最终完成” | 下载已确认报告、进入已解锁的正式下游 | 报告长期可追溯，确认不改写既有事实 |
| 正式动作结果不确定 | 不新建第二任务；显示查询原操作 | 查询最终结果 | 找到原任务或确认未创建 |
| 字段级隐藏 | 成本/来源敏感值掩码 | 其余授权查看 | 权限更新后重查 |
| 权限收回 | 清除报告、明细和下载链接；后台任务不回滚 | 返回有权模块 | 权限恢复后重查 |

## 10. 响应式与键盘

| 视口 | 布局变化 | 保留内容 | 允许降级 |
| --- | --- | --- | --- |
| 1440×900 | 列表高密度；任务中心进度、覆盖和明细同屏 | 任务范围、T、阶段、进度、三口径和主动作 | 无 |
| 1280×800 | 统计紧凑；明细横向滚动 | 正式范围、去重、未归集、失败 | 次要来源版本移详情 |
| 1024×768 | 图标侧栏；阶段与统计分两行 | T 前不下单提示、续跑原任务、覆盖率 | 审计时间线折叠 |
| 768×1024 | 导航抽屉；指标 2×N；明细固定事实身份和结果列 | 任务身份、范围、进度、结果和原因 | 高级筛选折叠 |
| 375×812 | 单列只读进度和报告摘要 | 任务号、范围、技术处理状态、报告确认状态、进度、ACTUAL/STANDARD/NONE、失败数 | 不提供创建、开始、续跑、重新归集和明细导出；提示桌面处理 |

键盘顺序：页头 → 指标 → 筛选 → 任务行 → 阶段条 → 进度 → 成本口径 → 子区明细 → 正式动作。创建/开始确认关闭后焦点回原动作；任务开始成功后焦点落结果标题并播报任务号。进度变化使用节制的 `aria-live=polite`，分别播报“技术处理完成”和“报告已确认”，不得用一次“完成”播报合并两种状态，也不持续朗读每一条计数。

## 11. 与其他工作面的关系

| 来源 / 去向 | Wxx | 携带上下文 | 返回规则 |
| --- | --- | --- | --- |
| 供应商商品映射 | W21 | 历史商品、消费时点、映射缺口 | 修复后回 W30 重新归集 |
| 商城消费订单 | W25 | 正式事实、商城订单、支付来源 | 返回原回填明细 |
| 卡券经营分析 | W28 | 回填范围、成本口径、覆盖率和报告确认状态 | 报告未确认时只能作为显式标记的技术结果查看，不得解锁或冒充正式最终口径；返回报告原指标 |
| 接口错误与对账 | W29 | 任务、事实键摘要、未归集/失败原因 | 解决后回原任务续跑或归集 |
| 销售单 | W05 | 原销售单身份、卡券明细 | 返回未归集行 |

## 12. 验收清单

### 12.1 范围与幂等

- [x] `rangeStart` 固定等于目标范围最早业务事实或已登记历史边界 `requiredHistoryStart`，正式任务范围严格为 `[requiredHistoryStart,T)`。
- [x] 来源覆盖起点晚于 `requiredHistoryStart`、存在任一区间缺口或边界无法证明时阻断正式执行；不能缩晚起点后标记“全历史完成”。
- [x] `occurredAt = T` 不进入历史回填，按实时/补投契约处理。
- [x] `T` 前支付只补台账，全部标记 `LEGACY_MANUAL`，不创建供应商订单。
- [x] 五类关键事实完整回填，同一订单下支付、取消、完成、多次退款和多次余额恢复不会被合并。
- [x] 实时与回填按同一业务事实键去重，只形成一份正式事实。
- [x] 失败或中断只续跑原任务、原范围和原幂等键，不新建重叠正式批次。

### 12.2 追加式事实与成本

- [x] 回填不覆盖现有实时事实、消费、退款、余额恢复、成本或成本评估。
- [x] 商城订单成本有完整税口径时标 ACTUAL；否则按消费时点供给版本标 STANDARD；仍无来源标 NONE。
- [x] 不使用当前供给价、不猜测税率、不用销项税率替代进项税率。
- [x] NONE 成本为空而不是 0，只进入消费金额和覆盖率分母。
- [ ] 映射修复后重新归集引用原事实，成本改善通过追加评估和差额表达。

### 12.3 报告、权限与体验

- [x] 技术报告及其确认后版本均包含范围、T、总笔数/金额、去重数、ACTUAL/STANDARD/NONE、覆盖率、未归集和失败清单。
- [x] 报告、列表和明细统计使用同一任务快照并可追溯规则/Schema 版本。
- [x] `processingStatus` 与 `reportReviewStatus` 分开验收；技术 `COMPLETED` 不等于报告已确认或全业务完成。
- [x] 报告复核策略未配置或报告未确认时，下载文件固定标“未确认”，确认动作和正式下游门禁 fail-closed；不会仅因技术完成解锁。
- [x] 普通页面、日志和导出不泄露卡号、卡密、绑定手机号、完整地址或原始敏感报文。
- [ ] §9 状态和 §10 五档视口全部验收。
- [x] 后台任务不伪装同步完成；进度滞留、部分完成和失败均有明确恢复路径。

## 13. 待确认事项

| ID | 问题 | 影响 | 建议决策人 | 当前建议 |
| --- | --- | --- | --- | --- |
| Q1 | 回填总量达到何种规模时按内部切片执行？ | 吞吐、进度和故障恢复 | 技术负责人 | 单个正式任务固定全范围，内部可按时间/哈希切片；切片不成为新的业务批次 |
| Q2 | 最终报告由谁进行业务确认，并具体解锁哪些正式下游？ | 报告责任、状态机、审计与下游门禁 | 财务负责人 + 系统管理员 | `processingStatus` 与 `reportReviewStatus` 分开；未配置报告确认策略时不得宣称最终完成或解锁正式下游，技术报告可下载但固定标“未确认”，待确认不回滚已入库事实 |
| Q3 | 历史来源无法提供成本含税标识/进项税率的预期比例是多少？ | NONE/ STANDARD 水位和项目准备 | 商城负责人 + 财务 | 切换前试算并设监控目标，但不通过前端猜测补齐 |
| Q4 | 执行中是否需要“停止尚未开始项目”的取消能力？ | 操作契约和后台任务状态 | 系统管理员 + 技术负责人 | 默认不提供；紧急停止由运维控制，已提交事实永不回滚 |

## 14. 业务依据

- `erp-phase-2.md` §9.2、§12.1.1、§12.3.2：T 边界、五类事实、历史成本降级和可审计回填报告。
- `erp-phase-2.md` §13.1、§18.2：消息/业务事实两层幂等、实时与回填去重、回填在 T 后执行。
- `erp-data-model.md` §6.17 `mall_consumption_backfill_job/item`：范围、状态、结果、成本口径和报告字段。
- `erp-data-model.md` §6.1 `background_job`：后台进度、部分成功、取消不回滚和文件保留规则。
- `erp-data-model.md` §8.4、§9.4：商城事实接收、成本评估和回填不覆盖正式事实的不变量。
- `erp-ui-design.md` §4.8、§11：M7 分阶段治理、后台任务、正式结果和状态契约。
- `erp-ui-flows.md` §11–§12：消费事实到经营分析、供应商订单与异常治理的钻取关系。
