import type { Metadata } from "next"
import { Suspense } from "react"

import { WorkspaceHomePage as WorkspaceHome } from "@/features/workspace/workspace-home-page"

export const metadata: Metadata = {
  title: "今日工作台",
}

function WorkspaceHomeFallback() {
  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <div className="h-8 w-64 animate-pulse rounded-2xl bg-muted" />
      <div className="h-24 w-full animate-pulse rounded-2xl bg-muted" />
      <div className="h-80 w-full animate-pulse rounded-2xl bg-muted" />
    </div>
  )
}

export default function WorkspaceHomePage() {
  return (
    <Suspense fallback={<WorkspaceHomeFallback />}>
      <WorkspaceHome />
    </Suspense>
  )
}
