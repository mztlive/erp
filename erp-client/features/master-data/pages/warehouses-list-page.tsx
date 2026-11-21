"use client"

import { PlusIcon } from "lucide-react"

import { BusinessFailureState, OptionCombobox } from "@/components/business"
import {
    ListDataTable,
    ListSearchField,
    ListWorkspaceFilterBar,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
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
import { REVISION_TIMING_FILTER_OPTIONS } from "@/features/master-data/lib/list-filters"
import { masterDataExportAction } from "@/features/master-data/lib/list-page-actions"
import type { MasterDataListItem } from "@/features/master-data/types"
import { hasPermission } from "@/lib/permissions"
import { dictionaryListStyles } from "./dictionary-list-styles"

const PREFIX = "master-data-warehouses-list"
const TOOLBAR = `${PREFIX}-toolbar`
const TABLE = `${PREFIX}-table`

export function WarehousesListPage() {
    const accountProfile = useAccountProfileQuery()
    const permissions = accountProfile.isSuccess
        ? accountProfile.data.permissions
        : undefined
    const canReadRevisions = hasPermission(
        permissions,
        "warehouse_revision:list",
    )
    const canMaintainHandlers = hasPermission(permissions, "warehouse:update")
    const { searchInputRef, resultsHeadingRef, lastFocusedRowId } =
        useListPageChrome()
    const state = useDictionaryListState({
        resource: "warehouses",
        searchInputRef,
        enablePreview: canReadRevisions,
        enableRevisionFilter: canReadRevisions,
        filterPermissionsReady: accountProfile.isSuccess,
    })
    const columns = useWarehouseListColumns({
        lastFocusedRowId,
        rows: state.rows,
        onPreview: state.setPreviewId,
        onReviseTarget: state.setReviseTarget,
        canMaintainHandlers,
        canReadRevisions,
    })
    const { filters, listQuery } = state
    const hasQueryFilters =
        filters.q.trim() !== "" || filters.revisionTiming !== "all"
    const hasActiveFilters =
        hasQueryFilters || filters.lifecycleStatus !== "all"
    const listLoadFailed =
        listQuery.isError || (!listQuery.isFetching && !listQuery.data)
    const noScope = listQuery.data?.emptyReason === "no_scope"
    const lastPageIndex = Math.max(
        0,
        Math.ceil(state.rows.length / filters.pagination.pageSize) - 1,
    )
    const pagination =
        filters.pagination.pageIndex > lastPageIndex
            ? { ...filters.pagination, pageIndex: lastPageIndex }
            : filters.pagination
    const pageRows =
        pagination === filters.pagination
            ? state.pageRows
            : state.rows.slice(pagination.pageIndex * pagination.pageSize)
    const openPreview = (row: MasterDataListItem) => {
        lastFocusedRowId.current = row.stableId
        state.setPreviewId(row.stableId)
    }

    return (
        <ListPageFrame
            resource="warehouses"
            eyebrow=""
            title="仓库"
            views={
                <LifecycleMetricStrip
                    idPrefix={`${PREFIX}-metrics`}
                    metrics={state.syncedMetrics}
                    metricKey={filters.metricKey}
                    ariaLabel="仓库指标筛选"
                    allLabel="全部仓库"
                    variant="inline"
                    onChangeLifecycle={filters.changeLifecycle}
                />
            }
            exportMeta={state.exportMeta}
            actions={[
                {
                    ...masterDataExportAction({
                        id: `${PREFIX}-export`,
                        pending: state.exportPending,
                        rowCount: state.rows.length,
                        onClick: state.onExport,
                    }),
                    tier: "quiet",
                },
                {
                    id: `${PREFIX}-create`,
                    actionKey: "create",
                    label: masterDataCopy.actionCreateClosed,
                    icon: PlusIcon,
                    variant: "secondary",
                    hidden: !hasPermission(permissions, "warehouse:create"),
                    disabled: true,
                    title: masterDataCopy.warehouseWriteBody,
                },
            ]}
            resultsLabel={`仓库 · ${state.rows.length} 条结果`}
            resultsHeadingRef={resultsHeadingRef}
            loading={listQuery.isPending}
            surface={{
                ariaLabel: "仓库列表",
                toolbar: (
                    <ListWorkspaceFilterBar
                        idPrefix={TOOLBAR}
                        formAriaLabel="仓库查询"
                        morePresentation="popover"
                        onSubmit={filters.applyListFilters}
                        search={
                            <ListSearchField
                                id={`${TOOLBAR}-search-input`}
                                searchInputRef={searchInputRef}
                                value={filters.searchDraft}
                                onChange={filters.setSearchDraft}
                                placeholder={masterDataSearchPlaceholder(
                                    "warehouses",
                                )}
                                aria-label="搜索仓库"
                            />
                        }
                        primaryFilters={
                            canReadRevisions ? (
                                <OptionCombobox
                                    id={`${TOOLBAR}-revision`}
                                    className="w-40 max-w-full min-w-0"
                                    filterLabel="版本"
                                    aria-label={
                                        masterDataCopy.filterVersionAria
                                    }
                                    value={
                                        filters.revisionTimingDraft === "all"
                                            ? null
                                            : filters.revisionTimingDraft
                                    }
                                    options={REVISION_TIMING_FILTER_OPTIONS}
                                    placeholder="全部"
                                    onValueChange={(value) =>
                                        filters.setRevisionTimingDraft(
                                            value === "current" ||
                                                value === "future"
                                                ? value
                                                : "all",
                                        )
                                    }
                                />
                            ) : undefined
                        }
                        extraPrimary={
                            hasQueryFilters ? (
                                <Button
                                    id={`${TOOLBAR}-clear-filters`}
                                    type="button"
                                    variant="ghost"
                                    size="sm"
                                    className="font-normal text-muted-foreground"
                                    onClick={filters.clearAllFilters}
                                >
                                    清除筛选
                                </Button>
                            ) : undefined
                        }
                        resultStatus={
                            listLoadFailed
                                ? "查询未完成"
                                : listQuery.isFetching
                                  ? "查询中…"
                                  : null
                        }
                        hasPendingChanges={filters.hasPendingChanges}
                        pendingHint="条件已修改，待查询 · 导出仍按已生效条件"
                    />
                ),
                tableClassName: dictionaryListStyles.table,
                table: (
                    <ListDataTable
                        id={TABLE}
                        entityName="仓库"
                        data={pageRows}
                        columns={columns}
                        defaultColumnOrder={[
                            "name",
                            "inboundHandler",
                            "outboundHandler",
                            "lifecycle",
                            "actions",
                            "stableNo",
                            "revisionNo",
                            "revisionTiming",
                            "period",
                            "blocker",
                        ]}
                        defaultColumnVisibility={{
                            stableNo: false,
                            revisionNo: false,
                            revisionTiming: false,
                            period: false,
                            blocker: false,
                        }}
                        defaultColumnPinning={{
                            left: ["name"],
                            right: ["actions"],
                        }}
                        getRowId={(row) => row.stableId}
                        rowLabel={(row) =>
                            row.name === row.stableNo
                                ? row.name
                                : `${row.name}（${row.stableNo}）`
                        }
                        rowCount={state.rows.length}
                        pagination={pagination}
                        onPaginationChange={filters.changePagination}
                        loading={listQuery.isFetching}
                        showToolbarSummary={false}
                        errorState={
                            listLoadFailed ? (
                                <BusinessFailureState
                                    idPrefix={PREFIX}
                                    error={listQuery.error}
                                    onRetry={() => void listQuery.refetch()}
                                />
                            ) : undefined
                        }
                        noScope={noScope}
                        hasActiveFilters={hasActiveFilters}
                        onClearFilters={
                            noScope ? undefined : filters.clearAllFilters
                        }
                        clearFiltersButtonId={`${TABLE}-clear-filters`}
                        emptyCopy={{
                            filterDescription:
                                masterDataCopy.filterEmptyDescription,
                            emptyTitle: "还没有仓库资料",
                            emptyDescription:
                                "仓库资料尚未建立，新建功能暂未开放。",
                        }}
                        highlightedRowId={state.previewRow?.stableId}
                        onRowPreview={openPreview}
                        onRowOpen={openPreview}
                    />
                ),
            }}
        >
            <WarehousePreviewSheet
                previewRow={state.previewRow}
                lastFocusedRowId={lastFocusedRowId}
                previewDetail={
                    canReadRevisions ? state.previewDetailQuery.data : undefined
                }
                previewDetailLoading={
                    canReadRevisions && state.previewDetailQuery.isPending
                }
                previewDetailFailed={
                    canReadRevisions && state.previewDetailQuery.isError
                }
                onClose={() => state.setPreviewId(null)}
                onRevise={state.setReviseTarget}
                onDisable={state.setDisableTarget}
                canMaintainHandlers={canMaintainHandlers}
                canReadRevisions={canReadRevisions}
            />
            {canMaintainHandlers ? (
                <>
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
                </>
            ) : null}
        </ListPageFrame>
    )
}
