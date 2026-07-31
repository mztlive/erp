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
  const res = await fetch(`/api/orders?${new URLSearchParams(params)}`);
  if (!res.ok) throw new Error("Failed to fetch orders");
  return res.json();
}

// features/orders/queries.ts — 唯一对外消费入口
export const orderKeys = {
  all: ["orders"] as const,
  list: (params: OrderListParams) => [...orderKeys.all, "list", params] as const,
};

export function useOrdersQuery(params: OrderListParams) {
  return useQuery({
    queryKey: orderKeys.list(params),
    queryFn: () => fetchOrders(params),
  });
}

// components — 只通过 hook 取数
function OrderList() {
  const { data, isPending, isError } = useOrdersQuery({ page: 1 });
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
"use client";

import { z } from "zod";
import { useAppForm } from "@/components/form";
import { useCreateOrderMutation } from "./queries";

const schema = z.object({
  title: z.string().min(1, "请输入标题"),
  remark: z.string().optional(),
});

export function CreateOrderForm() {
  const createOrder = useCreateOrderMutation();

  const form = useAppForm({
    defaultValues: {
      title: "",
      remark: "",
    },
    validators: {
      onChange: schema,
    },
    onSubmit: async ({ value }) => {
      await createOrder.mutateAsync(value);
    },
  });

  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        void form.handleSubmit();
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
  );
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

## 5. 改代码前自检清单

- [ ] 是否引入了任何 SSR / RSC 取数路径？若有，改为客户端 + TanStack Query。
- [ ] 是否存在裸 `fetch`/`axios`/`useEffect` 请求？若有，收进 `queryFn`/`mutationFn`。
- [ ] 新页面是否用 Client Component 承载业务？
- [ ] mutation 后是否正确失效或更新相关 queryKey？
- [ ] 新表单是否使用 `useAppForm`（TanStack Form），而非 useState/react-hook-form？
- [ ] 表单提交是否通过 `useMutation`，校验是否用 Zod / Standard Schema？
