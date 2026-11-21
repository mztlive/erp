"use client"

import * as React from "react"

import { PageScaffold } from "@/components/business"
import { listWorkspaceStyles } from "@/components/business/list-workspace"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { LoadingButton } from "@/components/ui/loading-button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { InventoryBalancePreview } from "@/features/inventory/components/inventory-balance-preview"
import { useInventoryColumns } from "@/features/inventory/hooks/use-inventory-columns"
import {
    useAdjustmentDetailQuery,
    useBalanceDetailQuery,
    useInventoryListQuery,
} from "@/features/inventory/hooks/queries"
import {
    MOVEMENT_TYPE_LABEL,
    type InventoryView,
} from "@/features/inventory/types"
import { mapWorkItemDto } from "@/features/work-items/types"
import { useWorkItemDetailQuery } from "@/features/work-items/queries"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { hasPermission } from "@/lib/permissions"
import { AdjustmentDetailSheet } from "./components/adjustment-detail-sheet"
import { AdjustmentDialog } from "./components/adjustment-dialog"
import { AdjustmentEditDialog } from "./components/adjustment-edit-dialog"
import { AdjustmentResultBanner } from "./components/adjustment-result-banner"
import { ExportJobProgress } from "./components/export-job-progress"
import { InventoryLedgerLoading } from "./components/ledger-gate-states"
import { LedgerHeader } from "./components/ledger-header"
import { LedgerMetricStrip } from "./components/ledger-metric-strip"
import { LedgerTableFrame } from "./components/ledger-table-frame"
import { useAdjustmentWorkflow } from "./hooks/use-adjustment-workflow"
import { useInventoryExportJob } from "./hooks/use-inventory-export-job"
import { useInventoryLedgerPagination } from "./hooks/use-inventory-ledger-pagination"
import { useInventoryLedgerUrlState } from "./hooks/use-inventory-ledger-url-state"
import { useLedgerFilterActions } from "./hooks/use-ledger-filter-actions"
import type { LedgerAppliedChip } from "./hooks/use-ledger-filters"
import { useLedgerFilters } from "./hooks/use-ledger-filters"
import { useLedgerSearch } from "./hooks/use-ledger-search"
import { bindAdjustmentDecisionWorkItem } from "./lib/adjustment-work-item-binding"
import {
    closeAdjustmentPreviewPatch,
    openAdjustmentPreviewPatch,
} from "./lib/adjustment-navigation"
import { buildListQuery } from "./lib/build-list-query"

export function InventoryLedgerPage() {
    const [editingAdjustmentId, setEditingAdjustmentId] = React.useState<
        string | null
    >(null)
    const {
        canFilterPeople,
        canFilterWarehouse,
        view,
        qParam,
        warehouseId,
        skuId,
        salesOrderLineId,
        availability,
        balanceIdParam,
        adjustmentIdParam,
        workItemIdParam,
        movementType,
        occurredFrom,
        occurredTo,
        sortValue,
        pageSize,
        cursorParam,
        cursorOffset,
        operatorUserIds,
        applicantUserIds,
        handlerUserIds,
        hasActiveFilters,
        patchUrl,
    } = useInventoryLedgerUrlState()
    const profile = useAccountProfileQuery()
    const grants = profile.data?.permissions
    const canCreateAdjustment = hasPermission(grants, "stock_adjustment:create")
    const canEditAdjustment = hasPermission(grants, "stock_adjustment:update")
    const canSubmitAdjustment = hasPermission(grants, "stock_adjustment:submit")
    const canReadBalance = hasPermission(grants, "stock_balance:detail")
    const canOpenImports = hasPermission(grants, "legacy_import_batch:list")
    const canListBalances = hasPermission(grants, "stock_balance:list")
    const canListAdjustments = hasPermission(grants, "stock_adjustment:list")
    const canViewMovements = hasPermission(grants, "stock_movement:list")
    const canExport = hasPermission(grants, `stock_${view}:list`)
    const { searchDraft, setSearchDraft, searchInputRef } = useLedgerSearch({
        qParam,
    })
    const { pagination, resetPagination, handlePaginationChange } =
        useInventoryLedgerPagination({
            view,
            pageSize,
            cursorOffset,
            patchUrl,
        })
    const filters = useLedgerFilters({
        canFilterPeople,
        canFilterWarehouse,
        view,
        q: qParam,
        warehouseId,
        availability,
        movementType,
        occurredFrom,
        occurredTo,
        operatorUserIds,
        applicantUserIds,
        handlerUserIds,
        searchDraft,
        setSearchDraft,
        patchUrl,
        resetPagination,
    })
    const { handleViewChange, handleSortChange } = useLedgerFilterActions({
        patchUrl,
        resetPagination,
        sortValue,
    })

    const previewBalanceId = balanceIdParam ?? null
    const lastFocusedRow = React.useRef<{
        view: InventoryView
        id: string
    } | null>(null)

    const restoreRowFocus = React.useCallback(() => {
        const row = lastFocusedRow.current
        lastFocusedRow.current = null
        if (row?.view === view) {
            const table = document.getElementById(
                `inventory-ledger-${row.view}-table`,
            )
            const target = table?.querySelector<HTMLElement>(
                `[data-row-id="${CSS.escape(row.id)}"]`,
            )
            if (target?.tabIndex === 0) return target
            const adjustAction = document.getElementById(
                `inventory-ledger-balance-row-${toAutomationIdSegment(row.id)}-adjust`,
            )
            if (
                row.view === "balance" &&
                adjustAction instanceof HTMLButtonElement &&
                !adjustAction.disabled
            ) {
                return adjustAction
            }
        }
        return searchInputRef.current ?? true
    }, [searchInputRef, view])

    const query = React.useMemo(
        () =>
            buildListQuery({
                view,
                qParam,
                warehouseId,
                skuId,
                salesOrderLineId,
                availability,
                movementType,
                occurredFrom,
                occurredTo,
                cursorParam,
                pageSize: pagination.pageSize,
                sortValue,
                balanceIdParam,
                adjustmentIdParam,
                operatorUserIds,
                applicantUserIds,
                handlerUserIds,
                scopeVersion: undefined,
            }),
        [
            view,
            qParam,
            warehouseId,
            skuId,
            salesOrderLineId,
            availability,
            movementType,
            occurredFrom,
            occurredTo,
            cursorParam,
            pagination.pageSize,
            sortValue,
            balanceIdParam,
            adjustmentIdParam,
            operatorUserIds,
            applicantUserIds,
            handlerUserIds,
        ],
    )

    const listQuery = useInventoryListQuery(query, !profile.isPending)
    const [hasResolvedList, setHasResolvedList] = React.useState(false)
    React.useEffect(() => {
        if (!listQuery.isPending) setHasResolvedList(true)
    }, [listQuery.isPending])

    // 深链标签只展示业务名称，不显示内部 ID。
    const allViewRows = React.useMemo(
        () => [
            ...(listQuery.data?.balances ?? []),
            ...(listQuery.data?.movements ?? []),
            ...(listQuery.data?.reservations ?? []),
            ...(listQuery.data?.adjustments ?? []),
        ],
        [listQuery.data],
    )
    const workItemQuery = useWorkItemDetailQuery(
        profile.isPending ? "" : (workItemIdParam ?? ""),
    )
    const workItem = workItemQuery.data
        ? mapWorkItemDto(workItemQuery.data)
        : undefined
    const workItemAdjustmentId =
        workItem?.businessObjectType === "stock_adjustment"
            ? workItem.businessObjectId
            : undefined
    const previewAdjustmentId =
        adjustmentIdParam ?? workItemAdjustmentId ?? null
    const detailQuery = useBalanceDetailQuery(
        profile.isPending ? null : previewBalanceId,
    )
    const adjustmentDetailQuery = useAdjustmentDetailQuery(
        profile.isPending ? null : previewAdjustmentId,
    )
    const decisionWorkItem = bindAdjustmentDecisionWorkItem(
        workItem,
        previewAdjustmentId,
        adjustmentDetailQuery.data?.approval?.instance,
    )

    const { exportJob, startExport, closeExport, isExporting } =
        useInventoryExportJob()

    const handleFocusRestore = React.useCallback((balanceId: string) => {
        lastFocusedRow.current = { view: "balance", id: balanceId }
    }, [])

    const closeDetail = React.useCallback(() => {
        patchUrl({ balanceId: null })
    }, [patchUrl])
    const handlePreviewClose = React.useCallback(() => {
        if (previewBalanceId) closeDetail()
    }, [closeDetail, previewBalanceId])

    const adjustment = useAdjustmentWorkflow({
        onFocusRestore: handleFocusRestore,
        onPreviewClose: handlePreviewClose,
    })

    const openDetail = React.useCallback(
        (balanceId: string) => {
            handleFocusRestore(balanceId)
            patchUrl({ balanceId })
        },
        [handleFocusRestore, patchUrl],
    )
    const openAdjustment = React.useCallback(
        (adjustmentId: string) => {
            if (view === "adjustment") {
                lastFocusedRow.current = { view, id: adjustmentId }
            }
            patchUrl(openAdjustmentPreviewPatch(adjustmentId))
        },
        [patchUrl, view],
    )
    const closeAdjustment = React.useCallback(() => {
        patchUrl(closeAdjustmentPreviewPatch())
    }, [patchUrl])

    const {
        adjustmentColumns,
        balanceColumns,
        movementColumns,
        reservationColumns,
    } = useInventoryColumns({
        openDetail: canReadBalance ? openDetail : undefined,
        startAdjustment:
            canCreateAdjustment && canSubmitAdjustment
                ? adjustment.startAdjustment
                : undefined,
        isCreating: adjustment.isCreating,
    })

    const data = listQuery.data

    const chipSkuName = allViewRows.find(
        (r) => r.skuId === skuId && r.skuName !== r.skuId,
    )?.skuName
    const chipSalesLineLabel = data?.reservations.find(
        (r) =>
            r.salesOrderLineId === salesOrderLineId &&
            r.salesOrderLineLabel !== r.salesOrderLineId,
    )?.salesOrderLineLabel
    const chipAdjustmentNo = (data?.adjustments ?? []).find(
        (a) => a.adjustmentId === adjustmentIdParam,
    )?.adjustmentNo

    const appliedChips = React.useMemo<readonly LedgerAppliedChip[]>(() => {
        const chips: LedgerAppliedChip[] = []
        if (view === "movement" && movementType.length > 0) {
            chips.push({
                key: "movementType",
                label: `流水类型：${movementType
                    .map((type) => MOVEMENT_TYPE_LABEL[type] ?? "未知流水类型")
                    .join("、")}`,
            })
        }
        if (view === "movement" && (occurredFrom || occurredTo)) {
            chips.push({
                key: "occurredRange",
                label: `发生日期：${occurredFrom ?? "不限"} 至 ${
                    occurredTo ?? "不限"
                }`,
            })
        }
        if (skuId) {
            chips.push({
                key: "skuId",
                label: `当前 SKU：${chipSkuName ?? "已定位单品"}`,
            })
        }
        if (view === "reservation" && salesOrderLineId) {
            chips.push({
                key: "salesOrderLineId",
                label: `销售单明细：${chipSalesLineLabel ?? "已定位"}`,
            })
        }
        if (view === "adjustment" && adjustmentIdParam) {
            chips.push({
                key: "adjustmentId",
                label: `调整单：${chipAdjustmentNo ?? "已定位"}`,
            })
        }
        if (view === "movement" && operatorUserIds) {
            chips.push({
                key: "operatorUserIds",
                label: "经办人已筛选",
            })
        }
        if (view === "adjustment" && operatorUserIds) {
            chips.push({
                key: "operatorUserIds",
                label: "经办人已筛选",
            })
        }
        if (view === "adjustment" && applicantUserIds) {
            chips.push({
                key: "applicantUserIds",
                label: "申请人已筛选",
            })
        }
        if (view === "adjustment" && handlerUserIds) {
            chips.push({
                key: "handlerUserIds",
                label: "当前审批人已筛选",
            })
        }
        return chips
    }, [
        adjustmentIdParam,
        chipAdjustmentNo,
        chipSalesLineLabel,
        chipSkuName,
        movementType,
        occurredFrom,
        occurredTo,
        salesOrderLineId,
        skuId,
        view,
        operatorUserIds,
        applicantUserIds,
        handlerUserIds,
    ])

    const listLoadFailed = listQuery.isError
    const denied = !listLoadFailed && data?.emptyReason === "PERMISSION_REVOKED"
    const noScope = !listLoadFailed && data?.emptyReason === "NO_DATA_SCOPE"
    const canUseLedger = !denied && !noScope
    const visibleData = canUseLedger ? data : undefined

    if (!hasResolvedList && listQuery.isPending) {
        return <InventoryLedgerLoading />
    }

    const detail = detailQuery.data

    return (
        <PageScaffold density="compact" className={listWorkspaceStyles.page}>
            <LedgerHeader
                view={view}
                onViewChange={handleViewChange}
                queriedAt={visibleData?.queriedAt}
                excludedKindsNote={visibleData?.excludedKindsNote}
                openingStockNote={visibleData?.openingStockNote}
                canExport={
                    canExport && canUseLedger && (data?.canExport ?? true)
                }
                listReady={
                    Boolean(data) && !listLoadFailed && !listQuery.isFetching
                }
                denied={denied}
                total={data?.total ?? 0}
                isExporting={isExporting}
                onRefresh={() => {
                    void listQuery.refetch()
                    if (previewBalanceId) void detailQuery.refetch()
                }}
                onExport={() => {
                    startExport({
                        total: data?.total ?? 0,
                        filterSummary: data?.filterSummary ?? "",
                        query: { ...query, scopeVersion: undefined },
                    })
                }}
            />

            {canUseLedger &&
            canCreateAdjustment &&
            canSubmitAdjustment &&
            adjustment.lastResult ? (
                <AdjustmentResultBanner
                    result={adjustment.lastResult}
                    isResolving={adjustment.isResolving}
                    onResolve={() => void adjustment.resolveLastUnknown()}
                />
            ) : null}

            {canUseLedger && canExport && exportJob ? (
                <ExportJobProgress job={exportJob} onClose={closeExport} />
            ) : null}

            {visibleData && !listLoadFailed ? (
                <LedgerMetricStrip
                    metrics={visibleData.metrics}
                    showBalanceMetrics={canListBalances}
                    showAdjustmentMetrics={canListAdjustments}
                />
            ) : null}

            <InventoryBalancePreview
                open={
                    canUseLedger &&
                    previewBalanceId != null &&
                    !adjustment.adjustDraftId
                }
                finalFocus={adjustment.adjustDraftId ? false : restoreRowFocus}
                detail={detail}
                isPending={detailQuery.isPending}
                isCreating={adjustment.isCreating}
                onClose={closeDetail}
                onViewMovements={
                    canViewMovements
                        ? (currentDetail) => {
                              patchUrl(
                                  {
                                      view: "movement",
                                      balanceId: null,
                                      warehouseId:
                                          currentDetail.balance.warehouseId,
                                      skuId: currentDetail.balance.skuId,
                                  },
                                  { replace: true },
                              )
                              resetPagination()
                          }
                        : undefined
                }
                onStartAdjustment={
                    canCreateAdjustment && canSubmitAdjustment
                        ? adjustment.startAdjustment
                        : undefined
                }
                onOpenAdjustment={openAdjustment}
            />

            <AdjustmentDetailSheet
                open={
                    canUseLedger &&
                    previewAdjustmentId != null &&
                    !editingAdjustmentId
                }
                finalFocus={editingAdjustmentId ? false : restoreRowFocus}
                detail={adjustmentDetailQuery.data}
                isPending={adjustmentDetailQuery.isPending}
                workItemId={decisionWorkItem?.workItemId}
                expectedTaskVersion={decisionWorkItem?.expectedTaskVersion}
                workItemAllowedActions={decisionWorkItem?.allowedActions}
                onClose={closeAdjustment}
                onEditOriginal={
                    canEditAdjustment ? setEditingAdjustmentId : undefined
                }
                onCancelled={() => {
                    void listQuery.refetch()
                    void adjustmentDetailQuery.refetch()
                    void workItemQuery.refetch()
                }}
                onDecisionApplied={() => {
                    void listQuery.refetch()
                    void adjustmentDetailQuery.refetch()
                    void workItemQuery.refetch()
                }}
            />

            {canUseLedger && canEditAdjustment && editingAdjustmentId ? (
                <AdjustmentEditDialog
                    key={editingAdjustmentId}
                    id={editingAdjustmentId}
                    canSubmit={canSubmitAdjustment}
                    onClose={() => setEditingAdjustmentId(null)}
                    onSubmitted={() => {
                        void listQuery.refetch()
                        void adjustmentDetailQuery.refetch()
                        void workItemQuery.refetch()
                    }}
                />
            ) : null}

            {canCreateAdjustment && canSubmitAdjustment && (
                <AdjustmentDialog
                    open={canUseLedger && adjustment.adjustDraftId != null}
                    finalFocus={restoreRowFocus}
                    meta={adjustment.adjustMeta}
                    form={adjustment.form}
                    pending={adjustment.isSubmitting}
                    unresolved={adjustment.lastResult?.status === "unknown"}
                    feedback={
                        <>
                            {adjustment.actionError ? (
                                <Alert variant="destructive">
                                    <AlertDescription>
                                        {adjustment.actionError}
                                    </AlertDescription>
                                </Alert>
                            ) : null}
                            {adjustment.lastResult?.status === "unknown" ? (
                                <Alert>
                                    <AlertDescription>
                                        {adjustment.lastResult.description}
                                        <LoadingButton
                                            id="inventory-adjustment-dialog-resolve"
                                            type="button"
                                            variant="outline"
                                            disabled={adjustment.isResolving}
                                            loading={adjustment.isResolving}
                                            onClick={() =>
                                                void adjustment.resolveLastUnknown()
                                            }
                                        >
                                            {adjustment.isResolving
                                                ? "查询中…"
                                                : "查询提交结果"}
                                        </LoadingButton>
                                        <LoadingButton
                                            id="inventory-adjustment-dialog-retry"
                                            type="button"
                                            variant="outline"
                                            disabled={
                                                adjustment.isResolving ||
                                                adjustment.isSubmitting
                                            }
                                            loading={adjustment.isSubmitting}
                                            onClick={() =>
                                                void adjustment.doSubmit()
                                            }
                                        >
                                            使用本次操作重试
                                        </LoadingButton>
                                    </AlertDescription>
                                </Alert>
                            ) : null}
                        </>
                    }
                    onCancel={adjustment.closeAdjustment}
                />
            )}
            <LedgerTableFrame
                view={view}
                data={data}
                loading={listQuery.isFetching}
                isError={listLoadFailed}
                error={listQuery.error}
                onRetry={() => void listQuery.refetch()}
                pagination={pagination}
                onPaginationChange={handlePaginationChange}
                balanceColumns={balanceColumns}
                movementColumns={movementColumns}
                reservationColumns={reservationColumns}
                adjustmentColumns={adjustmentColumns}
                onOpenDetail={canReadBalance ? openDetail : undefined}
                onOpenAdjustment={openAdjustment}
                previewBalanceId={previewBalanceId}
                previewAdjustmentId={previewAdjustmentId}
                canOpenImports={canOpenImports}
                sortValue={sortValue}
                onSortChange={handleSortChange}
                hasActiveFilters={hasActiveFilters}
                appliedChips={appliedChips}
                searchInputRef={searchInputRef}
                filters={filters}
            />
        </PageScaffold>
    )
}
