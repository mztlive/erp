"use client"

import * as React from "react"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"

import {
    BusinessFailureState,
    BusinessStatusBadge,
    TableIdentityCell,
} from "@/components/business"
import { ListDataTable } from "@/components/business/list-workspace"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import type { MasterDataListItem } from "@/features/master-data/types"
import { toAutomationIdSegment } from "@/lib/automation-id"

export function DictionaryCompactList({
    id,
    rows,
    pagination,
    onPaginationChange,
    loading,
    listLoadFailed,
    error,
    onRetry,
    hasActiveFilters,
    onClearFilters,
    emptyTitle,
    emptyDescription,
    emptyAction,
    codeLabel,
    selectedId,
    onPreview,
    renderIdentity,
    showRevision = true,
}: {
    id: string
    rows: readonly MasterDataListItem[]
    pagination: PaginationState
    onPaginationChange: (next: PaginationState) => void
    loading: boolean
    listLoadFailed: boolean
    error: unknown
    onRetry: () => void
    hasActiveFilters: boolean
    onClearFilters: () => void
    emptyTitle: string
    emptyDescription?: string
    emptyAction?: React.ReactNode
    codeLabel: string
    selectedId: string | null
    onPreview: (row: MasterDataListItem) => void
    renderIdentity?: (row: MasterDataListItem) => React.ReactNode
    showRevision?: boolean
}) {
    const pageIndex = Math.min(
        pagination.pageIndex,
        Math.max(0, Math.ceil(rows.length / pagination.pageSize) - 1),
    )
    React.useEffect(() => {
        if (!loading && !listLoadFailed && pageIndex !== pagination.pageIndex) {
            onPaginationChange({ ...pagination, pageIndex })
        }
    }, [loading, listLoadFailed, pageIndex, pagination, onPaginationChange])
    const pageRows = React.useMemo(
        () =>
            rows.slice(
                pageIndex * pagination.pageSize,
                (pageIndex + 1) * pagination.pageSize,
            ),
        [rows, pageIndex, pagination.pageSize],
    )
    const columns = React.useMemo<ColumnDef<MasterDataListItem>[]>(
        () => [
            {
                id: "name",
                header: "名称",
                accessorKey: "name",
                enableHiding: false,
                meta: { width: "flex" },
                cell: ({ row: { original: row } }) => {
                    const code = row.dictionaryCode ?? row.stableNo
                    return (
                        <div className="relative min-w-0">
                            {renderIdentity ? (
                                renderIdentity(row)
                            ) : (
                                <TableIdentityCell
                                    primary={row.name}
                                    primaryTitle={row.name}
                                    secondary={`${codeLabel}：${code}`}
                                    secondaryTitle={code}
                                    numericSecondary
                                />
                            )}
                            <button
                                id={`${id}-row-${toAutomationIdSegment(row.stableId)}-preview`}
                                data-row-id={row.stableId}
                                type="button"
                                aria-label={`查看${row.name}，${codeLabel}：${code}，${row.lifecycleStatusLabel}`}
                                aria-haspopup="dialog"
                                aria-expanded={selectedId === row.stableId}
                                onClick={() => onPreview(row)}
                                className="absolute inset-0 w-full cursor-pointer rounded-sm focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-primary"
                            >
                                <span aria-hidden="true" className="sr-only">
                                    {row.lifecycleStatusLabel}
                                </span>
                            </button>
                        </div>
                    )
                },
            },
            {
                id: "lifecycle",
                header: "当前状态",
                meta: { width: "status" },
                cell: ({ row }) => (
                    <BusinessStatusBadge
                        context="list"
                        label={row.original.lifecycleStatusLabel}
                        tone={row.original.lifecycleTone}
                        appearance={
                            row.original.lifecycleTone === "success"
                                ? "quiet"
                                : "badge"
                        }
                    />
                ),
            },
            ...(showRevision
                ? [
                      {
                          id: "revision",
                          header: "版本状态",
                          accessorKey: "revisionTimingLabel",
                          meta: { width: "status" as const },
                      },
                  ]
                : []),
        ],
        [codeLabel, id, onPreview, renderIdentity, selectedId, showRevision],
    )

    return (
        <ListDataTable
            id={id}
            data={pageRows}
            columns={columns}
            getRowId={(row) => row.stableId}
            rowLabel={(row) =>
                `${row.name}（${row.dictionaryCode ?? row.stableNo}）`
            }
            defaultColumnVisibility={{ revision: false }}
            defaultColumnPinning={{ left: ["name"] }}
            rowCount={rows.length}
            pagination={{ ...pagination, pageIndex }}
            onPaginationChange={onPaginationChange}
            highlightedRowId={selectedId ?? undefined}
            onRowPreview={onPreview}
            loading={loading}
            errorState={
                listLoadFailed ? (
                    <BusinessFailureState
                        idPrefix={id}
                        error={error}
                        onRetry={onRetry}
                    />
                ) : undefined
            }
            hasActiveFilters={hasActiveFilters}
            onClearFilters={onClearFilters}
            clearFiltersButtonId={`${id}-clear-filters`}
            emptyCopy={{
                filterDescription: masterDataCopy.filterEmptyDescription,
                emptyTitle,
                emptyDescription,
            }}
            emptyAction={emptyAction}
        />
    )
}
