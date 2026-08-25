"use client"

import { StatusBadge } from "@/components/ui/status-badge"
import { cn } from "@/lib/utils"

import { splitDetailSections } from "../lib/detail-facts"
import { findSourceSalesOrder } from "../lib/source-sales-order"
import { stripDocumentNumberPrefix } from "../lib/stable-number"
import { isBlockedWorkItem } from "../lib/work-item"
import type { WorkspaceWorkItem } from "../types"
import { WorkspaceDocumentBadge } from "./workspace-document-badge"

/**
 * 工作台队列行。首行单号与金额对齐扫读，次行放类型徽章与往来方。
 */
export function WorkspaceTaskCard({
    item,
    selected,
    onSelect,
}: {
    item: WorkspaceWorkItem
    selected: boolean
    onSelect: (item: WorkspaceWorkItem) => void
}) {
    const blocked = isBlockedWorkItem(item)
    const overdue = item.dueBucket === "overdue"
    const amount = splitDetailSections(
        item.summarySections,
        item.counterpartyName,
    ).amounts[0]
    const number = stripDocumentNumberPrefix(item.stableNumber)
    const sourceSales = findSourceSalesOrder(item.summarySections)
    const dueOrStatus = blocked || overdue || Boolean(item.dueAt)
    const counterpartyLine = [
        item.counterpartyName,
        sourceSales ? `来源 ${sourceSales.orderNo}` : undefined,
    ]
        .filter(Boolean)
        .join(" · ")

    return (
        <button
            type="button"
            id={`work-item-${item.stableNumber}`}
            data-testid={
                item.workItemType === "PROCUREMENT_ORDER_CREATION"
                    ? `work-item-procurement-order-creation-${item.workItemId}`
                    : undefined
            }
            aria-label={
                sourceSales
                    ? `${item.workItemTypeLabel} ${number} 来源 ${sourceSales.orderNo}`
                    : `${item.workItemTypeLabel} ${number}`
            }
            aria-current={selected ? "true" : undefined}
            onClick={() => onSelect(item)}
            className={cn(
                "relative flex w-full flex-col gap-1 px-3 py-2.5 text-left transition-colors",
                selected
                    ? "bg-muted/50 before:absolute before:inset-y-0 before:left-0 before:w-0.5 before:bg-foreground"
                    : "hover:bg-muted/40",
            )}
        >
            <span className="flex items-baseline justify-between gap-3">
                <span className="num min-w-0 truncate text-sm font-medium">
                    {number}
                </span>
                {amount ? (
                    <span className="num shrink-0 text-sm">{amount.value}</span>
                ) : null}
            </span>
            <span className="flex min-w-0 items-center justify-between gap-2">
                <span className="flex min-w-0 items-center gap-1.5">
                    <WorkspaceDocumentBadge item={item} decorative />
                    {counterpartyLine ? (
                        <span className="min-w-0 truncate text-xs text-muted-foreground">
                            {counterpartyLine}
                        </span>
                    ) : null}
                </span>
                {dueOrStatus ? (
                    blocked ? (
                        <StatusBadge label="受阻" tone="warning" />
                    ) : overdue ? (
                        <StatusBadge label="已超期" tone="destructive" />
                    ) : item.dueAt ? (
                        <time
                            dateTime={item.dueAt}
                            className="shrink-0 text-xs text-muted-foreground"
                        >
                            {item.dueAtLabel}
                        </time>
                    ) : null
                ) : null}
            </span>
        </button>
    )
}
