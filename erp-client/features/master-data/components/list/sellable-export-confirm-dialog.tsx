"use client"

import * as React from "react"
import { TriangleAlertIcon } from "lucide-react"
import type { ColumnDef } from "@tanstack/react-table"

import { DataTable, MoneyValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Spinner } from "@/components/ui/spinner"
import { SellableItemThumbnail } from "@/features/master-data/components/list/sellable-item-thumbnail"
import { DialogScrollBody } from "@/features/master-data/components/shared/action-dialog-shared"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { sellableSupplierLabel } from "@/features/master-data/lib/sellable-excel-rows"
import type { MasterDataListItem } from "@/features/master-data/types"
import { toAutomationIdSegment } from "@/lib/automation-id"

const prefix = "master-data-sellable-items-export-confirm"

function useConfirmColumns(onRemove: (id: string) => void, pending: boolean) {
    return React.useMemo<ColumnDef<MasterDataListItem>[]>(
        () => [
            {
                id: "name",
                accessorKey: "name",
                header: "商品名称 · 规格",
                meta: { label: "商品名称 · 规格", width: "flex" },
                enableSorting: false,
                cell: ({ row }) => {
                    const sellable = row.original.sellableItem
                    const spec =
                        sellable && sellable.specificationLabel !== "无规格"
                            ? sellable.specificationLabel
                            : undefined
                    return (
                        <div className="flex min-w-0 items-center gap-3">
                            <SellableItemThumbnail
                                assetId={sellable?.mainImageAssetId}
                                label={row.original.name}
                                className="size-12 w-12 shrink-0 rounded-lg"
                            />
                            <div className="min-w-0">
                                <div
                                    className="truncate text-sm font-medium"
                                    title={row.original.name}
                                >
                                    {row.original.name}
                                </div>
                                <div className="flex min-w-0 items-center gap-2 text-xs text-muted-foreground">
                                    <span className="num shrink-0">
                                        {row.original.stableNo}
                                    </span>
                                    {spec ? (
                                        <span
                                            className="truncate border-l border-border pl-2"
                                            title={spec}
                                        >
                                            {spec}
                                        </span>
                                    ) : null}
                                </div>
                            </div>
                        </div>
                    )
                },
            },
            {
                id: "price",
                header: "销售价（含税）",
                meta: {
                    label: "销售价（含税）",
                    width: "amount",
                    align: "end",
                    numeric: true,
                },
                enableSorting: false,
                cell: ({ row }) => (
                    <MoneyValue
                        className="font-semibold"
                        value={
                            row.original.sellableItem?.salesVisiblePriceGross
                        }
                    />
                ),
            },
            {
                id: "marketPrice",
                header: "市场参考价",
                meta: {
                    label: "市场参考价",
                    width: "amount",
                    align: "end",
                    numeric: true,
                },
                enableSorting: false,
                cell: ({ row }) => {
                    const marketPrice = row.original.sellableItem?.marketPrice
                    if (!marketPrice) {
                        return (
                            <span className="text-sm text-muted-foreground">
                                —
                            </span>
                        )
                    }
                    return (
                        <MoneyValue
                            className="font-normal text-muted-foreground"
                            value={marketPrice}
                        />
                    )
                },
            },
            {
                id: "supplyRegions",
                header: "可供区域",
                meta: { label: "可供区域", width: "default" },
                enableSorting: false,
                cell: ({ row }) => {
                    const regions =
                        row.original.sellableItem?.supplyRegions ?? []
                    if (regions.length === 0) {
                        return (
                            <span className="text-sm text-muted-foreground">
                                未标注
                            </span>
                        )
                    }
                    return (
                        <span
                            className="text-[13px] text-muted-foreground"
                            title={regions.join("、")}
                        >
                            {regions.slice(0, 2).join("、")}
                            {regions.length > 2 ? (
                                <span className="num">
                                    {" "}
                                    +{regions.length - 2}
                                </span>
                            ) : null}
                        </span>
                    )
                },
            },
            {
                id: "supplierCount",
                header: "供应保障",
                meta: { label: "供应保障", width: "status" },
                enableSorting: false,
                cell: ({ row }) => {
                    const count = row.original.sellableItem?.supplierCount ?? 0
                    const atRisk = count <= 1
                    return (
                        <span
                            className={
                                atRisk
                                    ? "inline-flex items-center gap-1.5 text-[13px] text-warning-soft-foreground"
                                    : "inline-flex items-center gap-1.5 text-[13px] text-muted-foreground"
                            }
                        >
                            {atRisk ? (
                                <TriangleAlertIcon
                                    className="size-3.5 shrink-0"
                                    aria-hidden="true"
                                />
                            ) : null}
                            {sellableSupplierLabel(count)}
                        </span>
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
                        id={`${prefix}-remove-${toAutomationIdSegment(row.original.stableId)}`}
                        type="button"
                        variant="default"
                        size="xs"
                        disabled={pending}
                        onClick={() => onRemove(row.original.stableId)}
                    >
                        {masterDataCopy.sellableExportConfirmRemove}
                    </Button>
                ),
            },
        ],
        [onRemove, pending],
    )
}

export function SellableExportConfirmDialog({
    open,
    rows,
    pending,
    onOpenChange,
    onRemove,
    onConfirm,
}: {
    open: boolean
    rows: readonly MasterDataListItem[]
    pending: boolean
    onOpenChange: (open: boolean) => void
    onRemove: (id: string) => void
    onConfirm: () => void
}) {
    const columns = useConfirmColumns(onRemove, pending)
    const tableRows = React.useMemo(() => [...rows], [rows])

    return (
        <Dialog
            open={open}
            onOpenChange={(next) => {
                if (pending && !next) return
                onOpenChange(next)
            }}
        >
            <DialogContent
                className="flex max-h-[92vh] w-full flex-col gap-4 overflow-hidden sm:max-w-5xl"
                closeButtonId={`${prefix}-dismiss`}
            >
                <DialogHeader>
                    <DialogTitle>
                        {masterDataCopy.sellableExportConfirmTitle}
                    </DialogTitle>
                    <DialogDescription>
                        将导出下列{" "}
                        <span className="num font-medium text-foreground">
                            {rows.length}
                        </span>{" "}
                        件已选商品，单元格含主图。请核对后确认。
                    </DialogDescription>
                </DialogHeader>
                <DialogScrollBody>
                    <DataTable
                        id={`${prefix}-table`}
                        data={tableRows}
                        columns={columns}
                        getRowId={(row) => row.stableId}
                        rowLabel={(row) =>
                            row.sellableItem
                                ? `${row.name} · ${row.sellableItem.specificationLabel}`
                                : row.name
                        }
                        rowCount={rows.length}
                        defaultPagination={{ pageIndex: 0, pageSize: 20 }}
                        manualPagination={false}
                        layout="flush"
                        density="compact"
                        caption="已选待导出商品"
                        showColumnVisibility={false}
                    />
                </DialogScrollBody>
                <DialogFooter>
                    <Button
                        id={`${prefix}-cancel`}
                        type="button"
                        variant="outline"
                        disabled={pending}
                        onClick={() => onOpenChange(false)}
                    >
                        {masterDataCopy.sellableExportConfirmCancel}
                    </Button>
                    <Button
                        id={`${prefix}-submit`}
                        type="button"
                        disabled={pending || rows.length === 0}
                        onClick={onConfirm}
                    >
                        {pending ? <Spinner data-icon="inline-start" /> : null}
                        {pending
                            ? "导出中…"
                            : `${masterDataCopy.sellableExportConfirmSubmit} ${rows.length} 件`}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
