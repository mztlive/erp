import { Suspense } from "react"

import { WorkspaceShell } from "@/components/layout/workspace-shell"

/**
 * 工作区壳：Suspense + WorkspaceShell。
 *
 * 鉴权说明：演示阶段无登录体系（数据全部来自 mock/session-state.ts，角色经
 * `demoRole` URL 参数切换，侧栏已标「演示环境」），故此处不做登录门禁。
 * 接真实后端时：本 layout 应先校验会话（TanStack Query 查 `/me`），未登录跳
 * `/(auth)/login` 独立路由组，并清掉 `demoRole` 等演示参数。
 */
export default function WorkspaceLayout({
  children,
}: {
  children: React.ReactNode
}) {
  return (
    <Suspense fallback={<div className="min-h-svh bg-background" />}>
      <WorkspaceShell>{children}</WorkspaceShell>
    </Suspense>
  )
}
