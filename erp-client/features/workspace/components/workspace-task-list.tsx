"use client"

import { useMemo } from "react"
import { ListDataTable } from "@/components/business/list-workspace"
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
    noScope,
    hasActiveFilters,
    onClearFilters,
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
    noScope: boolean
    hasActiveFilters: boolean
    onClearFilters: () => void
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
            <ListDataTable
                id="workspace-task-table"
                layout="flush"
                data={rows}
                columns={columns}
                getRowId={(row) => row.item.workItemId}
                rowLabel={(row) =>
                    `${row.presentation.typeLabel} ${row.presentation.number} ${row.presentation.title}`
                }
                caption={
                    tracking
                        ? "已发起的审批"
                        : history
                          ? "处理记录"
                          : "待办事项"
                }
                rowCount={total}
                defaultColumnVisibility={{ source: false }}
                defaultColumnPinning={{ left: ["number"], right: ["action"] }}
                scrollable
                showToolbarSummary={false}
                pagination={{ pageIndex: page - 1, pageSize: 50 }}
                pageSizeOptions={[50]}
                onPaginationChange={(next) => onPageChange(next.pageIndex + 1)}
                onRowOpen={(row) => onSelect(row.item)}
                loading={loading}
                entityName="事项"
                noScope={noScope}
                hasActiveFilters={hasActiveFilters}
                onClearFilters={noScope ? undefined : onClearFilters}
                emptyCopy={{
                    filterTitle: "没有符合条件的事项",
                    emptyTitle: tracking
                        ? "暂无已发起的审批"
                        : history
                          ? "暂无处理记录"
                          : "当前没有待处理事项",
                    emptyDescription: tracking
                        ? "已提交的审批会显示在这里。"
                        : history
                          ? "处理过的事项会显示在这里。"
                          : "新事项到达后会显示在这里。",
                }}
            />
        </div>
    )
}
