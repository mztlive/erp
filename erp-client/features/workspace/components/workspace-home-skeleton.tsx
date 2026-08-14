"use client"

import { PageScaffold } from "@/components/business"
import { Skeleton } from "@/components/ui/skeleton"

export function WorkspaceHomeSkeleton() {
    return (
        <PageScaffold>
            <div className="space-y-2">
                <Skeleton className="h-8 w-64" />
                <Skeleton className="h-4 w-96 max-w-full" />
            </div>
            <div className="grid gap-2 sm:grid-cols-2 sm:gap-3 lg:grid-cols-4">
                {Array.from({ length: 4 }).map((_, index) => (
                    <Skeleton key={index} className="h-20 rounded-lg" />
                ))}
            </div>
            <div className="grid min-w-0 gap-3 md:gap-4 xl:grid-cols-[minmax(0,3fr)_minmax(18rem,2fr)]">
                <Skeleton className="min-h-80 w-full rounded-lg" />
                <div className="space-y-3 md:space-y-4">
                    <Skeleton className="h-40 w-full rounded-lg" />
                    <Skeleton className="h-32 w-full rounded-lg" />
                </div>
            </div>
        </PageScaffold>
    )
}
