"use client"

import { cn } from "@/lib/utils"

import { isBlockedWorkItem } from "../lib/work-item"
import type { WorkspaceWorkItem } from "../types"

/**
 * 工作台左列待办列表。跨领域混排，当前项有文字选中态。
 */
export function WorkspaceTaskList({
    items,
    selectedWorkItemId,
    onSelect,
}: {
    items: readonly WorkspaceWorkItem[]
    selectedWorkItemId?: string
    onSelect: (item: WorkspaceWorkItem) => void
}) {
    return (
        <ul
            className="divide-y divide-grid overflow-auto"
            aria-label="待办列表"
        >
            {items.map((item) => {
                const selected = item.workItemId === selectedWorkItemId
                const blocked = isBlockedWorkItem(item)
                return (
                    <li key={item.workItemId}>
                        <button
                            type="button"
                            id={`work-item-${item.stableNumber}`}
                            className={cn(
                                "flex min-h-11 w-full flex-col items-start gap-0.5 px-3 py-2 text-left text-sm",
                                selected && "bg-muted font-medium",
                                blocked && "border-l-2 border-destructive",
                            )}
                            aria-current={selected ? "true" : undefined}
                            onClick={() => onSelect(item)}
                        >
                            <span>
                                {item.workItemTypeLabel} {item.stableNumber}
                                {blocked ? (
                                    <span className="ml-2 text-destructive">
                                        受阻
                                    </span>
                                ) : null}
                            </span>
                            {item.listSummary || item.counterpartyName ? (
                                <span className="text-muted-foreground">
                                    {item.listSummary ?? item.counterpartyName}
                                </span>
                            ) : null}
                            {item.dueBucket === "overdue" ? (
                                <span className="text-destructive">已超期</span>
                            ) : null}
                        </button>
                    </li>
                )
            })}
        </ul>
    )
}
