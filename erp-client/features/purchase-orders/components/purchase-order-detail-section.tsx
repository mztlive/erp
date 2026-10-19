"use client"

import type { ReactNode } from "react"
import { cn } from "@/lib/utils"

/** 采购详情内的业务分区：标题、说明和操作集中在头部，正文独立承载。 */
export function PurchaseOrderDetailSection({
    title,
    count,
    description,
    action,
    compact = false,
    children,
}: {
    title: string
    count?: number
    description?: ReactNode
    action?: ReactNode
    compact?: boolean
    children: ReactNode
}) {
    return (
        <section
            aria-label={title}
            className="min-w-0 rounded-xl border border-border/70 bg-card"
        >
            <div className="flex flex-wrap items-center justify-between gap-3 rounded-t-xl border-b border-border/60 bg-muted/30 px-5 py-4">
                <div className="min-w-0 space-y-1">
                    <div className="flex items-center gap-2">
                        <h2 className="text-base font-semibold">{title}</h2>
                        {count != null ? (
                            <span className="num rounded-md bg-background px-2 py-0.5 text-xs text-muted-foreground">
                                {count}
                            </span>
                        ) : null}
                    </div>
                    {description ? (
                        <div className="text-xs leading-5 text-muted-foreground">
                            {description}
                        </div>
                    ) : null}
                </div>
                {action ? (
                    <div className="flex flex-wrap items-center gap-2">
                        {action}
                    </div>
                ) : null}
            </div>
            <div
                className={cn(
                    "min-w-0 space-y-4 px-5",
                    compact ? "py-4" : "py-5",
                )}
            >
                {children}
            </div>
        </section>
    )
}
