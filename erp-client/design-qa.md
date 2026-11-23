# ERP 视觉与交互质量验收基线（Design & Interaction QA Baseline）

> **适用范围**：`erp-client` 全站工作台、任务与审批队列、4 大原型办理工作台、角色工作台（`WB-01..WB-05`）、浮层与双轨抽屉、键盘效率引擎及 UI 主题静态门禁。
> **架构真源**：[`docs/task-interaction-refactoring-plan.md`](../docs/task-interaction-refactoring-plan.md)、[`docs/typography-design-system.md`](../docs/typography-design-system.md)、[`erp-client/AGENTS.md`](./AGENTS.md)。

---

## 1. 视觉语言与设计令牌（Feishu Universe Flat Hairline + 5+4 字号体系）

### 1.1 平面化细线视觉基因

- **表面分层**：极浅冷灰画布 `--background`（`#f7f8fa`）+ 纯白工作面/卡片 `--card`（`#ffffff`）+ 极净近白双轨侧栏 `--sidebar` + 1px 细线分隔（`border-border/60`、`border-grid`）。
- **交互主色**：Feishu 商务蓝 `--primary`（`#1f6aee`，白字对比度 4.8:1，满足 WCAG AA）；正文主墨 `--foreground`（`#1f2227`），次级弱化 `--muted-foreground`（`#656b72`）。
- **零套娃卡片**：禁止在详情工作面或工作台内嵌区使用 `surfacePanelClassName`、旧版 `PageHeader` / `DocumentHeader` / `DetailPageHeader` / `ObjectSectionTabs` 或带厚阴影/粗边框的嵌套 `<Card>`；分区统一采用 `DetailWorkspaceSection` + 水平细线。
- **金额与状态规范**：
    - 金额统一消费 `MoneyValue` + `.num`（`tabular-nums`）保持沉稳墨色，不按量级变彩虹色。
    - 状态徽章统一消费 `StatusBadge` + [`lib/task-status-dictionary.ts`](./lib/task-status-dictionary.ts) 唯一真源（**严禁裸圆点表达状态**）：
        - 审批实例：`RUNNING` → `info`（审批中）、`BLOCKED` → `destructive`（审批受阻）、`APPROVED` → `success`（已通过）、`REJECTED` → `destructive`（已驳回）、`CANCELLED` → `void`（已撤回）。
        - 审批节点：`ACTIVE` → `warning`（待审批）、`SUPERSEDED` → `neutral`（往轮已替代）。
        - 事项任务：`OPEN` → `info`（待处理）、`APPROVAL_BLOCKED` / `EXECUTION_BLOCKED` → `destructive`（审批受阻 / 执行受阻）、`COMPLETED` → `success`（已完成）、`CLOSED` → `neutral`（已关闭）。
- **顶栏双徽章视觉分化**：`InboxPopover` 使用业务红点徽章并内置「全部标为已读」；`SystemFeedbackPanel` 使用琥珀色警告轮廓徽标（`warning-soft` + 警告三角），明确区分业务待阅与系统异常提示。

### 1.2 5+4 排版与字号角色（`app/globals.css` × `lib/theme-tokens.ts`）

所有界面严格消费全局 5 档正文角色 + 4 档关键数值角色，禁止手写任意字号/行高/原始色盘：

| 分类   | 令牌类               | 字号 / 行高   | 适用场景                                   |
| ------ | -------------------- | ------------- | ------------------------------------------ |
| 正文 1 | `text-tiny`          | `11px / 16px` | 微胶囊标签、极紧凑徽章与快捷键标识         |
| 正文 2 | `text-xs`            | `12px / 18px` | 元信息行、表头、时间戳、弱化辅助说明       |
| 正文 3 | `text-body-compact`  | `13px / 20px` | 桌面主正文、表格单元格、表单输入、按钮文案 |
| 正文 4 | `text-section-title` | `15px / 22px` | 分区标题、弹窗标题、抽屉标题               |
| 正文 5 | `text-page-title`    | `18px / 26px` | 工作面吸顶主标题                           |
| 数值 1 | `text-value-compact` | `20px / 28px` | 紧凑指标带、摘要金额                       |
| 数值 2 | `text-value-section` | `24px / 32px` | 角色工作台单行 KPI 主数值                  |
| 数值 3 | `text-value-summary` | `28px / 36px` | 核心汇总数值                               |
| 数值 4 | `text-value-hero`    | `40px / 44px` | 概览聚焦主数字                             |

---

## 2. `@container/workspace` 响应式验证矩阵

宿主壳双轨侧栏展开宽 `240px`（`--spacing-nav`）、折叠宽 `60px`（`--spacing-nav-icon`）。子 iframe `<main id="main-content">` 声明主命名容器 `@container/workspace`，所有顶层工作台栅格一律消费命名容器断点（禁止在 `features/workspace/**` 使用视口 `xl:grid-cols-`），同时宿主 `WorkspaceTabs` 将视口高度档位 `data-viewport-density`（`comfortable` / `compact ≤960px` / `dense ≤720px`）同步下发至子 iframe。

| 物理视口                     | 侧栏展开态 iframe 净宽 | 侧栏折叠态 iframe 净宽 | 生效命名容器断点                                            | 必须通过的布局验收点                                                                                                                                                                                                                                                                                                                                                                                                |
| ---------------------------- | ---------------------- | ---------------------- | ----------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **`1280×800`**（紧凑笔记本） | **1040px**             | **1220px**             | 展开：`@[960px]/workspace:`<br>折叠：`@[1120px]/workspace:` | 1. 角色工作台 KPI **单行 4 列不折行**（`@[960px]/workspace:grid-cols-4`），优先待办出现在 ≤192px 内；<br>2. 筛选托盘保持 **4 列**（`@container workspace (min-width: 60rem)`）；<br>3. 审批完整页进入 **66/34 双栏**（右轨 ≥340px）；<br>4. 展开态列表单击行以 **`TaskActionSheet`（768px）** 快审；折叠态（1220px）开启 **Master-Detail 分栏**；<br>5. 列表通栏 36px 紧凑行首屏可见 **16–17 行**，无横向滚动溢出。 |
| **`1440×900`**（主力桌面）   | **1200px**             | **1380px**             | 展开 & 折叠：`@[1120px]/workspace:`                         | 1. 列表页开启 **Master-Detail 分栏**（`WorkspaceSplitQueue`，左 36% / 右 64% 可拖拽）；<br>2. 列表通栏单页 Chrome ≤158px（多页 ≤194px，数据区占比 ≥82%），36px 单行首屏可见 **19–20 行**；分栏摘要卡首屏可见 **13–14 条**；<br>3. 审批工作台明细上方纯占高 **≤148px**（顶+底总 Chrome ≤196px），商品明细与差异 100% 进首屏；<br>4. 右侧上下文轨支持拖拽至 **50%** 宽度左右对照合同 PDF / 银行回单，零遮挡。         |
| **`1920×1080`**（宽屏桌面）  | **1680px**             | **1860px**             | 展开 & 折叠：`@[1480px]/workspace:`                         | 1. 列表分栏右窗格充裕展示业务明细与附件对照；<br>2. 审批/结算/采购宽表零非预期横向滚动；<br>3. 流程配置画布（`DefinitionFlowCanvas`）自适应 `clamp(480px, calc(100dvh - 16rem), 960px)`，无高分屏底部截断留白。                                                                                                                                                                                                     |
| **窄窗口**（`< 960px`）      | `< 720px`              | `< 900px`              | 默认单栏                                                    | 1. 导航抽屉可正常开关，页面不横向撑破；<br>2. 列表工具栏按 `AGENTS.md §8` 自然换行，新建/导出/登记/调整操作全部保留；<br>3. 任务预览与快审自动回落至 **`TaskActionSheet`（768px / 最大 92vw）**。                                                                                                                                                                                                                   |

---

## 3. 四大任务原型验收标准（`UnifiedTaskWorkbench` × `WorkspaceSplitQueue`）

| 原型                                   | `archetype` 值  | 涵盖范围                                                                                                                                                                                                            | 必须验证的布局与交互契约                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| -------------------------------------- | --------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Archetype 1 · 决策审批型**           | `decision`      | 13 类必审单据（`DOCUMENT_APPROVAL`）、我发起的审批（`APPROVAL_INSTANCE`）、退回重办（`APPROVAL_REWORK`）                                                                                                            | - 宽屏（`@[960px]/workspace:`）启用可拖拽/可折叠 **66/34 双栏**（按 `]` 一键收起/展开右轨）；<br>- 左侧为 56px 紧凑单行页头 + 紧凑 KPI 带 + 业务明细（销售单多地址行内展开、开票审批 3 分段 Tab 零嵌套 `<details>`、变更/盘点/财务使用 `StructuredDiffApprovalRenderer` / `FinancialApprovalRenderer`）；<br>- 右侧常驻 `ApprovalRouteStepper`（44px）+ `UnifiedAuditTrail` + `UnifiedMaterialViewer`（docked 模式）；<br>- 13 类单据提交前 **100% 展示 `SubmissionRouteConfirmation`**；撤回全站统一为 **`ApprovalWithdrawDialog`**（红色 `destructive` + BPM 512 UTF-8 字节计数 + 幂等恢复）。                                                                                                                                                           |
| **Archetype 2 · 结构化作业执行型**     | `action`        | 履约（`FULFILLMENT_OPERATION`）、客户验收（`CUSTOMER_ACCEPTANCE_REGISTRATION`）、付款执行/安排、开票执行、采购供给分配、采购依据变更、退货退款/红票、结算复核、导入确认、供应商新品审核（`SUPPLIER_PORTAL_REVIEW`） | - **100% 通栏专注画布**，彻底撤除冗余 `20rem` 通用侧栏；<br>- 所有嵌入工作台的领域页（`SupplierOrderCenterPage`、`BatchDetailView`、`SettlementCenter`、`IntegrationErrorsPage`）严格遵守 `embedded` 契约，**零双页头、零双侧栏、零双水平内边距**；<br>- 客户验收登记表与附件上传直接内联正文，仅提交时弹单层 `FormalActionConfirmDialog`；<br>- `SUPPLIER_PORTAL_REVIEW` 采用 **4 组粘性锚点折叠步骤** + 「一键应用默认单位到全部 SKU」；<br>- 退货退款/红票与付款安排提交成功后反应式更新缓存并自动触发 `onTaskCompleted` + `autoNext`，无底栏冗余刷新按钮；<br>- 14 类可转交事项 **100%（14/14）** 在 `WorkspaceTaskHeaderActions` 按 `allowedActions` 挂载 `WorkItemResponsibilityDialog`（非 `PROCESS` 态仍保留只读正文 + 管理者页头转交/关闭入口）。 |
| **Archetype 3 · 异常排查与治理型**     | `investigation` | 外部同步待核实（`INTEGRATION_RESULT_UNKNOWN`）、业务异常（`BUSINESS_EXCEPTION`）、受阻审批治理（`/workspace/approvals?view=blocked\|managed\|mine`）                                                                | - 顶部异常诊断横幅 + 扁平排查正文（去嵌套 `<Card>`）+ 可折叠右轨（轨迹/原始载荷）；<br>- `/workspace/approvals` 四视图（`started` / `mine` / `blocked` / `managed`）均可正常加载与翻页，零 HTTP 422；<br>- `/workspace/tasks?view=managed` 与异常列表支持多选 **批量转交**（并发拉取候选人并求交集、`reason ≤ 150 字`）与 **批量人工关闭**（`reason_code` + `comment ≤ 100 字` + `DUPLICATE` 必填替换单号）。                                                                                                                                                                                                                                                                                                                                              |
| **Archetype 4 · 知会抄送与系统动态型** | `informational` | 抄送给我（`/workspace/copies`，5 种触发时机）、顶栏 `InboxPopover`、`SystemFeedbackPanel`、`/account/*`、`/governance/background-jobs`                                                                              | - `/workspace/copies` 纳入统一队列壳（`pageSize=50`、单页隐藏分页条、即时搜索筛选、分栏/抽屉速览）；<br>- 单击行自动标已读并就地预览 `bare` 无边框快照 + `UnifiedMaterialViewer` + `UnifiedAuditTrail`；详情支持左上角返回、`上一项/下一项`、`J/K` 与 `autoNext`；支持勾选批量标已读；<br>- `/account/workbench` 与 `/account/data-transfers` 始终在多标签工作台壳内打开，不逃逸出 Tab 壳。                                                                                                                                                                                                                                                                                                                                                                |

---

## 4. 浮层体系、双轨抽屉契约与零弹窗套娃

### 4.1 双轨抽屉治理（ADR-3 × `AGENTS.md §7`）

- **轨道一 · 只读轻预览 `QuickPreviewSheet`**（`components/business/list.tsx`）：
    - 仅用于主数据/目录点读、角色工作台概览指标下钻（`WorkspaceOverviewDetail`）与只读查看，**严禁接入 `useAppForm`**。
    - 尺寸仅由 `size="preview"`（`400px`）或 `size="detail"`（`768px`）决定，禁止传 `contentClassName` / `overlayClassName`。
    - 槽位顺序固定为 `identity → title → description → summary`，正文外边距统一为 **`px-sheet-inline py-5`**（严禁手写 `px-7`），关闭后焦点归还至 `[data-row-id="…"]`。
- **轨道二 · 有界轻作业 `TaskActionSheet`**（`components/business/task-action-sheet.tsx`）：
    - **固定 `size="detail"`（`768px` / `--spacing-detail`，最大 `92vw`）**，专用于工作台窄屏快审、库存调整侧边查看/审批、财务应收/应付侧边审批。
    - 内置固定顶栏（队列位置 + `上一项/下一项` + `完整页面`）、独立滚动正文（`px-sheet-inline py-5`）与固定底栏（左状态/辅助、右主操作），`SheetHeader` 通过 `SheetCloseContext` 自动 `pr-12` 防关闭按钮遮挡标题。
    - **严禁抽屉内再开第二个抽屉（Sheet-on-Sheet）**。

### 4.2 标准化 `DialogContent` 尺寸契约（`components/ui/dialog.tsx`）

全站业务弹窗必须通过 `DialogContent` 的 `size` 与 `scroll` 属性声明，**严禁在 `className` 手写 `max-w-*` 或 `vh`/`svh` 高度**：

| `size`        | 最大宽度                | 高度上限（统一 `dvh`）      | `scroll` 推荐 | 典型场景                                        |
| ------------- | ----------------------- | --------------------------- | ------------- | ----------------------------------------------- |
| `sm`          | `sm:max-w-md` (448px)   | `max-h-[calc(100dvh-2rem)]` | `popup`       | 转交、关闭、跟进记录、恢复审批、轻量单字段确认  |
| `md` _(默认)_ | `sm:max-w-lg` (512px)   | `max-h-[calc(100dvh-2rem)]` | `popup`       | 撤回/驳回确认、升级流程绑定、单条退款/红票登记  |
| `lg`          | `sm:max-w-2xl` (672px)  | `max-h-[85dvh]`             | `body`        | 冲突处理、多字段编辑表单、库存调整编辑          |
| `xl`          | `sm:max-w-4xl` (896px)  | `max-h-[88dvh]`             | `body`        | 比价报价维护、供应商报价导入导出、横向宽确认    |
| `2xl`         | `sm:max-w-6xl` (1152px) | `max-h-[92dvh]`             | `body`        | 变更单编辑、采购生成预览、纸面单据/附件灯箱预览 |
| `full`        | `w-[calc(100vw-2rem)]`  | `h-[calc(100dvh-2rem)]`     | `body`        | 全屏高密度对比与大幅面附件精阅                  |

### 4.3 零弹窗套娃（Zero Modal-on-Modal）必检项

1. **零 `Dialog → AlertDialog / Dialog`**：客户验收已内联至任务正文；采购变更草稿弹窗采用弹窗内两步视图切换（编辑 → 影响核对）。
2. **零 `Dialog` 套完整任务页**：已废除履约大弹窗嵌套，统一在 `WorkspaceFulfillmentTask` 主作业面执行。
3. **零 `Sheet → Sheet`**：单据动态内置「当前任务 / 关联销售全链路」分段切换，不二次弹出时间线抽屉。
4. **零 `Popover → DatePicker Popover`**：采购付款跟进使用标准 `Dialog size="sm"`；所有顶栏 `Popover` 集成宿主 iframe 点击自动收起钩子（`useHostPopoverDismiss`）。

---

## 5. 核心工作流原语与效率引擎验收

### 5.1 ADR-1 分级风险审批决策条（`ApprovalActionBar`）

- **固定肌肉记忆排序（全站零镜像反转）**：左侧放置 `[☑ 处理后自动下一项]` + 辅助动作（`打开原单`、`转交`、`撤回审批`）；右侧固定为 `[驳回]`（红描边 `outline`）→ **`[通过审批]`**（最右唯一主蓝 `default`）。
- **低/普通风险直接通过**：点击「通过审批」或按 `A` 附带底栏内联意见直接提交，3 秒 toast 反馈并在开启 `autoNext` 时自动切入下一项，**零二级确认弹窗**。
- **高危单据强制单层核对**：命中以下任一条件时强制弹出单层 `FormalActionConfirmDialog`：
    1. 冲正红字单据（`receipt_reversal`、`payment_reversal`）；
    2. 负毛利行项目或超信用额度 / 超预算预警；
    3. 大额付款/退款超复核阈值（`purchase_payment_request`、`customer_refund`、`supplier_refund`）；
    4. 单据携带业务阻断级警告（blocking warning）。
- **严格原因长度校验真源（`lib/validation/reason-length.ts`）**：
    - BPM 审批决策 / 撤回 / 终止 / 升级 / 重提：**512 UTF-8 字节**（约 170 汉字，实时字节计数）；
    - 放弃退回重办（`ABANDON_REWORK`）：**512 字符**；
    - 任务转交（`REASSIGN`）：**150 字符**（必填）；
    - 任务关闭（`CLOSE`）：`reason_code` 必填 + `comment` **100 字符**（`DUPLICATE` 必填替换单号）。
- **驳回快捷短语**：按 `documentType` 动态呈现对应驳回短语芯片，不出现销售/财务/采购短语错位。

### 5.2 统一附件对照预览器（`UnifiedMaterialViewer`）

- **认证 Blob 渲染**：所有附件通过领域 `apiGetBlob(...)` 携带 Bearer Token 加载，严禁将受保护接口 URL 裸塞给 `<img>` 或 `pdfjs`；卸载时释放 `URL.createObjectURL`。
- **双形态无缝切换**：
    - 宽屏审批/抄送工作台右栏 `mode="docked"`，配合 `ResizableHandle` 可拖宽至 **50%** 视口实现左侧明细 × 右侧合同 PDF / 银行回单零遮挡对照；
    - 窄屏与非分栏入口自动以 `mode="dialog"`（`Dialog size="2xl"` / `"full"`）灯箱打开，工具栏提供多文件切换、缩放（50%–200%）、适应页宽、90° 旋转与下载原文件。
- **列表动作规范**：`MaterialFileList` 按钮统一使用 `LoadingButton`，顺序恒为 **`预览` → `下载`**。

### 5.3 统一审批路线与审计轨迹（`ApprovalRouteStepper` + `UnifiedAuditTrail`）

- **`ApprovalRouteStepper`**：紧凑单行（~44px）展示当前轮次节点流，已决节点通过 `formatDateTime` 展示 `MM-DD HH:mm` 精简时间；驳回态折叠为单行警示条，点击可锚定高亮右侧对应驳回记录。
- **`UnifiedAuditTrail`**：默认**倒序（最新轮次与事件置顶）**，合并 `审批与抄送`（多轮执行记录 + 节点抄送状态）与 `单据全动态` 双分段页签；翻页采用追加模式（不覆盖前页），全量时间戳统一经 `formatDateTime` / `displayUnixSeconds` + `.num` 渲染。

### 5.4 全局键盘效率引擎（`useWorkspaceHotkeys`）

| 快捷键                 | 生效范围                                   | 验收行为                                                           |
| ---------------------- | ------------------------------------------ | ------------------------------------------------------------------ |
| `J` / `↓` 与 `K` / `↑` | 队列、分栏检查器、任务/审批/抄送详情       | 切换下一项 / 上一项并自动滚动入视口                                |
| `A`                    | 审批检查器 / 审批详情（具 `APPROVE` 权限） | 按 ADR-1 触发通过（常规直通，高危弹单层确认）                      |
| `R`                    | 审批检查器 / 审批详情（具 `REJECT` 权限）  | 展开并聚焦驳回原因输入区与快捷短语                                 |
| `P`                    | 2 栏决策工作台                             | 切换 / 聚焦右侧「附件对照」面板                                    |
| `H`                    | 2 栏决策工作台                             | 切换 / 聚焦右侧「审批与动态」面板                                  |
| `]`                    | 2 栏工作台                                 | 折叠 / 展开右侧上下文轨（切换 100% 宽表视图）                      |
| `/`                    | 任意队列页                                 | 立即聚焦搜索输入框                                                 |
| `Esc`                  | 队列 / 检查器 / 抽屉 / 浮层                | 逐层收起当前浮层、关闭检查器或清空多选                             |
| `?`                    | 工作台队列与办理页                         | 打开 / 关闭快捷键速查弹窗（`WorkspaceHotkeyCheatSheetButton`）     |
| `Cmd/Ctrl + Enter`     | 全部决策/确认/作业表单                     | 提交当前表单或确认层                                               |
| `Cmd/Ctrl + K`         | 宿主壳 + 子 iframe                         | 唤起全局快搜，支持检索菜单、**已打开页签**、最近任务单号与往来单位 |

- **多 Iframe 安全守卫必检**：处于后台隐藏页签（`data-workspace-active !== "true"`）、输入控件/IME 组合态、或存在其他模态弹层时，快捷键必须 100% 让渡不误触；切换或关闭顶栏标签后宿主自动将焦点交还激活 iframe 的 `<main id="main-content">`。

### 5.5 五大角色工作台（`WB-01` .. `WB-05`）

| 编号    | 工作台名称     | `view` 模板   | 首屏与核心验收点                                                                                                                                        |
| ------- | -------------- | ------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `WB-01` | 销售工作台     | `sales`       | 48px 单行页头（切换岗位 + 期间胶囊 + 快捷操作），`@[960px]/workspace:grid-cols-4` 单行 KPI（签单/待审/待验收/风险），首屏 ≤192px 呈现优先待办与推进阶段 |
| `WB-02` | 履约仓储工作台 | `fulfillment` | 覆盖收货入库、仓发直发与交付执行，KPI 卡片点击打开 `QuickPreviewSheet` 下钻，直达履约与库存台账                                                         |
| `WB-03` | 采购工作台     | `procurement` | 覆盖询价比价、货源安排、付款安排与变更影响，单行 KPI 不折行                                                                                             |
| `WB-04` | 出纳工作台     | `finance`     | 覆盖客户回款核销、供应商付款与退货款办理，单行 KPI + 敏感银行账户权限显隐                                                                               |
| `WB-05` | 经营管理工作台 | `management`  | 覆盖实际经营盈亏、客户经营质量、重点审批与异常治理，期间切换与趋势正常渲染                                                                              |

---

## 6. 自动化工程门禁与发布前验证

### 6.1 `scripts/check-ui-theme.mjs` 六条零白名单精准门禁

Phase 2C 全部 4 份存量白名单已 **完全清零删除**，全仓执行严格门禁：

| 规则             | 检测目标                                                                                                                    | 白名单状态                                                                                | 拦截说明                                                                                     |
| ---------------- | --------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------- |
| **Rule-1**       | `features/**` 中 `<DialogContent className>` 手写 `max-w-*` 或 `max-h-[...vh\|svh]`                                         | **0 白名单**                                                                              | 强制使用 `DialogContent size` 与 `dvh` 高度                                                  |
| **Rule-2**       | `features/**` 中使用 `<NativeSelect>` 或导入 `@/components/ui/native-select`                                                | **0 白名单**                                                                              | 强制改用 `Select` 或 `OptionCombobox`                                                        |
| **Rule-3**       | `features/**` 中导入或使用 `surfacePanelClassName`、`PageHeader`、`DocumentHeader`、`DetailPageHeader`、`ObjectSectionTabs` | **0 白名单**                                                                              | 彻底封禁旧版布局壳与嵌套外框卡片                                                             |
| **Rule-4**       | `features/**` 中对 `Date` 实例调用 `.toLocaleString` / `.toLocaleDateString` / `.toLocaleTimeString`                        | **0 白名单**                                                                              | 强制使用 `formatDateTime` / `displayUnixSeconds`（不误伤 `number.toLocaleString()`）         |
| **Rule-5A / 5B** | `features/**` 中裸用 `<SheetContent>`（5A）及抽屉文件内手写 `px-7`（5B）                                                    | **5B: 0 白名单**<br>5A: 仅保留 5 处非预览结构抽屉显式豁免（`STRUCTURAL_SHEET_ALLOWLIST`） | 预览/任务抽屉强制走 `QuickPreviewSheet` / `TaskActionSheet` 且正文边距统一 `px-sheet-inline` |
| **Rule-6**       | `features/workspace/**` 中出现视口断点 `xl:grid-cols-`                                                                      | **0 白名单**                                                                              | 强制使用命名容器查询 `@[960px]/workspace:` / `@[1120px]/workspace:`                          |

> 同时 `check-ui-theme.mjs` 继续强校验 5+4 字号角色、禁止原始色盘/任意色值/内联字体色值、`QuickPreviewSheet` 禁止 `contentClassName`/`overlayClassName`、详情路由 `key` 不得绑定 `section`，以及 `app/globals.css` ↔ `lib/theme-tokens.ts` 令牌 1:1 注册。

### 6.2 发布前必过检查命令

在 `erp-client/` 目录下执行以下全量门禁，必须全部 0 错误通过：

```bash
npm run lint
npm run format:check
npx tsc --noEmit
npx next build --webpack
```

### 6.3 人工视觉与业务回归签收清单

- [ ] 在 `1280×800`、`1440×900`、`1920×1080`（侧栏展开 + 折叠 6 种组合）抽检队列通栏/分栏、2 栏审批工作台、通栏执行任务与 `WB-01..WB-05` 角色工作台，无断点坍塌与横向溢出。
- [ ] 抽检常规单据一键直接通过（0 二级弹窗 + 3s toast + `autoNext`）与高危单据（红字冲正/大额/负毛利/阻断）单层 `FormalActionConfirmDialog` 拦截，验证 BPM 512 UTF-8 字节边界（170 字通过 / 171 字拦截）。
- [ ] 抽检含 PDF 合同与图片的审批单据，在右栏 `UnifiedMaterialViewer` 拖拽至 50% 左右对照缩放旋转正常，窄屏灯箱与 `预览 → 下载` 正常。
- [ ] 抽检 `J/K/A/R/P/H/]/Esc/?` 与 `Cmd/Ctrl+K` 快捷键在多 Tab Iframe 切换与输入态下的激活守卫与焦点恢复。
- [ ] 验证浏览器控制台无 error、DOM 无重复原生 `id`（符合 `AGENTS.md §6`）、无业务接口 422 报错。
