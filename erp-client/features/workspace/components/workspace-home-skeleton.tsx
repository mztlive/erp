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
            <div className="flex min-h-0 flex-1 overflow-hidden rounded-lg border border-border">
                <div className="flex w-full flex-col gap-2 p-3 lg:w-80 xl:w-96">
                    <Skeleton className="h-7 w-56" />
                    <Skeleton className="h-8 w-full" />
                    <Skeleton className="h-8 w-32" />
                    <Skeleton className="h-14 w-full" />
                    <Skeleton className="h-14 w-full" />
                    <Skeleton className="h-14 w-full" />
                </div>
                <Skeleton className="hidden min-h-80 flex-1 rounded-none lg:block" />
            </div>
        </PageScaffold>
    )
}
