"use client"

import { PageScaffold, surfacePanelClassName } from "@/components/business"
import { Skeleton } from "@/components/ui/skeleton"
import { cn } from "@/lib/utils"

export function WorkspaceHomeSkeleton() {
    return (
        <PageScaffold className="min-h-0">
            <div className="flex flex-col gap-2">
                <Skeleton className="h-8 w-40" />
                <Skeleton className="h-4 w-64 max-w-full" />
            </div>
            <div
                className={cn(
                    surfacePanelClassName,
                    "flex min-h-0 flex-1 flex-col overflow-hidden",
                )}
            >
                <div className="flex flex-col gap-2 border-b border-border/30 px-3 py-2">
                    <Skeleton className="h-7 w-80 max-w-full" />
                    <Skeleton className="h-7 w-64 max-w-full" />
                </div>
                <div className="flex min-h-0 flex-1">
                    <Skeleton className="hidden h-full w-[min(24rem,38%)] lg:block" />
                    <Skeleton className="min-h-80 flex-1 rounded-none" />
                </div>
            </div>
        </PageScaffold>
    )
}
