"use client"

import Link from "next/link"
import type { ColumnDef } from "@tanstack/react-table"

import { BusinessStatusBadge, MoneyValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    costBasisLabel,
    factSummaryLabel,
    paymentCompositionLabel,
    supplierSummaryLabel,
} from "@/features/mall-consumption-orders/lib/labels"
import type { MallConsumptionOrderRow } from "@/features/mall-consumption-orders/types"
import {
    ATTRIBUTION_STATUS_LABEL,
    ATTRIBUTION_STATUS_TONE,
    COST_BASIS_LABEL,
    COST_BASIS_TONE,
    FULFILLMENT_CHAIN_LABEL,
    FULFILLMENT_CHAIN_TONE,
} from "@/features/mall-consumption-orders/types"
import { formatDateTime } from "@/lib/datetime"

export function buildConsumptionOrderColumns(
    listReturnHref: string,
): ColumnDef<MallConsumptionOrderRow>[] {
    return [
        {
            id: "mallOrder",
            header: "商城订单",
            meta: { label: "商城订单", width: "reference" },
            cell: ({ row }) => (
                <div className="min-w-[11rem] max-w-[14rem]">
                    <div className="truncate text-sm font-medium">
                        <span className="num">
                            {row.original.externalOrderNo}
                        </span>
                    </div>
                    <div className="truncate text-xs text-muted-foreground">
                        <span className="num">{row.original.mallOrderId}</span>
                        <span className="mx-1">·</span>
                        {row.original.mallName}
                    </div>
                </div>
            ),
        },
        {
            id: "customer",
            header: "客户",
            meta: { label: "客户", width: "default" },
            cell: ({ row }) => (
                <span className="text-sm">{row.original.customerLabel}</span>
            ),
        },
        {
            id: "paidAt",
            header: "支付时间",
            meta: { label: "支付时间", width: "default", numeric: true },
            cell: ({ row }) => (
                <span className="num text-sm text-muted-foreground">
                    {formatDateTime(
                        row.original.paidAt,
                        "monthDay",
                        "passthrough",
                    )}
                </span>
            ),
        },
        {
            id: "paidAmount",
            header: "实付",
            meta: {
                label: "实付",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <MoneyValue value={row.original.paidAmount} taxBasis="gross" />
            ),
        },
        {
            id: "paymentComposition",
            header: "支付构成",
            meta: { label: "支付构成", width: "default" },
            cell: ({ row }) => (
                <span className="text-sm">
                    {paymentCompositionLabel(row.original)}
                </span>
            ),
        },
        {
            id: "facts",
            header: "关键记录",
            meta: { label: "关键记录", width: "default" },
            cell: ({ row }) => (
                <span className="text-sm text-muted-foreground">
                    {factSummaryLabel(row.original)}
                </span>
            ),
        },
        {
            id: "fulfillmentChain",
            header: "履约链",
            meta: { label: "履约链", width: "status" },
            cell: ({ row }) => (
                <BusinessStatusBadge
                    context="list"
                    label={
                        FULFILLMENT_CHAIN_LABEL[row.original.fulfillmentChain]
                    }
                    tone={FULFILLMENT_CHAIN_TONE[row.original.fulfillmentChain]}
                />
            ),
        },
        {
            id: "supplier",
            header: "供应商订单摘要",
            meta: { label: "供应商订单摘要", width: "default" },
            cell: ({ row }) => {
                const label = supplierSummaryLabel(row.original)
                if (row.original.supplierOrderSummary.total > 0) {
                    return (
                        <Link
                            id={`mall-consumption-orders-list-row-${toAutomationIdSegment(row.original.mallOrderId)}-supplier-link`}
                            href={`/supplier-api/orders?q=${encodeURIComponent(row.original.externalOrderNo)}&view=all&from=W25&mallOrderId=${encodeURIComponent(row.original.mallOrderId)}&returnTo=${encodeURIComponent(listReturnHref)}`}
                            className="text-sm text-primary underline-offset-2 hover:underline"
                            aria-label={`查看供应商子订单 ${label}`}
                        >
                            {label}
                        </Link>
                    )
                }
                return (
                    <span
                        className={
                            row.original.supplierOrderSummary.hasException
                                ? "text-sm text-destructive"
                                : "text-sm text-muted-foreground"
                        }
                    >
                        {label}
                    </span>
                )
            },
        },
        {
            id: "attribution",
            header: "归集",
            meta: { label: "归集", width: "status" },
            cell: ({ row }) => (
                <BusinessStatusBadge
                    context="list"
                    label={
                        ATTRIBUTION_STATUS_LABEL[row.original.attributionStatus]
                    }
                    tone={
                        ATTRIBUTION_STATUS_TONE[row.original.attributionStatus]
                    }
                />
            ),
        },
        {
            id: "costBasis",
            header: "成本口径",
            meta: { label: "成本口径", width: "default" },
            cell: ({ row }) => {
                const primary = row.original.costBasisBreakdown[0]
                return (
                    <div className="flex flex-col gap-0.5">
                        {primary ? (
                            <BusinessStatusBadge
                                context="list"
                                label={COST_BASIS_LABEL[primary.basis]}
                                tone={COST_BASIS_TONE[primary.basis]}
                            />
                        ) : null}
                        <span className="text-xs text-muted-foreground">
                            {costBasisLabel(row.original)}
                        </span>
                    </div>
                )
            },
        },
        {
            id: "actions",
            header: "操作",
            meta: { label: "操作", width: "default", align: "end" },
            cell: ({ row }) => (
                <div className="flex justify-end gap-1">
                    <Button
                        id={`mall-consumption-orders-list-row-${toAutomationIdSegment(row.original.mallOrderId)}-open`}
                        type="button"
                        variant="outline"
                        size="xs"
                        render={
                            <Link
                                id={`mall-consumption-orders-list-row-${toAutomationIdSegment(row.original.mallOrderId)}-open-link`}
                                href={`/commerce/consumption-orders/${row.original.mallOrderId}?section=overview&returnTo=${encodeURIComponent(listReturnHref)}`}
                            />
                        }
                    >
                        打开中心
                    </Button>
                    {row.original.allowedActions.includes("OPEN_W29") &&
                    row.original.workItemId ? (
                        <Button
                            id={`mall-consumption-orders-list-row-${toAutomationIdSegment(row.original.mallOrderId)}-exception`}
                            type="button"
                            variant="ghost"
                            size="xs"
                            render={
                                <Link
                                    id={`mall-consumption-orders-list-row-${toAutomationIdSegment(row.original.mallOrderId)}-exception-link`}
                                    href={`/governance/integration-errors?resolveWorkItemId=${row.original.workItemId}&queueContextId=queue:W29:mine:all`}
                                />
                            }
                        >
                            查看异常
                        </Button>
                    ) : null}
                </div>
            ),
        },
    ]
}
