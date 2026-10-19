"use client"

import { PageScaffold } from "@/components/business"
import { Skeleton } from "@/components/ui/skeleton"

export function WorkspaceHomeSkeleton({
    detail = false,
}: {
    detail?: boolean
}) {
    return (
        <PageScaffold className="min-h-0 gap-2" density="compact">
            <div className="flex flex-wrap items-center justify-between gap-3">
                <Skeleton className="h-8 w-40" />
                <Skeleton className="h-7 w-64" />
            </div>
            <Skeleton className="h-8 w-72" />
            {detail ? (
                <div className="flex min-h-0 flex-1 flex-col gap-4">
                    <Skeleton className="h-44 w-full" />
                    <div className="grid min-h-0 flex-1 gap-4 xl:grid-cols-[minmax(0,1fr)_18rem]">
                        <Skeleton className="min-h-80 w-full" />
                        <Skeleton className="hidden xl:block" />
                    </div>
                    <Skeleton className="h-14 w-full" />
                </div>
            ) : (
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
            )}
        </PageScaffold>
    )
}
