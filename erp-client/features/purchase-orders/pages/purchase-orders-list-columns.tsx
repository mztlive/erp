"use client"

import Link from "next/link"
import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessStatusBadge,
    MoneyValue,
    StatusTrackSummary,
    TableEmptyCell,
    TableIdentityCell,
    TableTextCell,
    taxAmountToneClass,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { displayPurchaseOrderNo } from "@/features/purchase-orders/lib/purchase-orders-list-helpers"
import {
    FULFILLMENT_RESPONSIBILITY_LABEL,
    PURCHASE_TYPE_LABEL,
    type PurchaseOrderListItem,
} from "@/features/purchase-orders/types"
import { toAutomationIdSegment } from "@/lib/automation-id"

const PREFIX = "procurement-orders-list-row"

export function buildPurchaseOrdersListColumns({
    listReturnHref,
    canOpen,
    canOpenSource,
}: {
    listReturnHref: string
    canOpen: boolean
    canOpenSource: boolean
}): ColumnDef<PurchaseOrderListItem>[] {
    return [
        {
            id: "document",
            accessorFn: displayPurchaseOrderNo,
            header: "供应商 / 采购单",
            meta: { label: "供应商 / 采购单", width: "flex" },
            enableSorting: true,
            cell: ({ row }) => {
                const order = row.original
                const number = displayPurchaseOrderNo(order)
                const id = toAutomationIdSegment(order.purchaseOrderId)
                return (
                    <TableIdentityCell
                        className="w-0 min-w-full"
                        primary={order.supplierName}
                        primaryTitle={order.supplierName}
                        secondaryTitle={number}
                        numericSecondary
                        secondary={
                            canOpen ? (
                                <Button
                                    type="button"
                                    variant="link"
                                    size="xs"
                                    className="h-auto max-w-full justify-start px-0 text-xs font-normal text-muted-foreground hover:text-primary group-focus-within/row:text-primary group-hover/row:text-primary"
                                    aria-label={`打开采购单 ${number}`}
                                    render={
                                        <Link
                                            id={`${PREFIX}-${id}-open`}
                                            href={`/procurement/orders/${order.purchaseOrderId}`}
                                        />
                                    }
                                    onClick={(event) => event.stopPropagation()}
                                >
                                    <span className="truncate">{number}</span>
                                </Button>
                            ) : (
                                number
                            )
                        }
                    />
                )
            },
        },
        {
            id: "type",
            header: "类型 / 履约",
            meta: { label: "类型与履约责任", width: "default" },
            cell: ({ row }) => (
                <TableTextCell
                    value={`${PURCHASE_TYPE_LABEL[row.original.purchaseType]} / ${FULFILLMENT_RESPONSIBILITY_LABEL[row.original.fulfillmentResponsibility]}`}
                    clamp={1}
                />
            ),
        },
        {
            id: "status",
            header: "状态",
            meta: { label: "采购单状态", width: "status" },
            cell: ({ row }) => (
                <BusinessStatusBadge
                    context="list"
                    label={row.original.statusLabel}
                    tone={
                        row.original.status === "VOID"
                            ? "void"
                            : row.original.statusTone
                    }
                    appearance={
                        row.original.statusTone === "success"
                            ? "quiet"
                            : "badge"
                    }
                />
            ),
        },
        {
            id: "tracks",
            header: "进度",
            meta: { label: "付款与履约进度", width: "tracks" },
            cell: ({ row }) => (
                <StatusTrackSummary
                    variant="inline"
                    className="gap-x-2 gap-y-0"
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
                                            "部分付款"
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
            enableSorting: false,
            cell: ({ row }) =>
                row.original.costMasked ? (
                    <span className="text-muted-foreground">
                        <span className="sr-only">金额不可查看</span>
                        <span aria-hidden="true">•••</span>
                    </span>
                ) : (
                    <MoneyValue
                        display="compact"
                        value={row.original.grossAmount}
                        taxBasis="gross"
                        className={taxAmountToneClass("含税金额")}
                    />
                ),
        },
        {
            id: "owner",
            accessorKey: "ownerName",
            header: "负责人",
            meta: { label: "负责人", width: "default" },
            enableSorting: false,
            cell: ({ row }) => (
                <TableTextCell value={row.original.ownerName} clamp={1} />
            ),
        },
        {
            id: "paymentTerm",
            header: "付款条件",
            meta: { label: "付款条件", width: "default" },
            cell: ({ row }) => (
                <TableTextCell
                    value={row.original.paymentTermLabel}
                    clamp={1}
                />
            ),
        },
        {
            id: "source",
            accessorKey: "salesOrderNo",
            header: "来源销售单",
            meta: { label: "来源销售单", width: "default", align: "end" },
            enableSorting: false,
            cell: ({ row }) => {
                const order = row.original
                if (!order.salesOrderNo) return <TableEmptyCell />
                if (!canOpenSource || !order.salesOrderId) {
                    return (
                        <TableTextCell
                            value={order.salesOrderNo}
                            clamp={1}
                            numeric
                        />
                    )
                }
                const id = toAutomationIdSegment(order.purchaseOrderId)
                return (
                    <Link
                        id={`${PREFIX}-${id}-sales-order`}
                        href={`/sales/orders/${order.salesOrderId}?from=W08&returnTo=${encodeURIComponent(listReturnHref)}`}
                        className="num block w-0 min-w-full truncate rounded-sm text-muted-foreground underline-offset-2 hover:underline group-focus-within/row:text-primary group-hover/row:text-primary focus-visible:outline-2 focus-visible:outline-primary"
                        aria-label={`查看来源销售单 ${order.salesOrderNo}`}
                        title={order.salesOrderNo}
                        onClick={(event) => event.stopPropagation()}
                    >
                        {order.salesOrderNo}
                    </Link>
                )
            },
        },
    ]
}
