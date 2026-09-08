"use client"

import { PageScaffold } from "@/components/business"
import { Skeleton } from "@/components/ui/skeleton"

export function WorkspaceHomeSkeleton() {
    return (
        <PageScaffold className="min-h-0" density="compact">
            <div className="flex flex-wrap items-center justify-between gap-3">
                <Skeleton className="h-8 w-40" />
                <Skeleton className="h-7 w-64" />
            </div>
            <Skeleton className="h-8 w-72" />
            <div className="flex items-center gap-4">
                <Skeleton className="h-9 w-80" />
                <Skeleton className="h-9 w-32" />
            </div>
            <div className="flex min-h-0 flex-1 overflow-hidden border border-border">
                <div className="flex min-w-0 flex-1 flex-col gap-2 p-3 xl:w-[420px] xl:flex-none 2xl:w-[450px]">
                    <Skeleton className="h-7 w-56" />
                    <Skeleton className="h-8 w-full" />
                    <Skeleton className="h-8 w-32" />
                    <Skeleton className="h-24 w-full" />
                    <Skeleton className="h-24 w-full" />
                    <Skeleton className="h-24 w-full" />
                </div>
                <Skeleton className="hidden min-h-80 min-w-0 flex-1 rounded-none xl:block" />
            </div>
        </PageScaffold>
    )
}
