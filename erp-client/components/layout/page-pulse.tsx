import type { ReactNode } from "react"

import { PageScaffold } from "@/components/business/page"
import { cn } from "@/lib/utils"

/** 与各路由壳原来的 max-w-shell 骨架容器同一套 class。 */
const PAGE_PULSE_SHELL_CLASSNAME =
    "mx-auto flex w-full max-w-shell flex-col gap-3 p-4 md:gap-4 md:px-6 md:py-5"

/**
 * 路由 Suspense 骨架外壳。
 * 内层脉冲块仍由各页传入，避免把不同页面的占位形状收成同一种。
 */
export function PagePulseShell({
    children,
    className,
}: {
    children: ReactNode
    className?: string
}) {
    return (
        <div className={cn(PAGE_PULSE_SHELL_CLASSNAME, className)}>
            {children}
        </div>
    )
}

/** 一列脉冲块。`bars` 保留各页原来的完整 class。 */
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
                <div key={index} className={barClassName} />
            ))}
        </PagePulseShell>
    )
}

/** 客户往来、供应商往来范围页共用的占位。 */
export function ScopePagePulse() {
    return (
        <PageScaffold>
            <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
            <div className="h-12 animate-pulse rounded-lg bg-muted" />
            <div className="h-[28rem] animate-pulse rounded-lg bg-muted" />
        </PageScaffold>
    )
}
