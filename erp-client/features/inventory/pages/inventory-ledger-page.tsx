"use client"

import * as React from "react"

import { PageScaffold, surfaceInsetClassName } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { InventoryBalancePreview } from "@/features/inventory/components/inventory-balance-preview"
import { useInventoryColumns } from "@/features/inventory/hooks/use-inventory-columns"
import {
    useAdjustmentDetailQuery,
    useBalanceDetailQuery,
    useInventoryListQuery,
} from "@/features/inventory/hooks/queries"
import {
    AVAILABILITY_LABEL,
    MOVEMENT_TYPE_LABEL,
} from "@/features/inventory/types"
import { mapWorkItemDto } from "@/features/work-items/types"
import { useWorkItemDetailQuery } from "@/features/work-items/queries"
import { AdjustmentConfirmDialog } from "./components/adjustment-confirm-dialog"
import { AdjustmentDetailSheet } from "./components/adjustment-detail-sheet"
import { AdjustmentDialog } from "./components/adjustment-dialog"
import { AdjustmentResultBanner } from "./components/adjustment-result-banner"
import { ExportJobProgress } from "./components/export-job-progress"
import {
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
import type { LedgerAppliedChip } from "./hooks/use-ledger-filters"
import { useLedgerFilters } from "./hooks/use-ledger-filters"
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
        workItemIdParam,
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
        view,
        warehouseId,
        availability,
        movementType,
        occurredFrom,
        occurredTo,
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
    const workItemQuery = useWorkItemDetailQuery(workItemIdParam ?? "")
    const workItem = workItemQuery.data
        ? mapWorkItemDto(workItemQuery.data)
        : undefined
    const workItemAdjustmentId =
        workItem?.businessObjectType === "stock_adjustment"
            ? workItem.businessObjectId
            : undefined
    const previewAdjustmentId =
        adjustmentIdParam ?? workItemAdjustmentId ?? null
    const detailQuery = useBalanceDetailQuery(previewBalanceId)
    const adjustmentDetailQuery = useAdjustmentDetailQuery(previewAdjustmentId)

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

    const openAdjustment = React.useCallback(
        (adjustmentId: string) => {
            setPreviewBalanceId(null)
            patchUrl({
                view: "adjustment",
                adjustmentId,
                balanceId: null,
            })
        },
        [patchUrl],
    )

    const closeAdjustment = React.useCallback(() => {
        patchUrl({ adjustmentId: null })
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

    const data = listQuery.data

    // 深链筛选 chip 的业务名称（skuId/salesOrderLineId/adjustmentId 不直接上屏内部 ID）；
    // 与 appliedChips 一起放在 early return 之前，保证 Hook 调用顺序稳定。
    const chipSkuName = allViewRows.find((r) => r.skuId === skuId)?.skuName
    const chipSalesLineLabel = (data?.reservations ?? []).find(
        (r) => r.salesOrderLineId === salesOrderLineId,
    )?.salesOrderLineLabel
    const chipAdjustmentNo = (data?.adjustments ?? []).find(
        (a) => a.adjustmentId === adjustmentIdParam,
    )?.adjustmentNo

    /** 已生效条件全部显性为 chip；来源锁定参数（skuId 等）不得成为隐形查询参数。 */
    const appliedChips = React.useMemo<readonly LedgerAppliedChip[]>(() => {
        const chips: LedgerAppliedChip[] = []
        const trimmedQ = qParam.trim()
        if (trimmedQ) {
            chips.push({ key: "q", label: `搜索：${trimmedQ}` })
        }
        if (warehouseId) {
            const warehouseLabel = (data?.warehouses ?? []).find(
                (w) => w.id === warehouseId,
            )?.name
            chips.push({
                key: "warehouseId",
                label: `仓库：${warehouseLabel ?? warehouseId}`,
            })
        }
        if (view === "balance" && availability !== "all") {
            chips.push({
                key: "availability",
                label: `可用状态：${AVAILABILITY_LABEL[availability]}`,
            })
        }
        if (view === "movement" && movementType.length > 0) {
            chips.push({
                key: "movementType",
                label: `流水类型：${movementType
                    .map((type) => MOVEMENT_TYPE_LABEL[type] ?? type)
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
        if (salesOrderLineId) {
            chips.push({
                key: "salesOrderLineId",
                label: `销售单明细：${chipSalesLineLabel ?? "已定位"}`,
            })
        }
        if (adjustmentIdParam) {
            chips.push({
                key: "adjustmentId",
                label: `调整单：${chipAdjustmentNo ?? "已定位"}`,
            })
        }
        return chips
    }, [
        adjustmentIdParam,
        availability,
        chipAdjustmentNo,
        chipSalesLineLabel,
        chipSkuName,
        data?.warehouses,
        movementType,
        occurredFrom,
        occurredTo,
        qParam,
        salesOrderLineId,
        skuId,
        view,
        warehouseId,
    ])

    // 查询失败但无缓存数据时只替换表格内容为失败态，筛选区保持挂载（§11.2）
    const listLoadFailed = listQuery.isError || !data

    if (listQuery.isPending) {
        return <InventoryLedgerLoading />
    }

    if (data?.emptyReason === "PERMISSION_REVOKED") {
        return (
            <InventoryLedgerPermissionRevoked
                onRetry={() => void listQuery.refetch()}
            />
        )
    }

    if (data?.emptyReason === "NO_DATA_SCOPE") {
        return <InventoryLedgerNoScope />
    }

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
                queriedAt={data?.queriedAt ?? ""}
                canExport={data?.canExport ?? false}
                total={data?.total ?? 0}
                onRefresh={() => {
                    void listQuery.refetch()
                    if (previewBalanceId) void detailQuery.refetch()
                }}
                onExport={() => {
                    startExport({
                        total: data?.total ?? 0,
                        filterSummary: data?.filterSummary ?? "",
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
                    <AlertDescription>
                        {adjustment.actionError}
                    </AlertDescription>
                </Alert>
            ) : null}

            {data ? (
                <details
                    className={`${surfaceInsetClassName} px-3 py-2.5 text-sm`}
                >
                    <summary className="flex cursor-pointer list-none items-center gap-1 text-xs font-medium text-muted-foreground [&::-webkit-details-marker]:hidden">
                        自有实物库存边界说明
                    </summary>
                    <p className="mt-2 text-xs leading-relaxed text-muted-foreground">
                        {data.excludedKindsNote}
                        <span className="mt-1 block">
                            {data.openingStockNote}
                        </span>
                    </p>
                </details>
            ) : null}

            {data ? (
                <LedgerMetricStrip
                    metrics={data.metrics}
                    metricActive={metricActive}
                    view={view}
                    onSelect={(patch) => {
                        patchUrl(patch, { replace: true })
                        resetPagination()
                    }}
                />
            ) : null}

            <LedgerViewTabs view={view} onViewChange={handleViewChange} />

            <LedgerTableFrame
                view={view}
                data={data}
                loading={listQuery.isFetching && !listQuery.isPending}
                isError={listLoadFailed}
                error={listQuery.error}
                onRetry={() => void listQuery.refetch()}
                pagination={pagination}
                onPaginationChange={handlePaginationChange}
                balanceColumns={balanceColumns}
                movementColumns={movementColumns}
                reservationColumns={reservationColumns}
                adjustmentColumns={adjustmentColumns}
                onOpenDetail={openDetail}
                onOpenAdjustment={openAdjustment}
                sortValue={sortValue}
                onSortChange={handleSortChange}
                hasActiveFilters={hasActiveFilters}
                appliedChips={appliedChips}
                searchInputRef={searchInputRef}
                filters={filters}
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
                onOpenAdjustment={openAdjustment}
            />

            <AdjustmentDetailSheet
                open={previewAdjustmentId != null}
                detail={adjustmentDetailQuery.data}
                isPending={adjustmentDetailQuery.isPending}
                workItemId={workItem?.workItemId}
                expectedTaskVersion={workItem?.taskVersion}
                workItemAllowedActions={workItem?.allowedActions}
                onClose={closeAdjustment}
                onDecisionApplied={() => {
                    void listQuery.refetch()
                    void adjustmentDetailQuery.refetch()
                    void workItemQuery.refetch()
                }}
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
