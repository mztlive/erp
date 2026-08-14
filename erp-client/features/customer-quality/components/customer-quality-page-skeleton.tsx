"use client"

import { PageScaffold } from "@/components/business"
import { Skeleton } from "@/components/ui/skeleton"

const variantClasses = {
    "policy-loading": { summary: "h-24", table: "h-64" },
    "view-loading": { summary: "h-28", table: "h-72" },
} as const

export type CustomerQualityPageSkeletonVariant =
    keyof typeof variantClasses

export function CustomerQualityPageSkeleton({
    variant,
}: {
    variant: CustomerQualityPageSkeletonVariant
}) {
    const { summary, table } = variantClasses[variant]
    return (
        <PageScaffold>
            <Skeleton className="h-10 w-64 rounded-lg" />
            <Skeleton className={`${summary} w-full rounded-lg`} />
            <div className="grid grid-cols-2 gap-2 md:grid-cols-4">
                {Array.from({ length: 8 }).map((_, i) => (
                    <Skeleton key={i} className="h-20 rounded-lg" />
                ))}
            </div>
            <Skeleton className={`${table} w-full rounded-lg`} />
        </PageScaffold>
    )
}
