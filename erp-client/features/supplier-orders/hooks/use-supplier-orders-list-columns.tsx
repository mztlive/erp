"use client"

import * as React from "react"
import Link from "next/link"
import type { ColumnDef } from "@tanstack/react-table"
import {
    ArrowUpRightIcon,
    EyeIcon,
    Loader2Icon,
    SearchIcon,
} from "lucide-react"

import { StatusTrackSummary, TableRowActions } from "@/components/business"
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
}: {
    rows: SupplierOrderListRow[]
    focusedIndex: number
    rowRefs: React.MutableRefObject<Map<string, HTMLElement>>
    onPreview: (orderId: string) => void
    onQueryResult: (row: SupplierOrderListRow) => Promise<void>
    queryPending: boolean
}) {
    return React.useMemo<ColumnDef<SupplierOrderListRow>[]>(
        () => [
            {
                id: "identity",
                accessorKey: "orderNo",
                header: "供应商订单",
                meta: { label: "供应商订单", width: "reference" },
                cell: ({ row }) => (
                    <div
                        className="flex min-w-0 flex-col gap-0.5"
                        data-focused={
                            rows[focusedIndex]?.orderId === row.original.orderId
                                ? "true"
                                : undefined
                        }
                    >
                        <Button
                            id={`supplier-orders-list-row-${toAutomationIdSegment(row.original.orderId)}-preview`}
                            type="button"
                            variant="link"
                            size="xs"
                            className="num h-auto justify-start px-0 text-sm"
                            aria-label={`预览 ${row.original.orderNo}`}
                            ref={(element) => {
                                if (element) {
                                    rowRefs.current.set(
                                        row.original.orderId,
                                        element,
                                    )
                                } else {
                                    rowRefs.current.delete(row.original.orderId)
                                }
                            }}
                            tabIndex={
                                rows[focusedIndex]?.orderId ===
                                row.original.orderId
                                    ? 0
                                    : -1
                            }
                            onClick={() => onPreview(row.original.orderId)}
                        >
                            {row.original.orderNo}
                        </Button>
                        <span className="truncate text-xs text-muted-foreground">
                            {row.original.supplierName}
                        </span>
                    </div>
                ),
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
                    <span className="truncate text-[13px]">
                        {row.original.followUpUserName || "—"}
                    </span>
                ),
            },
            {
                id: "handler",
                header: "异常处理人",
                meta: { label: "异常处理人", width: "default" },
                enableSorting: false,
                cell: ({ row }) => (
                    <span className="truncate text-[13px]">
                        {row.original.handlerUserName || "—"}
                    </span>
                ),
            },
            {
                id: "external",
                accessorKey: "externalOrderNo",
                header: "外部单号",
                meta: { label: "供应商外部单号", width: "reference" },
                cell: ({ row }) =>
                    row.original.externalOrderNo ? (
                        <span className="num text-[13px]">
                            {row.original.externalOrderNo}
                        </span>
                    ) : (
                        <span className="text-xs text-muted-foreground">
                            尚未返回
                        </span>
                    ),
            },
            {
                id: "updated",
                accessorKey: "lastBusinessAt",
                header: "更新时间",
                meta: { label: "更新时间", width: "default" },
                cell: ({ row }) => (
                    <span className="num text-[13px] text-muted-foreground">
                        {formatDateTime(
                            row.original.lastBusinessAt,
                            "monthDayIntl",
                            "passthrough",
                        )}
                    </span>
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
                    <span className="num text-[13px]">
                        {row.original.itemCount}
                    </span>
                ),
            },
            {
                id: "actions",
                size: 284,
                minSize: 284,
                header: "操作",
                meta: { label: "操作", width: "default", align: "end" },
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
                                        ...(resultUnknown
                                            ? { placement: "menu" as const }
                                            : {}),
                                    },
                                    {
                                        id: `supplier-orders-list-row-${segment}-open`,
                                        label: "详情",
                                        icon: ArrowUpRightIcon,
                                        href: `/supplier-api/orders/${r.orderId}`,
                                    },
                                    ...(resultUnknown
                                        ? [
                                              {
                                                  id: `supplier-orders-list-row-${segment}-query`,
                                                  icon: SearchIcon,
                                                  label: queryPending
                                                      ? "查询中…"
                                                      : "查询原结果",
                                                  disabled:
                                                      !canQuery || queryPending,
                                                  emphasis: "outline" as const,
                                                  onClick: () => {
                                                      void onQueryResult(r)
                                                  },
                                                  ...(queryPending
                                                      ? {
                                                            leading: (
                                                                <Loader2Icon
                                                                    className="size-3.5 animate-spin"
                                                                    aria-hidden="true"
                                                                />
                                                            ),
                                                        }
                                                      : {}),
                                              },
                                          ]
                                        : []),
                                ]}
                            />
                            {resultUnknown && !canQuery && queryBlocker ? (
                                <span className="max-w-[14rem] text-xs leading-tight text-muted-foreground">
                                    {queryBlocker.message}
                                    {queryBlocker.destinationWorkspaceId ? (
                                        <>
                                            ，可
                                            <Link
                                                id={`supplier-orders-list-row-${segment}-integration-errors`}
                                                href="/governance/integration-errors"
                                                className="text-primary underline-offset-2 hover:underline"
                                            >
                                                前往接口错误中心
                                            </Link>
                                        </>
                                    ) : null}
                                </span>
                            ) : null}
                            {resultUnknown && !canReplay ? (
                                <span className="sr-only">
                                    重发需先查询确认无结果且系统允许重试
                                </span>
                            ) : null}
                        </div>
                    )
                },
            },
        ],
        // eslint-disable-next-line react-hooks/exhaustive-deps -- handlers stable enough for list
        [focusedIndex, onPreview, queryPending, rows],
    )
}
