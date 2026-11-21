"use client"

import { useMemo, type ReactNode } from "react"

import { BusinessFailureState } from "@/components/business"
import {
    ListDataTable,
    type ListDataTableProps,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"

export type LedgerTableState = {
    rowCount: number
    loading: boolean
    isError: boolean
    error: Error | null
    onRetry: () => void
    noScope: boolean
    denied: boolean
    hasActiveFilters: boolean
    onClearFilters: () => void
    emptyState?: ReactNode
}

export function LedgerDataTable<TData>({
    state,
    data,
    ...props
}: Omit<ListDataTableProps<TData>, keyof LedgerTableState | "data"> & {
    state: LedgerTableState
    data?: readonly TData[]
}) {
    const unavailable = state.denied || state.noScope
    const rows = useMemo(
        () => (unavailable || !data ? [] : [...data]),
        [data, unavailable],
    )
    return (
        <ListDataTable
            {...props}
            layout="flush"
            data={rows}
            highlightedRowId={unavailable ? undefined : props.highlightedRowId}
            onRowPreview={unavailable ? undefined : props.onRowPreview}
            onRowOpen={unavailable ? undefined : props.onRowOpen}
            enableColumnResizing={!unavailable}
            showToolbarSummary={false}
            showColumnVisibility={!unavailable}
            showPagination={
                !unavailable && !state.isError && state.rowCount > 0
            }
            loading={state.loading}
            showRefreshingBanner={!state.isError}
            rowCount={state.rowCount}
            emptyState={state.emptyState}
            isError={state.isError}
            error={state.error ?? undefined}
            noScope={state.noScope}
            hasActiveFilters={state.hasActiveFilters && !unavailable}
            onClearFilters={
                state.hasActiveFilters && !unavailable
                    ? state.onClearFilters
                    : undefined
            }
            clearFiltersButtonId="inventory-ledger-empty-clear-filters"
            emptyCopy={{
                noScopeTitle: "暂未分配可查看的仓库",
                noScopeDescription:
                    "您的账号尚未关联任何仓库查看权限，请联系管理员分配负责的仓库。",
                emptyTitle: `暂无${props.entityName ?? "库存"}记录`,
                filterTitle: "没有符合条件的库存",
                filterDescription:
                    "没有记录符合当前筛选条件。可清除筛选或切换视图。",
            }}
            errorState={
                state.denied ? (
                    <BusinessFailureState
                        kind="permission"
                        title="权限已收回"
                        description="当前账号无权查看此库存视图。"
                    />
                ) : state.isError ? (
                    <BusinessFailureState
                        id={`${props.id}-failure`}
                        title="库存台账加载失败"
                        error={state.error ?? undefined}
                        action={
                            <Button
                                id={`${props.id}-retry`}
                                type="button"
                                variant="outline"
                                size="sm"
                                onClick={state.onRetry}
                            >
                                重试
                            </Button>
                        }
                    />
                ) : undefined
            }
        />
    )
}
