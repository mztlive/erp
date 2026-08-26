"use client"

import { DownloadIcon, PlusIcon } from "lucide-react"

import { BusinessTableFrame } from "@/components/business"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { DictionaryListTable } from "@/features/master-data/components/list/dictionary-list-table"
import { DictionaryListToolbar } from "@/features/master-data/components/list/dictionary-list-toolbar"
import { LifecycleMetricStrip } from "@/features/master-data/components/list/lifecycle-metric-strip"
import { ListPageFrame } from "@/features/master-data/components/list/list-page-frame"
import { WarehousePreviewSheet } from "@/features/master-data/components/list/warehouse-preview-sheet"
import {
    WarehouseDisableDialog,
    WarehouseReviseDialog,
} from "@/features/master-data/components/warehouse/warehouse-action-dialogs"
import { useWarehouseListColumns } from "@/features/master-data/hooks/use-dictionary-list-columns"
import { useDictionaryListState } from "@/features/master-data/hooks/use-dictionary-list-state"
import { useListPageChrome } from "@/features/master-data/hooks/use-list-page-chrome"
import {
    masterDataCopy,
    masterDataSearchPlaceholder,
} from "@/features/master-data/lib/copy"
import { hasPermission } from "@/lib/permissions"

export function WarehousesListPage() {
    const accountProfile = useAccountProfileQuery()
    const canMaintainHandlers = hasPermission(
        accountProfile.data?.permissions,
        "warehouse:update",
    )
    const { searchInputRef, resultsHeadingRef, lastFocusedRowId } =
        useListPageChrome()
    const state = useDictionaryListState({
        resource: "warehouses",
        searchInputRef,
        enablePreview: true,
    })
    const columns = useWarehouseListColumns({
        lastFocusedRowId,
        rows: state.rows,
        onPreview: state.setPreviewId,
        onReviseTarget: state.setReviseTarget,
        canMaintainHandlers,
    })
    const { filters } = state
    const hasActiveFilters =
        filters.q.trim() !== "" ||
        filters.lifecycleStatus !== "all" ||
        filters.revisionTiming !== "all"
    const listLoadFailed = state.listQuery.isError || !state.listQuery.data

    return (
        <ListPageFrame
            title={masterDataCopy.pageTitle("仓库")}
            exportMeta={state.exportMeta}
            actions={[
                {
                    actionKey: "export",
                    label: masterDataCopy.actionExport,
                    icon: DownloadIcon,
                    variant: "outline",
                    mobileVisibility: "hide",
                    disabled: state.rows.length === 0,
                    onClick: state.onExport,
                },
                {
                    actionKey: "create",
                    label: masterDataCopy.actionCreateClosed,
                    mobileVisibility: "hide",
                    icon: PlusIcon,
                    disabled: true,
                    title: masterDataCopy.warehouseWriteBody,
                },
            ]}
            metrics={
                <LifecycleMetricStrip
                    metrics={state.syncedMetrics}
                    metricKey={filters.metricKey}
                    ariaLabel="仓库指标筛选"
                    onChangeLifecycle={filters.changeLifecycle}
                />
            }
            resultsLabel={`仓库 · ${state.rows.length} 条结果`}
            resultsHeadingRef={resultsHeadingRef}
            loading={state.listQuery.isPending}
        >
            <BusinessTableFrame
                showHeader
                title={
                    <span className="inline-flex items-baseline gap-2">
                        仓库列表
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
                        searchInputRef={searchInputRef}
                        searchDraft={filters.searchDraft}
                        setSearchDraft={filters.setSearchDraft}
                        searchPlaceholder={masterDataSearchPlaceholder(
                            "warehouses",
                        )}
                        countLabel="仓库"
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
                        emptyTitle="还没有仓库资料"
                        emptyDescription="请先建立仓库资料；已有仓库可在列表中配置收发责任。"
                        onRowPreview={(row) => {
                            lastFocusedRowId.current = row.stableId
                            state.setPreviewId(row.stableId)
                        }}
                        onRowOpen={(row) => {
                            lastFocusedRowId.current = row.stableId
                            state.setPreviewId(row.stableId)
                        }}
                    />
                }
            />
            <WarehousePreviewSheet
                previewRow={state.previewRow}
                lastFocusedRowId={lastFocusedRowId}
                previewDetail={state.previewDetailQuery.data}
                previewDetailLoading={state.previewDetailQuery.isPending}
                onClose={() => state.setPreviewId(null)}
                onRevise={state.setReviseTarget}
                onDisable={state.setDisableTarget}
                canMaintainHandlers={canMaintainHandlers}
            />
            <WarehouseReviseDialog
                open={state.reviseTarget != null}
                onOpenChange={(open) => {
                    if (!open) state.setReviseTarget(null)
                }}
                target={state.reviseTarget}
            />
            <WarehouseDisableDialog
                open={state.disableTarget != null}
                onOpenChange={(open) => {
                    if (!open) state.setDisableTarget(null)
                }}
                target={state.disableTarget}
            />
        </ListPageFrame>
    )
}
