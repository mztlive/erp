<!-- BEGIN:nextjs-agent-rules -->

# This is NOT the Next.js you know

This version has breaking changes — APIs, conventions, and file structure may all differ from your training data. Read the relevant guide in `node_modules/next/dist/docs/` before writing any code. Heed deprecation notices.
<!-- END:nextjs-agent-rules -->

# erp-client 开发规范

## 1. 本项目是纯 SPA（禁止服务端渲染）

本仓库虽基于 Next.js App Router 脚手架，但**产品形态是客户端 SPA**，不是 SSR / RSC 数据应用。

写任何代码时必须遵守：

- **不要做服务端渲染业务**：不要依赖 RSC 在服务端取数、不要用服务端组件承载业务状态。
- **不要使用 SSR / SSG 数据能力**，例如：
    - `getServerSideProps` / `getStaticProps` / `getInitialProps`
    - Server Actions 作为常规数据读写通道
    - Route Handler 以外的「服务端直接查库/调业务 API 再 hydrate」
    - `cookies()` / `headers()` 等服务端请求上下文去做页面数据
    - `export const dynamic` / `revalidate` 等缓存与渲染策略来服务业务数据
- **业务页面与交互组件优先 `"use client"`**。UI、表单、列表、弹窗、路由内业务逻辑都在客户端执行。
- **不要假设存在 Node 服务端运行时环境**（`fs`、服务端-only SDK 等）来支撑页面渲染。
- 若使用 Next 路由：页面壳可以是 Server Component，但**真正的数据请求与业务逻辑必须在 Client Component 中完成**。

一句话：**把 Next 当 SPA 壳用，数据与交互全在浏览器。**

## 2. 所有网络请求必须通过 TanStack Query

已接入 **@tanstack/react-query**（最新 v5）。全局 `QueryProvider` 挂在 `app/layout.tsx`。

### 强制规则

- **所有服务端/HTTP 数据请求必须走 TanStack Query**，包括：
    - 查询：`useQuery` / `useSuspenseQuery` / `useInfiniteQuery` / `useQueries`
    - 变更：`useMutation`
    - 缓存读写：`queryClient`（`invalidateQueries`、`setQueryData`、`prefetchQuery` 等）
- **禁止**在组件里直接 `fetch` / `axios` 后自己维护 loading、error、缓存与重试，除非该 `fetch`/`axios` **仅作为** `queryFn` / `mutationFn` 内部实现。
- **禁止**用 `useEffect` + 手动请求替代 Query。
- **禁止**在 Server Component / layout 服务端逻辑中发业务 API 请求。
- API 调用函数（纯 `async` 函数）可放在 `lib/api/` 或 feature 目录；**调用点**必须是 Query/Mutation 的 `queryFn` / `mutationFn`。

### 推荐结构

```
lib/
  query-client.ts          # QueryClient 工厂
  api/                     # 纯请求函数（无 React hooks）
components/
  providers/
    query-provider.tsx     # QueryClientProvider + Devtools
features/<domain>/
  api.ts                   # 该域的请求函数
  queries.ts               # queryKey + useXxxQuery / useXxxMutation
  components/              # 仅消费 hooks 的 UI
```

### 示例

```tsx
// features/orders/api.ts — 纯函数，可被 queryFn 调用
export async function fetchOrders(params: OrderListParams): Promise<Order[]> {
    const res = await fetch(`/api/orders?${new URLSearchParams(params)}`)
    if (!res.ok) throw new Error("Failed to fetch orders")
    return res.json()
}

// features/orders/queries.ts — 唯一对外消费入口
export const orderKeys = {
    all: ["orders"] as const,
    list: (params: OrderListParams) =>
        [...orderKeys.all, "list", params] as const,
}

export function useOrdersQuery(params: OrderListParams) {
    return useQuery({
        queryKey: orderKeys.list(params),
        queryFn: () => fetchOrders(params),
    })
}

// components — 只通过 hook 取数
function OrderList() {
    const { data, isPending, isError } = useOrdersQuery({ page: 1 })
    // ...
}
```

### 约定

- `queryKey` 使用稳定、可序列化结构；按资源分层（`all` / `list` / `detail`）。
- 写操作成功后用 `queryClient.invalidateQueries` 或乐观更新同步缓存。
- 默认 `staleTime` 等在 `lib/query-client.ts` 配置；单接口可覆盖。
- 开发环境已挂载 React Query Devtools；调试优先看缓存与请求状态，不要先加临时 `console.log`。

## 3. 所有表单必须使用 TanStack Form

已接入 **@tanstack/react-form**（最新 v1）与 **zod**（Standard Schema 校验）。
统一入口在 `components/form`（`useAppForm` / `withForm` / 预绑定 Field 组件）。

### 强制规则

- **所有业务表单必须使用 TanStack Form**，通过 `useAppForm`（或底层 `useForm`）管理状态。
- **禁止**引入或使用 `react-hook-form`、`formik`、`final-form` 等其它表单库。
- **禁止**用 `useState` / `useReducer` 手写整表字段状态、校验与提交流（单个无关紧要的 UI toggle 除外）。
- **禁止**仅用非受控原生 `<form>` + `FormData` 作为主路径承载复杂业务表单。
- 校验优先用 **Zod schema**（或其它 Standard Schema）挂到 `validators.onChange` / `onBlur` / `onSubmit`；字段级也可在 `form.AppField` 的 `validators` 上声明。
- 表单 UI 优先用项目预绑定组件（`field.TextField`、`field.TextareaField`、`form.SubmitButton` 等），与 `components/ui`（shadcn Field/Input/Button）保持一致。
- 需要新控件（Select、Checkbox、DatePicker…）时：在 `components/form/` 增加绑定 `useFieldContext` 的组件，并注册进 `createFormHook` 的 `fieldComponents` / `formComponents`。
- **提交副作用（调 API）必须走 TanStack Query 的 `useMutation`**，在 `onSubmit` 里 `mutate` / `mutateAsync`，不要在 Form 里裸 `fetch`。

### 推荐结构

```
components/form/
  form-context.ts      # createFormHookContexts
  index.ts             # createFormHook → useAppForm / withForm
  text-field.tsx       # field.TextField
  textarea-field.tsx   # field.TextareaField
  submit-button.tsx    # form.SubmitButton
features/<domain>/
  schema.ts            # Zod schema
  form.tsx             # useAppForm + UI
```

### 示例

```tsx
"use client"

import { z } from "zod"
import { useAppForm } from "@/components/form"
import { useCreateOrderMutation } from "./queries"

const schema = z.object({
    title: z.string().min(1, "请输入标题"),
    remark: z.string().optional(),
})

export function CreateOrderForm() {
    const createOrder = useCreateOrderMutation()

    const form = useAppForm({
        defaultValues: {
            title: "",
            remark: "",
        },
        validators: {
            onChange: schema,
        },
        onSubmit: async ({ value }) => {
            await createOrder.mutateAsync(value)
        },
    })

    return (
        <form
            onSubmit={(e) => {
                e.preventDefault()
                void form.handleSubmit()
            }}
        >
            <form.AppField
                name="title"
                children={(field) => <field.TextField label="标题" />}
            />
            <form.AppField
                name="remark"
                children={(field) => <field.TextareaField label="备注" />}
            />
            <form.AppForm>
                <form.SubmitButton label="创建" />
            </form.AppForm>
        </form>
    )
}
```

### 约定

- `defaultValues` 必须完整给出，且类型与 schema 的 **input** 一致。
- 错误展示使用 field `meta.errors`（预绑定组件已接 shadcn `FieldError`）。
- 可拆分的大表单用 `withForm` / `withFieldGroup`，避免 props 钻透丢失类型。
- 与 Query 协作：编辑页用 `useQuery` 取详情 → 作为 `defaultValues` 或 `form.reset`；提交用 `useMutation`。

## 4. 与 UI 栈的关系

- UI 使用项目内 shadcn / Base UI 组件（`components/ui`）。
- 数据层（TanStack Query）与展示层解耦：组件只消费 query/mutation hooks 的状态与数据。
- 表单层（TanStack Form）与展示层解耦：字段通过 `components/form` 绑定 UI，业务页只声明 schema 与 submit。
- 列表行点开的右侧窄栏预览，视觉与信息架构以公司商品池为准，见第 8 节。

## 5. 用户可见文案规则

写任何界面字符串前先查本节规则，跨页复用文案优先从 `lib/ui-text.ts` 引用。

本系统围绕内部工作流架构构建（work item、租约、投影、事实、幂等键）。这些词
**只出现在代码注释、字段名和设计文档里**，界面一律翻译成业务语言。

### 强制规则

- **禁止**把实现术语写进用户可见字符串：租约、投影、幂等键、work_item、指纹、水位、
  乐观更新、正式（作为前缀）等。替换口径见本节强制规则与 `lib/ui-text.ts`。
- **禁止**把枚举原值直接渲染：`POSTED`、`SHIPPED`、`BLOCKED`、`PENDING`…
  新增枚举时必须同时写中文映射表（如 `FORMAL_STATUS_LABEL`）。
- **禁止**把内部 ID 展示给用户：`rsv_*`、`pla_*`、`sv_*`、`wi_*`。
  换成「品名 + 数量 + 业务单号」这类用户认得的东西。
- **禁止**为了某个页面的措辞去改 `components/business` 的默认文案 —— 那会波及其它工作面。
  加可选 prop、保留原默认值（参考 `PrepaymentGate.copy`、`SequentialProcessBar.showProcess`）。
- 跨页复用的文案优先从 `lib/ui-text.ts` 引用，不要手写绕过。
- 按钮说**动作**不说机制，状态说**结果**不说锁，错误说**下一步**不说原理。

### 两条容易被忽略的界面契约

- **按钮文案必须与实际行为一致**。「确认并下一项」在关掉自动跳转后就不能再这么写。
- **URL 参数与界面控件一一对应**。被 `queryFn` 消费、却没有控件也无法清除的参数，
  是用户改不动的隐形状态：要么补控件，要么从查询里摘掉。

## 6. 改代码前自检清单

- [ ] 是否引入了任何 SSR / RSC 取数路径？若有，改为客户端 + TanStack Query。
- [ ] 是否存在裸 `fetch`/`axios`/`useEffect` 请求？若有，收进 `queryFn`/`mutationFn`。
- [ ] 新页面是否用 Client Component 承载业务？
- [ ] mutation 后是否正确失效或更新相关 queryKey？
- [ ] 新表单是否使用 `useAppForm`（TanStack Form），而非 useState/react-hook-form？
- [ ] 表单提交是否通过 `useMutation`，校验是否用 Zod / Standard Schema？
- [ ] 新增/修改的界面字符串是否符合本节规则并复用 `lib/ui-text.ts`？
- [ ] 新增枚举是否配了中文映射？内部 ID 是否漏进界面？
- [ ] 是否为了单个页面改了共享组件的默认文案（应改为加 prop）？
- [ ] 新增的 URL 查询参数是否有对应的界面控件和清除方式？
- [ ] 列表行预览是否走第 8 节的轻预览 Sheet（窄栏、浅遮罩、头/分区/脚），而不是半屏 detail 或字段表？
- [ ] identity / title / description / summary 是否各司其职？正文是否先给决策数字，而不是把列表字段再倒一遍？
- [ ] 关闭后焦点是否回到原行？主 CTA 是否是「打开对象」，而不是把表单塞进 Sheet？

## 7. 自动化 DOM id

- 所有真实可点击、可聚焦、可输入、可选择、可拖放或可键盘触发的生产 DOM 目标必须使用稳定且唯一的原生 `id`；`data-testid` 只能保留或辅助，不能替代 `id`。
- 静态 ID 使用小写 kebab-case，优先采用 `feature-surface-purpose`；重复项必须包含稳定业务键，禁止使用数组 index、随机数、时间戳或 `React.useId` 作为自动化 ID。
- 不安全的动态片段统一通过 `@/lib/automation-id` 的 `toAutomationIdSegment(value)` 清洗，不得在组件内复制清洗逻辑。
- ID 必须落在最终接收 click/focus/type 的 DOM 元素上；`render` / `asChild` 必须确认透传到最终按钮、链接或输入。
- 复合组件使用调用方提供的 `id` / `idPrefix` 派生 `-trigger`、`-clear`、`-option-<key>`、`-close`、`-remove`、分页和表格内部控件等子 ID，确保同页多实例与 portal 内容不重复。
- 修改输入 ID 时必须同步 `htmlFor`、`aria-describedby`、说明和错误节点 ID；未传新 ID 时保留原兼容行为。
- 纯 UI primitive 已完整透传 `id` 时无需改动；primitive 自行生成额外交互控件时必须提供可派生 ID 的 API。
- disabled 控件仍需 ID；路由互斥可复用概念，同一文档内同时挂载的列表、表格、对话框、抽屉和重复卡片不可重复。

## 8. 列表轻预览 Sheet（以公司商品池为基准）

列表行点开后的右侧窄栏，用来**对着表格读一张卡片**，不是半屏详情、不是 Dialog、不是对象中心。
视觉与信息架构以公司商品池为准；新的主数据/目录类点读应对齐它，而不是对齐半屏单据核对。

基准实现：

- 壳：`features/master-data/components/list/sellable-preview-sheet.tsx`
- 正文：`features/master-data/components/list/master-data-sellable-preview.tsx`

卡券类目预览（`voucher-category-preview-sheet.tsx`）已按同一套 chrome 对齐。抽成
`QuickPreviewSheet` 具名 chrome 之前，新 sheet **直接复用**商品池的
`overlayClassName` / `contentClassName`，不要再发明 480 / 500 / 520 或另一档遮罩。

### 何时用、何时不用

| 场景 | 用什么 |
| --- | --- |
| 列表里确认「这是谁、现在能不能用、关键数字是多少」，再决定要不要进对象中心 | 本节的轻预览 Sheet |
| 正式单据纸质核对 | `PaperDocument` 浮层（销售单列表已如此，不要再挂 Sheet） |
| 对照行项目、双栏或读完整主记录 | `QuickPreviewSheet size="detail"`（768px） |
| 编辑、校验、提交 | 对象中心或 Dialog + TanStack Form |
| 破坏性确认 | `FormalActionConfirmDialog` |

轻预览成立的前提：列表行投影已经够回答上面三个问题。不要为了打开 Sheet 再打一枪详情接口，
除非正文里确有列表没有的块（历史版本等），那时也只把该块做成可失败的局部状态，不要让整栏转圈。

### 壳：让列表仍在视野里

必须使用 `QuickPreviewSheet`，`size="preview"`，从右侧滑出。不要为点读再包一层裸 `Sheet`。

### 所有侧边 Sheet 的视觉契约

- `components/ui/sheet.tsx` 负责共享遮罩、头部、标题与页脚样式；`components/business/list.tsx` 的 `QuickPreviewSheet` 负责预览插槽与正文布局。业务调用方必须复用共享样式，不得复制后代选择器覆盖头、标题和页脚。
- 商品池、卡券类目、账号权限采用窄栏；其他 Sheet 按内容选择宽度。宽度差异不得改变字体层级、留白、边线和按钮风格。
- `size="detail"` 的正文由业务组件负责滚动，外缘统一 `px-7 py-6`。不得在共享壳与正文各加一次水平 padding，也不得把完整单据强制压成窄栏。
- 分区使用正文级标题、细分隔线与 24px 间距；仅对需要独立识别的业务单据、警示或输入区域使用边框容器。
- 页脚横排右对齐，空间不足时换行；保留关闭入口。右上角关闭与页脚关闭必须使用不同的稳定原生 ID。
- 高级筛选沿用同一外观，保留重置和应用操作；工作台沿用作业面内容与操作；移动端导航保留导航结构。

业务调用方只声明所需宽度，例如：

```tsx
contentClassName="data-[side=right]:sm:w-[460px] data-[side=right]:sm:max-w-[460px]"
```

原则（改 class 时先改原则，再改数字）：

- **宽度按内容选择**：轻预览采用窄栏，完整单据采用宽栏。宽度独立配置，不得通过统一宽度代替样式一致性。
- **浅遮罩、不模糊**（`bg-black/20` + 关闭 backdrop-blur）：表格还在，用户知道自己从哪一行进来。深遮罩或 blur 会把点读做成挡住整页的模态框。
- **头 / 正文 / 脚同一条竖边**：`px-7`。头额外 `pt-10`，标题才有呼吸；脚 `py-4` 横排右对齐，不要默认那一列全宽按钮。
- **标题按对象名排版**：`text-xl leading-8 font-semibold`，由共享 `SheetTitle` 提供。
- 关闭按钮（右上角 X）保留；页脚再给一个「关闭」，出口必须一眼能找到。

### 头部分层：四个插槽各司其职

`QuickPreviewSheet` 的渲染顺序是 identity → title → description → summary。商品池把它用成「先编号、后品名」的对象头，不要把四个槽塞成同一段话。

| 插槽 | 放什么 | 不放什么 |
| --- | --- | --- |
| `identity` | 稳定编号，带人类可读前缀，用 `.num`。例：`SKU 编号：A-001` | 名称、状态、价格、版本散文 |
| `title` | 用户认得的对象名（商品名、类目名） | 「商品预览」「详情」这类页面功能名；把名字藏进正文再在标题写「预览」 |
| `description` | **一条**次身份，例如规格。占位文案（「无规格」）则整个省略 | 多句说明、操作指引、「点击下方按钮…」 |
| `summary` | **一个**状态 Badge + **一句**弱化限定（商品类型、`v3`、角色数） | Badge 堆、金额、按钮、筛选项 |

状态用 `BusinessStatusBadge context="preview"` 或语义 `Badge`（如 `success` = 当前可售）。限定语用 `text-xs text-muted-foreground`，不要再做成第二个 Badge。

### 正文：叙事分区，不是字段表

轻预览的正文是短文，不是把列表列再排成 `dl`。商品池的顺序是范本：

1. **先回答这张列表存在的那个问题。** 商品池是销售价（含税）：`MoneyValue` 做成约 32px、`font-semibold`、`tracking-tight`。数量、比例走 `QuantityValue` / `RateValue`，不要手写金额格式。没有这种「一个数字」时（如卡券类目），才从资料区起笔，不要为了对称硬造 KPI。
2. **对照量压在主数字下面**，`text-xs text-muted-foreground`（市场参考价）。不要和主数字并排抢层级。
3. **分区有呼吸**：容器 `space-y-6 text-sm`；区块之间用 `border-b border-border pb-6`（末区可改 `border-t pt-6`）。不要密排 `Separator` + `grid-cols-[7rem_1fr]` 把身份 / 关键事实 / 可用性一次性倒进去——那是旧的 `MasterDataPreviewPanel`，不是这套。
4. **主分区标题用正文级** `h3` + `font-medium`（「可供区域」「商品资料」）。只有脚注区才用 `text-xs font-medium text-muted-foreground`（「当前可售期间」）。
5. **资料行当排版，不当表格**：左 `dt` 为 `text-xs text-muted-foreground`，右 `dd` 为 `text-[13px] text-right break-all` + `.num`，行用 `flex items-baseline justify-between gap-5`。长值加 `title`。
6. **支撑信息写成一句话**，不要再开一张表。例：图标 14px + 「当前由 **N** 家有效供应商支持供货」，数字用 `<strong className="num text-foreground">`。
7. **末段交代这是哪一时点的快照**（可售期间、资格核对日），`text-xs leading-5 text-muted-foreground`。不要假装 Sheet 里的数是实时锁。
8. **只留帮助决策的字段。** 名称已在 title，就不要再做「名称」行。编号可以在资料区再出现一次（方便复制），不要第三遍。内部 ID、阻断原因原文、审计字段不进轻预览。
9. **空值写出来**：`—`、`未标注区域`、`暂无描述`。禁止留空白格子。
10. 标签、数字、日期一律走本节规则和 `.num`；枚举走中文映射。

反例：为了「信息全」把仓库库存、敏感字段、修订时间线、可用性矩阵全塞进窄栏。那些属于对象中心或 `size="detail"`。

### 页脚：出路，不是工具条

- 次要：`variant="outline"` 的「关闭」。
- 主要：离开预览、进入对象中心。默认 Button，文案「打开{对象}资料」，尾随 `ArrowUpRightIcon`（`data-icon="inline-end"`）。这是「去正式页」，不是「在这里再看一遍」。
- 横排、右对齐（chrome 已声明 `flex-row justify-end`）。不要一列全宽、不要把主按钮放进正文。
- 允许额外动作（改状态、修订）仅当它是当下这一步、且**不会把 Sheet 变成表单**。真正的编辑、上传、多字段提交去对象中心或 Dialog。
- 主按钮禁用时，用外层提示说清原因（见 `DisabledActionHint`），不要静默 `disabled`。

### 交互

- 用 `DataTable` 的 `onRowPreview` 打开；同时把 `highlightedRowId` 设成当前行，表格要能看出「正在读哪一行」。
- 打开前记下 `lastFocusedRowId`；`onOpenChange(false)` 或 `onOpenChangeComplete` 后把焦点还回 `[data-row-id="…"]`。查询选择器对业务 ID 做 `CSS.escape`。
- Sheet 打开时，页面级 `/` 聚焦搜索框必须停（已有列表页都是这个口径）。
- 只读。不要在轻预览里接 `useAppForm`。

### 不要做

- 用 `size="detail"` 做主数据 / 目录点读。
- 标题写「预览」「详情」，把对象名埋进正文第一行。
- 深色遮罩或 blur，把背后的列表藏掉。
- 在窄栏里塞行项目表、纸质单据、完整时间线或筛选控件。
- 在业务页面复制或覆盖共享 padding、标题字号与页脚排列；仅为内容需要调整宽度。
- 页脚只放「查看详情」outline、没有主色「打开对象」；或反过来只有打开、没有关闭。

## 9. 列表表格工具栏

- 列设置统一放在表格上方工具栏最右侧，不得放入搜索、筛选表单或字段表头。
- `ListWorkSurface`、`BusinessTableFrame` 与独立 `DataTable` 必须复用 `TableToolbar` 的布局、按钮尺寸和窄屏换行规则。
- 左侧显示结果数量；支持勾选时由 `selectionBar` 提供已选数量、全选、清空和批量操作。右侧由 `tableActions` 提供视图切换等表格操作，列设置位于其后。
- 查询控件只放在查询工具栏。导出、新建等页面操作使用页头操作区。
- 每个列表框架使用独立 `TableToolbarScope`。同一框架包含多张表时，每张表使用自己的工具栏；禁止跨表共享列设置挂载点。
- 卡片视图不显示列设置。`showColumnVisibility={false}` 或所有字段均不可配置时不显示列设置；没有任何内容的工具栏必须收起。
- 新增或改动列表框架时，必须验证列设置唯一性、列显隐与恢复默认、查询表单隔离、多表隔离以及窄屏可用性。
