import type { Metadata } from "next"
import { Suspense } from "react"

import { PageScaffold } from "@/components/business"
import { WorkspaceHomePage as WorkspaceHome } from "@/features/workspace/pages/workspace-home-page"

export const metadata: Metadata = {
    title: "我的工作台",
}

function WorkspaceHomeFallback() {
    return (
        <PageScaffold className="min-h-0">
            <div className="flex flex-col gap-2">
                <div className="h-8 w-40 animate-pulse rounded-lg bg-muted" />
                <div className="h-4 w-64 max-w-full animate-pulse rounded-lg bg-muted" />
            </div>
            <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-border">
                <div className="flex flex-col gap-2 border-b border-border/30 px-3 py-2">
                    <div className="h-7 w-80 max-w-full animate-pulse rounded-lg bg-muted" />
                    <div className="h-7 w-64 max-w-full animate-pulse rounded-lg bg-muted" />
                </div>
                <div className="min-h-80 flex-1 animate-pulse bg-muted/40" />
            </div>
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
