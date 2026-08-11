"use client"

import * as React from "react"
import Link from "next/link"
import type { ColumnDef } from "@tanstack/react-table"
import { ExternalLinkIcon } from "lucide-react"

import { BusinessStatusBadge } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { formatQty } from "@/features/inventory/presentation"
import type {
    StockAdjustmentRow,
    StockBalanceRow,
    StockMovementRow,
    StockReservationRow,
} from "@/features/inventory/types"
import { formatDateTime } from "@/lib/datetime"

type InventoryColumnsInput = {
    isPhoneNarrow: boolean
    rowFocusRef: { current: Map<string, HTMLButtonElement | null> }
    openDetail: (balanceId: string) => void
    startAdjustment: (row: StockBalanceRow) => Promise<void>
}

function useInventoryColumns({
    isPhoneNarrow,
    rowFocusRef,
    openDetail,
    startAdjustment,
}: InventoryColumnsInput) {
    const balanceColumns = React.useMemo<ColumnDef<StockBalanceRow>[]>(
        () => [
            {
                id: "identity",
                header: "仓库 / SKU",
                meta: { label: "仓库 / SKU", width: "reference" },
                cell: ({ row }) => (
                    <div className="min-w-0">
                        <div className="truncate text-sm font-medium">
                            {row.original.warehouseName}
                            <span className="ml-1 num text-xs text-muted-foreground">
                                {row.original.warehouseCode}
                            </span>
                        </div>
                        <div className="truncate text-sm">
                            <span className="num">{row.original.skuCode}</span>
                            <span className="text-muted-foreground"> · </span>
                            {row.original.skuName}
                        </div>
                        <div className="truncate text-xs text-muted-foreground">
                            {row.original.specSummary}
                        </div>
                    </div>
                ),
            },
            {
                id: "onHand",
                header: "账面现存",
                meta: {
                    label: "账面现存",
                    width: "amount",
                    align: "end",
                    numeric: true,
                },
                cell: ({ row }) =>
                    formatQty(
                        row.original.onHandQuantity,
                        row.original.baseUnit,
                    ),
            },
            {
                id: "reserved",
                header: "有效预占",
                meta: {
                    label: "有效预占",
                    width: "amount",
                    align: "end",
                    numeric: true,
                },
                cell: ({ row }) =>
                    formatQty(
                        row.original.reservedQuantity,
                        row.original.baseUnit,
                    ),
            },
            {
                id: "available",
                header: "可用数量",
                meta: {
                    label: "可用数量",
                    width: "amount",
                    align: "end",
                    numeric: true,
                },
                cell: ({ row }) => (
                    <div className="flex flex-col items-end gap-0.5">
                        {formatQty(
                            row.original.availableQuantity,
                            row.original.baseUnit,
                        )}
                        {row.original.availableQuantity === "0" ? (
                            <Badge variant="destructive" className="text-2xs">
                                零可用
                            </Badge>
                        ) : null}
                    </div>
                ),
            },
            {
                id: "status",
                header: "状态",
                meta: { label: "状态", width: "status" },
                cell: ({ row }) => (
                    <BusinessStatusBadge
                        context="list"
                        label={row.original.statusLabel}
                        tone={row.original.statusTone}
                    />
                ),
            },
            {
                id: "lastMovement",
                header: "最后变动",
                meta: { label: "最后变动", width: "default" },
                cell: ({ row }) => (
                    <div className="text-sm">
                        <div>{row.original.lastMovementTypeLabel}</div>
                        <div className="num text-xs text-muted-foreground">
                            {formatDateTime(
                                row.original.lastMovementAt,
                                "full",
                                "passthrough",
                            )}
                        </div>
                    </div>
                ),
            },
            {
                id: "actions",
                header: "操作",
                meta: { label: "操作", width: "default", align: "end" },
                cell: ({ row }) => {
                    const canAdjust =
                        !isPhoneNarrow &&
                        row.original.allowedActions.includes(
                            "CREATE_ADJUSTMENT",
                        )
                    const blocker = isPhoneNarrow
                        ? {
                              action: "CREATE_ADJUSTMENT",
                              code: "MOBILE_READONLY",
                              message: "窄屏仅只读，请在桌面发起库存调整",
                          }
                        : row.original.actionBlockers.find(
                              (b) => b.action === "CREATE_ADJUSTMENT",
                          )
                    return (
                        <div className="flex justify-end gap-1">
                            <Button
                                type="button"
                                variant="ghost"
                                size="xs"
                                ref={(el) => {
                                    rowFocusRef.current.set(
                                        row.original.balanceId,
                                        el,
                                    )
                                }}
                                onClick={() =>
                                    openDetail(row.original.balanceId)
                                }
                            >
                                查看
                            </Button>
                            <Button
                                type="button"
                                variant="outline"
                                size="xs"
                                disabled={!canAdjust}
                                title={blocker?.message}
                                onClick={() =>
                                    void startAdjustment(row.original)
                                }
                            >
                                库存调整
                            </Button>
                        </div>
                    )
                },
            },
        ],
        [openDetail, startAdjustment, isPhoneNarrow, rowFocusRef],
    )

    const movementColumns = React.useMemo<ColumnDef<StockMovementRow>[]>(
        () => [
            {
                id: "identity",
                header: "仓库 / SKU",
                meta: { label: "仓库 / SKU", width: "reference" },
                cell: ({ row }) => (
                    <div className="min-w-0 text-sm">
                        <div className="truncate font-medium">
                            {row.original.warehouseName}
                        </div>
                        <div className="truncate">
                            <span className="num">{row.original.skuCode}</span>
                            <span className="text-muted-foreground"> · </span>
                            {row.original.skuName}
                        </div>
                    </div>
                ),
            },
            {
                id: "type",
                header: "流水类型",
                meta: { label: "流水类型", width: "default" },
                cell: ({ row }) => (
                    <div className="text-sm">
                        <div>{row.original.movementTypeLabel}</div>
                        <div className="text-xs text-muted-foreground">
                            {row.original.direction === "increase"
                                ? "增加"
                                : "减少"}
                        </div>
                    </div>
                ),
            },
            {
                id: "qty",
                header: "数量",
                meta: {
                    label: "数量",
                    width: "amount",
                    align: "end",
                    numeric: true,
                },
                cell: ({ row }) =>
                    formatQty(row.original.quantity, row.original.baseUnit),
            },
            {
                id: "occurred",
                header: "发生 / 记录",
                meta: { label: "时间", width: "default", numeric: true },
                cell: ({ row }) => (
                    <div className="num text-xs text-muted-foreground">
                        <div>
                            发生{" "}
                            {formatDateTime(
                                row.original.occurredAt,
                                "full",
                                "passthrough",
                            )}
                        </div>
                        <div>
                            记录{" "}
                            {formatDateTime(
                                row.original.recordedAt,
                                "full",
                                "passthrough",
                            )}
                        </div>
                    </div>
                ),
            },
            {
                id: "source",
                header: "来源单据",
                meta: { label: "来源单据", width: "default" },
                cell: ({ row }) =>
                    row.original.sourceHref ? (
                        <Button
                            type="button"
                            variant="link"
                            size="xs"
                            className="h-auto px-0"
                            render={
                                <Link
                                    href={row.original.sourceHref}
                                    aria-label={`查看来源 ${row.original.sourceDocumentNo}`}
                                />
                            }
                        >
                            <span className="num">
                                {row.original.sourceDocumentNo}
                            </span>
                            <ExternalLinkIcon
                                className="ml-1 size-3"
                                aria-hidden
                            />
                        </Button>
                    ) : (
                        <span className="num text-sm">
                            {row.original.sourceDocumentNo}
                        </span>
                    ),
            },
            {
                id: "recorder",
                header: "记录人",
                meta: { label: "记录人", width: "default" },
                cell: ({ row }) => (
                    <span className="text-sm">
                        {row.original.recordedByLabel}
                    </span>
                ),
            },
        ],
        [],
    )

    const reservationColumns = React.useMemo<ColumnDef<StockReservationRow>[]>(
        () => [
            {
                id: "identity",
                header: "仓库 / SKU",
                meta: { label: "仓库 / SKU", width: "reference" },
                cell: ({ row }) => (
                    <div className="min-w-0 text-sm">
                        <div className="truncate font-medium">
                            {row.original.warehouseName}
                        </div>
                        <div className="truncate">
                            <span className="num">{row.original.skuCode}</span>
                            <span className="text-muted-foreground"> · </span>
                            {row.original.skuName}
                        </div>
                    </div>
                ),
            },
            {
                id: "sales",
                header: "销售单 / 明细",
                meta: { label: "销售单", width: "default" },
                cell: ({ row }) => (
                    <div className="text-sm">
                        <div className="num">{row.original.salesOrderNo}</div>
                        <div className="text-xs text-muted-foreground">
                            {row.original.salesOrderLineLabel}
                        </div>
                    </div>
                ),
            },
            {
                id: "qty",
                header: "建立 / 剩余",
                meta: {
                    label: "数量",
                    width: "amount",
                    align: "end",
                    numeric: true,
                },
                cell: ({ row }) => (
                    <div className="text-end text-sm">
                        <div className="num">
                            {row.original.establishedQuantity} /{" "}
                            {row.original.remainingQuantity}
                            <span className="ml-1 text-xs text-muted-foreground">
                                {row.original.baseUnit}
                            </span>
                        </div>
                        <div className="text-xs text-muted-foreground">
                            已消耗 {row.original.consumedQuantity} · 已释放{" "}
                            {row.original.releasedQuantity}
                        </div>
                    </div>
                ),
            },
            {
                id: "status",
                header: "状态",
                meta: { label: "状态", width: "status" },
                cell: ({ row }) => (
                    <BusinessStatusBadge
                        context="list"
                        label={row.original.statusLabel}
                        tone={row.original.statusTone}
                    />
                ),
            },
            {
                id: "source",
                header: "入库来源",
                meta: { label: "入库来源", width: "default" },
                cell: ({ row }) => (
                    <span className="num text-sm">
                        {row.original.inboundSourceDocumentNo ?? "—"}
                    </span>
                ),
            },
            {
                id: "actions",
                header: "操作",
                meta: { label: "操作", width: "default", align: "end" },
                cell: ({ row }) => (
                    <div className="flex justify-end gap-1">
                        {row.original.fulfillmentHref ? (
                            <Button
                                type="button"
                                variant="outline"
                                size="xs"
                                render={
                                    <Link href={row.original.fulfillmentHref} />
                                }
                            >
                                履约上下文
                            </Button>
                        ) : null}
                        {/* 明确不提供释放预占入口 */}
                    </div>
                ),
            },
        ],
        [],
    )

    const adjustmentColumns = React.useMemo<ColumnDef<StockAdjustmentRow>[]>(
        () => [
            {
                id: "doc",
                header: "调整单",
                meta: { label: "调整单", width: "reference" },
                cell: ({ row }) => (
                    <div className="text-sm">
                        <div className="num font-medium">
                            {row.original.adjustmentNo}
                        </div>
                        <div className="text-xs text-muted-foreground">
                            {row.original.reasonTypeLabel} ·{" "}
                            {row.original.direction === "increase"
                                ? "增加"
                                : "减少"}{" "}
                            {row.original.quantity} {row.original.baseUnit}
                        </div>
                    </div>
                ),
            },
            {
                id: "identity",
                header: "仓库 / SKU",
                meta: { label: "仓库 / SKU", width: "default" },
                cell: ({ row }) => (
                    <div className="text-sm">
                        <div>{row.original.warehouseName}</div>
                        <div className="num text-xs text-muted-foreground">
                            {row.original.skuCode} · {row.original.skuName}
                        </div>
                    </div>
                ),
            },
            {
                id: "status",
                header: "状态",
                meta: { label: "状态", width: "status" },
                cell: ({ row }) => (
                    <BusinessStatusBadge
                        context="list"
                        label={row.original.statusLabel}
                        tone={row.original.statusTone}
                    />
                ),
            },
            {
                id: "people",
                header: "岗位",
                meta: { label: "岗位", width: "default" },
                cell: ({ row }) => (
                    <div className="text-xs text-muted-foreground">
                        <div>经办 {row.original.operatorLabel}</div>
                        <div>
                            仓储复核{" "}
                            {row.original.warehouseReviewerLabel ?? "—"}
                        </div>
                        <div>
                            财务确认 {row.original.financeConfirmerLabel ?? "—"}
                        </div>
                    </div>
                ),
            },
            {
                id: "time",
                header: "创建 / 确认入账",
                meta: { label: "时间", width: "default", numeric: true },
                cell: ({ row }) => (
                    <div className="num text-xs text-muted-foreground">
                        <div>
                            创建{" "}
                            {formatDateTime(
                                row.original.createdAt,
                                "full",
                                "passthrough",
                            )}
                        </div>
                        <div>
                            确认入账{" "}
                            {row.original.postedAt
                                ? formatDateTime(
                                      row.original.postedAt,
                                      "full",
                                      "passthrough",
                                  )
                                : "—"}
                        </div>
                    </div>
                ),
            },
        ],
        [],
    )

    return {
        adjustmentColumns,
        balanceColumns,
        movementColumns,
        reservationColumns,
    }
}

export { useInventoryColumns }
