"use client"

import { DownloadIcon, PlusIcon } from "lucide-react"
import { useIsMutating } from "@tanstack/react-query"

import { Button } from "@/components/ui/button"
import { ListWorkSurface } from "@/components/business/list-workspace"
import { DictionaryCompactList } from "@/features/master-data/components/list/dictionary-compact-list"
import { DictionaryListToolbar } from "@/features/master-data/components/list/dictionary-list-toolbar"
import { LifecycleMetricStrip } from "@/features/master-data/components/list/lifecycle-metric-strip"
import { ListPageFrame } from "@/features/master-data/components/list/list-page-frame"
import { UnitOfMeasurePreviewSheet } from "@/features/master-data/components/unit-of-measure/unit-of-measure-preview-sheet"
import {
    UnitOfMeasureCreateDialog,
    UnitOfMeasureReviseDialog,
} from "@/features/master-data/components/unit-of-measure/unit-of-measure-form-dialogs"
import { UnitOfMeasureDisableDialog } from "@/features/master-data/components/shared/disable-action-dialog"
import { useDictionaryListState } from "@/features/master-data/hooks/use-dictionary-list-state"
import { useListPageChrome } from "@/features/master-data/hooks/use-list-page-chrome"
import {
    masterDataCopy,
    masterDataSearchPlaceholder,
} from "@/features/master-data/lib/copy"

export function UnitOfMeasuresListPage() {
    const { searchInputRef, resultsHeadingRef, lastFocusedRowId } =
        useListPageChrome()
    const state = useDictionaryListState({
        resource: "unit-of-measures",
        createPermission: "unit_of_measure:create",
        searchInputRef,
    })
    const exportPending =
        useIsMutating({
            predicate: (mutation) => {
                const variables = mutation.state.variables
                return (
                    typeof variables === "object" &&
                    variables !== null &&
                    "resource" in variables &&
                    variables.resource === "unit-of-measures" &&
                    !("idempotencyKey" in variables)
                )
            },
        }) > 0
    const { filters } = state
    const hasActiveFilters =
        filters.q.trim() !== "" ||
        filters.lifecycleStatus !== "all" ||
        filters.revisionTiming !== "all"
    const listLoadFailed = state.listQuery.isError || !state.listQuery.data

    return (
        <ListPageFrame
            title="计量单位"
            description="查看计量单位，供商品选择基础单位。"
            exportMeta={state.exportMeta}
            actions={[
                {
                    id: "master-data-unit-of-measures-list-export",
                    actionKey: "export",
                    label: exportPending
                        ? "导出中…"
                        : masterDataCopy.actionExport,
                    icon: DownloadIcon,
                    variant: "outline",
                    disabled: exportPending || state.rows.length === 0,
                    onClick: state.onExport,
                },
                {
                    id: "master-data-unit-of-measures-list-create",
                    actionKey: "create",
                    label: masterDataCopy.actionCreate,
                    icon: PlusIcon,
                    disabled: !state.canCreate,
                    title: !state.canCreate
                        ? state.createBlockedReason
                        : undefined,
                    onClick: () => state.setCreateOpen(true),
                },
            ]}
            resultsLabel={`计量单位 · ${state.rows.length} 条结果`}
            resultsHeadingRef={resultsHeadingRef}
            loading={state.listQuery.isPending}
        >
            <div className="w-full">
                <ListWorkSurface
                    ariaLabel="计量单位列表"
                    views={
                        <LifecycleMetricStrip
                            idPrefix="master-data-unit-of-measures-list-metrics"
                            metrics={state.syncedMetrics}
                            metricKey={filters.metricKey}
                            ariaLabel="计量单位指标筛选"
                            allLabel="全部单位"
                            hint="选择单位查看详情"
                            onChangeLifecycle={filters.changeLifecycle}
                        />
                    }
                    toolbar={
                        <DictionaryListToolbar
                            idPrefix="master-data-unit-of-measures-list-toolbar"
                            searchInputRef={searchInputRef}
                            filters={filters}
                            searchPlaceholder={masterDataSearchPlaceholder(
                                "unit-of-measures",
                            )}
                            countLabel="计量单位"
                            resultCount={
                                state.listQuery.data
                                    ? state.rows.length
                                    : undefined
                            }
                            loading={state.listQuery.isFetching}
                            failed={state.listQuery.isError}
                        />
                    }
                    table={
                        <DictionaryCompactList
                            id="master-data-unit-of-measures-list"
                            rows={state.rows}
                            codeLabel={masterDataCopy.fUnitCode}
                            selectedId={state.previewId}
                            pagination={filters.pagination}
                            onPaginationChange={filters.changePagination}
                            loading={state.listQuery.isFetching}
                            listLoadFailed={listLoadFailed}
                            error={state.listQuery.error}
                            onRetry={() => void state.listQuery.refetch()}
                            hasActiveFilters={hasActiveFilters}
                            onClearFilters={filters.clearAllFilters}
                            emptyTitle="还没有计量单位资料"
                            emptyDescription="点击「新建」创建第一份资料；历史记录会随资料保留。"
                            emptyAction={
                                state.canCreate ? (
                                    <Button
                                        id="master-data-unit-of-measures-list-empty-create"
                                        type="button"
                                        variant="secondary"
                                        size="sm"
                                        className="rounded-lg shadow-none"
                                        onClick={() =>
                                            state.setCreateOpen(true)
                                        }
                                    >
                                        {masterDataCopy.actionCreate}
                                    </Button>
                                ) : undefined
                            }
                            onPreview={(row) => {
                                lastFocusedRowId.current = row.stableId
                                state.setPreviewId(row.stableId)
                            }}
                        />
                    }
                />
            </div>
            <UnitOfMeasurePreviewSheet
                row={state.previewRow}
                lastFocusedRowId={lastFocusedRowId}
                onClose={() => state.setPreviewId(null)}
                onRevise={state.setReviseTarget}
                onDisable={state.setDisableTarget}
            />
            <UnitOfMeasureCreateDialog
                idPrefix="master-data-unit-of-measures-list-create-dialog"
                open={state.createOpen}
                onOpenChange={state.setCreateOpen}
            />
            <UnitOfMeasureReviseDialog
                idPrefix="master-data-unit-of-measures-list-revise-dialog"
                open={state.reviseTarget != null}
                onOpenChange={(open) => {
                    if (!open) state.setReviseTarget(null)
                }}
                target={state.reviseTarget}
            />
            <UnitOfMeasureDisableDialog
                open={state.disableTarget != null}
                onOpenChange={(open) => {
                    if (!open) state.setDisableTarget(null)
                }}
                target={state.disableTarget}
            />
        </ListPageFrame>
    )
}
