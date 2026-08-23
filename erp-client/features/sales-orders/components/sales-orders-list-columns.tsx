"use client"

import Link from "next/link"
import { Loader2Icon } from "lucide-react"
import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessStatusBadge,
    MoneyValue,
    StatusTrackSummary,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    isPendingReviewStage,
    NATURE_LABEL,
    stageDueDisplay,
    stageOwnerDisplay,
} from "@/features/sales-orders/lib/labels"
import type { SalesOrderListItem } from "@/features/sales-orders/types"

export type SalesOrdersListColumnsContext = {
    downloadingContractId: string | null
    downloadContract: (order: SalesOrderListItem) => void
}

export function buildSalesOrdersListColumns(
    context: SalesOrdersListColumnsContext,
): ColumnDef<SalesOrderListItem>[] {
    const { downloadingContractId, downloadContract } = context

    return [
        {
            id: "document",
            accessorKey: "documentNumber",
            header: "销售单",
            meta: { label: "销售单", width: "reference" },
            cell: ({ row }) => (
                <div className="flex min-w-0 items-center gap-2">
                    <div className="min-w-0 flex-1 space-y-1">
                        <div className="flex items-center gap-2">
                            <Button
                                variant="link"
                                size="xs"
                                className="num px-0"
                                aria-label={`查看销售单 ${row.original.documentNumber}`}
                                render={
                                    <Link
                                        href={`/sales/orders/${row.original.id}`}
                                    />
                                }
                            >
                                {row.original.documentNumber}
                            </Button>
                            <BusinessStatusBadge
                                context="list"
                                label={row.original.primaryStatus.label}
                                tone={row.original.primaryStatus.tone}
                            />
                        </div>
                        <div className="truncate text-xs text-muted-foreground">
                            {row.original.customerName}
                        </div>
                    </div>
                </div>
            ),
        },
        {
            id: "nature",
            header: "业务性质",
            meta: { label: "业务性质", width: "status" },
            enableSorting: false,
            cell: ({ row }) => (
                <Badge variant="secondary">
                    {NATURE_LABEL[row.original.nature]}
                </Badge>
            ),
        },
        {
            id: "contract",
            accessorKey: "contractNumber",
            header: "合同",
            meta: { label: "合同", width: "reference" },
            cell: ({ row }) => {
                const order = row.original
                const contractNo = order.contractNumber.trim()
                const companyName = order.contractCompanyName.trim()
                if (!order.contractId && !contractNo) {
                    return (
                        <span className="text-sm text-muted-foreground">—</span>
                    )
                }
                const downloading = downloadingContractId === order.contractId
                return (
                    <div className="min-w-0 space-y-1">
                        {order.contractId ? (
                            <Button
                                type="button"
                                variant="link"
                                size="xs"
                                className="num px-0"
                                disabled={downloading}
                                aria-label={`下载合同 ${contractNo || order.contractId}`}
                                onClick={() => {
                                    void downloadContract(order)
                                }}
                            >
                                {downloading ? (
                                    <>
                                        <Loader2Icon
                                            data-icon="inline-start"
                                            className="animate-spin"
                                            aria-hidden="true"
                                        />
                                        下载中
                                    </>
                                ) : (
                                    contractNo || "下载合同"
                                )}
                            </Button>
                        ) : (
                            <span className="num text-sm">
                                {contractNo || "—"}
                            </span>
                        )}
                        <div className="truncate text-xs text-muted-foreground">
                            {companyName || "—"}
                        </div>
                    </div>
                )
            },
        },
        {
            id: "tracks",
            header: "进度",
            meta: { label: "多轨进度", width: "tracks" },
            enableSorting: false,
            cell: ({ row }) => (
                <StatusTrackSummary
                    variant="inline"
                    className="flex-nowrap gap-x-2 gap-y-0"
                    tracks={[
                        {
                            id: "fulfillment",
                            label: "履约",
                            status: row.original.fulfillment,
                        },
                        {
                            id: "collection",
                            label: "回款",
                            status: row.original.collection,
                        },
                        {
                            id: "invoicing",
                            label: "开票",
                            status: row.original.invoicing,
                        },
                    ]}
                />
            ),
        },
        {
            id: "amount",
            accessorKey: "amountGross",
            header: "成交金额",
            meta: {
                label: "成交金额",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <MoneyValue value={row.original.amountGross} taxBasis="gross" />
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
        {
            id: "currentOwner",
            header: "当前责任 / 时限",
            meta: { label: "当前责任 / 时限", width: "default" },
            enableSorting: false,
            cell: ({ row }) => {
                const order = row.original
                if (!isPendingReviewStage(order.primaryStatus.code)) {
                    return (
                        <span className="text-sm text-muted-foreground">—</span>
                    )
                }
                const due = stageDueDisplay(order)
                return (
                    <div className="text-sm">
                        <div>{stageOwnerDisplay(order)}</div>
                        <div className="text-xs text-muted-foreground">
                            {due ? (
                                <time dateTime={due.dateTime}>{due.label}</time>
                            ) : (
                                "时限未设置"
                            )}
                        </div>
                    </div>
                )
            },
        },
        {
            id: "submittedAt",
            accessorKey: "submittedAt",
            header: "提交时间",
            meta: { label: "提交时间", width: "default", numeric: true },
            cell: ({ row }) => (
                <span className="num text-sm text-muted-foreground">
                    {row.original.submittedAt}
                </span>
            ),
        },
    ]
}
