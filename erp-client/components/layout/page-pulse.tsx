import type { ReactNode } from "react"

import { Skeleton } from "@/components/ui/skeleton"
import { cn } from "@/lib/utils"

/** 与 DetailWorkspaceSkeleton / ListWorkspacePage 对齐的纯白通栏骨架容器。 */
const PAGE_PULSE_SHELL_CLASSNAME =
    "flex min-h-full w-full flex-1 flex-col gap-page-gap-compact bg-card px-workspace-inline py-page-block md:px-page-inline md:py-page-block-lg @[70rem]/workspace:px-page-inline-lg"

/**
 * 路由 Suspense 纯白通栏骨架外壳。
 * 内层占位块仍由各页传入，避免把不同页面的占位形状收成同一种。
 */
export function PagePulseShell({
    children,
    className,
}: {
    children: ReactNode
    className?: string
}) {
    return (
        <div
            role="status"
            aria-busy="true"
            aria-label="正在加载页面"
            className={cn(PAGE_PULSE_SHELL_CLASSNAME, className)}
        >
            {children}
        </div>
    )
}

/** 一列骨架块。`bars` 保留各页传入的尺寸与占位样式并统一由 Skeleton 渲染。 */
export function PagePulseBars({
    bars,
    className,
}: {
    bars: readonly string[]
    className?: string
}) {
    return (
        <PagePulseShell className={className}>
            {bars.map((barClassName, index) => (
                <Skeleton key={index} className={barClassName} />
            ))}
        </PagePulseShell>
    )
}

/** 客户往来、供应商往来范围页共用的纯白通栏骨架占位。 */
export function ScopePagePulse() {
    return (
        <div
            role="status"
            aria-busy="true"
            aria-label="正在加载页面"
            className="flex min-h-full w-full flex-1 flex-col bg-card px-workspace-inline md:px-page-inline @[70rem]/workspace:px-page-inline-lg"
        >
            <div className="flex flex-wrap items-center justify-between gap-4 border-b border-border/60 py-3.5">
                <div className="space-y-2">
                    <Skeleton className="h-6 w-44" />
                    <Skeleton className="h-3.5 w-64 bg-muted/70" />
                </div>
                <div className="flex items-center gap-2">
                    <Skeleton className="h-control w-20" />
                    <Skeleton className="h-control w-24" />
                </div>
            </div>
            <div className="flex flex-wrap items-center gap-2 border-b border-border/60 py-workspace-toolbar-block">
                <Skeleton className="h-control w-60" />
                <Skeleton className="h-control w-28" />
                <Skeleton className="h-control w-28" />
            </div>
            <div className="flex flex-1 flex-col gap-3 py-4">
                <Skeleton className="h-table-header w-full bg-muted/75" />
                {Array.from({ length: 6 }, (_, index) => (
                    <Skeleton
                        key={index}
                        className="h-row w-full bg-muted/55"
                    />
                ))}
            </div>
        </div>
    )
}
