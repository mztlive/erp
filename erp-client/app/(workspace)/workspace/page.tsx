import type { Metadata } from "next"
import { Suspense } from "react"

import { PageScaffold } from "@/components/business"
import { WorkspaceHomePage as WorkspaceHome } from "@/features/workspace/pages/workspace-home-page"

export const metadata: Metadata = {
    title: "我的工作台",
}

function WorkspaceHomeFallback() {
    return (
        <PageScaffold className="min-h-0" density="compact">
            <div className="flex flex-wrap items-center justify-between gap-3">
                <div className="h-8 w-40 animate-pulse rounded-lg bg-muted" />
                <div className="h-7 w-64 animate-pulse rounded-lg bg-muted" />
            </div>
            <div className="flex min-h-0 flex-1 overflow-hidden rounded-lg border border-border">
                <div className="flex w-full flex-col gap-2 p-3 lg:w-80 xl:w-96">
                    <div className="h-7 w-56 animate-pulse rounded-lg bg-muted" />
                    <div className="h-8 w-full animate-pulse rounded-lg bg-muted" />
                    <div className="h-8 w-32 animate-pulse rounded-lg bg-muted" />
                    <div className="h-14 w-full animate-pulse rounded-lg bg-muted" />
                    <div className="h-14 w-full animate-pulse rounded-lg bg-muted" />
                </div>
                <div className="hidden min-h-80 flex-1 animate-pulse bg-muted/40 lg:block" />
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
