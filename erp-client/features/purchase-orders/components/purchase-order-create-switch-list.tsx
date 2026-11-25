"use client"

import type { ReactNode } from "react"

import { MoneyValue } from "@/components/business"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

export type PurchaseOrderCreateSwitchItem = {
    id: string
    title: string
    description: string
    meta?: ReactNode
    amount?: string
}

export type PurchaseOrderCreateSwitchListProps = {
    items: readonly PurchaseOrderCreateSwitchItem[]
    activeId: string
    label: string
    onSelect: (id: string) => void
}

/**
 * 预览弹窗左侧的单据切换列表：纯白表面、左蓝指示条，与分单预览同源。
 */
export function PurchaseOrderCreateSwitchList({
    items,
    activeId,
    label,
    onSelect,
}: PurchaseOrderCreateSwitchListProps) {
    return (
        <nav className="flex flex-col p-3" aria-label={label}>
            <div className="mb-2 flex items-center justify-between gap-2 px-1">
                <p className="text-xs font-medium text-muted-foreground">
                    {label}
                </p>
                <span className="num rounded-sm bg-neutral-soft px-1.5 py-0.5 text-tiny font-medium text-neutral-soft-foreground">
                    {items.length}
                </span>
            </div>
            {items.length === 0 ? (
                <p className="px-2 py-6 text-center text-xs text-muted-foreground">
                    未找到匹配的销售单
                </p>
            ) : (
                <ul className="space-y-1">
                    {items.map((item) => {
                        const active = item.id === activeId
                        return (
                            <li key={item.id}>
                                <button
                                    id={`procurement-orders-create-switch-${toAutomationIdSegment(item.id)}`}
                                    type="button"
                                    aria-label={`${item.title}，${item.description}`}
                                    data-testid={`purchase-create-switch-${item.id}`}
                                    aria-current={active ? "true" : undefined}
                                    onClick={() => onSelect(item.id)}
                                    className={cn(
                                        "relative w-full cursor-pointer rounded-md border px-3 py-2 text-left transition-colors outline-none focus-visible:border-primary focus-visible:ring-3 focus-visible:ring-ring",
                                        active
                                            ? "border-info-border/80 bg-sidebar-accent/70 before:absolute before:inset-y-2 before:left-0 before:w-0.5 before:rounded-full before:bg-primary"
                                            : "border-transparent bg-card hover:bg-row-hover",
                                    )}
                                >
                                    <div
                                        className={cn(
                                            "num truncate text-body-compact",
                                            active
                                                ? "font-semibold text-sidebar-accent-foreground"
                                                : "font-medium text-foreground",
                                        )}
                                    >
                                        {item.title}
                                    </div>
                                    <p className="mt-0.5 truncate text-xs text-muted-foreground">
                                        {item.description}
                                    </p>
                                    {item.meta != null ||
                                    item.amount != null ? (
                                        <div className="mt-1.5 flex items-baseline justify-between gap-2 text-xs">
                                            <span className="num min-w-0 truncate text-muted-foreground">
                                                {item.meta}
                                            </span>
                                            {item.amount != null ? (
                                                <MoneyValue
                                                    display="exact"
                                                    value={item.amount}
                                                    className="shrink-0"
                                                />
                                            ) : null}
                                        </div>
                                    ) : null}
                                </button>
                            </li>
                        )
                    })}
                </ul>
            )}
        </nav>
    )
}
