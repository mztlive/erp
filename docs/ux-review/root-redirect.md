# UX 评审：根路由重定向页（/）

> 页面：`erp-client/app/page.tsx`（5 行，纯 `redirect("/workspace")`）
> 评审日期：2026-08-05

## 重定向行为

- `page.tsx` 为 Server Component，`redirect("/workspace")` 在服务端渲染前执行，返回 307 到 `/workspace`。
- `/workspace` 为真实存在的路由（`app/(workspace)/workspace/page.tsx`），**无重定向循环、无白屏闪烁**（重定向先于首次渲染，Suspense fallback 为纯背景色，未见闪烁）。
- 目标页选择本身合理：当前产品是单一工作台形态，`/workspace` 即统一落地页。

## 鉴权情况

- `app/(workspace)/layout.tsx` **无任何鉴权逻辑**（仅 Suspense + `WorkspaceShell`，无 session/token/角色校验）。
- 全项目无 login/auth 页面、无 `middleware.ts`、无 `app/api`（数据全部来自 `mock/session-state.ts` 会话模拟）。
- 结论：未登录用户访问 `/` 会直接被重定向进 `/workspace`，以 mock 数据正常浏览全部业务页面，登录被完全跳过。

## 问题清单

| 级别 | 问题 | 说明 |
|------|------|------|
| P1 | 登录门禁缺失，未登录可直达业务工作台 | 根路由与 workspace layout 均不校验登录态；无 login 页兜底。当前 mock 阶段可接受，但接真实后端时 `/` 应先判断登录态分流（未登录 → /login，已登录 → /workspace），否则任何未授权访问直接落入业务数据页 |
| P2 | 重定向硬编码，不感知登录态/角色 | 仅单一落地页（当前合理）；角色仅靠 URL 参数 `?demoRole=` 演示切换（`lib/demo-roles.ts`），无全局用户态。未来多角色时应按角色分流到对应首页，且登录后应清掉 `demoRole` 等演示参数 |

## 建议

1. 引入登录态时：根路由改为客户端判态（TanStack Query 查 `/me`）→ 未登录跳 `/login`，登录跳 `/workspace`，并处理 401 全局兜底。
2. 登录页应作为独立 route group（如 `app/(auth)/login/page.tsx`）新建，与 workspace 隔离。
3. 现阶段如保持演示形态，建议在 README 或界面入口明示"演示模式，无登录"，避免误导。

## 小结

重定向本身实现正确、无循环无闪烁，无 P0；主要缺口是登录/鉴权整体缺失（P1）与落地页不感知登录态/角色（P2）。
