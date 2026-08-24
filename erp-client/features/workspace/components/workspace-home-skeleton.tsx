"use client"

import { PageScaffold } from "@/components/business"
import { Skeleton } from "@/components/ui/skeleton"

export function WorkspaceHomeSkeleton() {
    return (
        <PageScaffold className="min-h-0">
            <Skeleton className="h-8 w-40" />
            <div className="flex gap-8">
                <Skeleton className="h-14 w-16" />
                <Skeleton className="h-14 w-16" />
                <Skeleton className="h-14 w-16" />
            </div>
            <div className="flex min-h-0 flex-1 gap-8">
                <div className="flex w-full flex-col gap-2 lg:w-80">
                    <Skeleton className="h-8 w-full" />
                    <Skeleton className="h-14 w-full" />
                    <Skeleton className="h-14 w-full" />
                    <Skeleton className="h-14 w-full" />
                </div>
                <Skeleton className="hidden min-h-80 flex-1 lg:block" />
            </div>
        </PageScaffold>
    )
}
