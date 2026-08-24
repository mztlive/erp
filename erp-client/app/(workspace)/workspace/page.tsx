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
            <div className="h-8 w-40 animate-pulse rounded-lg bg-muted" />
            <div className="flex gap-8">
                <div className="h-14 w-16 animate-pulse rounded-lg bg-muted" />
                <div className="h-14 w-16 animate-pulse rounded-lg bg-muted" />
                <div className="h-14 w-16 animate-pulse rounded-lg bg-muted" />
            </div>
            <div className="flex min-h-0 flex-1 gap-8">
                <div className="flex w-full flex-col gap-2 lg:w-80">
                    <div className="h-8 w-full animate-pulse rounded-lg bg-muted" />
                    <div className="h-14 w-full animate-pulse rounded-lg bg-muted" />
                    <div className="h-14 w-full animate-pulse rounded-lg bg-muted" />
                </div>
                <div className="hidden min-h-80 flex-1 animate-pulse rounded-lg bg-muted/40 lg:block" />
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
