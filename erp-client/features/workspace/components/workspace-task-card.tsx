"use client"

import { StatusBadge } from "@/components/ui/status-badge"
import { cn } from "@/lib/utils"

import { splitDetailSections } from "../lib/detail-facts"
import { isBlockedWorkItem } from "../lib/work-item"
import type { WorkspaceWorkItem } from "../types"

/**
 * 工作台左列条目。
 *
 * 与公共 `WorkTaskItem` 分开：工作台只有「待我处理」口径，责任方恒为当前用户，
 * 摘要长串在右侧详情已完整展开，列表只保留扫读所需的单号、往来方、金额与截止。
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
    // 单号常带单据类型前缀（「销售单 XS…」），与任务类型标签重复时去掉前缀。
    const number = stripTypePrefix(item.stableNumber, item.workItemTypeLabel)

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
                "flex w-full flex-col gap-1 border-b border-border/30 px-3 py-2.5 text-left transition-colors",
                selected ? "bg-muted" : "hover:bg-muted/50",
                blocked && "border-l-2 border-l-destructive",
            )}
        >
            <div className="flex items-center gap-2">
                <span className="text-xs text-muted-foreground">
                    {item.workItemTypeLabel}
                </span>
                {blocked ? (
                    <StatusBadge label="受阻" tone="warning" />
                ) : overdue ? (
                    <StatusBadge label="已超期" tone="destructive" />
                ) : null}
            </div>
            <div className="flex items-baseline justify-between gap-3">
                <span className="min-w-0 truncate font-medium">{number}</span>
                {amount ? (
                    <span className="num shrink-0 text-sm">{amount.value}</span>
                ) : null}
            </div>
            <div className="flex items-baseline justify-between gap-3 text-xs text-muted-foreground">
                <span className="min-w-0 truncate">
                    {item.counterpartyName ?? "—"}
                </span>
                {item.dueAt && !overdue ? (
                    <span className="shrink-0">
                        <time dateTime={item.dueAt}>{item.dueAtLabel}</time>{" "}
                        截止
                    </span>
                ) : null}
            </div>
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
