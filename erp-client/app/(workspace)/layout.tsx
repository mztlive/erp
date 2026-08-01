import { Suspense } from "react"

import { WorkspaceShell } from "@/components/layout/workspace-shell"

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
