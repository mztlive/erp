"use client"

import * as React from "react"
import { InfoIcon, TriangleAlertIcon } from "lucide-react"
import type { ColumnDef } from "@tanstack/react-table"

import {
    BatchImpactPreview,
    BatchOperationResult,
    BusinessStatusBadge,
    DataTable,
} from "@/components/business"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { LoadingButton } from "@/components/ui/loading-button"
import { Spinner } from "@/components/ui/spinner"
import { DialogScrollBody } from "@/features/master-data/components/shared/action-dialog-shared"
import {
    classifyProductForBatchListing,
    formatBatchListingSkippedSummary,
    formatProductListingStatusLabel,
    MAX_BATCH_PRODUCT_LISTING_COUNT,
    partitionProductsForBatchListing,
    type BatchProductListingProgress,
    type BatchProductListingResult,
    type BatchProductListingSkippedItem,
} from "@/features/master-data/lib/product-batch-listing"
import type { MasterDataListItem } from "@/features/master-data/types"
import { toAutomationIdSegment } from "@/lib/automation-id"

const PREFIX = "master-data-products-batch-listing"

function usePreviewColumns(
    onRemove: (productId: string) => void,
    pending: boolean,
) {
    return React.useMemo<ColumnDef<MasterDataListItem>[]>(
        () => [
            {
                id: "product",
                header: "商品名称 · 编号",
                meta: { label: "商品名称 · 编号", width: "flex" },
                enableSorting: false,
                cell: ({ row }) => (
                    <div className="min-w-0">
                        <div
                            className="truncate text-sm font-medium text-foreground"
                            title={row.original.name}
                        >
                            {row.original.name}
                        </div>
                        <div className="num text-xs text-muted-foreground">
                            {row.original.stableNo}
                        </div>
                    </div>
                ),
            },
            {
                id: "currentStatus",
                header: "当前状态",
                meta: { label: "当前状态", width: "status" },
                enableSorting: false,
                cell: ({ row }) => {
                    const item = row.original
                    if (item.lifecycleStatus !== "ENABLED") {
                        return (
                            <BusinessStatusBadge
                                context="list"
                                tone={item.lifecycleTone}
                                label={item.lifecycleStatusLabel}
                            />
                        )
                    }
                    const status = item.listingStatus ?? "UNLISTED"
                    return (
                        <BusinessStatusBadge
                            context="list"
                            tone={
                                status === "LISTED"
                                    ? "success"
                                    : status === "PARTIALLY_LISTED"
                                      ? "warning"
                                      : "neutral"
                            }
                            label={formatProductListingStatusLabel(status)}
                        />
                    )
                },
            },
            {
                id: "skuProgress",
                header: "已上架 / 启用 SKU",
                meta: {
                    label: "已上架 / 启用 SKU",
                    width: "quantity",
                    align: "end",
                    numeric: true,
                },
                enableSorting: false,
                cell: ({ row }) => (
                    <span className="num text-sm">
                        {row.original.listedSkuCount ?? 0} /{" "}
                        {row.original.skuCount ?? 0}
                    </span>
                ),
            },
            {
                id: "decision",
                header: "本次处理",
                meta: { label: "本次处理", width: "reference" },
                enableSorting: false,
                cell: ({ row }) => {
                    const decision = classifyProductForBatchListing(
                        row.original,
                    )
                    if (decision.eligible) {
                        return (
                            <div className="flex flex-col items-start gap-0.5">
                                <BusinessStatusBadge
                                    context="list"
                                    tone={
                                        decision.kind === "partial"
                                            ? "info"
                                            : "success"
                                    }
                                    label={
                                        decision.kind === "partial"
                                            ? "补齐上架"
                                            : "将上架"
                                    }
                                />
                                <span className="num text-xs text-muted-foreground">
                                    待上架 {decision.pendingSkuCount} 个 SKU
                                </span>
                            </div>
                        )
                    }
                    return (
                        <div className="flex flex-col items-start gap-0.5">
                            <BusinessStatusBadge
                                context="list"
                                tone="neutral"
                                label="跳过"
                            />
                            <span className="text-xs text-muted-foreground">
                                {decision.reason}
                            </span>
                        </div>
                    )
                },
            },
            {
                id: "remove",
                header: "操作",
                meta: { label: "操作", width: "status" },
                enableSorting: false,
                cell: ({ row }) => (
                    <Button
                        id={`${PREFIX}-remove-${toAutomationIdSegment(row.original.stableId)}`}
                        type="button"
                        variant="ghost"
                        size="xs"
                        disabled={pending}
                        onClick={() => onRemove(row.original.stableId)}
                    >
                        移出
                    </Button>
                ),
            },
        ],
        [onRemove, pending],
    )
}

export function ProductBatchListingDialog({
    open,
    rows,
    filterSummary,
    totalResultCount,
    pending,
    progress,
    result,
    onOpenChange,
    onRemove,
    onConfirm,
    onRetryFailed,
}: {
    open: boolean
    rows: readonly MasterDataListItem[]
    filterSummary: string
    totalResultCount: number
    pending: boolean
    progress: BatchProductListingProgress | null
    result: BatchProductListingResult | null
    onOpenChange: (open: boolean) => void
    onRemove: (productId: string) => void
    onConfirm: (
        actionable: readonly MasterDataListItem[],
        skipped: readonly BatchProductListingSkippedItem[],
    ) => void
    onRetryFailed: (failedRows: readonly MasterDataListItem[]) => void
}) {
    const columns = usePreviewColumns(onRemove, pending)
    const tableRows = React.useMemo(() => [...rows], [rows])
    const partition = React.useMemo(
        () => partitionProductsForBatchListing(rows),
        [rows],
    )
    const skippedSummary = React.useMemo(
        () => formatBatchListingSkippedSummary(partition),
        [partition],
    )
    const exceedsLimit =
        partition.actionable.length > MAX_BATCH_PRODUCT_LISTING_COUNT

    const retryableFailedRows = React.useMemo(() => {
        if (!result || result.failed.length === 0) return []
        const byId = new Map(rows.map((row) => [row.stableId, row]))
        return result.failed
            .map((item) => byId.get(item.productId))
            .filter((row): row is MasterDataListItem => row != null)
    }, [result, rows])

    return (
        <Dialog
            open={open}
            onOpenChange={(next) => {
                if (pending && !next) return
                onOpenChange(next)
            }}
        >
            <DialogContent
                closeButtonId={`${PREFIX}-dismiss`}
                showCloseButton={!pending}
                className="flex max-h-[90dvh] w-[calc(100vw-2rem)] flex-col gap-4 overflow-hidden sm:max-w-4xl"
            >
                {result ? (
                    <>
                        <DialogHeader>
                            <DialogTitle>批量上架结果</DialogTitle>
                            <DialogDescription>
                                已成功的商品状态已同步刷新；失败项仍保留在列表勾选状态中，可直接重试或关闭后逐项处理。
                            </DialogDescription>
                        </DialogHeader>
                        <DialogScrollBody>
                            <div className="space-y-4">
                                {pending && progress ? (
                                    <div
                                        role="status"
                                        className="flex items-center gap-2 rounded-lg border border-border bg-muted px-3.5 py-2.5 text-sm text-foreground"
                                    >
                                        <Spinner />
                                        <span>
                                            正在重试失败项：已完成{" "}
                                            <span className="num font-medium">
                                                {progress.completed}
                                            </span>{" "}
                                            /{" "}
                                            <span className="num">
                                                {progress.total}
                                            </span>{" "}
                                            个商品…
                                        </span>
                                    </div>
                                ) : null}
                                <BatchOperationResult
                                    title="本次批量上架汇总"
                                    succeeded={result.succeeded.map((item) => ({
                                        id: item.productId,
                                        label: `${item.productName}（${item.productNo}）`,
                                        detail:
                                            item.skuCount != null
                                                ? `已上架 ${item.listedSkuCount ?? item.skuCount}/${item.skuCount} 个启用 SKU`
                                                : "已设为整组上架",
                                    }))}
                                    skipped={result.skipped.map((item) => ({
                                        id: item.productId,
                                        label: `${item.productName}（${item.productNo}）`,
                                        detail: item.reason,
                                    }))}
                                    failed={result.failed.map((item) => ({
                                        id: item.productId,
                                        label: `${item.productName}（${item.productNo}）`,
                                        detail: item.reason,
                                    }))}
                                    successEmptyMessage="本次没有成功上架的商品"
                                    skippedEmptyMessage="本次没有跳过的商品"
                                    failureEmptyMessage="没有失败项"
                                />
                            </div>
                        </DialogScrollBody>
                        <DialogFooter>
                            <Button
                                id={`${PREFIX}-result-close`}
                                type="button"
                                variant="outline"
                                disabled={pending}
                                onClick={() => onOpenChange(false)}
                            >
                                关闭
                            </Button>
                            {retryableFailedRows.length > 0 ? (
                                <LoadingButton
                                    id={`${PREFIX}-retry-failed`}
                                    type="button"
                                    loading={pending}
                                    disabled={pending}
                                    onClick={() =>
                                        onRetryFailed(retryableFailedRows)
                                    }
                                >
                                    {pending
                                        ? progress
                                            ? `正在重试 ${progress.completed}/${progress.total}…`
                                            : "正在重试…"
                                        : `重试失败项（${retryableFailedRows.length}）`}
                                </LoadingButton>
                            ) : null}
                        </DialogFooter>
                    </>
                ) : (
                    <>
                        <DialogHeader>
                            <DialogTitle>批量上架商品</DialogTitle>
                            <DialogDescription>
                                核对本次上架范围。上架后，所选商品下全部启用中的
                                SKU 将设为已上架。
                            </DialogDescription>
                        </DialogHeader>
                        <DialogScrollBody>
                            <div className="space-y-4">
                                <BatchImpactPreview
                                    title="批量上架影响预览"
                                    description="系统已按当前商品状态自动区分可上架与将跳过项；提交前可从下方明细中移出任意商品。"
                                    filterSummary={
                                        filterSummary.trim() ||
                                        "全部商品（无附加筛选）"
                                    }
                                    selectionScope={`已勾选 ${rows.length} 个商品（当前筛选共 ${totalResultCount} 个）`}
                                    estimated={rows.length}
                                    estimatedLabel="已选商品"
                                    processable={partition.actionable.length}
                                    processableLabel="预计上架"
                                    skipped={partition.skipped.length}
                                    skippedLabel="将跳过"
                                    skippedReason={skippedSummary}
                                    background={false}
                                />

                                <Alert variant="info">
                                    <InfoIcon aria-hidden="true" />
                                    <AlertDescription>
                                        商品上架后，需同时具备有效供给关系才会进入「公司商品池」。
                                        {partition.actionable.length > 0 ? (
                                            <>
                                                {" "}
                                                本次预计为{" "}
                                                <span className="num font-medium text-foreground">
                                                    {
                                                        partition.actionable
                                                            .length
                                                    }
                                                </span>{" "}
                                                个商品上架约{" "}
                                                <span className="num font-medium text-foreground">
                                                    {partition.pendingSkuCount}
                                                </span>{" "}
                                                个启用 SKU
                                                {partition.partialCount > 0
                                                    ? `（含 ${partition.partialCount} 个部分上架商品补齐剩余 SKU）`
                                                    : ""}
                                                。
                                            </>
                                        ) : null}
                                    </AlertDescription>
                                </Alert>

                                {exceedsLimit ? (
                                    <Alert variant="warning" role="alert">
                                        <TriangleAlertIcon aria-hidden="true" />
                                        <AlertDescription>
                                            单次批量上架最多支持{" "}
                                            <span className="num font-medium">
                                                {
                                                    MAX_BATCH_PRODUCT_LISTING_COUNT
                                                }
                                            </span>{" "}
                                            个可上架商品，请移出部分商品后分批执行。
                                        </AlertDescription>
                                    </Alert>
                                ) : null}

                                {pending && progress ? (
                                    <div
                                        role="status"
                                        className="flex items-center gap-2 rounded-lg border border-border bg-muted px-3.5 py-2.5 text-sm text-foreground"
                                    >
                                        <Spinner />
                                        <span>
                                            正在批量上架：已完成{" "}
                                            <span className="num font-medium">
                                                {progress.completed}
                                            </span>{" "}
                                            /{" "}
                                            <span className="num">
                                                {progress.total}
                                            </span>{" "}
                                            个商品…
                                        </span>
                                    </div>
                                ) : null}

                                <DataTable
                                    id={`${PREFIX}-table`}
                                    data={tableRows}
                                    columns={columns}
                                    getRowId={(row) => row.stableId}
                                    rowLabel={(row) =>
                                        `${row.name}（${row.stableNo}）`
                                    }
                                    rowCount={tableRows.length}
                                    defaultPagination={{
                                        pageIndex: 0,
                                        pageSize: 10,
                                    }}
                                    manualPagination={false}
                                    layout="flush"
                                    density="compact"
                                    caption="已选待批量上架商品"
                                    showColumnVisibility={false}
                                    showToolbarSummary={false}
                                    emptyTitle="已无待处理商品"
                                    emptyDescription="可返回列表重新勾选需要上架的商品。"
                                />
                            </div>
                        </DialogScrollBody>
                        <DialogFooter>
                            <Button
                                id={`${PREFIX}-cancel`}
                                type="button"
                                variant="outline"
                                disabled={pending}
                                onClick={() => onOpenChange(false)}
                            >
                                返回修改
                            </Button>
                            <LoadingButton
                                id={`${PREFIX}-confirm`}
                                type="button"
                                loading={pending}
                                disabled={
                                    pending ||
                                    partition.actionable.length === 0 ||
                                    exceedsLimit
                                }
                                onClick={() =>
                                    onConfirm(
                                        partition.actionable,
                                        partition.skipped,
                                    )
                                }
                            >
                                {pending
                                    ? progress
                                        ? `正在上架 ${progress.completed}/${progress.total}…`
                                        : "正在上架…"
                                    : `确认上架 ${partition.actionable.length} 个商品`}
                            </LoadingButton>
                        </DialogFooter>
                    </>
                )}
            </DialogContent>
        </Dialog>
    )
}
