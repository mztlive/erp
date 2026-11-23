# ERP 任务（审批与事项）统一交互系统与设计系统重构方案

> **文档性质**：基于全仓前后端源码审计（涵盖工作区壳与设计系统、17+1 类事项任务、13 类必审单据与 5 类抄送体系）及核心实现核验形成的架构级重构方案与落地蓝图。  
> **配套规范**：[`docs/list-table-design-system.md`](./list-table-design-system.md)、[`docs/detail-page-design-system.md`](./detail-page-design-system.md)、[`docs/typography-design-system.md`](./typography-design-system.md)、[`erp-client/AGENTS.md`](../erp-client/AGENTS.md)  
> **重构目标**：在不破坏现有 Feishu Universe Design（清澈商务蓝 + 纯白通栏工作面 + 水平细线）基因和零套娃卡片原则的前提下，建立**统一信息架构（IA）、统一交互浮层体系、容器查询驱动的空间布局引擎、统一视觉令牌与高频操作效率引擎**，系统性提高**页面利用率、视觉美感与操作效率**。  
> **实施边界**：本文严格区分「纯前端主线零阻塞落地」「后端轻量必选协同（3 批随行）」与「P3+ 独立增强迭代」三类变更；所有仓库相对路径（`erp-client/...`、`backend/...`、`docs/...`）、组件符号、CSS 变量、后端接口契约与行号均经源码实证。

---

## 目录

0. [关键架构决策定案（ADR）](#0-关键架构决策定案adr)
1. [全局诊断与任务统一信息架构（IA & Task Taxonomy）](#1-全局诊断与任务统一信息架构ia--task-taxonomy)
2. [统一交互系统规范（Unified Interaction System & Overlay Architecture）](#2-统一交互系统规范unified-interaction-system--overlay-architecture)
3. [统一设计系统、视觉美化与页面利用率方案（Design System, Visual Polish & Space Budget）](#3-统一设计系统视觉美化与页面利用率方案design-system-visual-polish--space-budget)
4. [操作效率重构引擎（Operational Efficiency & Ergonomics）](#4-操作效率重构引擎operational-efficiency--ergonomics)
5. [全量逐项重构映射矩阵（Complete Per-Task & Per-Overlay Migration Matrix）](#5-全量逐项重构映射矩阵complete-per-task--per-overlay-migration-matrix)
6. [组件架构设计与分阶段落地路线图（Component Architecture, Backend Alignment & Phased Roadmap）](#6-组件架构设计与分阶段落地路线图component-architecture-backend-alignment--phased-roadmap)

---

## 0. 关键架构决策定案（ADR）

本节锁定本轮重构的 4 项核心产品与架构决策，全文各章节设计与分阶段路线图均严格以此为唯一准则：

| 决策编号 | 决策主题 | 最终定案 | 关联章节 |
|---|---|---|---|
| **ADR-1** | **审批通过确认策略** | 采用**分级风险决策制**：常规低/普通风险单据支持底栏内联意见 + 一键直接通过（`A` / 主按钮，3 秒 toast + `autoNext`，零二级弹窗）；高危单据（冲正红字 `receipt_reversal`/`payment_reversal`、负毛利或超信用/预算预警、大额付款/退款超阈值、阻断警告）强制弹单层 `FormalActionConfirmDialog` 核对；所有驳回（底栏展开原因 + 按单据动态快捷短语）与撤回（`ApprovalWithdrawDialog` 红色 `destructive`）保持严格必填校验与 BPM 512 UTF-8 字节计数。 | §2.3A/B/D、§5 表 B/C |
| **ADR-2** | **前后端协同与排期策略** | 采用**纯前端主线零阻塞推进 + 后端轻量化随行补齐**：Phase 0–P2 核心体验（~80% 收益）纯前端零阻塞上线；同步排期 **3 批轻量必选后端协同**：① **Phase 0/1**：`SUPPLIER_PORTAL_REVIEW` 业务族枚举改 `Procurement` + 通知全部已读接口；② **Phase 2/3**：`/admin/work-items/stats` 全维度扩展、`/admin/work-items` 往来单位检索、`/approval-instances?view=blocked\|managed\|mine` 对齐页码分页（在此之前由前端 `cursor` 适配层先行兜底）；③ **Phase 3 前置**：`erp-identity` `RoleWorkbench` 枚举补齐履约仓储与经营管理工作台。`batch-approve` / 审批评论 / 受阻替补指派明确划归 **P3+ 独立增强迭代**，不阻塞本轮交付。 | §1.2、§4.3/4.4、§6.2、§6.3 |
| **ADR-3** | **双轨抽屉治理与 `AGENTS.md §7` 同步修订** | 正式确立**只读轻预览（`QuickPreviewSheet`，严禁 `useAppForm`）**与**有界轻作业抽屉（`TaskActionSheet`，固定 `size="detail"` 768px，专用于工作台窄屏快审、库存调整与财务侧边审批，禁止套娃抽屉）**双轨规范；Phase 1 同步更新 `erp-client/AGENTS.md §7`，并将抽屉正文外边距统一为 `px-sheet-inline py-5`。 | §2.1、§2.2B、§5 表 C、§6.3 |
| **ADR-4** | **落地切入顺序** | 严格按 **Phase 0（高危缺陷/校验/口径/转交闭环）→ Phase 1（`@container/workspace` + `Dialog`/`Sheet`/`UnifiedMaterialViewer` + 6 条门禁基建）→ Phase 2（以「审批 2 栏工作台 + 统一分栏队列」作为首个业务打样率先上线，随后收口全部 17 事项与存量白名单）→ Phase 3（键盘效率引擎、2,044 行长表单拆解、补齐履约/管理工作台、死代码清零与 `design-qa.md` 刷新）** 递进落地。 | §6.3 |

---

## 1. 全局诊断与任务统一信息架构（IA & Task Taxonomy）

### 1.1 任务体系全量盘点（后端真源 × 前端承接全景）

当前 ERP 的“任务宇宙”横跨 **4 大主工作台路由 + 5 类站外平行红点队列 + 17 类活跃事项 + 13 类审批单据 + 5 种抄送触发时机**，但缺乏统一的顶层心智模型与容器约束：

| 体系分层 | 实体与数量 | 后端真源 / 前端核心文件 | 现状核心问题 |
|---|---|---|---|
| **活跃事项任务（Work Items）** | **17 个活跃类型** + **1 个前端合成类型**（`APPROVAL_INSTANCE`） | `backend/crates/domains/erp-workflow/src/dto/work_item/status.rs:6-24`、`erp-client/features/workspace/components/workspace-task-surface.tsx:43-459` | 17 类任务分散在 15+ 个独立组件和 5 套页头壳中；非 `PROCESS` 态降级只读后因硬编码过滤丢失页头 `REASSIGN`/`CLOSE` 入口 |
| **退役与历史兼容事项** | **7 个已退役**（`WORK_ITEM_TYPE_RETIRED`）+ **2 个历史反序列化兼容**（`CARD_FUNDS_REVIEW` / `CARD_FUNDS_DELTA_REVIEW`） | `backend/crates/domains/erp-workflow/src/entity/work_item/entity/types.rs:14-49`、`read/view.rs:406-415`、`work-item-meta.ts:245-288` | 前端 `DOCUMENT_META` 仍残留 9 个孤立未注册映射；路由注册表残留 `W02`/`W07`/硬编码 `so_1002` |
| **审批单据体系（BPM）** | **22 类注册单据**：**13 类必审（`PROCESS_REQUIRED`）** + 9 类免审（`NO_APPROVAL`） | `business_document.rs:22-100`、`policy.rs:247-280, 571-595`、`approval_objects.rs:1-138` | BPM 仅支持单节点类型 `USER_APPROVAL`（单人/最多 50 人或签/动态主管）和 `APPROVE`/`REJECT` 二元决定；前端却裂变出 **5 个撤回组件（4 套割裂实现）**、**6 处提交入口（4 套模式）**、**3 套审批/抄送预览（含新品审核图弹窗共 4 套）**、**9 套轨迹** |
| **审批抄送体系（Copies）** | **5 种触发时机**：`STARTED`、`NODE_APPROVED`、`APPROVED`、`REJECTED`、`CANCELLED` | `approval_copy.rs`、`features/approval-copies/{page,detail-page,columns,copy-files}.tsx` | 与任务队列完全割裂：独立使用 `ListWorkspacePage`（`pageSize=20` vs `50`）、唯一支持批量勾选、详情无 `上一项/下一项`、图片强塞 `70vh` iframe |
| **角色工作台（Role Workbenches）** | 已上线 **3 个**：`WB-01`（销售）、`WB-03`（采购）、`WB-04`（出纳）；**缺失 `WB-02`、履约/仓储、管理/经营** | `backend/crates/domains/erp-identity/src/entity/role_workbench.rs:12-16`、`lib/role-workbenches.ts:2-64`、`features/workspace/lib/overview.ts:8-22`、`role-workbench-entry.tsx:49-139` | `overview.ts` 已定义 5 套模板（含 `fulfillment`、`management`），但后端 `RoleWorkbench` 枚举仅注册 3 个，导致履约/管理用户访问 `/workspace` 被动重定向至 `/workspace/tasks` |
| **主工作台队列路由** | `/workspace/tasks`（含隐式 `?view=managed`）、`/workspace/approvals`、`/workspace/history`、`/workspace/copies` | `features/workspace/pages/workspace-home-page.tsx`、`workspace-queue-panel.tsx`、`approval-copies/page.tsx` | `WorkspaceTypeNav` 屏蔽 `task:DOCUMENT_APPROVAL` 导致无法一键看「全部待审批」；后端 `/approval-instances?view=blocked\|managed\|mine` 未暴露（且仅支持 `cursor` 分页） |
| **平行外部队列与红点入口** | 顶栏 `InboxPopover`（SSE 宿主机）、`SystemFeedbackPanel`、`/account/data-transfers`、`/account/workbench`、`/governance/background-jobs` (W18)、`/governance/integration-errors` (W29)、`/sales/inquiries`、`/procurement/inquiries`、`/sales/selection`、`/fulfillment` (W09) | `workspace-registry.ts:505-794`、`inbox-popover.tsx`、`system-feedback-panel.tsx`、`integration-errors-page.tsx` | 用户“待办”心智被撕裂在 9+ 个红点与入口中；顶栏 `InboxPopover` 与 `SystemFeedbackPanel` 使用同款红点徽章且收件箱缺“全部已读”；W29 在站外自建 Master-Detail 队列；`SequentialProcessBar` 在 4 个模块重复实现队列步进 |

#### 附：13 类必审单据与 9 类免审单据边界清单
- **13 类 `PROCESS_REQUIRED`**：`sales_order`、`voucher_sales_order`、`sales_change_order`、`purchase_order`、`purchase_change_order`、`stock_adjustment`、`sales_invoice_request`、`purchase_payment_request`、`customer_receipt`、`customer_refund`、`supplier_refund`、`receipt_reversal`、`payment_reversal`。
- **9 类 `NO_APPROVAL`（边界核对，不进入审批工作台）**：`supplier_payment`、`purchase_receipt`、`delivery`、`electronic_delivery`、`service_fulfillment`、`customer_acceptance`、`invoice`、`sales_return_case`、`purchase_return_order`。

---

### 1.2 统一四原型任务分类模型（Unified 4-Archetype Task Taxonomy）

为终结“17 类事项各自为政、审批与业务执行互抄布局”的混乱，将全站所有任务收敛为 **4 个正交的交互原型（Archetypes）**，每个原型绑定唯一标准壳体、布局结构与交互契约：

| 原型编号 | 原型名称 | 涵盖任务 / 入口 | 用户核心心智与作业模式 | 标准容器与布局模式 | 主操作语义 |
|---|---|---|---|---|---|
| **Archetype 1** | **决策审批型（Decision Approvals）** | `DOCUMENT_APPROVAL`（13 类单据）、`APPROVAL_INSTANCE`（发起跟踪）、`APPROVAL_REWORK`（退回重办） | **核对事实 → 查附件/历史 → 快速表态（通过/驳回/撤回/重提）**；高频连续流转 | **宽屏 2 栏决策工作台**：左侧 65–70% 单据与差异画布 + 右侧 30–35% 常驻上下文轨（`审批路线与轨迹` / `侧边对照附件` / `单据动态`）+ 吸底轻量化决策条 | `通过审批`（按 ADR-1 分级风险决策）、`驳回`（展开必填原因）、`修改重提`、`撤回` |
| **Archetype 2** | **结构化作业执行型（Structured Action Tasks）** | `FULFILLMENT_OPERATION`、`CUSTOMER_ACCEPTANCE_REGISTRATION`、`SUPPLIER_PAYMENT_EXECUTION`、`SALES_INVOICE_EXECUTION`、`PURCHASE_PAYMENT_ARRANGEMENT`、`PROCUREMENT_ORDER_CREATION`、`SALES_INQUIRY_PROCESSING`、`PURCHASE_SOURCE_UPDATE`、`PURCHASE_RETURN_REFUND`、`PURCHASE_RETURN_INVOICE`、`SUPPLIER_SETTLEMENT_REVIEW`、`IMPORT_BUSINESS_CONFIRMATION`、`SUPPLIER_PORTAL_REVIEW`*(见下)* | **读取待办上下文 → 在当前面直接填报/分单/上传凭证/提交执行**；重表单或宽表编辑 | **通栏专注作业面（Single-Canvas Action Surface）**：彻底撤除冗余 `20rem` 通用侧栏，状态与说明收入紧凑页头 + `?` 浮层，100% 宽度留给表单/宽表，吸底统一提交与 `autoNext` | `提交执行`、`保存草稿`、`登记完成`、`转交任务`（页头统一入口） |
| **Archetype 3** | **异常排查与治理型（Investigation & Exception Tasks）** | `INTEGRATION_RESULT_UNKNOWN`（W29/W26）、`BUSINESS_EXCEPTION`（W29/W26/W21）、受阻审批治理（`/workspace/approvals?view=blocked`） | **定位异常根因 → 对照上下游同步报文/阻断码 → 重试/人工核销/转交/关闭** | **分栏排查工作台（Split Investigation Surface）**：列表支持多选批量关闭/转交；详情采用「顶部异常诊断横幅 + 核心对照区 + 可折叠原始载荷/流水」单主壳（禁止再套子 `DetailWorkspacePage`） | `恢复审批` / `终止受阻` / `人工核实关闭` / `转交` / `重推同步` |
| **Archetype 4** | **知会抄送与系统动态型（Informational Copy & Feeds）** | 抄送给我（`/workspace/copies`，5 种时机）、顶栏 `InboxPopover`（SSE）、`SystemFeedbackPanel`、我的文件任务（`/account/data-transfers`）、批量任务记录（`/governance/background-jobs`） | **低打扰速览 → 确认已读 / 下载结果 / 按需跳转原单**；禁止重型整页冷启动 | **队列内侧滑预览抽屉（`QuickPreviewSheet size="detail"`）或 Master-Detail 右栏**：点击行即刻标已读并就地预览快照与附件，支持 `J/K` 连续翻阅与批量标已读 | `标记已读`（单条自动/批量勾选）、`查看原单`、`下载文件` |

> **分类归属定案（消除 `SUPPLIER_PORTAL_REVIEW` 漂移，见 ADR-2）**：  
> 后端 `status.rs:122` 将 `SUPPLIER_PORTAL_REVIEW` 归入 `Approval`，前端 `work-item-meta.ts:51` 归入 `procurement`。鉴于该任务使用 `WorkItem` 原生 `PROCESS`/`REASSIGN`（而非 BPM `ApprovalInstance`）且主体是采购商品建档与 2,044 行结构化审核表单，**定案统一归属为 `family = procurement`（业务族）+ Archetype 2（结构化审核执行型，带通过/驳回终态）**，由后端在 Phase 0/1 第 1 批轻量协同中同步修正 `status.rs:122`，彻底消除按业务族筛选时的前后端错配。

---

### 1.3 全仓严重缺陷、功能断层与死代码总清单（全仓前后端源码审计汇总）

| 优先级 | 缺陷分类 | 精确源码位置 | 影响与症状描述 |
|---|---|---|---|
| **P0-1** | **Iframe 1200px `< xl` 断点全面坍塌** | `components/layout/workspace-frame.tsx:127`、`workspace-task-pane.tsx:229`、`role-workbench-metrics.tsx:106-107`、`globals.css:639,1174,1203,1211,1449` | 1440px 桌面展开 240px 侧栏后 iframe 宽仅 **1200px `< xl (1280px)`**：任务 2 栏全部塌为单栏、角色工作台 4 列 KPI 折成 2×2 占掉半屏、`globals.css:1211` `@media (min-width: 80rem)` 4 列筛选托盘降级为 3 列（`L639/1174/1203` 亦以视口宽度求值）；且 961–1004px 屏高出现宿主/iframe 高度密度不同步 |
| **P0-2** | **Account 路由逸出多标签工作台壳** | `lib/workspace-tabs.ts:99-111`、`workspace-account-menu.tsx:52,117,124` | `workspaceUrl()` 白名单漏掉 `"account"`，点击「个人首页设置」「我的文件任务」直接逃逸出 Tab 壳并冲毁当前工作页上下文 |
| **P0-3** | **`REASSIGN`/`CLOSE` 功能大面积断层 + 虚假徽章与错别字** | `backend/.../action_policy.rs:81-118`、`workspace-document-task-detail.tsx:520`、`workspace-task-surface.tsx:216-228`、`supply-exception-task-panel.tsx:42,290-295,313` | 后端仅排除 3 类（`DOCUMENT_APPROVAL`/`APPROVAL_REWORK`/`SALES_INQUIRY_PROCESSING`），**14 类可转交事项中前端仅接入 2 类、缺失 12 类（覆盖率仅 2/14 = 14.3%）**；管理者在 `managed` 队列因缺 `PROCESS` 按权限安全降级至 `WorkspaceDocumentTaskDetail` 后，`L520` 将转交硬编码限定为 `FULFILLMENT_OPERATION`，导致管理者看得到却转交/关闭不了；`SupplyExceptionTaskPanel` 挂出虚假 `"可转交"` 徽章且含错字 `"负责人人"` |
| **P0-4** | **BPM 512 UTF-8 字节限制与前端多套原因校验错配** | `bpm/src/model/types.rs:403`（512 UTF-8 B）vs `approval-workflow/schema.ts:4-27`（无上限）、`use-resubmission-reason.tsx:42`（1000 字）、`request-detail.tsx:477,739`（2000/1000 字）；另注 `abandon_rework.rs:40` 为 512 字符，`command.rs:84-109` 中 `REASSIGN` 150 字符 / `CLOSE` 100 字符 | 后端 BPM 审批决策/撤回/终止/升级强限 **512 UTF-8 字节**（约 170 汉字）；前端无上限或按 1000/2000 字符放行导致超 ~170 汉字必然被 BPM 拒收；同时 `ABANDON_REWORK`（512 字符）、`REASSIGN`（150 字符）、`CLOSE`（100 字符）后端口径各异，前端缺乏统一校验真源与实时余量提示 |
| **P0-5** | **提交前审批链预览遗漏** | `sales-order-submit-confirm-dialog.tsx:25-122`、`voucher-sales-order-submit-confirm-dialog.tsx`、`invoice-requests/components/request-form.tsx:245-333` | 销售单、卡券销售单、开票申请提交前均未渲染 `SubmissionRouteConfirmation`，申请人看不到即将绑定的审批人；开票申请甚至无提交确认层 |
| **P0-6** | **工作台待审批聚合屏蔽 & 受阻治理视图盲区** | `workspace-type-nav.tsx:34`、`workspace-queue-panel.tsx`、`backend/.../approval_instance/http.rs:86-94,322-330` | `WorkspaceTypeNav` 硬过滤掉 `task:DOCUMENT_APPROVAL`，审批人无法一键只看全部待审；后端已存在 `/approval-instances?view=blocked\|managed\|mine` 但前端未暴露（须注意这 3 视图仅支持 `cursor` 分页，传 `page`/`filters` 会 422） |
| **P1-1** | **审批强制单栏 & 附件新 Tab 割裂** | `workspace-processing-layout.tsx:91-96`、`approval-material-files.tsx:73-91`、`approval-copies/copy-files.tsx:33-124` | 全部 13 类审批硬编码 `singleColumn=true`；12/13 类附件弹裸新 Tab `/approval-materials`（零工具栏、加载态贴边）；抄送页把图片硬塞 `70vh` iframe |
| **P1-2** | **双页头/双侧栏/双内边距/3重元数据冗余** | `workspace-task-pane.tsx:199,229`、`supplier-order-center-page.tsx`、`batch-detail-view.tsx`、`settlement-center.tsx`、`workspace-return-finance.tsx:135` 等 7 文件 | 嵌入页重复渲染 `<DetailWorkspacePage>`/`<PageScaffold>`/`sidebar`，形成双侧栏挤压；子页面在 `flat` 内叠加 `px-5`/`px-7` 导致左沿错位；同一份元数据在页头/summary/20rem aside/`?` 弹层重复 4 次 |
| **P1-3** | **状态色调、动作栏顺序与文案多处矛盾** | `approval-workflow/display.ts` vs `approval-copies/{columns,detail-page}.tsx` vs `workspace-task-columns.tsx:359`、`approval-action-bar.tsx:117-135`、`work-item-meta.ts` | `BLOCKED` 红/黄冲突、`RUNNING` 黄/蓝冲突、`ACTIVE` 蓝/黄冲突；`ApprovalActionBar` 在工作台与单据详情左右按钮镜像反转；审批/抄送 6 处（及全仓更多日期调用）裸用 `Date.toLocaleString("zh-CN")`；`PROCUREMENT_ORDER_CREATION` 等存在 3 套叫法 |
| **P1-4** | **重复 DOM ID、死代码与陈旧治理资产** | `workspace-document-paper-dialog.tsx:75,78`、`workspace-task-context.tsx:123,126,96`、`workspace-task-pane.tsx:289-447`、`components/ui/{drawer,resizable}.tsx`、`detail-page-header.tsx`、`object-section-tabs.tsx`、`workspace-overview.tsx`、`workflow-actions.tsx:592-719` (`SequentialProcessBar`)、`definition-flow-canvas.tsx:483`、`erp-client/design-qa.md` | 违背 `AGENTS.md §6` 唯一 DOM ID；`layout.compact` 在 `flat` 模式完全失效；160 行非 flat 遗留树与 6 个零引用组件长期滞留；`SequentialProcessBar` 在 4 个业务模块与工作台翻页重复；`DefinitionFlowCanvas` 高分屏被 `620px` 截断；`erp-client/design-qa.md` 仍记录已废弃旧分栏验收标准 |

---

## 2. 统一交互系统规范（Unified Interaction System & Overlay Architecture）

### 2.1 浮层与承载面决策矩阵（Overlay & Surface Decision Matrix）

全站所有任务交互严格按**风险等级、输入复杂度、是否需要对照主文档、是否跨单据长时作业**落入以下 6 个承载层级，禁止越级或混用：

| 承载层级 | 标准组件原语 | 适用场景 | 禁止场景 | 是否留驻当前 Tab |
|---|---|---|---|---|
| **L1：页内行内 / 轻量浮层（Inline / Popover）** | 行内展开区、`Popover`（含宿主 iframe 点击关闭补丁） | 快捷筛选条件、列排序、任务说明 `?`、收件箱/通知列表（**禁止 Popover 内再弹日期 Popover**） | 含日期选择器/下拉检索的复合表单、高风险写操作 | **留驻当前 Tab** |
| **L2：页内分栏检查器（Master-Detail Split-Pane）** | `WorkspaceSplitQueue`（激活 `components/ui/resizable.tsx` 的 `ResizablePanelGroup`） | `@[1120px]/workspace:` 宽屏下 `/workspace/tasks`、`/workspace/approvals`、`/workspace/copies` 的高频连续分诊、单据快审、抄送速览、W29 异常逐条核查 | 超过 12 列的超宽建单表单（如 `PurchaseOrderCreatePage`、`PortalReviewForm`） | **留驻当前 Tab（零 iframe 冷启动）** |
| **L3：右侧滑出抽屉（Slide-Over Sheet，ADR-3 双轨）** | 只读：`QuickPreviewSheet (size="preview" \| "detail")`<br>轻作业：`TaskActionSheet (固定 size="detail" 768px)` | 窄屏（`<1120px`）替代分栏快审；库存调整/财务应收应付列表侧边查看与审批（统一 `size="detail"` 768px）；业务概览指标下钻（`WorkspaceOverviewDetail`） | 在 `size="preview"` (400px) 中塞入审批路线/表格/动作栏；抽屉内再套抽屉；在 `QuickPreviewSheet` 内接 `useAppForm` | **留驻当前 Tab** |
| **L4：标准业务对话框（Standard Dialog）** | `DialogContent size="sm \| md \| lg \| xl \| 2xl \| full"` + `DialogBody` | 中等结构化子流程：任务转交/关闭、跟进记录、单条登记红票/退款、拆包/改地址、Excel 批量导入、流程配置草稿、窄屏附件灯箱预览 | 把整个详情页塞进弹窗（废除 `WorkspaceFulfillmentProcessAction`）；在弹窗上再弹第二个弹窗 | **留驻当前 Tab** |
| **L5：高风险/终态确认层（AlertDialog / RiskConfirm）** | `FormalActionConfirmDialog` (`AlertDialog`) | 不可逆或改变流转状态的动作：高危单据审批通过（见 ADR-1）、撤回审批、终止受阻、放弃退回重办、提交审批核对（含 `SubmissionRouteConfirmation`）、人工强制关闭异常 | 常规低/普通风险单据审批通过（改用底栏内联一键通过）；超过 2 屏的复杂主编辑表单 | **留驻当前 Tab** |
| **L6：独立工作区页签（New Workspace Iframe Tab）** | `WorkspaceTabs` (`navigateWorkspace` + `workspaceTabKey`) | 重型跨单据创作与长时编辑：销售询价报价比价、销售转采购供给分配建单、供应商新品 2,044 行建档审核、从工作台点击「完整模式打开 / 打开原单」、同时并行比对两张不同业务单据 | 3 秒快速看一眼抄送、审一条常规回款/退款、核对一张合同附件 | **新开 / 聚焦独立 Iframe Tab** |

#### 消除“弹窗套娃（Modal-on-Modal）”的四条铁律
1. **禁止 `Dialog → AlertDialog / Dialog`**：大型表单弹窗（`AcceptanceRegisterDialog`、`PurchaseSourceUpdateDraftDialog`、`RequestDraftDialog`）不再弹出第二层 `FormalActionConfirmDialog`；改为弹窗内**两步视图切换（Step 1 编辑 → Step 2 影响核对）**或将高频任务表单（客户验收登记）直接**内联至任务详情页正文**，仅在点击主提交时弹出唯一一层 `FormalActionConfirmDialog`。
2. **禁止 `Dialog` 嵌套完整任务页**：彻底废弃 `WorkspaceFulfillmentProcessAction`（`sm:max-w-5xl` 内嵌完整 `WorkspaceFulfillmentTask` 再弹地址/拆包弹窗），统一跳转或原位切换至 `WorkspaceFulfillmentTask` 主作业面。
3. **禁止 `Sheet → Sheet`**：`TaskTimeline` 内不再二次套开 `SalesTimelineSheet`，改用抽屉内分段切换「当前任务动态 / 关联销售全链路」。
4. **禁止 `Popover → DatePicker Popover`**：`PurchasePaymentFollowUp` 从底栏 `Popover w-96` 迁移为标准 `Dialog size="sm"` 或内联折叠行，彻底消除双浮层焦点劫持。

#### 何时留驻当前 Tab vs 新开 Iframe Tab（明确判定规则）
- **单击列表行（Primary Click）**：
  - 在 `/workspace/tasks`、`/workspace/approvals`、`/workspace/history`、`/workspace/copies` 中，**默认在当前 Tab 内打开右侧分栏检查器（宽屏 `@[1120px]/workspace:`）或 `TaskActionSheet size="detail"`（窄屏）**，共享同一份 React Query 缓存，响应 `< 100ms`，彻底消灭「点一条冷启动一个 Next.js iframe」的卡顿。
- **双击行 / 点击检查器顶栏「全屏展开」按钮 / 中键或 `Cmd/Ctrl+Click`**：
  - 打开或聚焦独立的 `/workspace/tasks/[workItemId]` 完整 Iframe Tab；重型任务（`SALES_INQUIRY_PROCESSING`、`PROCUREMENT_ORDER_CREATION`、`SUPPLIER_PORTAL_REVIEW`）在检查器内展示核心摘要与一键「进入完整作业面」直达。
- **工作台跳转关联业务单据（统一规范）**：
  - 全部通过工作台路由 `navigateWorkspace(href)` 打开受控 Iframe Tab（保留当前任务 Tab 不被覆盖，并支持切回），**全面废除零散的 `target="_blank"`**（如 `WorkspaceDeliveryBrief`、`FulfillmentWorkSurface`）以及覆盖当前页的 `router.push` 行为。

---

### 2.2 标准化 `Dialog` / `Sheet` 组件契约（含 ADR-3 双轨抽屉与 `AGENTS.md §7` 同步修订）

#### A. 升级 `DialogContent`（`components/ui/dialog.tsx`）
针对目前 `features/` 中 **152+ 个文件（实测 160 处）**手写 9 种宽度与 7 种高度单位（`vh`/`svh`/`dvh`）的失控现状，在 `DialogContent` 上引入强类型 `size` 与 `scroll` 契约：

| `size` 令牌 | 最大宽度 | 高度上限（统一 `dvh`） | 典型用途 |
|---|---|---|---|
| `"sm"` | `sm:max-w-md` (448px) | `max-h-[calc(100dvh-2rem)]` | 轻量单字段输入（转交、关闭、跟进记录、恢复审批、草稿命名） |
| `"md"` *(默认)* | `sm:max-w-lg` (512px) | `max-h-[calc(100dvh-2rem)]` | 驳回/撤回原因确认、升级流程绑定、单据退款/红票登记 |
| `"lg"` | `sm:max-w-2xl` (672px) | `max-h-[85dvh]` | 冲突解决、多字段编辑表单、库存调整编辑 |
| `"xl"` | `sm:max-w-4xl` (896px) | `max-h-[88dvh]` | 比价报价维护、供应商报价导入导出、横向宽确认 |
| `"2xl"` | `sm:max-w-6xl` (1152px) | `max-h-[92dvh]` | 采购变更单编辑、采购单生成预览、纸面单据/附件灯箱预览 |
| `"full"` | `w-[calc(100vw-2rem)] max-w-none` | `h-[calc(100dvh-2rem)]` | 全屏高密度对比/大幅面附件精阅 |

- **原生固定头尾槽位（`DialogBody`）**：新增 `DialogBody` 子原语；当 `DialogContent` 启用 `scroll="body"`（`lg`/`xl`/`2xl`/`full` 默认开启）时，自动应用 `flex flex-col overflow-hidden p-0`，由 `DialogHeader`（`shrink-0 border-b px-overlay-padding py-3.5 pr-12`）、`DialogBody`（`min-h-0 flex-1 overflow-y-auto px-overlay-padding py-4`）和 `DialogFooter`（`shrink-0 border-t px-overlay-padding py-3`）组成黄金三段结构，消灭 152+ 处手写 `flex flex-col overflow-hidden p-0` 与自定义关闭按钮。

#### B. 双轨抽屉治理：`QuickPreviewSheet` + `TaskActionSheet` 与 `AGENTS.md §7` 同步修订（ADR-3）
1. **`SheetCloseContext` 防遮挡修复（`components/ui/sheet.tsx`）**：参照 `DialogCloseContext`（`dialog.tsx:51,101`），为 `SheetContent` 增加 `SheetCloseContext`，`SheetHeader` 自动在 `showCloseButton === true` 时追加 `pr-12`，彻底避免标题与右上角 `XIcon` 碰撞。
2. **轨道一：只读轻预览 `QuickPreviewSheet`（`components/business/list.tsx:387`）**：
   - 严守只读定位，**严禁接入 `useAppForm`**。
   - 将正文容器硬编码的 `px-7 py-6` 统一改为 **`px-sheet-inline py-5`**，使其与 `SheetHeader` / `SheetFooter` 严格左对齐，并在 `≤960px` 高度断点自动从 `24px` 同步收敛至 `20px`；同时将 `WorkspaceOverviewDetail`（`workspace-overview-detail.tsx:102,218`）迁移复用 `QuickPreviewSheet`，消除孤立的 `px-7`。
3. **轨道二：有界轻作业抽屉 `TaskActionSheet`（`components/business/task-action-sheet.tsx`）**：
   - **固定采用 `size="detail"`（复用 `--spacing-detail` = 768px）**，正文外边距统一 `px-sheet-inline py-5`（注：若未来扩展新宽度令牌，须在 `app/globals.css` 新增 `--spacing-*` 并 1:1 同步登记至 `lib/theme-tokens.ts` 的 `themeSpacingNames`，以满足 `scripts/check-ui-theme.mjs:584-629` 强校验）。
   - **专用于工作台窄屏快审、库存调整侧边查看/审批（将 `AdjustmentDetailSheet` 从 `size="preview"` 400px 升级至 `TaskActionSheet size="detail"` 768px）与财务应收/应付侧边审批**，内置固定顶栏（含「全屏打开」+ `上一项/下一项`）、独立滚动正文与固定底栏（`ApprovalActionBar` / `WorkspaceTaskFooter`），**严格禁止抽屉内套娃抽屉**。
4. **Phase 1 同步修订 `erp-client/AGENTS.md §7` 治理条款**：
   - 现行 `erp-client/AGENTS.md §7` 在 `L98` 规定 `size="detail"` 外缘为 `px-7 py-6`，在 `L91/111/113` 禁止 Sheet 变表单/接 `useAppForm`。
   - **同步修订内容**：① 将 `L98` 统一更新为「抽屉正文外边距统一为 `px-sheet-inline py-5`，与 `SheetHeader`/`SheetFooter` 对齐并响应高度紧凑断点」；② 在 `L86-93` 决策表明确确立**双轨规范**：主数据/普通列表只读轻预览严守 `QuickPreviewSheet`（严禁 `useAppForm`），工作台窄屏快审、库存调整与财务侧边审批等有界轻作业统一使用 `TaskActionSheet`（固定 `size="detail"` 768px，禁止套娃抽屉）。
5. **共享 `Popover` Iframe 自动关闭修复（`components/ui/popover.tsx`）**：将 `InboxPopover`（`inbox-popover.tsx:24-111`）中手工实现的 `collectIframeDocuments()` + `pointerdown`/`blur` 监听下沉至宿主壳共享 Hook `useHostPopoverDismiss`，保证所有顶栏 Popover 点击 iframe 内任意位置均可自然收起。

---

### 2.3 统一确认与原因输入系统（Confirmation & Reason Engine）

#### A. 收敛 5 个撤回组件（4 套割裂实现）为 1 套 `ApprovalWithdrawDialog`
将 `CancelApprovalDialog`、`DocumentWithdrawApprovalButton`、`CancelAdjustmentApprovalDialog`、`CancelRequestDialog`、`DestructiveReasonConfirmDialog` 全部收敛至基于 `FormalActionConfirmDialog (layout="compact")` 的统一实现：
- **原语与视觉统一**：统一采用 `AlertDialog`（`FormalActionConfirmDialog`）+ 顶部紧凑状态变更条（如 `审批中 → 未提交` / `审批受阻 → 已终止`）+ 核心核对事实（单号、往来单位、金额）+ 强制 `actionVariant="destructive"` 红色主确认按钮（修复销售/采购单与库存调整非 destructive 的高危缺陷）。
- **表单与健壮性统一**：强制使用 `useAppForm` + 统一 `ReasonTextareaField`（严格必填 + BPM 512 UTF-8 字节计数，支持 `Ctrl/Cmd+Enter` 提交），统一接入 `FormalCommandKeyLedger` 幂等键与「提交结果未知」自动核对恢复流程。
- **预填文案规范化**：驳回后撤回修改统一文案为 `"按驳回意见修改原单后重新提交"`。

#### B. 统一原因长度校验真源（严格对齐各后端端点真实上限）

后端不同命令端点存在 4 档明确校验契约，在 `lib/validation/reason-length.ts` 建立单一真源校验库，彻底杜绝前后端口径错配：

| 业务命令域 | 后端真源与实测上限 | 前端校验器与实时计数器 |
|---|---|---|
| **BPM 审批决定 / 撤回 / 终止受阻 / 升级绑定 / 重提原因** | `bpm/src/model/types.rs:403` `REASON_MAX_LEN = 512` **UTF-8 字节**（约 170 汉字） | `bpmReasonSchema`（`new TextEncoder().encode(v).length <= 512`），实时显示 `已用 96/512 字节（约还可输 138 字）` |
| **放弃退回重办 (`ABANDON_REWORK`)** | `abandon_rework.rs:40` → `validation.rs:63-74` `normalize_required_text(..., 512)` **512 字符**（按字符非字节，不会因 >170 汉字被后端拒收） | `charLimitSchema(512)`（统一提示 ≤512 字符，不误导标注字节报错） |
| **任务转交 (`REASSIGN`)** | `backend/.../dto/work_item/command.rs:84-93` `reason` 上限 **150 字符**（必填） | `workItemReassignReasonSchema`（必填，≤150 字符，实时计数 `0/150 字`） |
| **任务关闭 (`CLOSE`)** | `backend/.../dto/work_item/command.rs:95-109` `reason_code` 必填(1–64)、`comment` 可选上限 **100 字符**、`DUPLICATE` 时必填 `replacement_work_item_id` | `workItemCloseSchema`（条件校验 `replacement_work_item_id` + `comment ≤ 100 字`） |

```ts
// erp-client/lib/validation/reason-length.ts
export const BPM_REASON_MAX_UTF8_BYTES = 512
export const WORK_ITEM_REASSIGN_REASON_MAX_CHARS = 150
export const WORK_ITEM_CLOSE_COMMENT_MAX_CHARS = 100
export const WORK_ITEM_ABANDON_REASON_MAX_CHARS = 512

export function utf8ByteLength(value: string): number {
    return new TextEncoder().encode(value).length
}

export function bpmReasonSchema(options: { required: boolean; label?: string }) {
    const label = options.label ?? "原因"
    return z.string().superRefine((raw, ctx) => {
        const trimmed = raw.trim()
        if (options.required && trimmed.length === 0) {
            ctx.addIssue({ code: "custom", message: `请填写${label}` })
        }
        const bytes = utf8ByteLength(trimmed)
        if (bytes > BPM_REASON_MAX_UTF8_BYTES) {
            ctx.addIssue({
                code: "custom",
                message: `${label}超出长度限制（当前 ${bytes} 字节，上限 512 字节，约 170 个汉字）`,
            })
        }
    })
}
```

#### C. 全量 13 类单据强制覆盖 `SubmissionRouteConfirmation`
- 将当前 6 处提交入口（4 套模式）收敛为 **2 套标准提交确认壳**：
  1. `PaperSubmitConfirmDialog`（`Dialog size="2xl"`，销售单与卡券销售单）：在纸面预览右侧/底部吸底栏补齐紧凑 `SubmissionRouteConfirmation` 与重提原因；
  2. `FinancialSubmitConfirmDialog`（`FormalActionConfirmDialog`，采购单、开票申请、回款、两类退款、两类冲正、付款申请）：为 `InvoiceRequestForm`（`invoice-requests/components/request-form.tsx`）补齐提交确认层与 `SubmissionRouteConfirmation`。

#### D. 分级风险审批决策交互（定案：ADR-1）
审批工作台底部决策条（`ApprovalDecisionBar`）直接内嵌**单行可扩展意见输入框**（placeholder：`可选填写审批意见，驳回时必填…`，实时接入 `bpmReasonSchema` 512 UTF-8 字节计数；聚焦或点驳回时平滑展开至 3 行并附带**按 `documentType` 动态匹配的快捷短语**，解决销售/财务短语硬编码错位问题），并严格按 **ADR-1 分级风险决策制**执行：
- **常规低/普通风险单据通过（Approve — 零二级弹窗）**：
  - 点击主蓝按钮「通过审批」或按快捷键 `A` → 直接附带底栏内联意见（可为空）提交，给予 3 秒 toast 反馈并在开启 `autoNext` 时自动切入下一项，彻底消灭无意义二次确认弹窗。
- **高危单据通过（High-Risk Approve — 强制单层核对）**：
  - 凡命中以下任一高危条件，点击「通过审批」或按 `A` 强制弹出单层 `FormalActionConfirmDialog`（列出风险红字提示、关键金额/余额变动与本次内联意见）核对后方可提交：
    1. **冲正红字单据**：`receipt_reversal`、`payment_reversal`；
    2. **负毛利或超信用/预算预警**：单据触发负毛利行项目、客户超信用额度或超预算告警；
    3. **大额付款/退款超阈值**：`purchase_payment_request`、`customer_refund`、`supplier_refund` 金额超过配置的高危复核阈值；
    4. **阻断警告**：单据当前携带业务核验阻断级告警（blocking warning）。
- **驳回（Reject）与撤回（Withdraw）— 统一严格校验**：
  - **驳回**：点击红色描边按钮「驳回」或按 `R` → 强制展开并聚焦底栏原因输入区（附带对应 `documentType` 快捷驳回短语 chip），严格校验必填与 **BPM 512 UTF-8 字节上限**，支持 `Cmd/Ctrl+Enter` 提交。
  - **撤回**：全站统一走 `ApprovalWithdrawDialog`（强制红色 `destructive` 主按钮 + 必填原因 + **BPM 512 UTF-8 字节计数** + 幂等恢复）。

---

### 2.4 统一附件与材料对照预览器（`UnifiedMaterialViewer`）

彻底淘汰当前 **3 套审批/抄送预览机制 + 1 套新品提报图弹窗（共 4 套）**（12 类审批的裸新 Tab `/approval-materials`、付款申请独占的遮挡式 `RequestMaterials` 弹窗、抄送页图片硬塞 iframe 的 `CopyFiles`、新品审核 `PortalReviewImages`）：

- **鉴权加载与 PDF 引擎复用（严格适配 Bearer Token 架构）**：
  - 后端接口采用 `Authorization: Bearer <token>`（`lib/api/client.ts:85-88`），严禁将受保护接口 URL 直接塞给 `<img src>` 或 `pdfjs.getDocument(url)`（必然 401）。所有附件统一通过各领域已有的 **`apiGetBlob(...)` 认证拉取器**（`approval-workflow/api/materials.ts:254-276`、`copy-files.tsx:60-74`）获取 `Blob`，由预览器内部统一管理 `URL.createObjectURL` 生命周期与卸载释放。
  - **直接复用并泛化现仓成熟的 `ContractPdfPages`**（`features/contracts/components/contract-pdf-pages.tsx:10-172`，配合 `scripts/prepare-pdf-assets.mjs` 消费 `blob.arrayBuffer()` 并从同源 `/pdfjs/${pdf.version}/` 加载 CMap/标准字体/WASM），下沉为 `components/business/pdf-document-canvas.tsx` 并扩展缩放、适应页宽与 90° 旋转能力；图片类使用原生 `<img>`（缩放/旋转/拖拽）；不支持格式降级下载卡。
- **两种自适应形态**：
  1. **宽屏侧边对照模式（Docked Side-by-Side Panel，主推）**：在 2 栏审批工作台右侧上下文轨切至「附件预览 (N)」Tab，支持拖拽分隔条（`ResizableHandle`）加宽至 45%–50% 视口宽度——**审单人左侧核对商品行/金额/税点，右侧同步对照合同 PDF / 银行回单 / 验收单，零遮挡、零切 Tab！**
  2. **灯箱弹窗模式（`Dialog size="2xl"` / `"full"`，窄屏与列表快览回退）**：统一顶栏工具条（左：同单多附件切页；中：缩放 `- / 100% / +`、适应宽度、旋转；右：`LoadingButton` 顺序固定为 `下载原文件` + `关闭`）。
- **附件触发条统一**：`WorkspaceSalesApproval` 与 `WorkspaceInvoiceApproval` 移除页顶重复弹窗按钮，将附件数量徽章直接挂在右侧栏「附件 (N)」Tab 上；文件列表统一使用 `MaterialFileList`，按钮统一采用 `LoadingButton` 且顺序固定为 **`预览` → `下载`**。

---

### 2.5 统一流程路线与审计时间线原语（`UnifiedProcessTrack`）

将全站 **9 套**割裂的路线/历史/时间线组件（`WorkspaceApprovalRouteSummary`、`ApprovalRoute` 水平/垂直、`ExecutionHistory`、`SalesOrderApprovalHistory`、`FlatApprovalHistory`、`ApprovalCopyDetailPage` 记录、`CopyHistory`、`TaskTimeline`）收敛为 **2 个核心原语 + 1 个标准数据源适配器**：

1. **`ApprovalRouteStepper`（紧凑横向步进条，定高 `44px`）**：
   - 仅占单行（替代当前 `110–150px` 的 `WorkspaceApprovalRouteSummary` 双行块），展示当前轮次节点流（图标 + 节点名 + 审批人 + 已决节点通过 `formatDateTime` 展示精简时间 `MM-DD HH:mm`，**修复 `ApprovalRoute vertical` 隐藏 `decidedAt` 的缺陷**）。
   - 驳回态直接将驳回原因折叠为单行红底警示条，点击锚定到右侧完整历史中的对应驳回记录。
2. **`UnifiedAuditTrail`（右侧常驻审计轨迹，合并审批历史 + 抄送记录 + 单据动态）**：
   - 默认**倒序（最新轮次/最新事件置顶）**，审单人一进页即可对照上一轮驳回意见核查本轮修改，不再需要打开遮挡正文的 `WorkspaceApprovalRouteSummary` 模态 Dialog。
   - 顶部轻量分段切换：`审批与抄送`（统一呈现多轮 `ExecutionHistory` + 节点关联 `CopyHistory`）| `单据全动态`（合并原右上角 `TaskTimeline` Sheet，并内置「当前任务 / 销售全链路」切换以消灭 `TaskTimeline → SalesTimelineSheet` 抽屉套娃）。
   - 强制所有时间戳走 `formatDateTime` / `displayUnixSeconds` + `.num`，支持「加载更多」追加而非整页替换丢弃前页（修复抄送详情翻页 bug）。

---

## 3. 统一设计系统、视觉美化与页面利用率方案（Design System, Visual Polish & Space Budget）

### 3.1 容器查询优先布局引擎（`@container`-First Architecture）

**根因复盘**：ERP 采用宿主侧栏（`--spacing-nav: 240px` / 折叠 `60px`）+ 子 `<iframe>` 架构。标准 `1440px` 笔记本展开侧栏后，iframe 实际宽仅 **`1200px`**；`1280px` 屏仅 **`1040px`**。所有使用视口断点 `xl:`（`1280px`）或 CSS `@media (min-width: 80rem)` 的布局全部发生坍塌。

**系统级修复方案（精确对齐 Tailwind CSS v4 实测断点，零新依赖）**：
1. **Tailwind v4 内置断点与嵌套容器陷阱规避**：
   - 经实测 `erp-client/node_modules/tailwindcss/theme.css:343-344`，Tailwind v4 内置 `--container-5xl` 为 `64rem`（**1024px**，非 960px），`--container-6xl` 为 `72rem`（**1152px**，非 1120px）。
   - 若同时嵌套 `@container/workspace`（iframe 根节点）和 `@container/pane`（扣除两侧 `--scaffold-px` 40–48px 与滚动条后，`1280px` 屏净宽仅 **~984–1000px**、`1440px` 屏净宽仅 **~1136–1152px**），在 `pane` 内写不带容器名的 `@5xl:`/`@6xl:` 或 `@[960px]:` 会查询内层 `pane` 而依然坍塌！
   - **明确规范**：在 `components/layout/workspace-frame.tsx:127` 的 `<main id="main-content">` 声明主命名容器 `@container/workspace`，所有工作台顶层栅格一律使用**命名容器断点**（如 `@[960px]/workspace:`、`@[1120px]/workspace:`、`@[1480px]/workspace:`）；局部组件自适应（如右栏内卡片）才显式声明独立局部容器名（如 `@container/inspector`）。

| 目标物理屏幕 | 侧栏展开态 iframe 净宽 | 侧栏折叠态 iframe 净宽 | 命名容器断点类（替代 `lg:` / `xl:` / `2xl:`） | 布局行为 |
|---|---|---|---|---|
| 窄窗口 / 移动端 | `< 768px` | `< 768px` | 默认单栏 | 单栏堆叠 + `TaskActionSheet` 抽屉 |
| `1280×800` 笔记本 | **1040px** | **1220px** | **`@[960px]/workspace:`** (`60rem`) | 角色工作台 4 列 KPI 单行铺满；详情进入 **65/35 双栏（右栏 340px）**；筛选托盘 4 列 |
| `1440×900` 主力桌面 | **1200px** | **1380px** | **`@[1120px]/workspace:`** (`70rem`) | 列表页开启 **Master-Detail 分栏（左 430px + 右 770px）**；详情 **65/35 双栏（右栏 380–410px，可拖宽至 50% 对照附件）** |
| `1920×1080+` 宽屏 | **1680px** | **1860px** | **`@[1480px]/workspace:`** (`92.5rem`) | 列表分栏右窗格支持**内嵌 2 栏（正文 + 附件对照）**；详情右栏 420–480px，单据宽表零横向滚动 |

2. **同步改造 `app/globals.css` 原生媒体查询与分栏直接子选择器**：
   - 如实对齐 `app/globals.css` 现存 4 处视口断点并按梯队迁至 `@container workspace`：
     - `L639` 原 `@media (min-width: 75rem)`（1200px，`--scaffold-px` 切换至 `page-inline-lg`）→ 改为 `@container workspace (min-width: 70rem)`（1120px，确保 1440px 屏展开侧栏与滚动条后仍稳态生效）；
     - `ListFilterTray` 三级筛选网格：原 `L1174` `@media (min-width: 48rem)`（768px，2 列）/ `L1203` `@media (min-width: 64rem)`（1024px，3 列）/ `L1211` `@media (min-width: 80rem)`（1280px，4 列）→ 梯队改写为 `@container workspace (min-width: 48rem)`（2 列）/ `@container workspace (min-width: 52rem)`（832px，3 列）/ `@container workspace (min-width: 60rem)`（960px，4 列），既不错并 2/3 列断点，又让 1280px/1440px 屏稳态呈现 4 列筛选托盘。
   - `app/globals.css:572-603, 912-1049` 大量使用直接子选择器 `>`（如 `.erp-list-workspace-page > [data-slot="list-work-surface"]`），在引入 `WorkspaceSplitQueue`（内含 `ResizablePanelGroup` 包裹层）时，须同步将对应规则扩展支持 `[data-slot="workspace-split-queue"] [data-slot="list-work-surface"]`，确保纯白通栏表头、滚动阴影与边线零回归。
3. **宿主/Iframe 高度密度同步（消除 `961–1004px` 不同步 Bug）**：
   - 宿主 `WorkspaceTabs` 计算外层视口高度档位（`comfortable` / `compact ≤960px` / `dense ≤720px`），通过 `data-viewport-density` 属性同步写入自身及所有子 iframe 的 `document.documentElement`，取代各自独立求值的 `@media (max-height: 960px)`。
4. **清除结构性空间杀手**：
   - **解除审批单栏硬编码**：删除 `workspace-processing-layout.tsx:91-96` 中的 `approvalDocument || paymentReview` 强制 `singleColumn` 限制，让 13 类审批全面启用双栏决策工作台。
   - **撤除执行类任务 376px 通用 `aside`**：对 10 类执行类任务（含付款安排、采购依据变更、退货退款/红票、客户验收、付款执行、开票执行、结算复核、新品审核、历史导入）与 2 类异常任务（共 12 类当前背负 `20rem aside` 的任务），不再渲染重复元数据的 `20rem` 侧栏（`+ gap-x-8 + pl-6 = 376px`），将全部水平宽度还给表格与表单；仅当任务需要**审批轨迹 / 附件对照 / 异常原始载荷**时激活右侧动态上下文轨（并允许用户按 `]` 一键折叠获 100% 全宽）。
   - **根除双页头、双侧栏与双水平内边距**：
     - 所有被嵌入工作台的业务页（`SupplierOrderCenterPage`、`BatchDetailView`、`SettlementCenter`、`IntegrationErrorsPage`）统一遵守 `embedded` 契约：**`embedded=true` 时严禁渲染外层 `<DetailWorkspacePage>`、`<PageScaffold>`、`<DocumentHeader>` 或独立 `<aside>`**，仅输出扁平 `<DetailWorkspaceSection>` 列表。
     - 清除 `WorkspaceReturnFinance`、`PurchasePaymentRequestTask`、`PurchaseSourceUpdateWorkspace`、`WorkspaceInvoiceTask`、`WorkspaceDeliveryBrief`、`PurchasePaymentArrangement` 根节点的重复 `px-5`/`px-7`，确保左沿与页头 `DetailWorkspaceChrome` 像素级对齐。
   - **释放高分屏流程画布高度**：`approval-processes/components/definition-flow-canvas.tsx:483` 由定高上限 `lg:h-[clamp(440px,54vh,620px)]` 升级为 `h-[clamp(480px,calc(100dvh-16rem),960px)]` 并增加画布全屏按钮，消除 2K/4K 屏下方大面积空白。

---

### 3.2 量化视口空间预算（Before vs After：`1440×900` 与 `1280×800`）

#### 场景 1：统一任务/审批/抄送列表页（严格遵循 `AGENTS.md §8` `TableToolbar` 规范）

| 界面层级 | 现状 `1440×900` | 重构后：通栏紧凑表 (`1440×900` / `1280×800`) | 重构后：Master-Detail 分栏 (`1440×900`) | 优化手段 |
|---|---:|---:|---:|---|
| 1. 全局 Topbar + Tab 栏 | 40px | 40px / 40px | 40px | 保持 |
| 2. 队列页头 + 类型视图条 | 48px + 37px = **85px** | **44px** / **44px** | **44px** | 迁移至 `ListWorkspacePage`，主聚合视图内联页头 |
| 3. 搜索与筛选工具栏 | 52px + 40px (`commonFilters`) = **92px** *(含芯片 126px)* | **42px** / **42px** | **42px** | 到期快捷与范围切换并入单行，取消第 2 行 |
| 4. `TableToolbar`（列设置 / 批量条） | 0px（`ListWorkSurface` 已内联于工具栏右侧） | **0px** / **0px** | **0px**（分栏卡按 `AGENTS.md §8 L124` 设 `showColumnVisibility={false}`） | 复用 `ListWorkSurface`（`list-workspace.tsx:287-318`）机制：`TableToolbarInlineControls`（排序+列设置）已内联在搜索工具栏右侧且对独立 `<TableToolbar>` 设 `hideControls={Boolean(toolbar)}`，勾选条默认采用底部悬浮 `selectionBarPlacement="dock"`（`L327-353`），**表上方零额外占高**且 100% 符合 `AGENTS.md §8` |
| 5. 表头行 | 34px | **32px** / **32px** | **32px**（左栏精简头） | `h-table-header` 紧凑档 |
| 6. 底部分页栏 | 45px（1 页也常驻） | **0px（单页）/ 36px（多页）** | **0px（单页）/ 36px（多页）** | 总条数内联至页头/工具栏，仅 `pageCount > 1` 渲染分页条 |
| **非数据 Chrome 总占用** | **296px (32.9%)** *(含芯片 330px)* | **单页 158px (17.6%) / 多页 194px (21.6%)**<br>*(1280×800: 19.8% / 24.3%)* | **单页 158px (17.6%) / 多页 194px (21.6%)** | **通栏与分栏均纵向净释放 138px（+15.3% 有效面积）** |
| **数据区净高 / 行高 / 首屏可见条目** | 604px / 48–64px / **9–12 行**<br>*(1280×800: 504px / 8–10 行)* | `1440×900` (`742/706px`, 36px 单行): **20 行（单页）/ 19 行（多页）**<br>`1280×800` (`642/606px`, 36px 单行): **17 行（单页）/ 16 行（多页）** | `1440×900` (`742/706px`, 54px 双行摘要卡): **13–14 条（单页）/ 12–13 条（多页）+ 右栏即时全貌** | **通栏首屏行数 +67%~+85%；分栏态零跳页即时审阅** |

#### 场景 2：决策审批与任务办理工作台（`/workspace/tasks/[id]`、`/workspace/approvals/[id]`）

| 界面层级 | 现状 `1440×900` | 重构后 `1440×900` | 现状 `1280×800` | 重构后 `1280×800` | 优化手段 |
|---|---:|---:|---:|---:|---|
| 1. 全局 Topbar | 40px | 40px | 40px | 40px | 保持 |
| 2. 顶部吸顶 `DetailWorkspaceChrome` | **104px**（双行：返回行 34px + 标题元信息行 70px） | **56px**（单行融合：返回 + 标题 + 状态/单号徽章 + `1/24 < >` + 图标组） | 104px | **56px** | 导航与标题融合为紧凑单行吸顶头；移除重复「当前审批人」 |
| 3. 审批流横条 / 任务 Summary 带 | **86–108px**（审批流双行）或 75px | **0px（移入右轨顶部 44px 步进条，不占正文纵高！）** | 86–108px | **0px** | 彻底释放正文首屏纵向高度 |
| 4. 金额概览区 | 96px（独立大块） | **52px**（紧凑横排 KPI 指标带） | 96px | **52px** | 4 项核心金额/毛利/额度一行展示，`.num` 强对齐 |
| **小计：明细表上方纯占高（1+2+3+4）** | **326–348px (36.2–38.7%)** | **148px (16.4%)** | **326–348px (40.8–43.5%)** | **148px (18.5%)** | **明细上方净释放 178–200px，明细与差异 100% 进首屏！** |
| 5. 底部吸底操作栏 | 57px | **48px**（内联意见 + 动作） | 57px | **48px** | 与 `autoNext` + 快捷键提示紧凑对齐 |
| **顶部 + 底部非明细 Chrome 总占用** | **383–405px (42.6–45.0%)** | **196px (21.8%)** | **383–405px (47.9–50.6%)** | **196px (24.5%)** | **总净释放 187–209px，且右侧同步常驻轨迹与附件对照** |

#### 场景 3：角色工作台首页（`/workspace/roles/WB-01` 等）

| 界面层级 | 现状 `1440×900`（iframe 1200px） | 重构后 `1440×900`（`@container/workspace`） | 现状 `1280×800`（iframe 1040px） | 重构后 `1280×800`（`@container/workspace`） | 优化手段 |
|---|---:|---:|---:|---:|---|
| 1. 宿主 Topbar + Sticky 页头 | 40px + 71px = **111px** | 40px + **48px** = **88px** | 111px | **88px** | 页头单行化，替换 `<NativeSelect>` 为轻量切换胶囊 |
| 2. 快捷操作条 | **53px**（独立行） | **0px**（高频 2–3 动作内联入页头右侧，其余收进「+ 新建」下拉） | 53px | **0px** | 节省整行高度 |
| 3. 岗位业务概览 KPI 区 | **296px**（`1200 < xl` 坍塌成 2×2 大卡片！） | **104px**（`@[960px]/workspace:grid-cols-4` 单行 4 列扁平 KPI 带） | 296px | **104px** | 命名容器查询根治折行，去厚边框改为细线分割指标带 |
| **到达「需优先处理待办」前占高** | **460px (51.1%)** | **192px (21.3%)** | **460px (57.5%)** | **192px (24.0%)** | **净释放 268px；首屏 78% 空间直接呈现待办与推进阶段** |

---

### 3.3 核心目标页面线框图（ASCII Wireframes）

#### (a) 统一任务/审批队列 + Master-Detail 分栏模式（`1440×900`，iframe `1200×860`）

```text
+---------------------------------------------------------------------------------------------------+
| [ERP Tab Bar: 我的工作台 | ● 待我处理 (24) | 销售单 SO-20260418 ×]      [🔍 ⌘K] [🔔3] [⚠1] [用户] | 40px
+---------------------------------------------------------------------------------------------------+
| 待我处理  [全部 24] [待审批 11] [执行 9] [异常 4]  | 范围内待办       更新于 18:10 [导出] [分栏⊞] [↻]| 44px
+---------------------------------------------------------------------------------------------------+
| [🔍 搜索单号/往来单位/商品 (/)      ] [全部状态▾] [已超期 3] [今日到期 5] [更多筛选]  [紧急优先▾] | 42px
+-------------------------------------------+--+----------------------------------------------------+
| [ ] 单号/往来单位      类型  金额    时限  |  | [SO-20260418] [销售单审批] [审批中] [⤢全屏][×]   | 44px
|-------------------------------------------+  +----------------------------------------------------+
| [✓] SO-20260418      [销售] ¥128,400 ⚠超期|  | 步进: ✓提交(李明) → ●销售总监(我) → ○财务复核       | 44px
|     上海星辰科技      审批中              |  +----------------------------------------------------+
|-------------------------------------------+  | 总额 ¥128,400 | 毛利率 24.6% | 回款 30% | 附件 2   | 44px
| [ ] PR-20260419      [付款] ¥45,000  今日 |║ +----------------------------------------------------+
|     杭州华通供应      待审批              |║ | [概览与明细]  [合同与附件 (2)]  [审批与动态 (5)]   | 34px
|-------------------------------------------+║ |----------------------------------------------------|
| [ ] INV-20260412     [开票] ¥82,000  明日 |║ | 商品名称        规格   数量   单价     小计   交付 |
|     南京鼎盛商贸      执行中              |║ | 高精度传感器A   M12    200  ¥420.00  ¥84,000 [2地▾]|
| ... (分栏双行摘要卡 54px/条,首屏 13-14 条; |║ | 工业控制模块B   V2     100  ¥444.00  ¥44,400  直发 |
|  切通栏表格态复用内联列设置,36px 单行      |║ |   └─ 行内展开: 苏州仓 120件 / 上海仓 80件 (免弹窗)|
|  首屏可见 19-20 行) ...                   |║ |                                                    |
+-------------------------------------------+  +----------------------------------------------------+
| [已选 1 项] [批量转交] [批量标已读] [取消] |  | [☑ 自动下一项] [可选审批意见...    ] [驳回] [通过]| 48px
+-------------------------------------------+--+----------------------------------------------------+
|<----------- 430px (36%) 队列列表 --------->|<----------- 770px (64%) 原位快审检查器 ------------->|
```

#### (b) 2 栏决策审批与办理工作台（完整 Tab：`1440×900`，iframe `1200×860`）

```text
+---------------------------------------------------------------------------------------------------+
| [← 返回队列]  上海星辰科技  [销售单审批] [审批中]  SO-20260418    第 3/24 项 [‹K] [J›] [?] [↻] [◧]| 56px
+----------------------------------------------------------------+--+-------------------------------+
| 订单总额 ¥128,400.00 | 预估毛利 24.6% | 账期 月结30天 | 含税13%|  | [审批与轨迹] [附件对照(2)] [动态]|
+----------------------------------------------------------------+  +-------------------------------+
| ⚠️ 上轮驳回核对(第1轮·王总监 04-17): 合同第4条交期需与明细一致 |  | ── 当前流程 (第2轮) ──────────|
+----------------------------------------------------------------+  | ✓ 1. 发起提交  李明 04-18 09:10|
| 【商品与交付明细 (12行)】                      [仅看关注项 (2)]|  |   "已补传修订后盖章合同"      |
| 编码     品名            数量   单价      小计     毛利  交付  |  | ● 2. 销售总监  我   待审批     |
| SKU-01   高精度传感器A   200   ¥420.00   ¥84,000  26.1% [2地▾]|║ | ○ 3. 财务复核  赵敏 未开始     |
|   ├─ 苏州园区仓 120件 · 04-25前交付                            |║ +-------------------------------+
|   └─ 上海松江仓  80件 · 04-28前交付                            |║ | ── 附件对照 (认证Blob渲染) ── |
| SKU-02   工业控制模块B   100   ¥444.00   ¥44,400  21.8%  直发  |║ | 📄 销售框架合同_v2.pdf  [预览]|
| ...                                                            |║ | 🖼 客户确认函.png       [预览]|
| 【商务条款与备注】 付款条件 / 发票类型 / 收货联系人 ...         |║ |  (点[预览]直接在右栏展示PDF，  |
|                                                                |║ |   拖拽中轴可拉宽至 50% 对照!) |
+----------------------------------------------------------------+--+-------------------------------+
| [☑ 处理后自动下一项] [打开原单] [转交]    [可选填写意见 (⌘↵)...      ]  [驳回 (R)]  [通过审批 (A)]| 48px
+---------------------------------------------------------------------------------------------------+
|<------------------ 66% (~790px) 单据与业务明细画布 ------------------>|<-- 34% (~410px) 上下文轨 -->|
```

#### (c) 紧凑型角色工作台页头与单行 KPI 指标带（`/workspace/roles/WB-01`，`@container/workspace` 修复后）

```text
+---------------------------------------------------------------------------------------------------+
| 销售工作台 [切换岗位▾]  [本周▾]              [+ 新建销售单] [+ 询价] [待我处理 (14)] [紧急 3] [↻] | 48px
+---------------------------------------------------------------------------------------------------+
| 本周签单额                | 待审批销售单        | 待客户验收          | 逾期风险单据              |
| ¥1,286,400  ↑12.4%       | 7 笔  · ¥412,000    | 5 笔  · ¥198,000    | 2 笔  [立即排查 →]        | 104px
+-------------------------------------------------------------+-------------------------------------+
| 【业务推进阶段】 报价(12) ▸ 审批(7) ▸ 履约(18) ▸ 回款(9)    | 【需优先处理 (Top 8)】   [全部 →]  |
| ----------------------------------------------------------- | ⚠ SO-0418 审批超期  ¥128,400  [快审]|
| 【近期关键动态】                                            | ⚠ SO-0415 退回重办   ¥64,200  [处理]|
| 17:42  SO-0418  销售变更已提交审批          [查看]          | ⏰ ACC-092 客户验收   ¥92,000  [登记]|
| 16:15  INQ-204  采购已完成报价反馈          [查看]          | ...                                 |
+-------------------------------------------------------------+-------------------------------------+
|<-------------------- 62% 业务推进与动态 ------------------->|<------- 38% 优先待办直达栏 -------->|
```

---

### 3.4 视觉统一、令牌收敛与自动化门禁扩展

#### A. 唯一权威状态 / 色调 / 文案字典（解决全仓状态语义与色板冲突）

建立 `erp-client/lib/task-status-dictionary.ts` 作为 `StatusBadge` 唯一真源，废弃分散在 `approval-copies/columns.tsx`、`approval-copies/detail-page.tsx`、`workspace-task-columns.tsx`、`dashboard.ts` 中的重复映射：

| 域 | 枚举值 | 权威中文文案 | 权威 `StatusBadge` Tone | 解决的冲突 |
|---|---|---|---|---|
| 审批实例 (`ApprovalInstanceStatus`) | `RUNNING` | `审批中` | **`info`**（商务蓝） | 统一 `display.ts` (`warning`) 与抄送页 (`info`) 冲突；“正常流转中”用蓝，`warning` 严格留给超期/待办提示 |
| 审批实例 & 节点 & 任务阻断 | `BLOCKED` / `APPROVAL_BLOCKED` / `EXECUTION_BLOCKED` | `审批受阻` / `执行受阻` | **`destructive`**（飞书红） | 统一列表/页头 (`warning`) 与审批真源 (`destructive`) 冲突，受阻属异常阻断必须高警示红 |
| 审批实例 & 节点 | `APPROVED` | `已通过` | **`success`**（飞书绿） | 保持一致 |
| 审批实例 & 节点 | `REJECTED` | `已驳回` | **`destructive`**（飞书红） | 保持一致 |
| 审批实例 & 节点 | `CANCELLED` | `已撤回` | **`void`**（浅灰删除线/中性暗） | 保持一致 |
| 审批节点 (`NodeExecutionStatus`) | `ACTIVE` | `待审批` | **`warning`**（琥珀提示，仅用于需人动作的当前待办节点） | 统一 `办理中(info)` / `当前审批(info)` / `待审批(warning)` 三套分裂 |
| 审批节点 | `SUPERSEDED` | `往轮已替代` | **`neutral`** | 统一 `已由后续轮次替代` / `往轮记录` |
| 事项状态 (`WorkItemStatus`) | `OPEN` / `COMPLETED` / `CLOSED` | `待处理` / `已完成` / `已关闭` | `info` / `success` / `neutral` | 全站统一使用 `StatusBadge`（禁用裸 `<Badge>` 充当状态徽章） |

#### B. 统一任务与单据中文命名 + 顶栏双徽章视觉分化

| 枚举标识 / 入口 | 权威唯一名称与呈现规范 | 解决的分歧 |
|---|---|---|
| `PROCUREMENT_ORDER_CREATION` | **采购供给分配**（短徽章：`供给分配`） | 废弃 ~~待安排采购~~ / ~~采购分单~~ 3 套叫法 |
| `SALES_INQUIRY_PROCESSING` | **询价采购处理**（短徽章：`询价处理`） | 废弃 ~~采购询价~~ |
| `PURCHASE_SOURCE_UPDATE` | **采购依据变更**（短徽章：`变更影响`） | 废弃 ~~处理销售变更影响~~ / ~~采购依据更新~~ |
| `INTEGRATION_RESULT_UNKNOWN` | **外部同步待核实**（短徽章：`同步待核`） | 废弃 ~~集成结果未知~~ / ~~外部同步结果待确认~~ |
| `sales_invoice_request` | **销项开票申请**（短徽章：`开票申请`） | 统一流程目录与单据名 |
| `autoNext` 开关文案 | **处理后自动下一项** | 统一替换 `FULFILLMENT_OPERATION` 的 ~~完成后下一项~~ |
| 顶栏 `InboxPopover` vs `SystemFeedbackPanel` | `InboxPopover` 保留业务红点徽章并新增**「全部标为已读」**按钮；`SystemFeedbackPanel` 改为**琥珀色警告轮廓徽标（`warning-soft` + 警告三角图标）** | 彻底区分“业务未读消息”与“前端接口/系统异常提示” |

#### C. 权威 `ApprovalActionBar` 与任务底栏肌肉记忆规范
- **统一排序（全站零镜像反转）**：无论在工作台详情、`TaskActionSheet`、还是单据详情内嵌区，主决策按钮永远固定为左次右主：
  - **左侧区**：`[☑ 处理后自动下一项]` + 辅助/管理动作（`打开原单`、`转交`、`撤回审批`、`放弃重办`，均用 `variant="outline"` 或 `ghost`）
  - **右侧决策区**：`[驳回]`（固定在左，统一 `variant="outline"` + 红色描边 `border-destructive-border text-destructive-soft-foreground hover:bg-destructive-soft`，弹窗/确认态主按钮为实心 `destructive`）→ **`[通过审批]`**（固定最右，唯一主蓝 `variant="default"`）。
  - 彻底删除 `approval-action-bar.tsx:117-135` 中按 `presentation === "workspace"` 左右调换与样式突变逻辑。

#### D. 排版、数值密度、去套娃卡片与 `<NativeSelect>` 清理
- **严守 5 档正文 + 4 档数值角色 + 零裸圆点原则（对齐 `status-badge.tsx:17`）**：
  - 通栏列表默认 `density="compact"`（`36px` 单行），严格遵守 `components/ui/status-badge.tsx:17`（「不使用仅靠红黄绿圆点表达状态」）：状态列仅展示 1 枚主 `StatusBadge`（图标+短文字），超期/今日到期以**图标+短文字**（如 `⚠ 超期`、`今日`）清晰呈现在时限列，严禁将 3 个徽章垂直堆叠把行高拉成 60–84px。
- **全面消除裸 `Date` 本地化调用**：替换审批/抄送 6 处（`copy-history.tsx:77`、`approval-sales-order-paper.tsx:77`、`approval-copies/columns.tsx:118-120`、`approval-copies/detail-page.tsx:22`、`workspace-sales-approval.tsx:55`、`workspace-invoice-approval.tsx:110`）及全仓其余 `Date` 实例的 `.toLocaleString`/`.toLocaleDateString` 为 `formatDateTime` / `displayUnixSeconds` + `.num`（保留数值千分位格式化不受影响）。
- **扁平化去套娃卡片**：重构 `DocumentApprovalArea`、`ApprovalReadonly`、`RuntimeSummary`、`DefinitionBindingCard`、`SupplyExceptionTaskPanel`，去除内层带阴影/粗边框的 `<Card>`，改用 `DetailWorkspaceSection` + `border-t border-border/60` 水平细线分隔。
- **清除 `<NativeSelect>` 漂移**：将 `role-workbench-header.tsx:83,254` 及另外 4 个 feature 文件中的 `<NativeSelect>` 全部替换为 `Select` / `OptionCombobox`。

#### E. 扩展 `erp-client/scripts/check-ui-theme.mjs` 新增 6 条精准门禁规则（零误伤设计）

| 规则编号 | 精准检测目标（AST / Regex） | 拦截策略与存量迁移边界 | 治理目的 |
|---|---|---|---|
| **Rule-1** | `features/**` 中 `<DialogContent>` 的 `className` 出现 `max-w-*` 或 `max-h-\[.*?(?:vh\|svh)\]` | Error：`弹窗尺寸须通过 DialogContent size 属性声明，高度统一使用 dvh，禁止手写 max-w/vh` | 根治 152+ 文件（实测 160 处）宽度与高度单位蔓延 |
| **Rule-2** | `features/**` 中 JSX 标签为 `NativeSelect` 或从 `@/components/ui/native-select` 导入 | Error：`业务页面禁止使用 NativeSelect，请改用 Select 或 OptionCombobox` | 落实 `components/business/README.md:33,120` 规定 |
| **Rule-3** | `features/**` 中出现 `surfacePanelClassName`、`PageHeader`、`DocumentHeader`、`DetailPageHeader`、`ObjectSectionTabs` | Phase 1 对新增文件直接报错，对现存 **12 个非详情 `surfacePanelClassName` 文件与 8 个 `PageHeader` 文件**挂临时白名单，Phase 2/3 清零后全面封禁 | 消除旧版布局壳与嵌套外框卡片回潮 |
| **Rule-4** | `features/**` 中针对 **Date 实例**的本地化调用（`new Date(...).toLocale(String\|DateString\|TimeString)` 或 `.toLocaleDateString(` / `.toLocaleTimeString(`） | Error：`禁止裸用 Date.toLocale*，时间统一使用 formatDateTime / displayUnixSeconds`（**不误伤数值千分位 `number.toLocaleString()`**） | 消除全仓日期格式与等宽列抖动 |
| **Rule-5** | `features/**` 中右侧预览/业务抽屉手写 `px-7` 或新增裸 `<SheetContent>` | Error：`预览或任务抽屉须复用 QuickPreviewSheet / TaskActionSheet，禁止手写 px-7`；对 6 处现有非预览结构抽屉（如 `category-tree-page.tsx:448` 左侧移动树、`copy-rule-editor.tsx:69`、`sales-delivery-workspace-sheet.tsx:164` 等）实行显式结构白名单或 Phase 2 迁至 `TaskActionSheet` | 保证抽屉对齐与高度紧凑响应，不误伤左侧导航树 |
| **Rule-6** | `features/workspace/**` 中出现视口断点 `xl:grid-cols-` | Error：`工作台 iframe 在 1440px 屏净宽仅 1200px，禁止使用视口 xl:grid-cols，请改用命名容器查询 @[960px]/workspace: / @[1120px]/workspace:` | 永久防止 1440px 布局坍塌回归 |

---

## 4. 操作效率重构引擎（Operational Efficiency & Ergonomics）

### 4.1 核心高频业务流量化对比（Before vs After）

| 核心业务流 | 重构前实测路径与成本 | 重构后路径与成本 | 提效幅度与关键抓手 |
|---|---|---|---|
| **1. 审批销售单（含合同 PDF + 多地址交付）** | 选销售 Tab → 冷启详情 Tab → 滚屏 → 逐行开交付弹窗 → 关弹窗 → 点合同跳新浏览器 Tab 来回切 → 开审批历史弹窗 → 关弹窗 → 点通过 → 弹窗再确认 (**≥9 次点击 + 2 弹窗 + 跨 Tab 往返**) | 点击队列行进入分栏/双栏 → 行内点击展开多地址（免弹窗）→ 右栏切「附件对照」同屏看合同 PDF → 点击「通过审批」或按 `A`（常规单据直接通过，高危单层核对） (**2–3 次点击 + 0 弹窗 + 0 跨 Tab**) | **点击 -70%，上下文切换 100% 消除** |
| **2. 客户验收登记 (`CUSTOMER_ACCEPTANCE_REGISTRATION`)** | 列表 → 只读详情占位页 → 底栏「登记客户验收」→ `max-w-5xl` 弹窗上传附件 → 「下一步」→ **嵌套 AlertDialog** → 确认 (**4–5 步 + 2 层弹窗套娃**) | 详情正文**直接内联可编辑验收行表与附件上传区** → 核对后点底栏「提交本次验收」→ 单层影响确认 (**2 步 + 1 层确认，0 套娃**) | **点击 -55%，消除双层弹窗** |
| **3. 销项开票申请审批 (`sales_invoice_request`)** | 详情 → 依次展开 **3 层嵌套 `<details>`**（来源销售单 → 关联开票 → 关联回款）→ 展附件跳新 Tab → 通过 → 弹窗确认 (**≥8 次点击 + 3 层折叠 + 新 Tab**) | 顶部「风险与额度核验条」直接高亮冲突证据 → 平铺分段 Tab（`来源单据 \| 关联回款 \| 历史开票`）零嵌套折叠 → 右栏同屏看发票凭证 → 一键通过 (**2–3 次点击 + 0 嵌套**) | **点击 -65%，核销证据首屏直达** |
| **4. 供应商新品审核 (`SUPPLIER_PORTAL_REVIEW`，2,044 行表单)** | 连续滚屏 5–8 屏逐项核对品牌/类目/查重/每 SKU 单位映射/同义词 → 缺字典开弹窗 → 提交 (**10–20+ 次操作，极易遗漏**) | 拆分为 **4 步可折叠分组 + 左侧粘性进度锚点栏**（基本信息与查重 / 品牌类目 / SKU 单位批量映射 / 审核结论）+「一键应用默认单位到全部 SKU」+ 异常项红点定位 (**约 5–8 次操作，滚屏减半**) | **操作量 -50%，核对零漏项** |
| **5. 采购退货退款/红票 & 付款安排刷新闭环** | 详情 → 行按钮弹窗登记 → 提交关闭后**界面停滞，需手动点底栏重复「刷新处理结果」**等轮询触发完成 (**4+ 步 + 人工盲等**) | 提交 mutation `onSuccess` **即时写入缓存并直接触发 `onTaskCompleted`** → 开启 `autoNext` 时 300ms 自动切入下一项；彻底移除底栏冗余刷新按钮 (**2 步闭环**) | **消除人工刷新循环，处理时长 -50%** |
| **6. 队列分诊切换 & 抄送连续查阅（10 条批量场景）** | 10 条审批：10 次冷启 iframe Tab + 10 次二次弹窗确认 (**≥30 次点击**)；10 条抄送：10 次进详情 + 10 次点返回重找 (**≥20 次整页往返**) | 在单 Tab 分栏模式下按 `J` 切下一条、右栏即审按 `A` 通过（或抄送按 `J` 自动标已读并连续预览）；支持多选批量操作 (**10–12 次按键/点击，0 次 iframe 重载**) | **点击 -65%，切换耗时从秒级降至毫秒级** |

### 4.2 全局键盘效率体系（`useWorkspaceHotkeys` — 适配多 Iframe 壳）

**多 Iframe 焦点与激活守卫（关键工程约束）**：
- `workspace-tabs.tsx:331-335, 374-423` 会将隐藏页签常驻 DOM，且点击或关闭顶栏标签后焦点会停留在宿主 `#workspace-tab-*`。
- 因此：① `useWorkspaceHotkeys` 必须校验**激活态守卫**（处于 iframe 内时检查 `window.frameElement?.getAttribute("data-workspace-active") === "true"`，复用 `lib/workspace-tabs.ts:49-51` 模式，且在输入框/IME 组合/打开模态框时自动让渡）；② `WorkspaceTabs` 在切页签或关闭页签后，通过 `iframe.contentWindow?.focus()` 同步把键盘焦点交还激活页的 `<main id="main-content">`，保证 `J/K/A/R` 立即可用；③ `Cmd/Ctrl+K` 搜已打开页签由 `WorkspaceTabs` 向宿主 `WorkspaceQuickSearch` 暴露响应式 `tabs` 状态。

| 快捷键 | 生效范围 | 行为 |
|---|---|---|
| `J` / `↓` 与 `K` / `↑` | 队列列表、分栏模式、任务/审批/抄送详情 | 切换下一项 / 上一项（同步绑定至 `WorkspaceDetailPage` 与 `ApprovalCopyDetailPage` 翻页按钮） |
| `A` | 审批检查器 / 审批详情（具 `APPROVE` 权限） | 按 ADR-1 触发通过审批（常规低/普通风险单据直接通过，高危单据唤起单层 `FormalActionConfirmDialog`） |
| `R` | 审批检查器 / 审批详情（具 `REJECT` 权限） | 展开并聚焦驳回原因输入框，展示单据上下文快捷短语（严格必填 + 512 UTF-8 字节计数） |
| `P` | 2 栏决策工作台 | 开关 / 聚焦右侧「附件对照预览」面板 |
| `H` | 2 栏决策工作台 | 开关 / 聚焦右侧「审批与动态轨迹」面板 |
| `]` | 2 栏工作台 | 折叠 / 展开右侧上下文轨（瞬间获得 100% 宽表视口） |
| `Cmd/Ctrl + Enter` | 全部决策/确认/作业表单 | 提交当前表单或确认操作（补齐 `DocumentWithdrawApprovalButton` 等遗漏） |
| `Esc` | 分栏检查器 / `TaskActionSheet` / 浮层 | 逐层优雅收起当前浮层或取消多选 |
| `/` | 任意队列页 | 立即聚焦搜索框 |
| `Cmd/Ctrl + K` | 全站宿主 + 子 iframe | 升级 `workspace-quick-search.tsx`：除静态菜单外，支持检索**已打开页签（来自 `WorkspaceTabs`）、最近任务编号/单号与往来单位** |

### 4.3 队列连续性、筛选心智与查询性能优化

1. **全队列统一 `autoNext`、上下项遍历与淘汰 `SequentialProcessBar`**：
   - 解除 `workspace-detail-page.tsx:214,286,356` 对审批路由的 `!approval` 硬禁用，让审批流与任务流共用 `erp.workspace.autoNext` 持久化偏好；
   - 为 `/workspace/copies/[receiptId]` 补齐 `上一项 / 下一项` 队列遍历及 `autoNext` 连续速览能力；
   - 将遗留在 4 个业务模块（`fulfillment-operations`、`integration-errors`、`supplier-settlements`、`supplier-orders`）的重复步进栏 `SequentialProcessBar`（`workflow-actions.tsx:592-719`）统一收口至标准工作台队列导航协议，彻底消除双套步进栏。
2. **恢复「全部待审批」与兼容 `cursor` 分页暴露受阻治理视图**：
   - 删除 `workspace-type-nav.tsx:34` 对 `task:DOCUMENT_APPROVAL` 的硬过滤，将队列顶层视图梳理为 5 个清晰聚合桶：`全部`、`待审批`（`task:DOCUMENT_APPROVAL`）、`待执行`、`异常处理`、`退回重办`。
   - **适配后端 422 契约暴露 `/workspace/approvals` 四视图**：后端 `approval_instance/http.rs:86-94` 仅允许 `view=started` 携带 `page` 与 `filters` JSON，`view=blocked | managed | mine` 传 `page`/`filters` 会返回 HTTP 422（仅接受 `cursor` + `document_type`/`status`/`limit`/`q`）。前端统一队列壳通过**视图分页适配器**（`started` 走页码分页，`blocked`/`managed`/`mine` 走 `cursor` 游标分页 + 基础筛选栏），**纯前端即可安全上线 `受阻治理 (blocked)` 与 `管理范围 (managed)` 视图**；待 ADR-2 后端第 2 批扩展 `PreparedInstanceListQuery` 后对齐页码分页与结构化筛选。
3. **消灭「即时 vs 手动提交」分裂的筛选栏**：
   - `WorkspaceSearchPanel` 全面切换为**全即时响应式筛选**：搜索框 250ms 防抖即时查询、Popover 内选择条件即时生效并生成可一键 `×` 清除的筛选芯片，取消强制点「查询」按钮的割裂体验。
   - **放开往来单位筛选与搜索**：前端预留所有视图的往来单位检索交互；`started` 视图立即可用，普通 `work-items` 视图随 ADR-2 后端第 2 批开放 `q` 匹配往来单位名与 `counterparty_ids` 参数（见 §6.2）。
4. **队列头部计数与跨 Iframe 重复查询优化（明确前后端边界）**：
   - 经核查 `backend/crates/domains/erp-workflow/src/service/work_item/read/stats.rs:200-204`，`GET /admin/work-items/stats` 目前仅对无搜索/无筛选的 `scope=mine` 生效（且不返回按 `document_type` 拆分的 `approval:*` 计数，也不覆盖 `started`）。
   - **分层优化**：① **Phase 1 纯前端**：默认无搜索、无筛选的「待我处理 (`scope=mine`)」首屏直接走单次 `GET /admin/work-items/stats` 替代 8 路并行基础计数（默认首屏头部请求 **-87%**）；② **ADR-2 后端第 2 批（Phase 2/3）**：扩展 `/admin/work-items/stats` 的 `scope`/`filters`/`document_type` 维度聚合后，全面覆盖 `managed`/`history`/带筛选场景；③ **分栏消除重复队列请求**：分栏模式下列表与检查器共享同一 React Query 缓存；独立 Tab 打开详情时由 `workspaceHost` 传递轻量队列 ID 列表，免除重复拉取全量 `useWorkspaceDashboardQuery`。

### 4.4 批量操作能力分层（对齐 ADR-2 前后端边界）

| 批量动作 | 适用队列 | 落地归属 | 实现机制与风控设计 |
|---|---|---|---|
| **批量标记已读** | `/workspace/copies`、`InboxPopover` | **抄送纯前端已就绪；顶栏收件箱随 ADR-2 第 1 批（Phase 0/1）补全** | 将 `ListSelectionBar` 纳入统一队列壳（复用 `POST /account/approval-copies/read`）；顶栏 `InboxPopover` 接入通知全部已读接口 |
| **批量导出 Excel** | 付款执行 / 开票执行筛选视图 | **已就绪（纯前端整理）** | 保留现有导出能力，收口至页头 `tier="quiet"` 动作区 |
| **批量转交任务 (`REASSIGN`)** | `/workspace/tasks?view=managed`（有 `REASSIGN` 权限行） | **纯前端先行（受控并发 + 候选人求交集）；原子批量接口划归 P3+** | 因 `GET /admin/work-items/{id}/reassign-candidates` 按单条任务独立计算（含采购级联权限），多选打开 `WorkItemResponsibilityDialog` 时并发拉取各任务候选人并**计算可用接收人交集**（若交集为空则明确提示按任务类型分批转交），原因严格校验 `≤150 字符`，以并发度 3 逐条调用 `POST /admin/work-items/{id}/reassign` 并回报成功/失败明细 |
| **批量人工关闭 (`CLOSE`)** | W29 异常 / `managed` 队列（有 `CLOSE` 权限行） | **纯前端先行（受控并发）；原子批量接口划归 P3+** | 仅允许选中行全部包含 `CLOSE` 权限时激活；必填 `reason_code`、可选 `comment ≤100 字符`（批量模式默认隐藏单条关联的 `DUPLICATE` 替换单号，或仅单选时开放），逐条调 `POST /admin/work-items/{id}/close` |
| **批量通过审批 (`BATCH_APPROVE`)** | `/workspace/tasks`（待审批视图） | **P3+ 独立增强迭代（不阻塞本轮交付）** | 需后端新增 `POST /work-items/batch-approve`（携带幂等键 + 服务端风控白名单校验，仅允许低/普通风险单据批量放行，高风险单据返回跳过原因） |

---

## 5. 全量逐项重构映射矩阵（Complete Per-Task & Per-Overlay Migration Matrix）

### 表 A：全部 17 类活跃事项（+ 合成/退役项）逐项重构映射表

| # | `WorkItemType` | 目标原型 | 当前核心痛点（审计实证） | 目标布局与交互重构方案 | 影响核心文件 |
|---|---|---|---|---|---|
| 1 | `SALES_INQUIRY_PROCESSING` | Archetype 2（重型专注作业） | 与 `/sales/inquiries` 双重红点曝光；问号说明样式异化为 `outline` 按钮；5 个独立弹窗高度单位混用（`88svh`/`90vh`/`85svh`） | 保留通栏 `fullWidth` 作业面（列表分栏中仅展示摘要+「进入完整报价面」）；统一 `WorkspaceTaskHeaderActions` 样式；5 个内部弹窗迁移至 `DialogContent size="lg\|xl"` 统一 `dvh`（*注：后端排除 `REASSIGN`*） | `workspace-task-surface.tsx:64-86`、`sales-inquiry-detail-page.tsx`、`inquiry-actions.tsx`、`inquiry-internal-quotes.tsx`、`supplier-quote-{export,import}-dialog.tsx` |
| 2 | `PROCUREMENT_ORDER_CREATION` | Archetype 2（重型专注作业） | 3 套命名分裂（供给分配/待安排采购/采购分单）；绕过标准页头；问号按钮异化为 `"任务详情"`；`PROCESS` 与只读态布局跳变；缺失 `REASSIGN` | 统一命名为「采购供给分配」；接入标准 `WorkspaceTaskIdentityHeader`；保留 `fullWidth` 供给分配画布；预览与选单弹窗迁移 `Dialog size="2xl\|xl"`；页头补齐 `REASSIGN` | `workspace-procurement-task.tsx`、`work-item-meta.ts:62-66`、`purchase-order-create-preview.tsx`、`purchase-order-create-source-picker-dialog.tsx` |
| 3 | `PURCHASE_PAYMENT_ARRANGEMENT` | Archetype 2（专注作业） | `compact` 传参在 `flat` 模式完全失效；绕过标准页头缺失状态/单据/超期徽章；背负 376px 冗余 `aside`；`p-5 sm:p-6` 双内边距；跟进 `Popover` 嵌套日期 `Popover`；提交后依赖刷新闭环；缺失 `REASSIGN` | 切换至统一 `WorkspaceTaskIdentityHeader`（补齐全部徽章）；撤除 376px `aside` 与冗余 `p-5`；删除死 `compact` 传参；跟进改为 `Dialog size="sm"`；付款申请提交后 `onSuccess` 直接触发 `onTaskCompleted`；页头补 `REASSIGN` | `workspace-task-surface.tsx:263-297`、`workspace-processing-layout.tsx:100`、`purchase-payment-arrangement.tsx`、`purchase-payment-follow-up.tsx`、`request-draft-dialog.tsx` |
| 4 | `PURCHASE_SOURCE_UPDATE` | Archetype 2（专注作业） | 命名分裂；手写 `<Tooltip><Button>` 替代标准图标按钮；`px-5 py-5` 双内边距；底部重复刷新按钮；草稿弹窗嵌套离开+提交 2 个弹窗；「维持原单」用裸 `AlertDialog` 嵌 `<Textarea>`；缺失 `REASSIGN` | 统一命名为「采购依据变更」与标准页头；撤除冗余 `aside`、双内边距与底部刷新；草稿弹窗改内部两步核对消除套娃；「维持原单」迁至 `FormalActionConfirmDialog (layout="compact")`；页头补 `REASSIGN` | `workspace-purchase-source-update-task.tsx`、`purchase-source-update-workspace.tsx`、`purchase-source-update-draft-dialog.tsx`、`purchase-order-source-update-notice.tsx:275-337` |
| 5 | `PURCHASE_RETURN_REFUND` | Archetype 2（专注作业） | `px-7 py-6` 双内边距；底栏重复刷新按钮；登记退款后被动等轮询状态变更；缺失 `REASSIGN` | 撤除 `px-7`、冗余 `aside` 与底栏刷新按钮；统一标准页头；`FinancialRequestDialog` 迁至 `Dialog size="md"`；mutation `onSuccess` 直接更新缓存触发 `onTaskCompleted`；页头补 `REASSIGN` | `workspace-return-finance.tsx`、`financial-request-dialog.tsx`、`supplier-refund-request-dialog.tsx`、`return-settlement-dialog.tsx` |
| 6 | `PURCHASE_RETURN_INVOICE` | Archetype 2（专注作业） | 同上，且 `ReturnInvoiceDialog` 未显式限高；缺失 `REASSIGN` | 同上；`ReturnInvoiceDialog` 迁至 `Dialog size="md"` 并接入反应式完成流转；页头补 `REASSIGN` | `workspace-return-finance.tsx`、`return-invoice-dialog.tsx:89-160` |
| 7 | `FULFILLMENT_OPERATION` | Archetype 2（高频专注作业） | 页头用裸 `<Badge variant="teal">`；`autoNext` 文案异化；`target="_blank"` 跳转外单；非 `PROCESS` 入口把完整履约面塞进 `max-w-5xl` 弹窗再套 2 层子弹窗；`singleOperation` 模式反而隐藏已有 `J/K` 快捷键提示；遗留 `SequentialProcessBar` | 统一 `WorkspaceDocumentBadge`、`autoNext` 文案及工作台 Tab 跳转；**彻底废除 `WorkspaceFulfillmentProcessAction` 弹窗套娃**；开放并统一 `J/K/Ctrl+S/Ctrl+Enter` 快捷键提示；地址/拆包/导入迁标准 `Dialog size="md\|xl"`；转交接入通用 `WorkItemResponsibilityDialog`；收口 `SequentialProcessBar` | `workspace-fulfillment-task.tsx`、`workspace-delivery-brief.tsx`、`fulfillment-work-surface.tsx:295,793`、`fulfillment-{address-dialog,package-split,package-import}.tsx` |
| 8 | `CUSTOMER_ACCEPTANCE_REGISTRATION` | Archetype 2（专注作业） | 详情页近乎只读占位，必须点底栏开 `max-w-5xl` 弹窗填验收再弹嵌套 `FormalActionConfirmDialog`（双层弹窗）；背负 376px `aside`；缺失 `REASSIGN` | **将验收行登记表与附件上传直接内联进详情主画布**（撤除 376px `aside`），点击底栏「提交本次验收」仅弹单层 `FormalActionConfirmDialog`；作废/补录迁移至标准 `Dialog size="md"`；页头补 `REASSIGN` | `workspace-acceptance-task.tsx`、`acceptance-register-dialog.tsx`、`acceptance-dialogs.tsx` |
| 9 | `SUPPLIER_PAYMENT_EXECUTION` | Archetype 2（专注作业） | 付款执行模式硬编码 `px-7 py-6` 双内边距；背负 376px 冗余 `aside`；缺失 `REASSIGN` | 撤除 `px-7` 与冗余 `aside`，全宽呈现付款账户（保留 `SensitiveValue` 显隐）、金额与水单上传内联表单；附件预览复用 `UnifiedMaterialViewer`；页头补 `REASSIGN` | `workspace-task-surface.tsx:87-197`、`purchase-payment-request-task.tsx:94`、`request-payment-execution.tsx`、`request-payment-form.tsx` |
| 10 | `SALES_INVOICE_EXECUTION` | Archetype 2（专注作业） | `workspaceTaskSurfacePadClassName` (`px-5`) 双内边距；标题硬编码 `"登记销项发票"`；背负 376px `aside`；外部单据跳转未统一；缺失 `REASSIGN` | 撤除重复 `px-5` 与冗余 `aside`；使用标准 `WorkspaceTaskIdentityHeader`（主体为客户名 + 单据徽章）；保留 `VIEW`/`COMPLETED` 直达只读核查能力；页头补 `REASSIGN` | `workspace-invoice-task.tsx:122,297`、`workspace-invoice-upload.tsx` |
| 11 | `SUPPLIER_SETTLEMENT_REVIEW` | Archetype 2（宽表作业） | 内嵌 `SettlementCenter` 重复渲染 `<DetailWorkspacePage>` + 页头 + 刷新按钮 + `SequentialProcessBar`，且遭外层 376px `aside` 挤压致对账宽表横向滚动；3 个原生 Dialog 未复用 `FormalActionConfirmDialog`；缺失 `REASSIGN` | `SettlementCenter(embedded)` 移除嵌套壳/页头/刷新/`SequentialProcessBar`；外层撤除 `aside` 释放 100% 宽度给对账表；驳回迁至 `FormalActionConfirmDialog`；页头补 `REASSIGN` | `workspace-settlement-task.tsx`、`settlement-center.tsx`、`settlement-center-dialogs.tsx` |
| 12 | `SUPPLIER_PORTAL_REVIEW` | Archetype 2（结构化审核） | 前后端 Family 冲突（`approval` vs `procurement`）；**2,044 行超长平铺单页表单**；转交做成非标内联展开且在 `managed` 只读页不可见；图片弹窗未复用统一预览器 | 统一 Family 为 `procurement`（ADR-2 第 1 批后端同步）；拆分 `PortalReviewForm` 为 **4 组可折叠区块 + 粘性锚点导航 + SKU 单位一键批量应用**；废除内联 `PortalReassignForm`，由页头统一挂载 `WorkItemResponsibilityDialog`（保留非 `PROCESS` 正文只读降级，解除 `workspace-document-task-detail.tsx:520` 限制）；图片预览切至 `UnifiedMaterialViewer` | `review-form.tsx:1-2044`、`reassign-form.tsx`、`review-images.tsx`、`dictionary-create-dialog.tsx`、`work-item-meta.ts:51`、`backend/crates/domains/erp-workflow/src/dto/work_item/status.rs:122` |
| 13 | `IMPORT_BUSINESS_CONFIRMATION` | Archetype 2（分角色确认） | 内嵌 `BatchDetailView` 重复渲染 `<PageScaffold>` + 废弃 `<DocumentHeader>` + 4 列指标带，叠加外层页头与 376px `aside`；缺失 `REASSIGN` | `BatchDetailView(embedded)` 剥离 `<PageScaffold>` 与 `<DocumentHeader>`（顺带清除全仓最后一处 `DocumentHeader`）；外层撤除冗余 `aside`；页头补 `REASSIGN` | `workspace-import-task.tsx`、`batch-detail-view.tsx:124-129,257-417`、`confirm-section.tsx` |
| 14 | `INTEGRATION_RESULT_UNKNOWN` | Archetype 3（异常排查） | W29 站外重复自建 Master-Detail 队列与 `SequentialProcessBar`；W26 `SupplierOrderCenterPage` 形成**双侧边栏嵌套**；W29 `IntegrationErrorsPage` 叠 `<PageScaffold>`；缺 `PROCESS` 只读态无 `CLOSE`/`REASSIGN` | **保留 `workspace-task-surface.tsx:216-228` 只读正文安全边界**，解除 `workspace-document-task-detail.tsx:520` 类型硬编码并在 `WorkspaceTaskHeaderActions` 统一按 `allowedActions` 挂载 `REASSIGN`/`CLOSE`；`SupplierOrderCenterPage(embedded)` 与 `IntegrationErrorsPage(embedded)` 去除外层壳、内嵌侧栏与 `SequentialProcessBar`；列表支持批量关闭/转交 | `workspace-integration-task.tsx`、`workspace-supplier-investigation-task.tsx`、`integration-errors-page.tsx`、`supplier-order-center-page.tsx`、`terminal-action-dialog.tsx` |
| 15 | `BUSINESS_EXCEPTION` | Archetype 3（异常排查） | 同 #14（W29/W26）；W21 `SupplyExceptionTaskPanel` 信息与外层 100% 重复且多层 `<Card>` 套娃；虚假 `"可转交"` 徽章 + 错别字 `"负责人人"`；缺失 `REASSIGN` | 同 #14；重构 `SupplyExceptionTaskPanel` 去除嵌套 `<Card>` 与重复元数据，修正 `"负责人人"` 错别字，接入页头统一 `WorkItemResponsibilityDialog` | 同 #14 + `workspace-supply-exception-task.tsx`、`supply-exception-task-panel.tsx:42,290-350` |
| 16 | `DOCUMENT_APPROVAL` | Archetype 1（决策审批） | 12 类强制 `singleColumn=true`；`purchase_payment_request` 另立门户；无「全部待审批」聚合 Tab；附件跳裸新 Tab；历史藏弹窗；通过强制弹窗；512 UTF-8 字节校验错配 | 启用 **2 栏决策工作台**（左侧单据 + 右侧「审批与动态 / 附件对照」轨）+ 列表分栏快审；将 `purchase_payment_request` 纳入统一审批壳；恢复 `task:DOCUMENT_APPROVAL` 视图；上线 ADR-1 分级风险决策条 + UTF-8 512B 校验 | `workspace-document-task-detail.tsx`、`workspace-processing-layout.tsx:91-96`、`workspace-task-surface.tsx:87-197,449-458`、`workspace-type-nav.tsx:34`、见表 B |
| 17 | `APPROVAL_REWORK` | Archetype 1（退回重办） | 详情为纯只读驳回页，改 1 个字段也要整页跳出工作台再返回（5–6 步）；缺少内联重提入口（*注：后端 `abandon_rework.rs:40` 校验 512 字符，前端已对齐 512 字符，但仍缺统一计数与 `SubmissionRouteConfirmation`*） | 保持 2 栏决策壳（右侧常驻上轮驳回明细与原单对照）；财务/变更/库存调整类重办直接原位唤起对应 `EditDialog` 就地修改重提，销售/采购类一键在工作台新 Tab 编辑并带回自动刷新；放弃确认接入统一 `ReasonTextareaField (512 字符)` 计数器 | `workspace-approval-rework.tsx:53,120-164`、`workspace-rejection-notice.tsx` |
| 18* | `APPROVAL_INSTANCE` *(前端合成)* | Archetype 1（发起跟踪） | 强制关闭 `autoNext`（`L214,286,356`）；仅暴露 `view=started`，后端 `blocked`/`managed`/`mine` 未开放；5 个撤回组件（4 套实现）分裂 | 通过 `cursor` 适配层开放 `started` / `mine` / `blocked` / `managed` 四视图；启用 2 栏决策壳与 `autoNext` 连续翻看；5 个撤回组件统一为 `ApprovalWithdrawDialog` | `workspace-detail-page.tsx:214,286,356`、`workspace-home-page.tsx`、`dashboard.ts:509`、`approval-action-bar.tsx` |
| — | 7 退役 + 2 历史兼容 + 9 孤立 `DOCUMENT_META` | 清理项 | 前端元数据与路由注册表残留历史遗留条目 | 清理 `work-item-meta.ts` 中无实际引用的孤立项，清理 `workspace-registry.ts` 中 `W02`/`W06(so_1002)`/`W07` 及死 badge key `delivery-count`/`warehouse-count` | `work-item-meta.ts:245-288`、`workspace-registry.ts:99-100,336-401`、`workspace-sidebar-context.tsx:55-57` |

---

### 表 B：全部 13 类审批单据与 5 类抄送重构映射表

| # | `document_type` / 类别 | 当前渲染器与载体 | 目标统一渲染器与工作台形态 | 浮层与交互收敛要点 |
|---|---|---|---|---|
| 1 | `sales_order` | `WorkspaceSalesApproval` + `ApprovalSalesOrderPaper` + 双重附件入口 + 交付弹窗 | **2 栏审批工作台**：左为紧凑金额/毛利带 + 自适应明细表（取消强制 `min-w-[1080px]`，低频列折叠）+ **行内展开多地址交付**；右为 `UnifiedAuditTrail` + `UnifiedMaterialViewer`（合同 PDF 左右对照） | 废除顶部重复「合同与附件」弹窗按钮与逐行交付弹窗；`SalesOrderSubmitConfirmDialog` 补齐 `SubmissionRouteConfirmation`；`SalesOrderCancelApprovalButton` 切至 `ApprovalWithdrawDialog`；统一 `SalesOrderApprovalPanel` 与纸面预览；触发负毛利/超信用按 ADR-1 走高危单层确认 |
| 2 | `voucher_sales_order` | 复用 `WorkspaceSalesApproval` (`VOUCHER` 分支) + `VoucherSalesOrderApprovalArea` | 同 `sales_order`（卡面值/配赠/卡形态集中呈现在首屏 KPI 带与明细列） | `VoucherSalesOrderSubmitConfirmDialog` 补齐 `SubmissionRouteConfirmation`；统一撤回与历史原语 |
| 3 | `sales_change_order` | 通用 `ApprovalSubmittedSummary` + `ChangeOrderPendingCard` + `SalesChangeOrderEditDialog` | 升级为 **`StructuredDiffApprovalRenderer`**（置顶「核心金额/交期/毛利变动对比卡片」+ 前后高亮 Diff 表），置入 2 栏决策工作台 | 去除内层 `Card` 套娃；编辑弹窗规范为 `Dialog size="2xl"`；撤回与历史收敛至统一原语 |
| 4 | `purchase_order` | `WorkspacePurchaseApproval` + `WorkspaceSupplierRating` + 9 列 `min-w-[1080px]` 对比表 | **2 栏审批工作台**：首屏置顶「成本/售价异常摘要警示条 + 紧凑供应商评分徽章」；对比表默认精简列并支持「仅看异常项」一键切换，避免 1280px 横向滚动；右栏附件/历史常驻 | `PurchaseOrderCancelApprovalButton` 切至 `ApprovalWithdrawDialog`；详情 `FlatApprovalHistory` 统一至 `UnifiedAuditTrail` |
| 5 | `purchase_change_order` | 通用 `ApprovalSubmittedSummary` + `PurchaseChangeOrderEditDialog` | 复用 **`StructuredDiffApprovalRenderer`** 置入 2 栏决策工作台（突出采购成本与关联销售毛利影响） | 统一编辑弹窗 `Dialog size="2xl"`、历史轨迹与撤回弹窗 |
| 6 | `stock_adjustment` | 通用 `ApprovalSubmittedSummary`；列表侧用 **`QuickPreviewSheet size="preview"` (400px)** 极度拥挤 | 工作台走 **`StructuredDiffApprovalRenderer`**（高亮盘盈/盘亏数量与成本影响）；库存列表侧升级为 **`TaskActionSheet size="detail"` (768px)** | **彻底修复 400px 窄抽屉挤压问题**；`CancelAdjustmentApprovalDialog` 废除并切至 `ApprovalWithdrawDialog`（补 `destructive`）；`AdjustmentEditDialog` 升至 `Dialog size="lg"` |
| 7 | `sales_invoice_request` | `WorkspaceInvoiceApproval`（**3 层嵌套 `<details>`** + 双重附件入口）；申请提交**无确认弹窗** | **2 栏审批工作台**：顶部实时回款/可开票额度核验条 + 扁平 3 分段 Tab（`来源销售单 \| 关联回款 \| 历史开票`），彻底消灭 3 层 `<details>`；右栏 `UnifiedMaterialViewer` | 移除顶部重复「预览材料」弹窗；申请页补齐提交确认 + `SubmissionRouteConfirmation`；`CancelRequestDialog` 切至 `ApprovalWithdrawDialog` + UTF-8 512B 校验；统一单据名为「销项开票申请」 |
| 8 | `purchase_payment_request` | 绕过 `WorkspaceDocumentTaskDetail` 独立分发 `PurchasePaymentRequestTask` + 独占弹窗 `RequestMaterials` | **收编进统一 2 栏审批工作台骨架**（保留其优秀的 4 余额指标与 `SensitiveValue` 银行账号显隐能力），将 `RequestMaterials` 升级下沉为全站共享 `UnifiedMaterialViewer` | 消除独立壳分裂；`DestructiveReasonConfirmDialog`（原 2000 字符）切至 `ApprovalWithdrawDialog` + UTF-8 512B 校验；履约状态改用 `StatusBadge`；超阈值大额付款按 ADR-1 强制高危单层确认 |
| 9 | `customer_receipt` | `WorkspaceReceiptApproval` + 列表 `CustomerAccountDetailPreview (detail 768px)` | **2 栏审批工作台**：保留 3 指标行 + 核销分配表 + 补齐银行账号按权限 `SensitiveValue` 查看；右栏同步看银行回单图片/PDF（零新 Tab） | 应收抽屉动作位置统一固定于 `TaskActionSheet` `SheetFooter`；提交确认复用标准 `FinancialSubmitConfirmDialog` 金额头；去除裸 Date 本地化调用 |
| 10 | `customer_refund` | 通用 `ApprovalSubmittedSummary` + 应收抽屉正文底动作 | 复用 **`FinancialApprovalRenderer`**（突出退款金额、原回款/退货依据、收款账户校验）置入 2 栏工作台 | 应收抽屉审批动作与 `customer_receipt` 对齐统一放至 `TaskActionSheet` `SheetFooter`；去除内层 `<Card>` 套娃；超阈值大额退款按 ADR-1 走高危单层确认 |
| 11 | `supplier_refund` | 通用 `ApprovalSubmittedSummary` + 应付抽屉正文底动作 | 同 #10，复用 **`FinancialApprovalRenderer`** 置入 2 栏工作台 | 应付抽屉动作统一放至 `TaskActionSheet` `SheetFooter`；统一 `ApprovalWithdrawDialog` 与 `UnifiedAuditTrail`；超阈值大额退款按 ADR-1 走高危单层确认 |
| 12 | `receipt_reversal` | 通用 `ApprovalSubmittedSummary` + 应收抽屉 | 复用 **`FinancialApprovalRenderer`**（顶部红字/冲正警示条 + 原单与冲正后余额对比） | 同 #10，统一抽屉页脚动作；冲正红字按 ADR-1 强制单层 `FormalActionConfirmDialog` 核对 |
| 13 | `payment_reversal` | 通用 `ApprovalSubmittedSummary` + 应付抽屉 | 同 #12，复用 **`FinancialApprovalRenderer`** | 同 #11，统一抽屉页脚动作；冲正红字按 ADR-1 强制单层 `FormalActionConfirmDialog` 核对 |
| **抄送** | **5 类抄送（`STARTED` / `NODE_APPROVED` / `APPROVED` / `REJECTED` / `CANCELLED`）** | 独立 `ApprovalCopiesPage` (`pageSize=20`) + `ApprovalCopyDetailPage`（无上下条、返回按钮放右侧、图片强塞 `70vh` iframe、CSS `[&>article]` 暴力覆盖） | **纳入统一队列壳（`pageSize=50` + 分栏速览 + 批量已读）**；点击行即刻在右栏/抽屉自动标已读并渲染无边框单据快照 + `UnifiedMaterialViewer` + `UnifiedAuditTrail`；完整详情补齐左上方 `back` 与 `上一项/下一项` + `J/K` | 彻底废弃 `CopyFiles` 70vh iframe 与 `[&>article]` 脆弱样式补丁（为 `ApprovalSubmittedMaterialsContent` 增加 `bare` 属性）；统一 `RUNNING`/`BLOCKED`/`ACTIVE` 徽章色调；修复记录游标翻页为追加模式 |

---

### 表 C：核心弹窗、抽屉与浮层整合映射表（Overlay Consolidation Map）

| 旧碎片组件 / 入口（现状） | 数量（全任务域） | 新统一原语与规格 | 核心重构变更 |
|---|---:|---|---|
| `CancelApprovalDialog`、`DocumentWithdrawApprovalButton`、`CancelAdjustmentApprovalDialog`、`CancelRequestDialog`、`DestructiveReasonConfirmDialog` | **5 组件（4 套模式）→ 1** | **`ApprovalWithdrawDialog`**（基于 `FormalActionConfirmDialog layout="compact"`） | 统一 `AlertDialog` + `destructive` 主按钮 + `useAppForm` + `Ctrl/⌘+Enter` + 幂等恢复 + **UTF-8 512B 校验** |
| `/approval-materials`（新 Tab）、`RequestMaterials`（付款弹窗）、`CopyFiles`（70vh iframe）、`PortalReviewImages`（新品图弹窗） | **4 套（3 审批/抄送 + 1 提报图）→ 1** | **`UnifiedMaterialViewer`**（宽屏右轨对接 + 窄屏 `Dialog size="2xl"`） | 认证 `Blob` 加载 + 复用 `ContractPdfPages` (`pdfjs-dist`) + 原生图片缩放旋转 + 统一 `预览 → 下载` `LoadingButton` |
| `WorkspaceApprovalRouteSummary` (Dialog)、`ApprovalRoute` (H/V)、`ExecutionHistory`、`SalesOrderApprovalHistory`、`FlatApprovalHistory`、`CopyDetail`记录、`CopyHistory`、`TaskTimeline` (Sheet) | **9 → 2** | **`ApprovalRouteStepper`** (44px 横条) + **`UnifiedAuditTrail`** (右轨/抽屉倒序流) | 合并审批历史/抄送记录/单据动态；消除模态遮挡；修复 `vertical` 隐时间与裸 Date 格式化；去除 `TaskTimeline → SalesTimelineSheet` 嵌套 |
| `WorkspaceFulfillmentReassignAction` (Dialog)、`PortalReassignForm` (inline)、`TerminalActionDialog` (W29 close)、12 类缺失转交入口 | **3(+12缺) → 1** | **`WorkItemResponsibilityDialog`**（`Dialog size="sm"`，`mode="reassign" \| "close"`，含批量态） | 由 `item.allowedActions` 驱动，在 `WorkspaceTaskHeaderActions` 与列表 `ListSelectionBar` 统一挂载；严格对齐 `reason≤150字`、`reason_code`+`comment≤100字`+`DUPLICATE` 替换单号 |
| `DecisionDialog`（强制弹窗通过/驳回） | **1 → 升级** | **`ApprovalDecisionBar`**（底栏内联意见 + ADR-1 分级风险通过 + 动态驳回短语）+ 高危单层 `FormalActionConfirmDialog` | 常规单据一键通过零弹窗；高危单据（冲正红字/负毛利超信用预算/大额超阈值/阻断警告）强制单层确认；全量接入 UTF-8 512B 校验 |
| `SalesOrderSubmitConfirmDialog`、`VoucherSalesOrderSubmitConfirmDialog`、`PurchaseOrderSubmitConfirmDialog`、`CustomerReceiptSubmitConfirmDialog`、`FinancialSubmitConfirmDialog`、`InvoiceRequestForm` 直接提交 | **6 入口（4 套模式）→ 2** | **`PaperSubmitConfirmDialog`** (`Dialog size="2xl"`，销售/卡券纸面+路由) + **`FinancialSubmitConfirmDialog`** (`FormalActionConfirmDialog`，采购/财务/开票) | 13 类单据 100% 展示 `SubmissionRouteConfirmation` 与统一 UTF-8 512B 重提原因 |
| `WorkspaceDocumentPaperDialog`、`SalesOrderPaperPreviewDialog`（透明无框壳） | **2 → 1** | **`DocumentPaperPreviewDialog`** (`Dialog size="2xl"`，`PaperDocument compact` 内边距) | 修正重复 DOM ID；收紧 `PaperDocument` 弹窗态留白（`px-6 py-6` 替代 `px-10 py-12 min-h-screen`） |
| `AcceptanceRegisterDialog` + 嵌套确认、`PurchaseSourceUpdateDraftDialog` + 嵌套确认、`WorkspaceFulfillmentProcessAction` + 嵌套子弹窗 | **3 套娃 → 0 套娃** | 验收改**详情内联表单** + 单层确认；采购变更改**弹窗内两步视图**；履约弹窗**直接废除** | 100% 根除 Modal-on-Modal |
| `PurchasePaymentFollowUp` (`Popover w-96` 嵌日期 `Popover`) | **1 → 1** | **`Dialog size="sm"`**（或详情内联折叠区） | 根除双 Popover 焦点冲突 |
| `QuickPreviewSheet`（只读 `px-7` 错位）+ `AdjustmentDetailSheet` (`preview 400px`) + `WorkspaceOverviewDetail` (裸 `Sheet`) | **3 → 2（ADR-3 双轨）** | **`QuickPreviewSheet`**（只读严禁 `useAppForm`，修复 `px-sheet-inline py-5` + `pr-12`）+ **`TaskActionSheet`**（固定 `size="detail"` 768px，禁止套娃抽屉） | Phase 1 同步修订 `AGENTS.md §7` 明确双轨边界；库存调整升至 768px；概览下钻复用 `QuickPreviewSheet` |

---

## 6. 组件架构设计与分阶段落地路线图（Component Architecture, Backend Alignment & Phased Roadmap）

### 6.1 目标组件树与核心 TypeScript 接口契约

```text
erp-client/
├── AGENTS.md                               # [同步修订] §7 明确 px-sheet-inline py-5 与 QuickPreviewSheet / TaskActionSheet 双轨边界
├── lib/
│   ├── workspace-tabs.ts                   # [修复] allowlist 补齐 "account"；暴露 tabs 状态给 Cmd+K；切页聚焦激活 iframe
│   ├── theme-tokens.ts                     # [同步] 如新增 --spacing-* 令牌必 1:1 登记至 themeSpacingNames
│   ├── task-status-dictionary.ts           # [新增] 唯一权威 StatusBadge 映射与任务中文命名真源
│   └── validation/reason-length.ts         # [新增] BPM 512B UTF-8 / ABANDON 512字 / REASSIGN 150字 / CLOSE 100字校验真源
├── components/
│   ├── ui/
│   │   ├── dialog.tsx                      # [升级] 新增 size("sm"|"md"|"lg"|"xl"|"2xl"|"full") + scroll("body"|"popup") + DialogBody
│   │   ├── sheet.tsx                       # [升级] 新增 SheetCloseContext 自动 pr-12
│   │   ├── popover.tsx                     # [升级] 集成宿主 iframe 点击自动收起钩子
│   │   ├── resizable.tsx                   # [激活] 支撑 Master-Detail 与 2 栏附件对照拖拽
│   │   └── drawer.tsx                      # [删除] 249 行零引用死代码
│   └── business/
│       ├── list.tsx                        # [修复] QuickPreviewSheet 对齐 px-sheet-inline py-5
│       ├── task-action-sheet.tsx           # [新增] 受控轻作业抽屉（固定复用 --spacing-detail 768px，禁止套娃抽屉）
│       ├── pdf-document-canvas.tsx         # [新增] 泛化自 features/contracts/components/contract-pdf-pages.tsx（消费 Blob）
│       ├── unified-material-viewer.tsx     # [新增] 认证 Blob + pdfjs-dist + img 侧边对照 / 灯箱双形态预览器
│       ├── workspace-split-queue.tsx       # [新增] 基于 ResizablePanelGroup 的统一 Master-Detail 队列壳（同步更新 globals.css）
│       ├── unified-task-workbench.tsx      # [重构替代 workspace-task-pane.tsx] 容器查询 2 栏/通栏骨架（删 160 行非 flat 遗留）
│       ├── work-item-responsibility-dialog.tsx # [新增] 统一单条/批量 REASSIGN 与 CLOSE 弹窗（完整对齐后端字段与候选人交集）
│       ├── workflow-actions.tsx            # [升级/清理] FormalActionConfirmDialog 精简嵌套；淘汰 SequentialProcessBar（4 模块收口后删除）
│       ├── detail-page-header.tsx          # [删除] 零引用遗留
│       ├── object-section-tabs.tsx         # [删除] 零引用遗留
│       ├── page-header.tsx                 # [废弃→删除] 迁移剩余 8 处调用后移除
│       └── document.tsx (DocumentHeader)   # [废弃→删除] 迁移 batch-detail-view.tsx 后移除
├── features/
│   ├── approval-workflow/
│   │   ├── components/
│   │   │   ├── approval-action-bar.tsx     # [升级] 固定左次右主顺序 + 内联意见 + ADR-1 分级风险通过
│   │   │   ├── approval-withdraw-dialog.tsx# [新增] 统一 5 个撤回组件（4 套模式）
│   │   │   ├── approval-route-stepper.tsx  # [新增] 44px 紧凑水平步进条
│   │   │   ├── unified-audit-trail.tsx     # [新增] 倒序合并审批历史 + 抄送记录 + 单据动态
│   │   │   └── structured-diff-approval.tsx# [新增] 替代 6 类通用快照审批的差异高亮渲染器
│   │   └── schema.ts                       # [修复] 全量接入 bpmReasonSchema (512 UTF-8 B)
│   ├── approval-processes/
│   │   └── components/definition-flow-canvas.tsx # [优化] 释放高分屏 620px 高度上限
│   ├── notifications/
│   │   └── components/inbox-popover.tsx    # [优化] 新增「全部标为已读」，与 SystemFeedbackPanel 琥珀色警告徽标分化
│   └── workspace/
│       ├── hooks/
│       │   ├── use-workspace-hotkeys.ts    # [新增] 含 data-workspace-active 激活守卫的 J/K/A/R/P/H/]/Esc 引擎
│       │   └── use-workspace-queue-counts.ts # [优化] 默认无筛选 scope=mine 首屏走单次 /admin/work-items/stats
│       ├── pages/
│       │   ├── workspace-home-page.tsx     # [重构] 统一承接 tasks/approvals(含cursor适配)/history/copies 队列壳
│       │   └── role-workbench-page.tsx     # [扩展] @[960px]/workspace 单行 KPI + 协同后端补齐履约/管理工作台
│       └── components/
│           ├── workspace-overview.tsx      # [清理] 删除 3 个死导出组件
│           └── workspace-pane-actions.tsx  # [删除] 废弃分栏遗留上下文
```

#### 核心新原语 TypeScript 接口契约（严格对齐现仓鉴权与后端字段）

```ts
// 1. components/ui/dialog.tsx
export type DialogSize = "sm" | "md" | "lg" | "xl" | "2xl" | "full"

export type DialogContentProps = DialogPrimitive.Popup.Props & {
    size?: DialogSize
    /** "body" 固定头尾、DialogBody 独立滚动；"popup" 整体滚动（sm/md 默认） */
    scroll?: "body" | "popup"
    showCloseButton?: boolean
    closeButtonId?: string
    forceOverlay?: boolean
}

// 2. components/business/unified-task-workbench.tsx
export type TaskWorkbenchArchetype =
    | "decision"       // Archetype 1: 65/35 双栏，右轨挂载 Route + Trail + MaterialViewer
    | "action"         // Archetype 2: 100% 通栏专注作业面，无冗余 20rem 侧栏
    | "investigation"  // Archetype 3: 顶部诊断横幅 + 排查正文 + 可折叠载荷轨
    | "informational"  // Archetype 4: 精简速览面（常运行在 SplitQueue / TaskActionSheet 内）

export type UnifiedTaskWorkbenchProps = Readonly<{
    archetype: TaskWorkbenchArchetype
    header: React.ReactNode
    alertBanner?: React.ReactNode
    kpiStrip?: React.ReactNode
    /** Archetype 1/3 专用右侧上下文轨；支持用户按 `]` 折叠与拖拽调宽 */
    contextRail?: Readonly<{
        defaultTab?: "trail" | "materials" | "timeline"
        collapsible?: boolean
        resizable?: boolean
        content: React.ReactNode
    }>
    footerLeading?: React.ReactNode
    footerActions?: React.ReactNode
    children: React.ReactNode
}>

// 3. components/business/unified-material-viewer.tsx
// 严格遵循 Bearer Token 鉴权，不直接传递裸 URL 给 <img> 或 pdfjs.getDocument(url)
export type MaterialFileItem = Readonly<{
    id: string
    name: string
    mimeType?: string
    sizeBytes?: number
    /** 由各业务域 apiGetBlob(...) 提供；预览器托管 ObjectURL 与 Blob.arrayBuffer 释放 */
    loadPreview: (signal: AbortSignal) => Promise<Blob>
    onDownload: () => Promise<void>
}>

export type UnifiedMaterialViewerProps = Readonly<{
    files: readonly MaterialFileItem[]
    activeFileId?: string
    onActiveFileChange?: (id: string) => void
    /** "docked" 嵌入工作台右栏做左右对照；"dialog" 窄屏全幅灯箱 */
    mode: "docked" | "dialog"
    open?: boolean
    onOpenChange?: (open: boolean) => void
}>

// 4. components/business/work-item-responsibility-dialog.tsx
// 严格对齐 backend/.../dto/work_item/command.rs:84-109 与 features/work-items/api.ts:198-235
export type WorkItemCloseReasonOption = Readonly<{
    code: string // 1–64 chars，如 "DUPLICATE" | "OBSOLETE" | "MANUAL_RESOLVED"
    label: string
    requiresReplacement?: boolean // DUPLICATE 时必填 replacement_work_item_id
}>

export type WorkItemResponsibilityTarget = Readonly<{
    workItemId: string
    label: string
    expectedTaskVersion: string
}>

export type WorkItemResponsibilityDialogProps = Readonly<{
    mode: "reassign" | "close"
    targets: readonly WorkItemResponsibilityTarget[]
    /** mode="close" 时传入后端允许的关闭原因码选项；comment 上限 100 字符 */
    closeReasonOptions?: readonly WorkItemCloseReasonOption[]
    /** mode="reassign" 时按 targets 拉取候选人并自动求交集；reason 必填且上限 150 字符 */
    open: boolean
    onOpenChange: (open: boolean) => void
    onCompleted: (summary: {
        succeededIds: string[]
        failed: { id: string; message: string }[]
    }) => void
}>
```

---

### 6.2 纯前端主线 vs 后端轻量必选协同 vs P3+ 独立增强边界（定案：ADR-2）

| 类别 | 变更项与随行排期 | 阻塞性 | 源码核实说明 |
|---|---|---|---|
| **纯前端主线（Phase 0–P2 零阻塞上线，贡献 ~80% 核心收益）** | 1. `"account"` 路由白名单修复、宿主/iframe 密度同步与切页焦点恢复<br>2. `@container/workspace` 命名容器查询改造（含 `app/globals.css:639,1174,1203,1211` 同步改写）<br>3. `DialogContent size` / `SheetHeader pr-12` / `QuickPreviewSheet px-sheet-inline py-5` / `TaskActionSheet`（固定 `size="detail"` 768px，同步修订 `AGENTS.md §7`）/ `Popover` iframe 关闭<br>4. 保留 `workspace-task-surface.tsx:216-228` 只读正文边界，解除 `workspace-document-task-detail.tsx:520` 硬编码，在 `WorkspaceTaskHeaderActions` 与列表统一挂载 `REASSIGN`（≤150 字 + 批量候选人求交集）与 `CLOSE`（`reason_code` + `comment ≤100 字` + `DUPLICATE` 替换单号）<br>5. 5 个撤回组件（4 套模式）收敛为 `ApprovalWithdrawDialog` + `reason-length.ts` 精确校验（BPM 512B UTF-8 / `ABANDON_REWORK` 512 字 / `REASSIGN` 150 字 / `CLOSE` 100 字）+ 补齐 3 类单据 `SubmissionRouteConfirmation`<br>6. `UnifiedMaterialViewer`（认证 `Blob` + 泛化现有 `ContractPdfPages`）+ `ApprovalRouteStepper` + `UnifiedAuditTrail`<br>7. 统一队列壳（`ListWorkspacePage` + `WorkspaceSplitQueue` + `globals.css` 子选择器适配）+ 恢复 `task:DOCUMENT_APPROVAL` + **以 `cursor` 分页适配层暴露 `/approval-instances?view=blocked\|managed\|mine`**<br>8. 默认无筛选 `scope=mine` 首屏改用单次 `GET /admin/work-items/stats`<br>9. 2 栏决策工作台（解除 `singleColumn`）+ 撤除 12 类任务冗余 376px `aside` + 清理双页头/双侧栏/双内边距 + 淘汰 `SequentialProcessBar` + 释放 `DefinitionFlowCanvas` 高分屏高度<br>10. ADR-1 分级风险决策条 + 激活态守卫快捷键 + 审批/抄送统一 `autoNext` + 乐观更新 + 顶栏红点分化 + `PortalReviewForm` 分组重构 + 状态色典 + `check-ui-theme.mjs` 6 条精准门禁与死代码清理 | **零后端阻塞，主线立即分阶段推进** | 严格遵守后端既有契约（不向 `blocked`/`managed`/`mine` 传 `page`/`filters`；不绕过 Bearer Blob 鉴权；不破坏非 `PROCESS` 权限安全边界） |
| **后端轻量必选协同：第 1 批（Phase 0/1 随行）** | 1. **`SUPPLIER_PORTAL_REVIEW` 业务族枚举对齐**：`backend/crates/domains/erp-workflow/src/dto/work_item/status.rs:122` 由 `Approval` 改为 `Procurement`<br>2. **通知全部已读接口**：提供 `POST /account/notifications/read-all`，支撑顶栏 `InboxPopover` 一键标为已读 | **轻量化（~1 人天），与 Phase 0/1 并行随行** | 不阻塞前端主线启动；合入后即消除业务族筛选错配并补齐收件箱全部已读闭环 |
| **后端轻量必选协同：第 2 批（Phase 2/3 随行）** | 3. **`GET /admin/work-items/stats` 全维度扩展**：支持 `managed`/`history`/带 `q` 与 `filters` JSON 及按 `document_type` 拆分的 `approval:*` 计数，全面淘汰剩余场景的 8 路并发计数<br>4. **`GET /admin/work-items` 开放往来单位检索**：支持 `q` 模糊匹配往来单位名及 `counterparty_ids` 参数<br>5. **`/approval-instances?view=blocked\|managed\|mine` 对齐页码分页**：扩展 `PreparedInstanceListQuery` 对齐 `view=started` 的页码与结构化筛选，退休前端 `cursor` 适配分支 | **低–中成本（2–3 人天），与 Phase 2/3 并行随行** | 在此之前，前端已通过默认首屏 `stats`、`started` 往来单位筛选与 `cursor` 适配层先行兜底，零阻塞交付 |
| **后端轻量必选协同：第 3 批（Phase 3 前置）** | 6. **履约仓储与经营管理角色工作台枚举补齐**：`backend/crates/domains/erp-identity/src/entity/role_workbench.rs:12-16` 新增履约仓储与经营管理 `RoleWorkbench` 枚举及角色权限映射（否则 `role-workbench-page.tsx:85` 会拦截报无权访问） | **Phase 3 上线新角色工作台必选前置（~1 人天）** | 仅前置阻塞 Phase 3 两个新增工作台路由，不影响现有 3 个角色工作台（`WB-01`/`WB-03`/`WB-04`）与全部 Phase 0–P2 交付 |
| **P3+ 独立增强迭代（明确划出本轮交付）** | 1. **批量审批端点** `POST /work-items/batch-approve`（带风控白名单与部分成功语义）<br>2. **审批评论/补件澄清** `POST /approval-instances/{id}/comments`（免除轻微笔误被迫整单 `REJECT` 重跑）<br>3. **受阻实例现场指派替补审批人** `POST /approval-instances/{id}/reassign-blocked`<br>4. **批量转交/关闭原子端点** `POST /work-items/batch-reassign`（含批量候选人接口）/`batch-close` | **不阻塞本轮交付，独立排期演进** | 前端在 `ApprovalActionBar` 与批量选择栏预留扩展槽，后续插拔式上线 |

---

### 6.3 四阶段落地路线图（定案：ADR-4 × ADR-2）

#### Phase 0（第 1–2 周）：高危缺陷修复、校验/口径统一与转交闭环
- **目标**：消除阻断性 Bug、校验错配、权限断层与状态语义混乱；不改动大体量页面布局，低风险快交付。
- **前端核心交付**：
  1. `lib/workspace-tabs.ts` 白名单补齐 `"account"`；修复 `workspace-document-paper-dialog.tsx` 与 `workspace-task-context.tsx` 重复 DOM ID；修正 `supply-exception-task-panel.tsx:313` 错别字 `"负责人人"`。
  2. 上线 `lib/validation/reason-length.ts`，严格对齐 BPM 512 UTF-8 字节、`ABANDON_REWORK` 512 字符、`REASSIGN` 150 字符、`CLOSE` 100 字符上限并配实时余量提示。
  3. 上线 `WorkItemResponsibilityDialog`（完整覆盖 `REASSIGN` 与 `CLOSE` 字段）；**保留 `workspace-task-surface.tsx:216-228` 只读正文安全边界**，移除 `workspace-document-task-detail.tsx:520` 类型硬编码并在 `WorkspaceTaskHeaderActions` 按 `allowedActions` 统一挂载，将 `REASSIGN` 覆盖率从 `2/14` 拉至 `14/14`。
  4. 将 5 个撤回组件（4 套模式）收敛为 `ApprovalWithdrawDialog`（强制红色 `destructive` + 512 UTF-8 字节校验）；为 `sales_order` / `voucher_sales_order` / `sales_invoice_request` 补齐 `SubmissionRouteConfirmation`。
  5. 上线 `lib/task-status-dictionary.ts` 统一 `RUNNING`/`BLOCKED`/`ACTIVE` 色板与 `ApprovalActionBar` 左右固定顺序；替换审批/抄送 6 处及全仓裸 `Date.toLocale*` 调用；分化顶栏 `InboxPopover` 与 `SystemFeedbackPanel` 徽章视觉。
  6. 恢复 `task:DOCUMENT_APPROVAL` 聚合入口，并以 `cursor` 分页适配层安全暴露 `/workspace/approvals?view=blocked|managed|mine`（避免 422）。
- **随行后端协同（ADR-2 第 1 批启动）**：同步排期 `SUPPLIER_PORTAL_REVIEW` 枚举由 `Approval` 改 `Procurement`（`status.rs:122`）及通知全部已读接口（`POST /account/notifications/read-all`）。
- **风险与验证门禁**：低风险；通过 `npm run lint`、`npx tsc --noEmit` 及 13 类单据提交/撤回/驳回 UTF-8 字节边界（170 字 vs 171 字）与 `blocked`/`managed` 视图零 422 核验。

#### Phase 1（第 3–5 周）：容器查询引擎、浮层原语、`UnifiedMaterialViewer` 与 6 条门禁基建
- **目标**：夯实底层 UI/布局基建，根治 1440px 断点坍塌，落地双轨抽屉规范与统一附件对照/审计轨迹原语。
- **核心交付**：
  1. **容器查询与 CSS 升级**：`<main id="main-content">` 增加 `@container/workspace`，同步宿主/iframe 高度密度属性；改造 `RoleWorkbenchMetrics`、`DetailWorkspaceBody`、`UnifiedTaskWorkbench` 为 `@[960px]/workspace:` / `@[1120px]/workspace:`；按原梯队将 `app/globals.css` 中 `L639` (`75rem`→`70rem`) 与 `L1174/1203/1211` (`48/64/80rem`→`48/52/60rem`) 改写为 `@container workspace`。
  2. **浮层原语与 ADR-3 治理同步**：`DialogContent` 新增 `size` + `scroll` + `DialogBody`；`SheetHeader` 新增 `SheetCloseContext` 自动 `pr-12`；修复 `QuickPreviewSheet` 正文边距为 `px-sheet-inline py-5`（严禁 `useAppForm`）并升级 `AdjustmentDetailSheet` 至 `TaskActionSheet size="detail"` (768px)；新增受控 `TaskActionSheet`（固定 `size="detail"` 768px，禁止套娃抽屉）；**同步更新 `erp-client/AGENTS.md §7`**；`Popover` 集成 iframe 点击关闭；接入 ADR-2 第 1 批通知全部已读接口。
  3. **核心工作流组件**：泛化 `ContractPdfPages` 落地 `UnifiedMaterialViewer`（认证 `Blob` 双模式）、`ApprovalRouteStepper` (44px) 与 `UnifiedAuditTrail`（合并 9 套历史/时间线），淘汰 `/approval-materials` 裸 Tab 与 `CopyFiles` 70vh iframe；释放 `DefinitionFlowCanvas` 高分屏 620px 截断上限。
  4. **队列 Chrome 瘦身与首屏计数收敛**：`WorkspaceHomePage` 迁至 `ListWorkspacePage`（遵守 `AGENTS.md §8` `TableToolbar` 契约），合并页头视图与单行工具栏，单页隐藏分页条，默认 `density="compact"` (`36px`)；默认无筛选 `scope=mine` 首屏切至单次 `/admin/work-items/stats`。
  5. **扩展 `scripts/check-ui-theme.mjs`**：上线 6 条精准门禁规则（零误伤 `number.toLocaleString` 与非预览左侧导航抽屉；存量 12 个 `surfacePanelClassName` 与 8 个 `PageHeader` 文件登记白名单，新增代码 0 容忍）。
- **风险与验证门禁**：中风险；在 `1280×800`、`1440×900`、`1920×1080`（侧栏展开/折叠双态）全量截屏回归，验证 `npm run lint`（含 `theme-tokens.ts` 1:1 校验）全绿。

#### Phase 2（第 6–9 周）：审批 2 栏工作台 + 统一分栏队列率先打样，全量 17 事项收口与白名单清零
- **目标**：严格按 **ADR-4** 以最高频、最高收益的「审批 2 栏工作台 + 统一分栏队列」作为首个业务打样率先上线，随后收口全部 17 事项、根除弹窗套娃并清零存量白名单。
- **阶段内分批交付**：
  1. **Batch 2A（首个业务打样率先上线：审批 2 栏工作台 + 统一分栏队列与抄送）**：
     - 激活 `ResizablePanelGroup` 落地 `WorkspaceSplitQueue`（同步调整 `app/globals.css:572-603, 912-1049` 的 `>` 直接子选择器），统一承接 `/workspace/tasks`、`/workspace/approvals`（含 `cursor` 适配）、`/workspace/history`、`/workspace/copies`（统一 `pageSize=50`、即时搜索筛选、批量选择栏 `ListSelectionBar`；分栏卡模式设 `showColumnVisibility={false}`）；
     - 启用 `UnifiedTaskWorkbench`，解除 13 类审批 `singleColumn=true`，右侧常驻 `UnifiedAuditTrail` + `UnifiedMaterialViewer`（支持拖拽至 50% 左右对照合同/回单）；
     - 落地 **ADR-1 分级风险决策条**（常规低/普通风险单据底栏内联意见 + 一键直接通过 + 3 秒 toast + `autoNext`；冲正红字/负毛利超信用预算/大额超阈值/阻断警告强制单层 `FormalActionConfirmDialog`；驳回动态短语 + 512 UTF-8 字节校验）；
     - 收编 `purchase_payment_request` 至统一壳；6 类通用快照升级为 `StructuredDiffApprovalRenderer` / `FinancialApprovalRenderer`；开票审批消除 3 层 `<details>`；销售审批交付分配改为行内展开；抄送接入统一队列分栏与 `bare` 无框快照。
  2. **Batch 2B（全量 17 事项收口 + 消灭弹窗套娃与刷新循环）**：
     - 撤除 12 类执行与异常任务冗余 376px `aside` 与 7 处双水平内边距；
     - 剥离 `SupplierOrderCenterPage`、`BatchDetailView`、`SettlementCenter`、`IntegrationErrorsPage` 的嵌套壳、双侧栏与 `SequentialProcessBar`；扁平化 `SupplyExceptionTaskPanel` 与旧审批 `<Card>`；
     - 客户验收登记内联化；采购变更草稿改弹窗内两步视图；废除 `WorkspaceFulfillmentProcessAction`；付款跟进改 `Dialog size="sm"`；`WorkspaceReturnFinance` 与 `PurchasePaymentArrangement` 接入反应式 `onTaskCompleted`。
  3. **Batch 2C（清零存量门禁白名单 + 随行对接 ADR-2 后端第 2 批）**：
     - 将剩余历史业务 Dialog 迁移至 `DialogContent size`、清除 `<NativeSelect>`、清理 12 处非详情 `surfacePanelClassName` 与 8 处 `PageHeader`；
     - 后端并行推进 ADR-2 第 2 批（`stats` 全维度扩展、往来单位检索、`/approval-instances` 页码分页对齐），就绪后前端按开关平滑切换。
- **风险与验证门禁**：中高风险；按「审批与抄送打样（2A）→ 采购/财务/履约与异常全量收口（2B）→ 白名单清零（2C）」三步灰度上线，每批完成主流程端到端核验与 `check-ui-theme.mjs` 渐进清零。

#### Phase 3（第 10–12 周）：键盘效率引擎、长表单拆解、角色工作台补齐、死代码清零与验收闭环
- **目标**：把高频效率推至极致，协同后端补齐全岗位工作台，清理全部历史技术债并刷新验收基线。
- **核心交付**：
  1. **键盘效率与批量引擎**：上线带 `data-workspace-active` 守卫与切页焦点恢复的 `useWorkspaceHotkeys`（`J/K/A/R/P/H/]/Esc` + 速查卡）、审批与抄送全量 `autoNext` + 上下项遍历、列表多选批量转交（候选人交集）/关闭、升级 `Cmd/Ctrl+K`（`WorkspaceTabs` 暴露 `tabs` 状态支持搜已开页签、任务单号与往来单位）。
  2. **2,044 行长表单拆解与角色工作台补齐**：将 2,044 行 `PortalReviewForm` 拆分为 4 组粘性锚点折叠步骤 + SKU 单位一键批量应用；在 **ADR-2 后端第 3 批（`erp-identity` `RoleWorkbench` 枚举扩展）**就绪后上线**履约仓储工作台**与**经营管理工作台**，并将所有 5 个角色工作台首屏 KPI 压缩至单行紧凑带。
  3. **ADR-2 后端第 2 批全面收口**：全量切换至扩展后的 `/admin/work-items/stats`、全视图往来单位检索与审批列表统一页码分页（移除前端 `cursor` 临时适配分支）。*（注：`batch-approve`、审批评论、受阻替补指派按 ADR-2 划归 P3+ 独立增强迭代，不阻塞本阶段签收。）*
  4. **死代码清零与文档刷新**：删除 `drawer.tsx`、`detail-page-header.tsx`、`object-section-tabs.tsx`、`page-header.tsx`、`DocumentHeader`、`SequentialProcessBar`、`workspace-pane-actions.tsx`、`workspace-overview.tsx` 死导出、遗留非 `flat` 分支（160 行）及 `workspace-registry.ts` 过期项；重写 `erp-client/design-qa.md` 建立新版视觉与交互验收基线。
- **风险与验证门禁**：低中风险；执行全量 `npm run lint && npm run format:check && npx tsc --noEmit && npm run build`，按新版 `design-qa.md` 完成 3 档分辨率 × 4 大原型任务全覆盖 QA 签收。

---

### 6.4 可量化成功指标（KPI Targets）

| 评估维度 | 核心指标 | 重构前基线（实测） | 目标值（Phase 3 验收线） |
|---|---|---|---|
| **页面空间利用率** | `1440×900` 队列页数据区占比 | ~67%（Chrome 占 296–330px） | **通栏与分栏均 ≥ 82%（单页 Chrome ≤ 158px，内联 `TableToolbarInlineControls` + 悬浮 `dock` 零额外行；多页 ≤ 194px）** |
| **页面空间利用率** | `1440×900` 队列首屏可见条目数 | 9–12 行（48–64px 行高） | **通栏表格 19–20 行（36px 单行，+67%~+85%↑；`1280×800` 为 16–17 行）；分栏摘要卡 13–14 条 + 右栏即阅** |
| **页面空间利用率** | `1440×900` 审批页明细上方纯占高 / 顶+底总 Chrome | 上方 326–348px / 总 383–405px | **上方 ≤ 148px（-55%↓）/ 总 ≤ 196px**，核心明细与差异 100% 进首屏 |
| **页面空间利用率** | `1440×900` 角色工作台首屏待办出现位置 | 460px 以下（51% 被占） | **≤ 192px**（首屏 78% 展示可操作内容） |
| **操作效率** | 含合同销售单审批点击数 / 跨 Tab 数 | ≥ 9 次点击 / 1+ 次新 Tab | **≤ 3 次点击 / 0 次跨 Tab（-67%↓）** |
| **操作效率** | 连续处理 10 条常规审批/抄送 iframe 冷启动次数 | 10 次冷启动 | **0 次冷启动（100% 页内分栏/抽屉流转）** |
| **操作效率** | 默认无筛选待办首屏头部计数并发请求数 | 8 路并行请求 | **1 次 `/admin/work-items/stats`（-87%↓）**（其余视图随后端第 2 批 stats 扩展同步收敛） |
| **交互与设计一致性** | 撤回组件 / 附件预览 / 轨迹实现套数 | 5 撤回(4 套) / 4 预览(3 审批+1 提报) / 9 轨迹 | **1 撤回 / 1 预览 / 2 轨迹** |
| **交互与设计一致性** | 弹窗套娃（Modal-on-Modal）数量 | ≥ 5 处高频路径 | **0 处** |
| **交互与设计一致性** | 14 类可转交事项 `REASSIGN` 覆盖率 / 13 类单据提交链覆盖率 | **2/14（14.3%）** / 10/13（77%） | **100% (14/14) / 100% (13/13)** |
| **工程门禁** | `check-ui-theme.mjs` 新增 6 条精准规则违例数 | 160 处弹窗手写宽、全仓裸 Date 本地化调用（含审批/抄送 6 处）、5 处 `NativeSelect`、12 处非详情 `surfacePanelClassName` | **0 违例（全仓通过，零误伤数值千分位与左侧导航树）** |
