"use client"

import * as React from "react"

import { PageScaffold, surfaceInsetClassName } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { InventoryBalancePreview } from "@/features/inventory/components/inventory-balance-preview"
import { useInventoryColumns } from "@/features/inventory/hooks/use-inventory-columns"
import {
    useBalanceDetailQuery,
    useInventoryListQuery,
} from "@/features/inventory/hooks/queries"
import { AdjustmentConfirmDialog } from "./components/adjustment-confirm-dialog"
import { AdjustmentDialog } from "./components/adjustment-dialog"
import { AdjustmentResultBanner } from "./components/adjustment-result-banner"
import { ExportJobProgress } from "./components/export-job-progress"
import {
    InventoryLedgerError,
    InventoryLedgerLoading,
    InventoryLedgerNoScope,
    InventoryLedgerPermissionRevoked,
} from "./components/ledger-gate-states"
import { LedgerHeader } from "./components/ledger-header"
import { LedgerMetricStrip } from "./components/ledger-metric-strip"
import { LedgerTableFrame } from "./components/ledger-table-frame"
import { LedgerViewTabs } from "./components/ledger-view-tabs"
import { useAdjustmentWorkflow } from "./hooks/use-adjustment-workflow"
import { useInventoryExportJob } from "./hooks/use-inventory-export-job"
import { useInventoryLedgerPagination } from "./hooks/use-inventory-ledger-pagination"
import { useInventoryLedgerUrlState } from "./hooks/use-inventory-ledger-url-state"
import { useLedgerFilterActions } from "./hooks/use-ledger-filter-actions"
import { useLedgerSearch } from "./hooks/use-ledger-search"
import { usePhoneNarrow } from "./hooks/use-phone-narrow"
import { buildListQuery } from "./lib/build-list-query"

export function InventoryLedgerPage() {
    const {
        view,
        qParam,
        warehouseId,
        skuId,
        salesOrderLineId,
        availability,
        balanceIdParam,
        adjustmentIdParam,
        movementType,
        occurredFrom,
        occurredTo,
        sortValue,
        pageSize,
        cursorParam,
        cursorOffset,
        hasActiveFilters,
        patchUrl,
    } = useInventoryLedgerUrlState()
    const isPhoneNarrow = usePhoneNarrow()
    const { searchInput, setSearchInput, searchInputRef } = useLedgerSearch({
        qParam,
        patchUrl,
    })
    const { pagination, resetPagination, handlePaginationChange } =
        useInventoryLedgerPagination({
            view,
            pageSize,
            cursorOffset,
            patchUrl,
        })

    const [previewBalanceId, setPreviewBalanceId] = React.useState<
        string | null
    >(balanceIdParam ?? null)

    const rowFocusRef = React.useRef<Map<string, HTMLButtonElement | null>>(
        new Map(),
    )
    const restoreFocusIdRef = React.useRef<string | null>(null)

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
        ],
    )

    const listQuery = useInventoryListQuery(query)

    /** 深链筛选 chip 的业务名称（skuId/salesOrderLineId/adjustmentId 不直接上屏内部 ID）。 */
    const allViewRows = React.useMemo(
        () => [
            ...(listQuery.data?.balances ?? []),
            ...(listQuery.data?.movements ?? []),
            ...(listQuery.data?.reservations ?? []),
            ...(listQuery.data?.adjustments ?? []),
        ],
        [listQuery.data],
    )
    const detailQuery = useBalanceDetailQuery(previewBalanceId)

    const { exportJob, startExport, closeExport } = useInventoryExportJob()

    const handleFocusRestore = React.useCallback((balanceId: string) => {
        restoreFocusIdRef.current = balanceId
    }, [])
    const handlePreviewClose = React.useCallback(() => {
        setPreviewBalanceId(null)
    }, [])

    const adjustment = useAdjustmentWorkflow({
        isPhoneNarrow,
        onFocusRestore: handleFocusRestore,
        onPreviewClose: handlePreviewClose,
    })

    // Restore focus after detail/adjust close
    React.useEffect(() => {
        if (previewBalanceId || adjustment.adjustDraftId) return
        const id = restoreFocusIdRef.current
        if (!id) return
        const el = rowFocusRef.current.get(id)
        if (el) {
            el.focus()
            restoreFocusIdRef.current = null
        }
    }, [previewBalanceId, adjustment.adjustDraftId])

    const openDetail = React.useCallback(
        (balanceId: string) => {
            restoreFocusIdRef.current = balanceId
            setPreviewBalanceId(balanceId)
            // P2：打开详情属导航，用 push（不压缩历史）
            patchUrl({ balanceId })
        },
        [patchUrl],
    )

    const closeDetail = React.useCallback(() => {
        setPreviewBalanceId(null)
        // P2：关闭详情属导航，用 push
        patchUrl({ balanceId: null })
    }, [patchUrl])

    const {
        adjustmentColumns,
        balanceColumns,
        movementColumns,
        reservationColumns,
    } = useInventoryColumns({
        isPhoneNarrow,
        rowFocusRef,
        openDetail,
        startAdjustment: adjustment.startAdjustment,
    })

    const {
        handleApplyFilterPatch,
        handleViewChange,
        handleClearAllFilters,
        handleClearFiltersEmptyState,
    } = useLedgerFilterActions({
        patchUrl,
        resetPagination,
        setSearchInput,
        sortValue,
    })

    const data = listQuery.data

    if (listQuery.isPending) {
        return <InventoryLedgerLoading />
    }

    if (listQuery.isError || !data) {
        return (
            <InventoryLedgerError
                error={listQuery.error}
                onRetry={() => void listQuery.refetch()}
            />
        )
    }

    if (data.emptyReason === "PERMISSION_REVOKED") {
        return (
            <InventoryLedgerPermissionRevoked
                onRetry={() => void listQuery.refetch()}
            />
        )
    }

    if (data.emptyReason === "NO_DATA_SCOPE") {
        return <InventoryLedgerNoScope />
    }

    const chipSkuName = allViewRows.find((r) => r.skuId === skuId)?.skuName
    const chipSalesLineLabel = data.reservations.find(
        (r) => r.salesOrderLineId === salesOrderLineId,
    )?.salesOrderLineLabel
    const chipAdjustmentNo = data.adjustments.find(
        (a) => a.adjustmentId === adjustmentIdParam,
    )?.adjustmentNo

    const metricActive =
        availability === "zero"
            ? "zero"
            : availability === "reserved"
              ? "reserved"
              : view === "adjustment"
                ? "pending"
                : "combos"

    const detail = detailQuery.data

    return (
        <PageScaffold>
            <LedgerHeader
                isPhoneNarrow={isPhoneNarrow}
                queriedAt={data.queriedAt}
                canExport={data.canExport}
                total={data.total}
                onRefresh={() => {
                    void listQuery.refetch()
                    if (previewBalanceId) void detailQuery.refetch()
                }}
                onExport={() => {
                    startExport({
                        total: data.total,
                        filterSummary: data.filterSummary,
                    })
                }}
            />

            {adjustment.lastResult ? (
                <AdjustmentResultBanner
                    result={adjustment.lastResult}
                    isResolving={adjustment.isResolving}
                    onResolve={() => void adjustment.resolveLastUnknown()}
                />
            ) : null}

            {exportJob ? (
                <ExportJobProgress job={exportJob} onClose={closeExport} />
            ) : null}

            {adjustment.actionError ? (
                <Alert variant="destructive">
                    <AlertTitle>操作未完成</AlertTitle>
                    <AlertDescription>{adjustment.actionError}</AlertDescription>
                </Alert>
            ) : null}

            <details className={`${surfaceInsetClassName} px-3 py-2.5 text-sm`}>
                <summary className="flex cursor-pointer list-none items-center gap-1 text-xs font-medium text-muted-foreground [&::-webkit-details-marker]:hidden">
                    自有实物库存边界说明
                </summary>
                <p className="mt-2 text-xs leading-relaxed text-muted-foreground">
                    {data.excludedKindsNote}
                    <span className="mt-1 block">{data.openingStockNote}</span>
                </p>
            </details>

            <LedgerMetricStrip
                metrics={data.metrics}
                metricActive={metricActive}
                view={view}
                onSelect={(patch) => {
                    patchUrl(patch, { replace: true })
                    resetPagination()
                }}
            />

            <LedgerViewTabs view={view} onViewChange={handleViewChange} />

            <LedgerTableFrame
                view={view}
                data={data}
                loading={listQuery.isFetching && !listQuery.isPending}
                pagination={pagination}
                onPaginationChange={handlePaginationChange}
                balanceColumns={balanceColumns}
                movementColumns={movementColumns}
                reservationColumns={reservationColumns}
                adjustmentColumns={adjustmentColumns}
                onOpenDetail={openDetail}
                searchInput={searchInput}
                searchInputRef={searchInputRef}
                onSearchChange={(value) => {
                    setSearchInput(value)
                    resetPagination()
                }}
                warehouseId={warehouseId}
                availability={availability}
                movementType={movementType}
                occurredFrom={occurredFrom}
                occurredTo={occurredTo}
                sortValue={sortValue}
                hasActiveFilters={hasActiveFilters}
                skuId={skuId}
                salesOrderLineId={salesOrderLineId}
                adjustmentIdParam={adjustmentIdParam}
                chipSkuName={chipSkuName}
                chipSalesLineLabel={chipSalesLineLabel}
                chipAdjustmentNo={chipAdjustmentNo}
                onApplyPatch={handleApplyFilterPatch}
                onClearAll={handleClearAllFilters}
                onClearFiltersEmpty={handleClearFiltersEmptyState}
            />

            <InventoryBalancePreview
                open={previewBalanceId != null}
                detail={detail}
                isPending={detailQuery.isPending}
                onClose={closeDetail}
                onViewMovements={(currentDetail) => {
                    setPreviewBalanceId(null)
                    patchUrl(
                        {
                            view: "movement",
                            balanceId: null,
                            warehouseId: currentDetail.balance.warehouseId,
                            skuId: currentDetail.balance.skuId,
                        },
                        { replace: true },
                    )
                    resetPagination()
                }}
                onStartAdjustment={adjustment.startAdjustment}
            />

            <AdjustmentDialog
                open={adjustment.adjustDraftId != null}
                meta={adjustment.adjustMeta}
                form={adjustment.form}
                onCancel={adjustment.closeAdjustment}
            />

            <AdjustmentConfirmDialog
                open={adjustment.confirmOpen}
                pending={adjustment.isSubmitting}
                meta={adjustment.adjustMeta}
                onOpenChange={adjustment.setConfirmOpen}
                onConfirm={() => void adjustment.doSubmit()}
            />
        </PageScaffold>
    )
}
