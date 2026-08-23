"use client"

import { WorkTaskItem } from "@/components/business"
import { cn } from "@/lib/utils"

import { isBlockedWorkItem, responsiblePartyLabel } from "../lib/work-item"
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
            className="flex min-h-0 flex-1 flex-col overflow-auto"
            aria-label="待办列表"
        >
            {items.map((item) => {
                const selected = item.workItemId === selectedWorkItemId
                const blocked = isBlockedWorkItem(item)
                return (
                    <li key={item.workItemId}>
                        <WorkTaskItem
                            render={
                                <button
                                    type="button"
                                    id={`work-item-${item.stableNumber}`}
                                    aria-label={`${item.workItemTypeLabel} ${item.stableNumber}`}
                                    aria-current={selected ? "true" : undefined}
                                />
                            }
                            density="compact"
                            className={cn(
                                "w-full rounded-none border-x-0 border-t-0 text-left shadow-none",
                                selected && "bg-muted",
                                blocked && "border-l-2 border-l-destructive",
                            )}
                            taskType={item.workItemTypeLabel}
                            businessObject={item.stableNumber}
                            counterparty={item.counterpartyName}
                            contentSummary={item.listSummary}
                            enteredAt={item.enteredAtLabel}
                            enteredDateTime={item.createdAt}
                            dueAt={item.dueAtLabel}
                            dueDateTime={item.dueAt}
                            responsibleParty={responsiblePartyLabel(item)}
                            reason={item.reasonLabel}
                            impact={item.impactSummary}
                            status={
                                blocked
                                    ? { label: "受阻", tone: "warning" }
                                    : item.dueBucket === "overdue"
                                      ? {
                                            label: "已超期",
                                            tone: "destructive",
                                        }
                                      : undefined
                            }
                            onClick={() => onSelect(item)}
                        />
                    </li>
                )
            })}
        </ul>
    )
}
