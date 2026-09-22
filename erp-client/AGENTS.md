<!-- BEGIN:nextjs-agent-rules -->

# This is NOT the Next.js you know

This version has breaking changes — APIs, conventions, and file structure may all differ from your training data. Read the relevant guide in `node_modules/next/dist/docs/` before writing any code. Heed deprecation notices.
<!-- END:nextjs-agent-rules -->

# erp-client 开发规范

## 1. 本项目是纯 SPA

产品形态是客户端 SPA。Next.js App Router 只做路由壳。

- `app/**/page.tsx` 只做 `metadata`、`Suspense` 和 feature 页面组件。业务 UI、表单、列表、弹窗放在 `"use client"` 的 feature 组件里。
- 数据请求与业务状态在 Client Component 中完成。不在 Server Component、layout 或 Route Handler 里查库、调业务 API 再 hydrate。
- 不用 Server Actions 做常规读写，不用 `cookies()` / `headers()` 做页面数据，不用 `export const dynamic` / `revalidate` 服务业务数据，也不用 `fs` 或服务端-only SDK 支撑页面渲染。

## 2. 网络请求走 TanStack Query 和 `@/lib/api`

`QueryProvider` 在 `app/layout.tsx`。默认 `staleTime` 等在 `lib/query-client.ts`。

- 查询用 `useQuery` / `useSuspenseQuery` / `useInfiniteQuery` / `useQueries`，变更用 `useMutation`，缓存读写用 `queryClient`。
- 请求函数放在 `features/<domain>/api.ts`（共享客户端在 `lib/api/`），内部使用 `apiGet` / `apiPost` / `apiPut` / `apiDelete` 等（`lib/api/client.ts`）。调用点只在 `queryFn` / `mutationFn`。
- hook 与 `queryKey` 放在 `features/<domain>/hooks/queries.ts`。根上的 `queries.ts` 只做再导出。`queryKey` 按资源分层（`all` / `list` / `detail`），使用稳定、可序列化结构。
- 写操作成功后 `invalidateQueries` 或乐观更新。组件只消费这些 hook。
- 不用 `useEffect` 加手动请求代替 Query，不在组件里自行维护 loading、error、缓存和重试。

## 3. 表单走 TanStack Form

入口是 `components/form` 的 `useAppForm` / `withForm` / `withFieldGroup`。已注册控件见 `components/form/index.ts`。

- 业务表单用 `useAppForm`。校验用 Zod 或其它 Standard Schema，挂在 `validators.onChange` / `onBlur` / `onSubmit`，或字段的 `validators`。
- `defaultValues` 给全，类型与 schema 的 input 一致。错误走 field `meta.errors`。大表单用 `withForm` / `withFieldGroup`。
- 编辑页用 `useQuery` 的结果做 `defaultValues` 或 `form.reset`。提交在 `onSubmit` 里 `mutate` / `mutateAsync`（第 2 节）。
- 新控件在 `components/form/` 用 `useFieldContext` 绑定，并登记进 `createFormHook`。
- 不引入 `react-hook-form`、`formik`、`final-form`。不用 `useState` / `useReducer` 手写整表字段、校验和提交流（单个无关紧要的 UI toggle 除外）。不用非受控 `<form>` + `FormData` 承载复杂业务表单。

## 4. 数值、模块边界与 UI

- 金额、数量、税率用 `lib/fixed-decimal.ts`。不用 `Number()`、`parseFloat` 或 epsilon 比较。`lint:fixed-decimal` 会检查。
- `features/*` 之间禁止循环 import。`lint:feature-cycles` 会检查。
- UI 用 `components/ui`（shadcn / Base UI）和 `components/business`。列表点读见第 7 节。

## 5. 用户可见文案

写界面字符串前先看本节。跨页复用的按钮、状态、结果反馈从 `lib/ui-text.ts` 引用；替换用词以该文件里的常量为准。

work item、租约、投影、事实、幂等键只出现在代码注释、字段名和设计文档里。

- 用户可见字符串不用实现术语：租约、投影、幂等键、work_item、指纹、水位、乐观更新、正式（作为前缀）。
- 不把枚举原值直接渲染（`POSTED`、`SHIPPED`、`BLOCKED`、`PENDING` 等）。新增枚举同时写中文映射（如 `FORMAL_STATUS_LABEL`）。
- 不展示内部 ID（`rsv_*`、`pla_*`、`sv_*`、`wi_*`）。用「品名 + 数量 + 业务单号」这类用户认得的东西。
- 不为单个页面改 `components/business` 的默认文案。加可选 prop，保留原默认值（`PrepaymentGate.copy`、`SequentialProcessBar.showProcess`）。
- 按钮说动作，状态说结果，错误说下一步。
- 按钮文案与实际行为一致。「确认并下一项」在关掉自动跳转后就不能再这么写。
- URL 参数与界面控件一一对应。被 `queryFn` 消费、却没有控件也无法清除的参数，要么补控件，要么从查询里摘掉。

## 6. 自动化 DOM id

- 所有真实可点击、可聚焦、可输入、可选择、可拖放或可键盘触发的生产 DOM 目标必须使用稳定且唯一的原生 `id`；`data-testid` 只能保留或辅助，不能替代 `id`。
- 静态 ID 使用小写 kebab-case，优先采用 `feature-surface-purpose`；重复项必须包含稳定业务键，禁止使用数组 index、随机数、时间戳或 `React.useId` 作为自动化 ID。
- 不安全的动态片段统一通过 `@/lib/automation-id` 的 `toAutomationIdSegment(value)` 清洗，不得在组件内复制清洗逻辑。
- ID 必须落在最终接收 click/focus/type 的 DOM 元素上；`render` / `asChild` 必须确认透传到最终按钮、链接或输入。
- 复合组件使用调用方提供的 `id` / `idPrefix` 派生 `-trigger`、`-clear`、`-option-<key>`、`-close`、`-remove`、分页和表格内部控件等子 ID，确保同页多实例与 portal 内容不重复。
- 修改输入 ID 时必须同步 `htmlFor`、`aria-describedby`、说明和错误节点 ID；未传新 ID 时保留原兼容行为。
- 纯 UI primitive 已完整透传 `id` 时无需改动；primitive 自行生成额外交互控件时必须提供可派生 ID 的 API。
- disabled 控件仍需 ID；路由互斥可复用概念，同一文档内同时挂载的列表、表格、对话框、抽屉和重复卡片不可重复。

## 7. 列表轻预览 Sheet

列表行点开后的右侧窄栏，用来对着表格读一张卡片。信息架构以公司商品池为准：

- 壳：`features/master-data/components/list/sellable-preview-sheet.tsx`
- 正文：`features/master-data/components/list/master-data-sellable-preview.tsx`

共享样式在 `components/ui/sheet.tsx`（遮罩、头、标题、页脚）和 `components/business/list.tsx` 的 `QuickPreviewSheet`（插槽与正文布局）。业务页面复用这两处，不复制后代选择器去覆盖头、标题、页脚、padding 或字号。

| 场景 | 用什么 |
| --- | --- |
| 列表里确认「这是谁、现在能不能用、关键数字是多少」 | `QuickPreviewSheet` `size="preview"` |
| 正式单据纸质核对 | `PaperDocument` 浮层 |
| 对照行项目、双栏或读完整主记录 | `QuickPreviewSheet` `size="detail"` |
| 编辑、校验、提交 | 对象中心或 Dialog + TanStack Form |
| 破坏性确认 | `FormalActionConfirmDialog` |

宽度和遮罩只来自 `size`（`app/globals.css` 的 `--spacing-preview` / `--spacing-detail`）。不传 `contentClassName` / `overlayClassName` 改宽度或遮罩。商品池上残留的 460px `contentClassName` 不要照抄。主数据 / 目录点读用 `preview`。

点读前列表行已经要能回答上面三个问题。不为了打开 Sheet 再请求整份详情；正文里确有列表没有的块时，只把该块做成可失败的局部状态。

遮罩、竖边距、标题字号和页脚横排由共享壳提供：浅遮罩、不模糊。`size="detail"` 的正文滚动由业务组件负责，外缘 `px-7 py-6` 只加一次。

`QuickPreviewSheet` 的顺序是 identity → title → description → summary。

| 插槽 | 放什么 |
| --- | --- |
| `identity` | 稳定编号，带人类可读前缀，用 `.num`。例：`SKU 编号：A-001` |
| `title` | 用户认得的对象名。不写「预览」「详情」 |
| `description` | 一条次身份，例如规格。占位文案（「无规格」）则省略整个插槽 |
| `summary` | 一个状态 Badge + 一句弱化限定。状态用 `BusinessStatusBadge context="preview"` 或语义 `Badge` |

正文是短文：先给这张列表要回答的那个数字（金额 `MoneyValue`，数量 `QuantityValue`，比例 `RateValue`），没有这种数字就从资料区起笔。分区用正文级标题；空值写成 `—` 或「未标注」；名称已在 title 就不再做名称行。内部 ID、审计字段、行项目表、纸质单据、完整时间线和筛选控件不进窄栏，放到对象中心或 `size="detail"`。

页脚横排右对齐：次要是「关闭」，主要是「打开{对象}资料」（`ArrowUpRightIcon`，`data-icon="inline-end"`）。右上角关闭和页脚关闭使用不同的稳定原生 id。额外动作不能把 Sheet 变成表单。主按钮禁用时用 `DisabledActionHint` 说明原因。

交互：用 `DataTable` 的 `onRowPreview` 打开，并设置 `highlightedRowId`。打开前记下 `lastFocusedRowId`，关闭后把焦点还回 `[data-row-id="…"]`，选择器对业务 ID 做 `CSS.escape`。Sheet 打开时，页面级 `/` 聚焦搜索框停掉。轻预览只读，不接 `useAppForm`。

高级筛选沿用同一外观，保留重置和应用。工作台沿用作业面内容与操作。移动端导航保留导航结构。

## 8. 列表表格工具栏

- 列设置统一放在表格上方工具栏最右侧，不得放入搜索、筛选表单或字段表头。
- `ListWorkSurface`、`BusinessTableFrame` 与独立 `DataTable` 必须复用 `TableToolbar` 的布局、按钮尺寸和窄屏换行规则。
- 左侧显示结果数量；支持勾选时由 `selectionBar` 提供已选数量、全选、清空和批量操作。右侧由 `tableActions` 提供视图切换等表格操作，列设置位于其后。
- 查询控件只放在查询工具栏。导出、新建等页面操作使用页头操作区。
- 每个列表框架使用独立 `TableToolbarScope`。同一框架包含多张表时，每张表使用自己的工具栏；禁止跨表共享列设置挂载点。
- 卡片视图不显示列设置。`showColumnVisibility={false}` 或所有字段均不可配置时不显示列设置；没有任何内容的工具栏必须收起。
- 新增或改动列表框架时，必须验证列设置唯一性、列显隐与恢复默认、查询表单隔离、多表隔离以及窄屏可用性。
- 管理端按桌面宽度验收。窄窗口保持导航可打开、页面不横向撑破，列表工具栏仍按本节换行。新建、导出、登记、调整在任何宽度都保留，不要按视口隐藏这些操作，也不要做手机只读模式。客户选品页 `app/s/[token]` 按移动端设计，不在此列。

## 9. 不写单元测试

改 `erp-client` 时不新增、不修改单元测试，包括 vitest（`*.test.ts` / `*.test.tsx`）和 `node --test`（`*.test.mts`）。用户在当次任务里明确要求写测试时再写。
