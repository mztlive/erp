"use client"

import Link from "next/link"
import { DownloadIcon, PlusIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessTableFrame,
    DataTable,
    FormalActionResult,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { DictionaryListToolbar } from "@/features/master-data/components/list/dictionary-list-toolbar"
import { LifecycleMetricStrip } from "@/features/master-data/components/list/lifecycle-metric-strip"
import { ListPageFrame } from "@/features/master-data/components/list/list-page-frame"
import { WarehousePreviewSheet } from "@/features/master-data/components/list/warehouse-preview-sheet"
import {
    MasterDataDisableDialog,
    MasterDataReviseDialog,
} from "@/features/master-data/components/shared/master-data-action-dialog"
import { useWarehouseListColumns } from "@/features/master-data/hooks/use-dictionary-list-columns"
import { useDictionaryListState } from "@/features/master-data/hooks/use-dictionary-list-state"
import { useListPageChrome } from "@/features/master-data/hooks/use-list-page-chrome"
import { masterDataCopy, masterDataSearchPlaceholder } from "@/features/master-data/lib/copy"

export function WarehousesListPage() {
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
            title={masterDataCopy.pageTitle("仓库")}
            currentLabel="仓库"
            banner={
                <FormalActionResult
                    status="blocked"
                    title={masterDataCopy.warehouseWriteTitle}
                    description={masterDataCopy.warehouseWriteBody}
                    actions={
                        <div className="flex flex-wrap gap-2">
                            <Button
                                type="button"
                                size="sm"
                                variant="outline"
                                render={
                                    <Link href="/master-data/sellable-items" />
                                }
                            >
                                去公司商品池
                            </Button>
                            <Button
                                type="button"
                                size="sm"
                                variant="outline"
                                render={<Link href="/inventory?view=balance" />}
                            >
                                打开库存台账
                            </Button>
                        </div>
                    }
                />
            }
            exportMeta={state.exportMeta}
            queriedAt={state.listQuery.data?.queriedAt}
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
                title="仓库列表"
                description={
                    state.listTableDescription ??
                    masterDataCopy.listDescription(state.rows.length)
                }
                toolbar={
                    <DictionaryListToolbar
                        searchInputRef={searchInputRef}
                        searchDraft={filters.searchDraft}
                        setSearchDraft={filters.setSearchDraft}
                        searchPlaceholder={masterDataSearchPlaceholder(
                            "warehouses",
                        )}
                        countLabel="仓库"
                        rowCount={state.rows.length}
                        hasActiveFilters={hasActiveFilters}
                        clearAllFilters={filters.clearAllFilters}
                        filterPanelOpen={filters.filterPanelOpen}
                        setFilterPanelOpen={filters.setFilterPanelOpen}
                        hasStructuredListFilters={
                            filters.hasStructuredListFilters
                        }
                        applyListFilters={filters.applyListFilters}
                        lifecycleStatusDraft={filters.lifecycleStatusDraft}
                        setLifecycleStatusDraft={
                            filters.setLifecycleStatusDraft
                        }
                        revisionTimingDraft={filters.revisionTimingDraft}
                        setRevisionTimingDraft={filters.setRevisionTimingDraft}
                    />
                }
                table={
                    <DataTable
                        data={state.pageRows}
                        columns={columns}
                        getRowId={(row) => row.stableId}
                        rowCount={state.rows.length}
                        pagination={filters.pagination}
                        onPaginationChange={filters.changePagination}
                        layout="flush"
                        density="compact"
                        defaultColumnPinning={{
                            left: ["stableNo"],
                            right: ["actions"],
                        }}
                        errorState={
                            listLoadFailed ? (
                                <BusinessFailureState
                                    error={state.listQuery.error}
                                    onRetry={() => void state.listQuery.refetch()}
                                />
                            ) : undefined
                        }
                        emptyState={
                            !listLoadFailed && state.rows.length === 0 ? (
                                <BusinessEmptyState
                                    kind={
                                        hasActiveFilters ? "filter" : "no-data"
                                    }
                                    className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                                    title={
                                        hasActiveFilters
                                            ? "当前筛选无结果"
                                            : "还没有仓库资料"
                                    }
                                    description={
                                        hasActiveFilters
                                            ? "没有记录符合当前筛选条件，可清除筛选后重试。"
                                            : "仓库资料暂不可新建，请从库存台账核对现有仓库。"
                                    }
                                    action={
                                        hasActiveFilters ? (
                                            <Button
                                                type="button"
                                                variant="secondary"
                                                size="sm"
                                                className="rounded-lg shadow-none"
                                                onClick={filters.clearAllFilters}
                                            >
                                                清除筛选
                                            </Button>
                                        ) : undefined
                                    }
                                />
                            ) : undefined
                        }
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
            />
            <MasterDataReviseDialog
                open={state.reviseTarget != null}
                onOpenChange={(open) => {
                    if (!open) state.setReviseTarget(null)
                }}
                resource="warehouses"
                target={state.reviseTarget}
            />
            <MasterDataDisableDialog
                open={state.disableTarget != null}
                onOpenChange={(open) => {
                    if (!open) state.setDisableTarget(null)
                }}
                resource="warehouses"
                target={state.disableTarget}
            />
        </ListPageFrame>
    )
}
