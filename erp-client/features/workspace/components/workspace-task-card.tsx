"use client"

import { StatusBadge } from "@/components/ui/status-badge"
import { cn } from "@/lib/utils"

import { splitDetailSections } from "../lib/detail-facts"
import { isBlockedWorkItem } from "../lib/work-item"
import type { WorkspaceWorkItem } from "../types"
import { WorkspaceDocumentBadge } from "./workspace-document-badge"

/**
 * 工作台队列行。两行账本扫读：单据徽章与单号、金额；往来方与截止。
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
    const number = stripTypePrefix(item.stableNumber, item.workItemTypeLabel)
    const dueOrStatus = blocked || overdue || Boolean(item.dueAt)

    return (
        <button
            type="button"
            id={`work-item-${item.stableNumber}`}
            data-testid={
                item.workItemType === "PROCUREMENT_ORDER_CREATION"
                    ? `work-item-procurement-order-creation-${item.workItemId}`
                    : undefined
            }
            aria-label={`${item.workItemTypeLabel} ${item.stableNumber}`}
            aria-current={selected ? "true" : undefined}
            onClick={() => onSelect(item)}
            className={cn(
                "flex w-full flex-col gap-1 px-2 py-2.5 text-left transition-colors",
                selected ? "bg-muted" : "hover:bg-muted/60",
            )}
        >
            <div className="flex items-center justify-between gap-3">
                <span className="flex min-w-0 items-center gap-1.5">
                    <WorkspaceDocumentBadge item={item} decorative />
                    <span className="min-w-0 truncate font-medium">
                        {number}
                    </span>
                </span>
                {amount ? (
                    <span className="num shrink-0 text-sm">{amount.value}</span>
                ) : null}
            </div>
            {item.counterpartyName || dueOrStatus ? (
                <div className="flex items-center justify-between gap-3 text-xs text-muted-foreground">
                    <span className="min-w-0 truncate">
                        {item.counterpartyName ?? ""}
                    </span>
                    {blocked ? (
                        <StatusBadge label="受阻" tone="warning" />
                    ) : overdue ? (
                        <StatusBadge label="已超期" tone="destructive" />
                    ) : item.dueAt ? (
                        <span className="shrink-0">
                            <time dateTime={item.dueAt}>{item.dueAtLabel}</time>
                        </span>
                    ) : null}
                </div>
            ) : null}
        </button>
    )
}

/**
 * 去掉单号里与任务类型标签重复的前缀。
 *
 * # 参数
 * * `stableNumber` - 服务端单据标签，如「销售单 XS20260823114925」
 * * `typeLabel` - 任务类型标签，如「销售单审批」
 *
 * # 返回
 * 前缀重复时返回去掉前缀的单号，否则原样返回。
 */
export function stripTypePrefix(
    stableNumber: string,
    typeLabel: string,
): string {
    const number = stableNumber.trim()
    const prefix = number.split(/\s+/)[0]
    if (!prefix || prefix === number) return number
    if (!typeLabel.startsWith(prefix)) return number
    return number.slice(prefix.length).trim() || number
}
