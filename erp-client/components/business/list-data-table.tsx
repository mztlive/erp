"use client"

import * as React from "react"

import {
    DataTable,
    type DataTableProps,
} from "@/components/business/data-table"
import {
    BusinessEmptyState,
    BusinessFailureState,
    type BusinessEmptyStateKind,
} from "@/components/business/feedback-states"
import { listWorkspaceEmptyStateClassName } from "@/components/business/list-workspace-styles"
import { Button } from "@/components/ui/button"

export type ListDataTableEmptyCopy = Readonly<{
    noScopeKind?: BusinessEmptyStateKind
    noScopeTitle?: string
    noScopeDescription?: React.ReactNode
    filterTitle?: string
    filterDescription?: React.ReactNode
    emptyTitle?: string
    emptyDescription?: React.ReactNode
}>

export type ListDataTableProps<TData> = Omit<
    DataTableProps<TData>,
    "onRetry"
> & {
    /** 业务对象名称（如「商品」「采购单」），用于生成默认三态空态文案。 */
    entityName?: string
    /** 列表查询是否失败（优先级高于空态，避免故障误报成「无结果」）。 */
    isError?: boolean
    /** 查询异常对象，透传给 BusinessFailureState 统一解析。 */
    error?: unknown
    /** 失败态重试回调。 */
    onRetry?: () => void
    /**
     * 按 list-table-design-system.md §7.2 为失败重试与空态 CTA 派生页面级 ID 前缀
     * （`${actionIdPrefix}-retry` / `${actionIdPrefix}-empty-clear-filters` / `${actionIdPrefix}-empty-create`）；
     * 未传时回退至 `idPrefix ?? id ?? "data-table"` 以兼容存量页面聚焦契约（如 `companies-table-empty-create`）。
     */
    actionIdPrefix?: string
    /** 失败重试按钮原生 id；缺省按 `${actionIdPrefix ?? idPrefix ?? id}-retry` 派生。 */
    retryButtonId?: string
    retryLabel?: string
    /** 当前账号是否无数据查看范围（`emptyReason === "no_scope"`）。 */
    noScope?: boolean
    /** 是否存在生效中的搜索或结构化筛选条件。 */
    hasActiveFilters?: boolean
    onClearFilters?: () => void
    clearFiltersButtonId?: string
    clearFiltersLabel?: string
    canCreate?: boolean
    onCreate?: () => void
    createButtonId?: string
    createLabel?: string
    /** 覆盖任意一种空态的默认标题、说明或图标类型。 */
    emptyCopy?: ListDataTableEmptyCopy
}

/**
 * 列表工作面专用 DataTable 封装：
 * - 默认 `layout="flush"`；
 * - 内置标准失败重试态（`BusinessFailureState`）与三态空态（无范围 / 筛选无结果 / 首次无数据）；
 * - 仍完整支持通过 `errorState` / `emptyState` 显式覆盖。
 */
export function ListDataTable<TData>({
    entityName,
    isError = false,
    error,
    onRetry,
    actionIdPrefix,
    retryButtonId,
    retryLabel = "重试",
    noScope = false,
    hasActiveFilters = false,
    onClearFilters,
    clearFiltersButtonId,
    clearFiltersLabel = "清除筛选",
    canCreate = false,
    onCreate,
    createButtonId,
    createLabel = "新建",
    emptyCopy,
    errorState,
    emptyState,
    emptyAction: customEmptyAction,
    layout = "flush",
    showToolbarSummary = false,
    ...tableProps
}: ListDataTableProps<TData>) {
    const baseId =
        actionIdPrefix ?? tableProps.idPrefix ?? tableProps.id ?? "data-table"
    const resolvedRetryId = retryButtonId ?? `${baseId}-retry`
    const resolvedClearId =
        clearFiltersButtonId ?? `${baseId}-empty-clear-filters`
    const resolvedCreateId = createButtonId ?? `${baseId}-empty-create`

    const resolvedErrorState =
        errorState !== undefined ? (
            errorState
        ) : isError ? (
            <BusinessFailureState
                error={error}
                action={
                    onRetry ? (
                        <Button
                            id={resolvedRetryId}
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={onRetry}
                        >
                            {retryLabel}
                        </Button>
                    ) : undefined
                }
            />
        ) : undefined

    const shouldRenderBuiltinEmpty =
        emptyState === undefined &&
        resolvedErrorState === undefined &&
        tableProps.rowCount === 0 &&
        (entityName !== undefined ||
            emptyCopy !== undefined ||
            noScope ||
            hasActiveFilters ||
            onClearFilters !== undefined ||
            onCreate !== undefined)

    let emptyAction = noScope ? undefined : customEmptyAction
    if (!noScope && hasActiveFilters && onClearFilters) {
        emptyAction = (
            <Button
                id={resolvedClearId}
                type="button"
                variant="secondary"
                size="sm"
                className="rounded-lg shadow-none"
                onClick={onClearFilters}
            >
                {clearFiltersLabel}
            </Button>
        )
    } else if (!noScope && canCreate && onCreate) {
        emptyAction = (
            <Button
                id={resolvedCreateId}
                type="button"
                variant="secondary"
                size="sm"
                className="rounded-lg shadow-none"
                onClick={onCreate}
            >
                {createLabel}
            </Button>
        )
    }

    const resolvedEmptyState =
        emptyState !== undefined ? (
            emptyState
        ) : shouldRenderBuiltinEmpty ? (
            <BusinessEmptyState
                kind={
                    noScope
                        ? (emptyCopy?.noScopeKind ?? "no-scope")
                        : hasActiveFilters
                          ? "filter"
                          : "no-data"
                }
                className={listWorkspaceEmptyStateClassName}
                title={
                    noScope
                        ? (emptyCopy?.noScopeTitle ??
                          (entityName
                              ? `暂无可查看的${entityName}`
                              : "暂无可查看的数据"))
                        : hasActiveFilters
                          ? (emptyCopy?.filterTitle ?? "当前筛选无结果")
                          : (emptyCopy?.emptyTitle ??
                            (entityName
                                ? `还没有${entityName}`
                                : "尚无业务数据"))
                }
                description={
                    noScope
                        ? (emptyCopy?.noScopeDescription ??
                          (entityName
                              ? `您当前尚未分配${entityName}的数据查看权限，请联系管理员开通后再试。`
                              : "您当前尚未被分配该模块的查看范围，如需查看请联系管理员开通。"))
                        : hasActiveFilters
                          ? (emptyCopy?.filterDescription ??
                            "没有记录符合当前筛选条件，可清除筛选后重试。")
                          : emptyCopy?.emptyDescription !== undefined
                            ? emptyCopy.emptyDescription
                            : canCreate
                              ? "点击「新建」创建第一份资料；历史记录会随资料保留。"
                              : undefined
                }
                action={emptyAction}
            />
        ) : undefined

    return (
        <DataTable
            {...tableProps}
            layout={layout}
            showToolbarSummary={showToolbarSummary}
            emptyAction={customEmptyAction}
            errorState={resolvedErrorState}
            emptyState={resolvedEmptyState}
        />
    )
}
