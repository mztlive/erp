"use client"

import type { ReactNode } from "react"
import { InfoIcon } from "lucide-react"
import {
    Popover,
    PopoverContent,
    PopoverTrigger,
} from "@/components/ui/popover"
import { cn } from "@/lib/utils"

/** 业务口径按需查看；支持鼠标、键盘和触屏。 */
export function DetailHint({
    id,
    label,
    children,
}: {
    id: string
    label: string
    children: ReactNode
}) {
    return (
        <Popover>
            <PopoverTrigger
                id={id}
                aria-label={`${label}说明`}
                className="inline-flex size-6 shrink-0 items-center justify-center rounded-md text-muted-foreground/70 transition-colors hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
                <InfoIcon className="size-3.5" aria-hidden="true" />
            </PopoverTrigger>
            <PopoverContent
                className="max-w-[calc(100vw-2rem)] rounded-lg text-xs leading-5"
                side="bottom"
                align="start"
            >
                {children}
            </PopoverContent>
        </Popover>
    )
}

/** 分组摘要共用一块浅底，通过分隔线区分业务维度。 */
export function DetailSummary({
    children,
    label,
    className,
}: {
    children: ReactNode
    label: string
    className?: string
}) {
    return (
        <div
            aria-label={label}
            className={cn(
                "grid divide-y divide-border/70 overflow-hidden rounded-xl border border-border/70 bg-muted/35 sm:grid-cols-2 sm:divide-x sm:divide-y-0",
                className,
            )}
        >
            {children}
        </div>
    )
}

/** 主次明确的摘要项；不同单位和业务口径由调用方提供。 */
export function DetailSummaryItem({
    title,
    label,
    value,
    detail,
    hint,
}: {
    title?: ReactNode
    label: string
    value: ReactNode
    detail?: ReactNode
    hint?: ReactNode
}) {
    return (
        <div className="min-w-0 px-5 py-4">
            {title ? (
                <div className="mb-3 text-sm font-semibold">{title}</div>
            ) : null}
            <dl>
                <dt className="text-xs text-muted-foreground">{label}</dt>
                <dd className="num mt-1.5 text-2xl font-semibold tracking-tight">
                    {value}
                </dd>
            </dl>
            {detail ? (
                <div className="mt-2 flex flex-wrap items-center gap-x-2 text-xs text-muted-foreground">
                    {detail}
                    {hint}
                </div>
            ) : null}
        </div>
    )
}

/** 明细与空记录使用相同标题层级，空记录不占据完整表格高度。 */
export function DetailRecordSection({
    title,
    count,
    compact = false,
    children,
}: {
    title: string
    count?: number
    compact?: boolean
    children: ReactNode
}) {
    return (
        <section
            aria-label={title}
            className={cn(
                "py-5 first:pt-0 last:pb-0",
                compact
                    ? "flex flex-wrap items-center gap-x-5 gap-y-2"
                    : "space-y-3",
            )}
        >
            <div className="flex shrink-0 items-center gap-2">
                <h2 className="text-sm font-semibold">{title}</h2>
                {count != null && count > 0 ? (
                    <span className="num rounded-md bg-muted px-1.5 py-0.5 text-xs text-muted-foreground">
                        {count}
                    </span>
                ) : null}
            </div>
            {children}
        </section>
    )
}

/** 销售单审批摘要沿用分组浅底，仅覆盖当前摘要，不影响工作台审批组件。 */
export const salesApprovalSummaryClassName =
    "space-y-6 [&_h2]:font-semibold [&>[data-slot=card]:first-child]:rounded-xl [&>[data-slot=card]:first-child]:border [&>[data-slot=card]:first-child]:border-border/70 [&>[data-slot=card]:first-child]:bg-muted/35 [&>[data-slot=card]:first-child>[data-slot=card-header]]:px-5 [&>[data-slot=card]:first-child>[data-slot=card-content]]:px-5"
