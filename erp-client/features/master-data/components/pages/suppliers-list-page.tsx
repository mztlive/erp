"use client"

import { useRouter } from "next/navigation"
import { DownloadIcon, PlusIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessTableFrame,
    DataTable,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { LifecycleMetricStrip } from "@/features/master-data/components/list/lifecycle-metric-strip"
import { ListPageFrame } from "@/features/master-data/components/list/list-page-frame"
import { SupplierListToolbar } from "@/features/master-data/components/list/supplier-list-toolbar"
import { SupplierDisableDialog } from "@/features/master-data/components/shared/disable-action-dialog"
import { useListPageChrome } from "@/features/master-data/hooks/use-list-page-chrome"
import { useSupplierListColumns } from "@/features/master-data/hooks/use-supplier-list-columns"
import { useSupplierListState } from "@/features/master-data/hooks/use-supplier-list-state"
import { masterDataCopy } from "@/features/master-data/lib/copy"

export function SuppliersListPage() {
    const router = useRouter()
    const { searchInputRef, resultsHeadingRef, lastFocusedRowId } =
        useListPageChrome()
    const state = useSupplierListState(searchInputRef)
    const { filters } = state
    const openDetail = (stableId: string) => {
        lastFocusedRowId.current = stableId
        router.push(`/master-data/suppliers/${stableId}?section=overview`)
    }
    const columns = useSupplierListColumns({
        lastFocusedRowId,
        rows: state.rows,
        onOpen: (item) => openDetail(item.stableId),
        onDisableTarget: state.setDisableTarget,
    })
    const hasActiveFilters =
        filters.q.trim() !== "" || filters.hasStructuredSupplierFilters
    const listLoadFailed = state.listQuery.isError || !state.listQuery.data

    return (
        <ListPageFrame
            title={masterDataCopy.pageTitle("供应商与资质")}
            currentLabel="供应商与资质"
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
                    label: masterDataCopy.actionCreate,
                    mobileVisibility: "hide",
                    icon: PlusIcon,
                    disabled: !state.canCreate,
                    title: !state.canCreate
                        ? state.createBlockedReason
                        : undefined,
                    onClick: () => router.push("/master-data/suppliers/new"),
                },
            ]}
            metrics={
                <LifecycleMetricStrip
                    metrics={state.syncedMetrics}
                    metricKey={filters.metricKey}
                    ariaLabel="供应商与资质指标"
                    interactive={false}
                />
            }
            resultsLabel={`供应商与资质 · ${state.rows.length} 条结果`}
            resultsHeadingRef={resultsHeadingRef}
            loading={state.listQuery.isPending}
        >
            <BusinessTableFrame
                title="供应商与资质列表"
                description={masterDataCopy.supplierListDescription(
                    state.rows.length,
                )}
                toolbar={
                    <SupplierListToolbar
                        searchInputRef={searchInputRef}
                        searchDraft={filters.searchDraft}
                        setSearchDraft={filters.setSearchDraft}
                        rowCount={state.rows.length}
                        hasActiveFilters={hasActiveFilters}
                        clearAllFilters={filters.clearAllFilters}
                        supplierFilterPanelOpen={
                            filters.supplierFilterPanelOpen
                        }
                        setSupplierFilterPanelOpen={
                            filters.setSupplierFilterPanelOpen
                        }
                        hasStructuredSupplierFilters={
                            filters.hasStructuredSupplierFilters
                        }
                        applySupplierFilters={filters.applySupplierFilters}
                        lifecycleStatusDraft={filters.lifecycleStatusDraft}
                        setLifecycleStatusDraft={
                            filters.setLifecycleStatusDraft
                        }
                        supplierQualificationHealthDraft={
                            filters.supplierQualificationHealthDraft
                        }
                        setSupplierQualificationHealthDraft={
                            filters.setSupplierQualificationHealthDraft
                        }
                        supplierCapabilityCodesDraft={
                            filters.supplierCapabilityCodesDraft
                        }
                        setSupplierCapabilityCodesDraft={
                            filters.setSupplierCapabilityCodesDraft
                        }
                        supplierQualificationTypesDraft={
                            filters.supplierQualificationTypesDraft
                        }
                        setSupplierQualificationTypesDraft={
                            filters.setSupplierQualificationTypesDraft
                        }
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
                        loading={state.listQuery.isFetching}
                        layout="flush"
                        density="compact"
                        defaultColumnPinning={{
                            left: ["name"],
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
                                            : "还没有供应商与资质资料"
                                    }
                                    description={
                                        hasActiveFilters
                                            ? "没有记录符合当前筛选条件，可清除筛选后重试。"
                                            : "点击「新建」创建第一份资料；历史记录会随资料保留。"
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
                                        ) : state.canCreate ? (
                                            <Button
                                                type="button"
                                                variant="secondary"
                                                size="sm"
                                                className="rounded-lg shadow-none"
                                                onClick={() =>
                                                    router.push(
                                                        "/master-data/suppliers/new",
                                                    )
                                                }
                                            >
                                                {masterDataCopy.actionCreate}
                                            </Button>
                                        ) : undefined
                                    }
                                />
                            ) : undefined
                        }
                        onRowPreview={(row) => openDetail(row.stableId)}
                        onRowOpen={(row) => openDetail(row.stableId)}
                    />
                }
            />
            <SupplierDisableDialog
                open={state.disableTarget != null}
                onOpenChange={(open) => {
                    if (!open) state.setDisableTarget(null)
                }}
                target={state.disableTarget}
            />
        </ListPageFrame>
    )
}
