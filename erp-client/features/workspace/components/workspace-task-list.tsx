"use client"

import { WorkspaceTaskCard, workspaceTaskColumns } from "./workspace-task-card"
import { workspaceTaskColumnLabels } from "../lib/task-right-content"
import { useWorkspaceScroll } from "../hooks/use-workspace-scroll"
import type { WorkspaceWorkItem } from "../types"

/**
 * 工作台左列待办列表。跨领域混排，通栏贴分割线，与作业面同一条竖线。
 */
export function WorkspaceTaskList({
    items,
    selectedWorkItemId,
    onSelect,
    tracking = false,
    workItemType,
    scrollKey = "",
}: {
    items: readonly WorkspaceWorkItem[]
    selectedWorkItemId?: string
    onSelect: (item: WorkspaceWorkItem) => void
    tracking?: boolean
    workItemType?: string
    scrollKey?: string
}) {
    const scrollRef = useWorkspaceScroll<HTMLUListElement>(`queue:${scrollKey}`)
    const columns = workspaceTaskColumnLabels(workItemType, tracking)
    return (
        <div className="@container/workspace-queue flex min-h-0 flex-1 flex-col">
            <div
                aria-hidden="true"
                className={
                    workspaceTaskColumns +
                    " shrink-0 border-b border-grid bg-muted/35 px-4 py-3 text-xs font-medium sm:px-5"
                }
            >
                <span>事项</span>
                <span>{columns.metric}</span>
                <span className="hidden @min-[600px]/workspace-queue:block">
                    {columns.status}
                </span>
                <span className="hidden @min-[600px]/workspace-queue:block" />
            </div>
            <ul
                ref={scrollRef}
                className="flex min-h-0 flex-1 flex-col divide-y divide-grid overflow-auto scroll-fade-b"
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
        </div>
    )
}
