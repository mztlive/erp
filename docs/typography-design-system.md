# ERP 排版与字号设计系统（Typography Design System）

> **适用范围**：`erp-client` 全站（`app/`、`components/`、`features/`、`lib/`），与 [`docs/list-table-design-system.md`](./list-table-design-system.md) 及 [`docs/detail-page-design-system.md`](./detail-page-design-system.md) 100% 同源。  
> **主题体系**：飞书 Universe Design 高效率紧凑桌面排版。  
> **实施路线**：全站存量重构分阶段执行方案见 [`docs/typography-refactoring-plan.md`](./typography-refactoring-plan.md)。

---

## 1. 设计目标与五条硬约束

本规范旨在消除当前项目“14px shadcn 旧默认体系与 13px 飞书新紧凑体系并行打架、10px–16px 逐像素密排、原语层与业务层层级倒置”的问题，建立唯一、可由 Lint 强制校验的字号与行高体系。

1. **绝不修改根字号（`html` 保持 `16px`），在 `body` 确立桌面默认正文 `13px`**
   - `rem` 始终以 `16px` 为基准，保证所有 `--spacing-*` 尺寸计算与高度断点零漂移。
   - `body` 默认应用 `text-body-compact`（`13px / 20px`）。常规正文、表格单元格、控件文本直接继承 `13px`，**无需也不应处处重复手写 `text-body-compact` 或 `text-sm`**。
2. **语义角色优先，禁用裸物理字号**
   - 桌面管理端只允许消费本规范定义的 **5 档文本角色** 与 **4 档 KPI 数值角色**。
   - **禁止**在桌面管理端非豁免代码中使用 Tailwind 原始物理字号类：`text-sm`、`text-base`、`text-lg`、`text-xl`、`text-2xl`、`text-3xl` 及以上，禁止任意值 `text-[...]` 与内联 `style={{ fontSize }}` / `fontSize` 属性。
3. **消灭 `10px` 与 `14px` / `16px` 造成的 1px 逐像素密排**
   - 全站最小可读字号底线为 **`11px`（`text-tiny`）**，废弃 `--text-2xs`（`10px`）。
   - 正文之上严格保持 `≥ 2px` 可辨阶梯：**`13px` 正文 → `15px` 分区/卡片/弹窗标题 → `18px` 页面主标题 → `20/24/28/40px` 专用数值**，消除 `13/14/15/16px` 混杂与 `20px SheetTitle > 18px PageTitle` 的层级倒置。
4. **行高随字号 Token 绑定，适配中文阅读**
   - 所有字号 Token 必须在 `app/globals.css` 的 `@theme` 中成对定义 `--text-<name>` 与 `--text-<name>--line-height`。
   - 12px（`text-xs`）默认行高统一为 `18px`（`1.125rem`），消除当前全仓上百处手写 `leading-5` / `leading-4.5` / `leading-relaxed` 补丁。仅单行极紧凑角标或精确排版允许局部覆盖 `leading-none` / `leading-4`。
5. **豁免边界显式白名单化**
   - 仅允许规范第 5 节列出的 **4 类豁免场景**使用额外字号，并在 `scripts/check-ui-theme.mjs` 中显式登记文件路径；其余所有文件统一受自动化检查约束。

---

## 2. 权威字号阶梯（唯一真相）

### 2.1 字体栈与数字排版

- **中文正文字体栈（`--font-sans` / `--font-heading`）**：`"Noto Sans SC Variable", system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", "WenQuanYi Micro Hei", sans-serif`
- **等宽字体栈（`--font-mono`）**：用于代码、JSON 载荷、技术标识。
- **业务数字排版（`.num`）**：所有金额、数量、税率、日期时间、单号、SKU 编号、计数角标必须加 `.num`（启用 `tabular-nums lining-nums slashed-zero`），确保纵向严格对齐。

### 2.2 文本层级阶梯（5 档）

| 档位 | Token / Tailwind 类 | 字号 / 行高 | rem 值 | 推荐字重 | 标准使用场景 | 明确禁止 |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **微标** | `text-tiny` | `11px / 16px` | `0.6875rem / 1rem` | `font-medium` / `font-semibold` | **仅限 1–6 字非正文微件**：计数红点/角标、快捷键 `Kbd`、导航 60px 窄轨 2 字短签、分组 Eyebrow、未保存/待查询微胶囊、图片角标 | 禁止用于任何完整句子、段落说明、表单正文、表格常规单元格 |
| **辅助 / 标签** | `text-xs` | `12px / 18px` | `0.75rem / 1.125rem` | 标签/表头/徽章 `font-medium`；说明 `font-normal` | 表单字段 `Label`、`FieldDescription`、`FieldError`、表头 `<th>`、`StatusBadge` / `Badge`、`FilterChip`、元信息行、名称下方次身份编号、数量单位、分页条、`Tooltip`、分组 `legend`、侧栏分组标题、时间戳 | 禁止用作主名称、主输入值或页面主操作文案 |
| **核心正文（默认）** | `text-body-compact` *(或直接继承 `body`)* | `13px / 20px` | `0.8125rem / 1.25rem` | 正文 `font-normal`；主名称/强调 `font-medium`；小节子标题 `font-semibold` | **全站默认工作面字号**：表格单元格、输入框/文本域/下拉触发器与选项、按钮、导航菜单项、视图/分区 Tab、下拉/右键菜单、弹窗/抽屉/卡片/告警/Toast 正文、描述列表值 `dd`、行操作链 | 禁止再写 `text-sm`；`<table>` 与常规容器内无须重复声明 |
| **二级结构标题** | `text-section-title` | `15px / 22px` | `0.9375rem / 1.375rem` | `font-semibold` + `tracking-tight` | 详情页分区标题 `DetailWorkspaceSection` / `DocumentSection` `<h2>`、`CardTitle`、`DialogTitle`、`AlertDialogTitle`、`DrawerTitle`、`PopoverTitle`、`EmptyTitle`、`BusinessTableFrame` 标题、带图 `QuickPreviewSheet` 紧凑标题 | 禁止使用 `text-base` / `text-lg` / `text-xl` 替代 |
| **页面主标题** | `text-page-title` | `18px / 26px` | `1.125rem / 1.625rem` | `font-semibold` + `tracking-tight` | 页面唯一主标题 `<h1>`（`ListWorkspaceHeader`、`DetailWorkspaceChrome`、`PageHeader`、`DetailPageHeader`、`DocumentHeader`）、无图默认 `SheetTitle` | 禁止使用 `text-lg` / `text-xl` / `text-2xl`；任何浮层标题不得超过 `18px` |

> **关于 `h3` / 块内子标题**：若分区（`15px`）内部需要再细分小块标题（如侧栏子块、表单内子组标题），使用 **`13px`（继承正文）+ `font-semibold text-foreground`**，不引入额外的 `14px` 档位。

### 2.3 关键指标 / KPI 数值阶梯（4 档，仅限 `.num` 数值）

此阶梯**专用于重点汇总金额、统计数字与经营看板指标**，必须配合 `.num font-semibold tracking-tight` 使用，**严禁用于普通中文标题**。

| 档位 | Token / Tailwind 类 | 字号 / 行高 | rem 值 | 标准使用场景 |
| :--- | :--- | :--- | :--- | :--- |
| **紧凑指标** | `text-value-compact` | `20px / 28px` | `1.25rem / 1.75rem` | `MetricItem density="compact"`、紧凑卡片/侧栏汇总数字 |
| **分区汇总** | `text-value-section` | `24px / 32px` | `1.5rem / 2rem` | `MoneyValue size="section"`、`MetricItem density="default"`、`DetailSummaryItem`、流程状态统计数字 |
| **页面摘要** | `text-value-summary` | `28px / 36px` | `1.75rem / 2.25rem` | `MoneyValue size="summary"`、工作台顶部核心 KPI、大卡片主汇总金额 |
| **首屏主值** | `text-value-hero` | `40px / 44px` | `2.5rem / 2.75rem` | `MoneyValue size="hero"`、单据/看板唯一视觉焦点总额 |

### 2.4 `app/globals.css` 权威 `@theme` 定义

```css
@theme {
    /* ...间距令牌保持不变... */

    /* 排版与字号阶梯（唯一真相）：文本 5 档 + 数值 KPI 4 档；均配套中文友好行高。 */
    --text-tiny: 0.6875rem; /* 11px 微标 / 计数角标 / Kbd */
    --text-tiny--line-height: 1rem; /* 16px */

    --text-xs: 0.75rem; /* 12px 辅助 / 表单标签 / 表头 / 元信息 / 徽章 */
    --text-xs--line-height: 1.125rem; /* 18px */

    --text-body-compact: 0.8125rem; /* 13px 桌面核心正文 / 控件 / 表格默认 */
    --text-body-compact--line-height: 1.25rem; /* 20px */

    --text-section-title: 0.9375rem; /* 15px 分区 / 卡片 / 弹窗标题 */
    --text-section-title--line-height: 1.375rem; /* 22px */

    --text-page-title: 1.125rem; /* 18px 页面主标题 / 默认抽屉对象标题 */
    --text-page-title--line-height: 1.625rem; /* 26px */

    /* KPI 与关键金额专用阶梯（须配合 .num 使用，禁止用于普通文字标题） */
    --text-value-compact: 1.25rem; /* 20px */
    --text-value-compact--line-height: 1.75rem; /* 28px */

    --text-value-section: 1.5rem; /* 24px */
    --text-value-section--line-height: 2rem; /* 32px */

    --text-value-summary: 1.75rem; /* 28px */
    --text-value-summary--line-height: 2.25rem; /* 36px */

    --text-value-hero: 2.5rem; /* 40px */
    --text-value-hero--line-height: 2.75rem; /* 44px */
}
```

> **运行时变量与 `tailwind-merge` 注册**：
> 1. 所有自定义字号（`tiny`、`body-compact`、`section-title`、`page-title`、`value-compact`、`value-section`、`value-summary`、`value-hero`）须登记到 `erp-client/lib/theme-tokens.ts` 的 `themeTextNames`，避免 `tailwind-merge` 把 `text-page-title` 与 `text-foreground` 误判为同类冲突并互相覆盖。
> 2. `xs` 属于 Tailwind 内置字号名，`tailwind-merge` 原生已识别；`scripts/check-ui-theme.mjs` 检查 CSS 与 `theme-tokens.ts` 同步时应忽略 Tailwind 内置字号名（`xs` / `sm` / `base` / `lg` / `xl` / `2xl`–`9xl`），避免重复登记。
> 3. 同步删除 `app/globals.css` 高度断点中与默认值完全一致的冗余 `--text-page-title` 覆盖。

---

## 3. 废弃与禁止清单

| 废弃/禁止项 | 处置方式 | 替代方案 |
| :--- | :--- | :--- |
| `--text-2xs` / `text-2xs` (`10px`) | **彻底删除** | 非正文极小角标/窄轨短签改用 `text-tiny` (`11px`)；可读业务文案改用 `text-xs` (`12px`) |
| `--text-login-title` (`28px`) | **彻底删除**（单页私有且与 `value-summary` 数值完全重复） | 登录页属于品牌展示豁免区，或直接复用 `text-value-summary`（推荐登录表单标题用豁免区 `text-2xl` / `24px` 或 `text-[...]` 外的标准展示阶梯 `text-2xl`/`text-3xl`，详见第 5 节） |
| `text-sm` (`14px`)（桌面非豁免区） | **禁止新增，分批清零** | 常规正文直接删除（继承 `body` 13px）；独立容器或显式覆盖场景写 `text-body-compact` |
| `text-base` (`16px`)（桌面非豁免区） | **禁止**（除 `<Input>`/`<Textarea>` 移动端防缩放特例外） | 分区/卡片/弹窗标题改用 `text-section-title` (`15px`) |
| `text-lg` (`18px`) | **禁止**（桌面非豁免区） | 页面主标题改用 `text-page-title` (`18px`)；弹窗/空态/分区标题改用 `text-section-title` (`15px`) |
| `text-xl` (`20px`) | **禁止**（桌面非豁免区） | 抽屉标题改用 `text-page-title` (`18px`)；紧凑 KPI 数值改用 `text-value-compact` (`20px`) |
| `text-2xl` / `text-3xl` | **禁止**（桌面非豁免区） | KPI/汇总金额改用 `text-value-section` (`24px`) / `text-value-summary` (`28px`)；普通标题降级到 `text-page-title` (`18px`) 或 `text-section-title` (`15px`) |
| `text-[13px]` 等任意字号 | **全仓禁止**（包括 `components/`） | 改用对应语义 Token |
| `style={{ fontSize }}` / JSX `fontSize` prop | **全仓禁止**（`components/` 与 `features/`） | 改用语义 Token 类 |

---

## 4. 共享组件字号契约矩阵

所有基础原语与共享业务组件必须严格遵守下表，从源头杜绝跨组件倒置：

| 组件 / 槽位 | 规范字号 | 规范字重 & 附加要求 | 纠正的既有问题 |
| :--- | :--- | :--- | :--- |
| **`body` 根正文** | `text-body-compact` (13px) | `font-normal` | 消除无字号时掉回浏览器 16px |
| **页面主标题 `<h1>`**（`ListWorkspaceHeader` / `DetailWorkspaceChrome` / `PageHeader` / `DetailPageHeader` / `DocumentHeader`） | `text-page-title` (18px) | `font-semibold tracking-tight` | 消除 `h1` 出现 6 种字号的问题 |
| **页面副标题 / 元信息**（`ListWorkspaceHeader.description` / `DetailWorkspaceMeta` / `PageHeader.metadata` / `DocumentHeader` meta） | `text-xs` (12px) | `font-normal text-muted-foreground` | 统一 `DocumentHeader` default/compact 混用 `text-sm`/`text-xs` |
| **分区标题 `<h2>`**（`DetailWorkspaceSection` / `DocumentSection` / `DetailRecordSection` / `BusinessTableFrame`） | `text-section-title` (15px) | `font-semibold tracking-tight` | 统一原先 14px / 15px / 16px 分裂 |
| **Card**（`Card` / `CardTitle` / `CardDescription`） | 容器 `text-body-compact` (13px)；标题 `text-section-title` (15px)；描述 `text-xs` (12px) | `CardTitle`: `font-semibold tracking-tight` | 消除 Card 16px/14px 与 DetailWorkspace 15px/13px 割裂 |
| **Dialog / AlertDialog / Drawer / Popover** | 容器 `text-body-compact` (13px)；标题 `text-section-title` (15px)；描述 `text-xs` (12px) | Title: `font-semibold` | 统一 Dialog(16px)、AlertDialog(18px)、Popover(16px) 各自为政 |
| **Sheet / QuickPreviewSheet** | 容器 `text-body-compact` (13px)；默认 `SheetTitle` `text-page-title` (18px)；`headerMedia` 紧凑态 `text-section-title` (15px)；`SheetDescription` `text-xs` (12px) | 抽屉标题绝不超过页面主标题 18px | 纠正原先 `SheetTitle` 20px 比页面 `<h1>` 18px 还大的倒置 |
| **Form 表单原语**（`Label` / `FieldLabel` / `FieldTitle` / `FieldDescription` / `FieldError` / `FormError` / `FormValidationSummary`） | `Label` / `FieldLabel` / `FieldTitle`：`text-xs font-medium text-foreground/75` (12px)；`FieldDescription` / `FieldError`：`text-xs` (12px)；`FieldLegend(legend)`：`text-section-title` (15px)；`FieldLegend(label)`：`text-xs` (12px)；`FormError` / `FormValidationSummary`：`text-body-compact` (13px) | 标签退后于 13px 输入值 | 彻底根治全站“14px 标签压过 13px 输入框”，可移除 `globals.css` 局部 label 字号补丁 |
| **输入类控件**（`Input` / `Textarea` / `Select` / `NativeSelect` / `Combobox` / `CommandInput` / `InputGroup*` / `InputOTP`） | 桌面统一 `text-body-compact` (13px)；其中 `Input` / `Textarea` 保留 `text-base md:text-body-compact` 防 iOS 缩放；`InputGroupText` / `ComboboxEmpty` / `CommandEmpty` / `CommandInput` 统一为 `text-body-compact` (13px)（或空态提示 `text-xs`） | 输入值、前后缀、下拉项同高同字号 | 消除 `InputGroupText`、`CommandInput`、`ComboboxEmpty` 残留 14px |
| **Button** | `default` / `sm` / `lg`：`text-body-compact` (13px)；`xs`：`text-xs` (12px) | `font-medium` | 已基本对齐，保持不变 |
| **Table / DataTable** | `<table>`：`text-body-compact` (13px)；`<th>`：`text-xs font-medium` (12px)；`<caption>`：`text-xs` (12px)；单元格内主文本直接继承 13px，次信息/单位/状态用 `text-xs` (12px) | 严禁在表格/列定义内手写 `text-sm` | 纠正 `TableCaption` 14px 及 12 个业务表列残留 `text-sm` |
| **Tabs** | `TabsTrigger`：`text-body-compact` (13px)；`TabsContent`：移除 `text-sm`，继承 `body` 13px | — | 纠正标签头 13px、内容区 14px 错位 |
| **Alert & Toast** | `Alert` / `AlertTitle` / `ToastTitle`：`text-body-compact font-medium` (13px)；`AlertDescription` / `ToastDescription`：`text-xs` (12px)（单段纯文字提示时 `text-body-compact`） | 描述绝不大于标题 | 纠正 `AlertDescription` (14px) 比 `AlertTitle` (13px) 更大的倒置 |
| **Badge / StatusBadge / FilterChip / Kbd** | `Badge` & `StatusBadge` & `FilterChip`：统一 `text-xs` (12px)；`Kbd` / 计数角标 / Eyebrow / 未保存微胶囊：`text-tiny` (11px) | 消除 `Badge`(11px) 与 `StatusBadge`(12px) 互覆盖 | 减少 11px/12px 徽章混用 |
| **DescriptionList** | `dt`：`text-xs font-medium text-muted-foreground` (12px)；`dd`：`text-body-compact text-foreground` (13px) | 键轻值重 | 统一详情信息键值对与表单字号一致 |
| **Empty 空状态** | `EmptyTitle`：`text-section-title` (15px)；`EmptyDescription` / `EmptyContent`：`text-body-compact` (13px) | — | 纠正 `EmptyTitle` 18px 抢戏 |
| **Sidebar & Topbar 导航** | 窄轨短签 & 数字角标：`text-tiny` (11px)；分组标题 / 快捷搜索提示 / 次要账号信息：`text-xs` (12px)；菜单项 / 页签：`text-body-compact` (13px) | 侧栏从 5 档收敛为清晰 3 档（11/12/13px） | 消灭侧栏内 10/11/12/13/14px 混杂 |
| **Values & Metrics** | `MoneyValue`: `inherit` / `section(text-value-section)` / `summary(text-value-summary)` / `hero(text-value-hero)`；`MetricItem`: `compact(text-value-compact)` / `default(text-value-section)`；标签与单位 `text-xs` (12px) | 必须带 `.num` | 告别裸写 `text-xl` / `text-2xl` |

---

## 5. 四类显式豁免区（Allowed Exceptions）

为避免“一刀切”破坏特殊场景体验，以下 4 类场景显式豁免桌面管理端字号白名单，且在 Lint 脚本中单独声明路径：

1. **移动端防 iOS 自动缩放输入特例**
   - 位置：`erp-client/components/ui/input.tsx`、`erp-client/components/ui/textarea.tsx`
   - 规则：允许保留 `text-base md:text-body-compact`（`< 768px` 保持 `16px` 防止 iOS Safari 聚焦输入框时强制放大视口；`≥ 768px` 桌面端为 `13px`）。
2. **纸质正式打印单据（Print Surface）**
   - 位置：`erp-client/components/business/paper-document.tsx`
   - 规则：面向 A4 纸面核对与打印输出，根节点显式声明 `text-sm`（14px 正文），主标题保持 `text-3xl`（30px），允许使用 `text-xs` / `text-sm` / `text-base` / `text-lg` / `text-3xl`。
3. **移动端客户选品页（Mobile Public Surface）**
   - 位置：`erp-client/app/s/[token]/**`、`erp-client/features/sales-selection/pages/public-selection-page.tsx`、`erp-client/features/sales-selection/components/public-selection-*.tsx`
   - 规则：依 `AGENTS.md §8`，客户选品页按移动端 C 端体验设计，容器根节点显式设定 `text-sm`（14px）基线，允许使用 `text-tiny` / `text-xs` / `text-sm` / `text-base` / `text-lg` / `text-xl` / `text-2xl` 及响应式变体；但同样**禁止 `text-2xs`（10px）和任意值 `text-[...]`**。
   - **注意**：`features/sales-selection/` 下的后台管理页（如 `books-list-*`、`book-detail-*`、`proposal-*`、`preview-grid.tsx`）属于桌面管理端，**不豁免**。
4. **品牌登录展示页（Auth Showcase Surface）**
   - 位置：`erp-client/features/auth/components/login-stage.tsx`、`login-stage-art.tsx`、`login-form.tsx`、`erp-client/features/supplier-portal/pages/login-page.tsx`
   - 规则：深色品牌舞台与登录卡允许使用 `text-sm` / `text-2xl` / `text-3xl` 作为展示层级，但同样禁止 `text-2xs` 与任意值 `text-[...]`。
   - **注意**：`features/supplier-portal/` 的其余页面（供货列表、报价、新品提报等）属于桌面工作台，**不豁免**。

---

## 6. 语义标题与无障碍契约

HTML 标题层级（`<h1>` / `<h2>` / `<h3>`）表达文档大纲结构，视觉字号统一遵循下列映射：

| 语义标签 | 桌面默认视觉角色 | 允许特例 |
| :--- | :--- | :--- |
| `<h1>` | 始终为 `text-page-title` (`18px`)，由共享页头组件渲染 | 纸质单据 / 品牌登录舞台 / 移动端客户选品页（豁免区） |
| `<h2>` | 始终为 `text-section-title` (`15px`)，优先由 `DetailWorkspaceSection` / `DocumentSection` / `DetailRecordSection` 渲染 | 若用于紧凑列表内小分组（如轻预览 Sheet 分区），可用正文级 `font-semibold`（继承 13px） |
| `<h3>` | 默认正文级 `font-semibold text-foreground`（继承 13px）；若作为次级分组标签可用 `text-xs font-medium text-muted-foreground`（12px） | 禁止大于所在分区的 `<h2>`（15px） |

> 清理 `components/business/page.tsx` 中 `PageScaffold` 通过 `:has(>[data-slot=document-header])>[data-slot=page-header]_h1` 强行把 `h1` 改写为 `text-sm` 的选择器逻辑（若需降级，统一改为 `text-body-compact` 或按 `PageHeader variant="object-chrome"` 省略重复 `h1`）。

---

## 7. 自动化护栏规范（`scripts/check-ui-theme.mjs`）

在全仓重构完成后，`npm run lint:ui-theme` 将把本规范升级为硬性门禁：

1. **扫描范围扩展**：从仅扫描 `features/` 扩展至 **`app/`、`components/`、`features/`** 全部非测试源码与 CSS 文件。
2. **字号白名单校验**：
   - 非豁免文件只允许出现白名单字号类：`text-tiny`、`text-xs`、`text-body-compact`、`text-section-title`、`text-page-title`、`text-value-compact`、`text-value-section`、`text-value-summary`、`text-value-hero`（含响应式前缀，但仍禁 `text-[...]` 和 `text-2xs`）。
   - 明确拦截桌面非豁免区中的裸物理字号：`text-sm`、`text-base`、`text-lg`、`text-xl`、`text-2xl`、`text-3xl`、`text-4xl`–`text-9xl`（支持为 `components/ui/input.tsx` 与 `textarea.tsx` 的 `text-base` 移动端防缩放行配置精确白名单豁免）。
3. **表格与列定义专项校验**：任何 `*column*` / `*table*` 文件（豁免 `paper-document.tsx`）禁止出现 `text-sm`。
4. **令牌注册一致性**：保持 `app/globals.css` 自定义 `--text-*` 与 `lib/theme-tokens.ts` 的 `themeTextNames` 严格双向一致（自动跳过 Tailwind 内置字号名如 `xs`）。
