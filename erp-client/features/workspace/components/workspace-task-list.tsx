"use client"

import { useMemo } from "react"
import { DataTable } from "@/components/business/data-table"
import { useWorkspaceListMaterials } from "../hooks/use-workspace-list-materials"
import { useWorkspaceScroll } from "../hooks/use-workspace-scroll"
import type { WorkspaceWorkItem } from "../types"
import { queueTaskRow, workspaceTaskColumns } from "./workspace-task-columns"

/** 通栏表格复用列设置和键盘操作，页码及滚动位置由队列状态恢复。 */
export function WorkspaceTaskList({
    items,
    onSelect,
    tracking = false,
    history = false,
    selectedTypes,
    scrollKey,
    total,
    page,
    onPageChange,
    loading,
    emptyState,
}: {
    items: readonly WorkspaceWorkItem[]
    onSelect: (item: WorkspaceWorkItem) => void
    tracking?: boolean
    history?: boolean
    selectedTypes: readonly string[]
    scrollKey: string
    total: number
    page: number
    onPageChange: (page: number) => void
    loading: boolean
    emptyState: React.ReactNode
}) {
    const scrollRef = useWorkspaceScroll<HTMLDivElement>(
        `queue:${scrollKey}`,
        "[data-slot=table-container]",
    )
    const materials = useWorkspaceListMaterials(items)
    const rows = useMemo(
        () =>
            materials.map(({ item, materialNotice, retryMaterials }) =>
                queueTaskRow(item, materialNotice, retryMaterials),
            ),
        [materials],
    )
    const columns = useMemo(
        () =>
            workspaceTaskColumns({
                selectedTypes,
                tracking,
                history,
                onSelect,
            }),
        [selectedTypes, tracking, history, onSelect],
    )
    return (
        <div ref={scrollRef} className="flex min-h-0 min-w-0 flex-1 flex-col">
            <DataTable
                idPrefix="workspace-task-table"
                data={rows}
                columns={columns}
                getRowId={(row) => row.item.workItemId}
                rowLabel={(row) =>
                    `${row.presentation.typeLabel} ${row.presentation.number} ${row.presentation.title}`
                }
                caption="待办事项"
                rowCount={total}
                density="comfortable"
                defaultColumnVisibility={{ source: false }}
                scrollable
                enableColumnResizing={false}
                showToolbarSummary={false}
                showRefreshingBanner={false}
                pagination={{ pageIndex: page - 1, pageSize: 50 }}
                pageSizeOptions={[50]}
                onPaginationChange={(next) => onPageChange(next.pageIndex + 1)}
                onRowOpen={(row) => onSelect(row.item)}
                loading={loading}
                emptyState={emptyState}
                className="gap-0"
            />
        </div>
    )
}
