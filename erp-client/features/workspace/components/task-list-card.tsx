"use client"

import { WorkspaceTaskList } from "@/features/workspace/components/workspace-task-list"
import type { WorkspaceWorkItem } from "@/features/workspace/types"

/**
 * 兼容旧工作台卡片入口。正式主从列表使用 WorkspaceTaskList。
 */
export function TaskListCard({
    items,
    selectedWorkItemId,
    onSelect,
}: {
    items: readonly WorkspaceWorkItem[]
    selectedWorkItemId?: string
    onSelect: (item: WorkspaceWorkItem) => void
}) {
    return (
        <WorkspaceTaskList
            items={items}
            selectedWorkItemId={selectedWorkItemId}
            onSelect={onSelect}
        />
    )
}
