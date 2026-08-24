"use client"

import { WorkspaceTaskCard } from "./workspace-task-card"
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
            className="flex min-h-0 flex-1 flex-col divide-y divide-border/30 overflow-auto scroll-fade-b"
            aria-label="待办列表"
        >
            {items.map((item) => (
                <li key={item.workItemId}>
                    <WorkspaceTaskCard
                        item={item}
                        selected={item.workItemId === selectedWorkItemId}
                        onSelect={onSelect}
                    />
                </li>
            ))}
        </ul>
    )
}
