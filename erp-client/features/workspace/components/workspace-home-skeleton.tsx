"use client"

import {
    DetailWorkspacePage,
    DetailWorkspaceBody,
    DetailWorkspaceSection,
    PageScaffold,
} from "@/components/business"
import { Skeleton } from "@/components/ui/skeleton"

export function WorkspaceHomeSkeleton({
    detail = false,
}: {
    detail?: boolean
}) {
    if (detail)
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection>
                        <div className="flex flex-col gap-4" aria-busy="true">
                            <Skeleton className="h-12 max-w-md" />
                            <div className="grid gap-4 @[960px]/workspace:grid-cols-[minmax(0,1fr)_18rem]">
                                <Skeleton className="min-h-80" />
                                <Skeleton className="hidden @[960px]/workspace:block" />
                            </div>
                        </div>
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )

    return (
        <PageScaffold className="min-h-0 gap-2" density="compact">
            <div className="flex flex-wrap items-center justify-between gap-3">
                <Skeleton className="h-8 w-40" />
                <Skeleton className="h-7 w-64" />
            </div>
            <Skeleton className="h-8 w-72" />
            <div className="flex min-h-0 flex-1 overflow-hidden border border-border">
                <div className="flex min-w-0 flex-1 flex-col gap-2 p-3 ">
                    <Skeleton className="h-7 w-56" />
                    <Skeleton className="h-8 w-full" />
                    <Skeleton className="h-8 w-32" />
                    <Skeleton className="h-24 w-full" />
                    <Skeleton className="h-24 w-full" />
                    <Skeleton className="h-24 w-full" />
                </div>
            </div>
        </PageScaffold>
    )
}
