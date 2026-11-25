"use client"

import * as React from "react"
import Link from "next/link"
import type { ColumnDef } from "@tanstack/react-table"
import { ArrowUpRightIcon, EyeIcon, SearchIcon } from "lucide-react"

import {
    QuantityValue,
    StatusTrackSummary,
    TableIdentityCell,
    TableRowActions,
    TableTextCell,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import type { SupplierOrderListRow } from "@/features/supplier-orders/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { formatDateTime } from "@/lib/datetime"

export function useSupplierOrdersListColumns({
    rows,
    focusedIndex,
    rowRefs,
    onPreview,
    onQueryResult,
    queryPending,
    canOpenDetail,
    canInvestigate,
    canOpenIntegrationErrors,
}: {
    rows: SupplierOrderListRow[]
    focusedIndex: number
    rowRefs: React.MutableRefObject<Map<string, HTMLElement>>
    onPreview: (orderId: string) => void
    onQueryResult: (row: SupplierOrderListRow) => Promise<void>
    queryPending: boolean
    canOpenDetail: boolean
    canInvestigate: boolean
    canOpenIntegrationErrors: boolean
}) {
    return React.useMemo<ColumnDef<SupplierOrderListRow>[]>(() => {
        const columns: ColumnDef<SupplierOrderListRow>[] = [
            {
                id: "identity",
                accessorKey: "orderNo",
                header: "供应商订单",
                meta: { label: "供应商订单", width: "flex" },
                cell: ({ row }) => {
                    const { orderId, orderNo, supplierName } = row.original
                    const focused = rows[focusedIndex]?.orderId === orderId

                    return (
                        <div data-focused={focused ? "true" : undefined}>
                            <TableIdentityCell
                                primary={
                                    canOpenDetail ? (
                                        <Button
                                            id={`supplier-orders-list-row-${toAutomationIdSegment(orderId)}-preview`}
                                            type="button"
                                            variant="link"
                                            size="xs"
                                            className="num h-auto max-w-full justify-start px-0 text-body-compact font-medium"
                                            aria-label={`预览 ${orderNo}`}
                                            ref={(element) => {
                                                if (element)
                                                    rowRefs.current.set(
                                                        orderId,
                                                        element,
                                                    )
                                                else
                                                    rowRefs.current.delete(
                                                        orderId,
                                                    )
                                            }}
                                            tabIndex={focused ? 0 : -1}
                                            onClick={(event) => {
                                                event.stopPropagation()
                                                onPreview(orderId)
                                            }}
                                        >
                                            <span className="truncate">
                                                {orderNo}
                                            </span>
                                        </Button>
                                    ) : (
                                        <span className="num">{orderNo}</span>
                                    )
                                }
                                primaryTitle={orderNo}
                                secondary={supplierName}
                                secondaryTitle={supplierName}
                            />
                        </div>
                    )
                },
            },
            {
                id: "tracks",
                header: "履约 / 取消 / 退款",
                meta: { label: "三轨状态", width: "tracks" },
                enableSorting: false,
                cell: ({ row }) => (
                    <StatusTrackSummary
                        variant="inline"
                        className="max-w-64 flex-wrap gap-x-2 gap-y-1"
                        aria-label={`${row.original.orderNo} 三轨状态`}
                        tracks={[
                            {
                                id: "ff",
                                label: "履约",
                                status: {
                                    label: row.original.fulfillmentLabel,
                                    tone: row.original.fulfillmentTone,
                                },
                            },
                            {
                                id: "cancel",
                                label: "取消",
                                status: {
                                    label: row.original.cancelLabel,
                                    tone: row.original.cancelTone,
                                },
                            },
                            {
                                id: "refund",
                                label: "退款",
                                status: {
                                    label: row.original.refundLabel,
                                    tone: row.original.refundTone,
                                },
                            },
                        ]}
                    />
                ),
            },
            {
                id: "followUp",
                header: "跟进人",
                meta: { label: "跟进人", width: "default" },
                enableSorting: false,
                cell: ({ row }) => (
                    <TableTextCell
                        value={row.original.followUpUserName}
                        clamp={1}
                    />
                ),
            },
            {
                id: "handler",
                header: "异常处理人",
                meta: { label: "异常处理人", width: "default" },
                enableSorting: false,
                cell: ({ row }) => (
                    <TableTextCell
                        value={row.original.handlerUserName}
                        clamp={1}
                    />
                ),
            },
            {
                id: "external",
                accessorKey: "externalOrderNo",
                header: "外部单号",
                meta: { label: "供应商外部单号", width: "reference" },
                cell: ({ row }) => (
                    <TableTextCell
                        value={row.original.externalOrderNo}
                        emptyVariant="label"
                        emptyLabel="尚未返回"
                        clamp={1}
                        numeric
                    />
                ),
            },
            {
                id: "updated",
                accessorKey: "lastBusinessAt",
                header: "更新时间",
                meta: { label: "更新时间", width: "default" },
                cell: ({ row }) => (
                    <TableTextCell
                        value={formatDateTime(
                            row.original.lastBusinessAt,
                            "monthDayIntl",
                            "passthrough",
                        )}
                        numeric
                        className="text-muted-foreground"
                    />
                ),
            },
            {
                id: "itemCount",
                accessorFn: (row) => row.itemCount,
                header: "商品数",
                meta: {
                    label: "商品数",
                    width: "quantity",
                    align: "end",
                    numeric: true,
                },
                cell: ({ row }) => (
                    <QuantityValue
                        value={String(row.original.itemCount)}
                        unit="件"
                    />
                ),
            },
            {
                id: "actions",
                header: "操作",
                meta: {
                    label: "操作",
                    width: "default",
                    align: "end",
                },
                enableSorting: false,
                cell: ({ row }) => {
                    const r = row.original
                    const segment = toAutomationIdSegment(r.orderId)
                    const canQuery = r.allowedActions.includes("QUERY_RESULT")
                    const canReplay = r.allowedActions.includes("REPLAY")
                    const queryBlocker = r.actionBlockers.find(
                        (b) => b.action === "QUERY_RESULT",
                    )
                    const resultUnknown =
                        r.fulfillmentStatus === "RESULT_UNKNOWN"
                    return (
                        <div className="flex flex-col items-end gap-1">
                            <TableRowActions
                                moreId={`supplier-orders-list-row-${segment}-more`}
                                moreLabel={`${r.orderNo} 更多操作`}
                                actions={[
                                    {
                                        id: `supplier-orders-list-row-${segment}-preview-action`,
                                        label: "预览",
                                        icon: EyeIcon,
                                        onClick: () => onPreview(r.orderId),
                                        ...(resultUnknown && canInvestigate
                                            ? {
                                                  placement: "menu" as const,
                                              }
                                            : {}),
                                    },
                                    {
                                        id: `supplier-orders-list-row-${segment}-open`,
                                        label: "详情",
                                        icon: ArrowUpRightIcon,
                                        href: `/supplier-api/orders/${r.orderId}`,
                                    },
                                    ...(resultUnknown && canInvestigate
                                        ? [
                                              {
                                                  id: `supplier-orders-list-row-${segment}-query`,
                                                  icon: SearchIcon,
                                                  label: queryPending
                                                      ? "查询中…"
                                                      : "查询原结果",
                                                  disabled:
                                                      !canQuery || queryPending,
                                                  loading: queryPending,
                                                  disabledReason: !canQuery
                                                      ? queryBlocker?.message
                                                      : undefined,
                                                  onClick: () => {
                                                      void onQueryResult(r)
                                                  },
                                              },
                                          ]
                                        : []),
                                ]}
                            />
                            {canInvestigate &&
                            resultUnknown &&
                            !canQuery &&
                            queryBlocker ? (
                                <span className="max-w-36 text-xs whitespace-normal text-muted-foreground">
                                    {queryBlocker.message}
                                    {canOpenIntegrationErrors &&
                                    queryBlocker.destinationWorkspaceId ? (
                                        <>
                                            ，可
                                            <Link
                                                id={`supplier-orders-list-row-${segment}-integration-errors`}
                                                href="/governance/integration-errors"
                                                className="text-primary underline-offset-2 hover:underline"
                                                onClick={(event) =>
                                                    event.stopPropagation()
                                                }
                                            >
                                                前往接口错误中心
                                            </Link>
                                        </>
                                    ) : null}
                                </span>
                            ) : null}
                            {canInvestigate && resultUnknown && !canReplay ? (
                                <span className="sr-only">
                                    重发需先查询确认无结果且系统允许重试
                                </span>
                            ) : null}
                        </div>
                    )
                },
            },
        ]
        return canOpenDetail
            ? columns
            : columns.filter((column) => column.id !== "actions")
    }, [
        canOpenDetail,
        canInvestigate,
        canOpenIntegrationErrors,
        focusedIndex,
        onPreview,
        onQueryResult,
        queryPending,
        rowRefs,
        rows,
    ])
}
