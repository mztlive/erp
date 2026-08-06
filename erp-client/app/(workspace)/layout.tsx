import { Suspense } from "react"

import { WorkspaceShell } from "@/components/layout/workspace-shell"

/**
 * 工作区壳：Suspense + WorkspaceShell。
 *
 * 业务数据经各 feature 的 api.ts 走真实 HTTP（@/lib/api）。
 * 鉴权：当前 layout 不做登录门禁；接登录后应先校验会话（TanStack Query 查
 * /account/profile 或等价接口），未登录跳独立登录路由，并清掉 demoRole 等演示参数。
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
