import { Suspense } from "react"

import { RequireAuth } from "@/components/providers/auth-session-provider"
import { WorkspaceShell } from "@/components/layout/workspace-shell"

/**
 * 工作区壳：登录门禁 + Suspense + WorkspaceShell。
 *
 * 无 token 时 RequireAuth 跳转 /login；业务请求 401 时 AuthSessionProvider
 * 清会话并跳转登录。
 */
export default function WorkspaceLayout({
  children,
}: {
  children: React.ReactNode
}) {
  return (
    <Suspense fallback={<div className="min-h-svh bg-background" />}>
      <RequireAuth>
        <WorkspaceShell>{children}</WorkspaceShell>
      </RequireAuth>
    </Suspense>
  )
}
