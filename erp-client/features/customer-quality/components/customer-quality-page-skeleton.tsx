"use client"

import { ListWorkspacePage } from "@/components/business"
import { Skeleton } from "@/components/ui/skeleton"

const variantClasses = {
    "policy-loading": { summary: "h-24", table: "h-64" },
    "view-loading": { summary: "h-28", table: "h-72" },
} as const

export type CustomerQualityPageSkeletonVariant = keyof typeof variantClasses

export function CustomerQualityPageSkeleton({
    variant,
}: {
    variant: CustomerQualityPageSkeletonVariant
}) {
    const { summary, table } = variantClasses[variant]
    return (
        <ListWorkspacePage
            eyebrow={variant === "policy-loading" ? "分析" : undefined}
            title="客户经营质量"
        >
            <Skeleton className={`${summary} w-full rounded-lg`} />
            <div className="grid grid-cols-2 gap-2 md:grid-cols-4">
                {Array.from({ length: 8 }).map((_, i) => (
                    <Skeleton key={i} className="h-20 rounded-lg" />
                ))}
            </div>
            {variant === "view-loading" ? (
                <div className="flex min-w-0 flex-col gap-2" aria-busy="true">
                    <Skeleton className="h-control w-72 max-w-full rounded-md" />
                    <Skeleton className={`${table} w-full rounded-none`} />
                </div>
            ) : (
                <Skeleton className={`${table} w-full rounded-lg`} />
            )}
        </ListWorkspacePage>
    )
}
