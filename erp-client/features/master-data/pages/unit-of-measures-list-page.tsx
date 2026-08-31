"use client"

import { DownloadIcon, PlusIcon } from "lucide-react"
import { useIsMutating } from "@tanstack/react-query"

import { BusinessTableFrame } from "@/components/business"
import { Button } from "@/components/ui/button"
import { DictionaryListTable } from "@/features/master-data/components/list/dictionary-list-table"
import { DictionaryListToolbar } from "@/features/master-data/components/list/dictionary-list-toolbar"
import { LifecycleMetricStrip } from "@/features/master-data/components/list/lifecycle-metric-strip"
import { ListPageFrame } from "@/features/master-data/components/list/list-page-frame"
import {
    UnitOfMeasureCreateDialog,
    UnitOfMeasureReviseDialog,
} from "@/features/master-data/components/unit-of-measure/unit-of-measure-form-dialogs"
import { UnitOfMeasureDisableDialog } from "@/features/master-data/components/shared/disable-action-dialog"
import { useUnitOfMeasureListColumns } from "@/features/master-data/hooks/use-dictionary-list-columns"
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
    const columns = useUnitOfMeasureListColumns({
        lastFocusedRowId,
        rows: state.rows,
        onDisableTarget: state.setDisableTarget,
    })
    const { filters } = state
    const hasActiveFilters =
        filters.q.trim() !== "" ||
        filters.lifecycleStatus !== "all" ||
        filters.revisionTiming !== "all"
    const listLoadFailed = state.listQuery.isError || !state.listQuery.data

    return (
        <ListPageFrame
            title={masterDataCopy.pageTitle("计量单位")}
            hint={masterDataCopy.unitListHint}
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
                    mobileVisibility: "hide",
                    disabled: exportPending || state.rows.length === 0,
                    onClick: state.onExport,
                },
                {
                    id: "master-data-unit-of-measures-list-create",
                    actionKey: "create",
                    label: masterDataCopy.actionCreate,
                    mobileVisibility: "hide",
                    icon: PlusIcon,
                    disabled: !state.canCreate,
                    title: !state.canCreate
                        ? state.createBlockedReason
                        : undefined,
                    onClick: () => state.setCreateOpen(true),
                },
            ]}
            metrics={
                <LifecycleMetricStrip
                    idPrefix="master-data-unit-of-measures-list-metrics"
                    metrics={state.syncedMetrics}
                    metricKey={filters.metricKey}
                    ariaLabel="计量单位指标筛选"
                    onChangeLifecycle={filters.changeLifecycle}
                />
            }
            resultsLabel={`计量单位 · ${state.rows.length} 条结果`}
            resultsHeadingRef={resultsHeadingRef}
            loading={state.listQuery.isPending}
        >
            <BusinessTableFrame
                showHeader
                title={
                    <span className="inline-flex items-baseline gap-2">
                        计量单位列表
                        <span
                            className="font-normal text-muted-foreground"
                            aria-live="polite"
                        >
                            {state.rows.length} 条
                        </span>
                    </span>
                }
                description={state.listTableDescription}
                toolbar={
                    <DictionaryListToolbar
                        idPrefix="master-data-unit-of-measures-list-toolbar"
                        searchInputRef={searchInputRef}
                        searchDraft={filters.searchDraft}
                        setSearchDraft={filters.setSearchDraft}
                        searchPlaceholder={masterDataSearchPlaceholder(
                            "unit-of-measures",
                        )}
                        countLabel="计量单位"
                        hasActiveFilters={hasActiveFilters}
                        clearAllFilters={filters.clearAllFilters}
                        appliedChips={filters.appliedChips}
                        removeFilter={filters.removeFilter}
                        filterPanelOpen={filters.filterPanelOpen}
                        setFilterPanelOpen={filters.setFilterPanelOpen}
                        hasStructuredListFilters={
                            filters.hasStructuredListFilters
                        }
                        applyListFilters={filters.applyListFilters}
                        resetMoreFilters={filters.resetMoreFilters}
                        lifecycleStatusDraft={filters.lifecycleStatusDraft}
                        setLifecycleStatusDraft={
                            filters.setLifecycleStatusDraft
                        }
                        revisionTimingDraft={filters.revisionTimingDraft}
                        setRevisionTimingDraft={filters.setRevisionTimingDraft}
                    />
                }
                table={
                    <DictionaryListTable
                        id="master-data-unit-of-measures-list-table"
                        rows={state.rows}
                        pageRows={state.pageRows}
                        columns={columns}
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
                                    onClick={() => state.setCreateOpen(true)}
                                >
                                    {masterDataCopy.actionCreate}
                                </Button>
                            ) : undefined
                        }
                        onRowPreview={(row) => {
                            lastFocusedRowId.current = row.stableId
                            state.setReviseTarget(row)
                        }}
                        onRowOpen={(row) => {
                            lastFocusedRowId.current = row.stableId
                            state.setReviseTarget(row)
                        }}
                    />
                }
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
