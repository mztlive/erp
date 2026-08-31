"use client"

import * as React from "react"
import Link from "next/link"
import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessStatusBadge,
    MoneyValue,
    StatusTrackSummary,
    taxAmountToneClass,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { displayPurchaseOrderNo } from "@/features/purchase-orders/lib/purchase-orders-list-helpers"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { PurchaseOrderListItem } from "@/features/purchase-orders/types"
import {
    FULFILLMENT_RESPONSIBILITY_LABEL,
    PURCHASE_TYPE_LABEL,
} from "@/features/purchase-orders/types"

export type PurchaseOrdersListColumnsOptions = {
    pageRows: readonly PurchaseOrderListItem[]
    focusedIndex: number
    listReturnHref: string
    rowRefs: React.RefObject<Map<string, HTMLElement>>
}

export function buildPurchaseOrdersListColumns({
    pageRows,
    focusedIndex,
    listReturnHref,
    rowRefs,
}: PurchaseOrdersListColumnsOptions): ColumnDef<PurchaseOrderListItem>[] {
    return [
        {
            id: "document",
            accessorFn: (row) => displayPurchaseOrderNo(row),
            header: "采购单号",
            meta: { label: "采购单号", width: "reference" },
            cell: ({ row }) => (
                <div
                    className="flex min-w-0 items-center gap-2"
                    ref={(el) => {
                        if (el) {
                            rowRefs.current.set(
                                row.original.purchaseOrderId,
                                el,
                            )
                        } else {
                            rowRefs.current.delete(row.original.purchaseOrderId)
                        }
                    }}
                    data-focused={
                        pageRows[focusedIndex]?.purchaseOrderId ===
                        row.original.purchaseOrderId
                            ? "true"
                            : undefined
                    }
                    style={
                        pageRows[focusedIndex]?.purchaseOrderId ===
                        row.original.purchaseOrderId
                            ? {
                                  backgroundColor: "var(--accent)",
                                  borderRadius: "0.375rem",
                              }
                            : undefined
                    }
                >
                    <div className="min-w-0 flex-1">
                        <div className="flex items-center gap-2">
                            <Button
                                type="button"
                                variant="link"
                                size="xs"
                                className="num px-0"
                                aria-label={`打开采购单 ${displayPurchaseOrderNo(row.original)}`}
                                render={
                                    <Link
                                        href={`/procurement/orders/${row.original.purchaseOrderId}`}
                                        id={`procurement-orders-list-row-${toAutomationIdSegment(row.original.purchaseOrderId)}-open`}
                                    />
                                }
                            >
                                {displayPurchaseOrderNo(row.original)}
                            </Button>
                            <BusinessStatusBadge
                                context="list"
                                label={row.original.statusLabel}
                                tone={row.original.statusTone}
                            />
                        </div>
                        <div className="truncate text-xs text-muted-foreground">
                            {row.original.supplierName}
                        </div>
                    </div>
                    <Badge variant="secondary" className="shrink-0">
                        {PURCHASE_TYPE_LABEL[row.original.purchaseType]}
                    </Badge>
                </div>
            ),
        },
        {
            id: "source",
            accessorKey: "salesOrderNo",
            header: "来源销售单",
            meta: { label: "来源销售单", width: "reference" },
            cell: ({ row }) => (
                <Link
                    id={`procurement-orders-list-row-${toAutomationIdSegment(row.original.purchaseOrderId)}-sales-order`}
                    href={`/sales/orders/${row.original.salesOrderId}?from=W08&returnTo=${encodeURIComponent(listReturnHref)}`}
                    className="num text-sm text-primary underline-offset-2 hover:underline"
                    aria-label={`查看来源销售单 ${row.original.salesOrderNo}`}
                >
                    {row.original.salesOrderNo}
                </Link>
            ),
        },
        {
            id: "type",
            header: "类型 / 履约",
            meta: { label: "类型与履约责任", width: "default" },
            cell: ({ row }) => (
                <span className="whitespace-nowrap text-sm">
                    {PURCHASE_TYPE_LABEL[row.original.purchaseType]}
                    <span className="text-muted-foreground">
                        {" "}
                        /{" "}
                        {
                            FULFILLMENT_RESPONSIBILITY_LABEL[
                                row.original.fulfillmentResponsibility
                            ]
                        }
                    </span>
                </span>
            ),
        },
        {
            id: "tracks",
            header: "进度",
            meta: { label: "多轨进度", width: "tracks" },
            cell: ({ row }) => (
                <StatusTrackSummary
                    variant="inline"
                    className="flex-nowrap gap-x-2 gap-y-0"
                    tracks={[
                        {
                            id: "pay",
                            label: "付款",
                            status: {
                                label: row.original.paymentProgress,
                                tone:
                                    row.original.paymentProgress === "已付"
                                        ? "success"
                                        : row.original.paymentProgress ===
                                            "部分"
                                          ? "info"
                                          : "neutral",
                            },
                        },
                        {
                            id: "ff",
                            label: "履约",
                            status: {
                                label: row.original.fulfillmentProgress,
                                tone:
                                    row.original.paymentGate === "BLOCKED"
                                        ? "warning"
                                        : row.original.fulfillmentProgress ===
                                            "完成"
                                          ? "success"
                                          : "neutral",
                            },
                        },
                    ]}
                />
            ),
        },
        {
            id: "amount",
            accessorKey: "grossAmount",
            header: "含税金额",
            meta: {
                label: "含税金额",
                width: "amount",
                align: "end",
                numeric: true,
            },
            enableSorting: true,
            cell: ({ row }) =>
                row.original.costMasked ? (
                    <span className="text-sm text-muted-foreground">•••</span>
                ) : (
                    <MoneyValue
                        value={row.original.grossAmount}
                        taxBasis="gross"
                        className={taxAmountToneClass("含税金额")}
                    />
                ),
        },
        {
            id: "paymentTerm",
            header: "付款条件",
            meta: { label: "付款条件", width: "default" },
            cell: ({ row }) => (
                <span className="text-sm">
                    {row.original.paymentTermLabel || "—"}
                </span>
            ),
        },
        {
            id: "owner",
            accessorKey: "ownerName",
            header: "负责人",
            meta: { label: "负责人", width: "default" },
            cell: ({ row }) => (
                <span className="text-sm">{row.original.ownerName || "—"}</span>
            ),
        },
    ]
}
