import type * as React from "react"

import { cn } from "@/lib/utils"

/**
 * 说明列表嵌在分区里时不再自带底边。
 * DocumentSummary 与 DocumentSection 各画一条 border-b 会叠成双分割线。
 */
export const embeddedSummaryClassName = "border-0 bg-transparent py-0"

/** 正文内缩，分割线仍与页签下划线同宽。 */
export const detailSectionClassName = "px-4"

export function DetailRecordColumns({
    children,
}: {
    children: React.ReactNode
}) {
    return (
        <div className="grid gap-x-12 gap-y-4 lg:grid-cols-2">{children}</div>
    )
}

export function DetailRecordColumn({
    label,
    children,
}: {
    label: string
    children: React.ReactNode
}) {
    return (
        <div className="min-w-0">
            <div className="mb-1.5 text-xs font-medium text-muted-foreground">
                {label}
            </div>
            <div className="divide-y divide-grid">{children}</div>
        </div>
    )
}

export function DetailRecordRow({
    children,
    className,
}: {
    children: React.ReactNode
    className?: string
}) {
    return (
        <div
            className={cn(
                "flex min-h-11 flex-wrap items-center gap-x-2 gap-y-1 py-1.5 text-sm",
                className,
            )}
        >
            {children}
        </div>
    )
}

export function periodLabel(from: string, to?: string) {
    return to ? `${from} ~ ${to}` : `${from} 起`
}
