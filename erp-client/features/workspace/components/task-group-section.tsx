"use client"

import { WorkspaceTaskList } from "@/features/workspace/components/workspace-task-list"
import type { WorkspaceWorkItem } from "@/features/workspace/types"

/**
 * 旧分组列表已并入唯一左列。保留导出以免既有测试导入失败。
 */
export function TaskGroupSection({
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
