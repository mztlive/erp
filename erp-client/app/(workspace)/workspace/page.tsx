import type { Metadata } from "next"
import { Suspense } from "react"

import { PageScaffold } from "@/components/business"
import { WorkspaceHomePage as WorkspaceHome } from "@/features/workspace/workspace-home-page"

export const metadata: Metadata = {
  title: "今日工作台",
}

function WorkspaceHomeFallback() {
  return (
    <PageScaffold>
      <div className="h-8 w-64 animate-pulse rounded-lg bg-muted" />
      <div className="grid gap-2 sm:grid-cols-2 sm:gap-3 lg:grid-cols-4">
        {Array.from({ length: 4 }).map((_, i) => (
          <div key={i} className="h-20 animate-pulse rounded-lg bg-muted" />
        ))}
      </div>
      <div className="h-80 w-full animate-pulse rounded-lg bg-muted" />
    </PageScaffold>
  )
}

export default function WorkspaceHomePage() {
  return (
    <Suspense fallback={<WorkspaceHomeFallback />}>
      <WorkspaceHome />
    </Suspense>
  )
}
