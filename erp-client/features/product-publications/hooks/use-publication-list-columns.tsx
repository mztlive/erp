"use client"

import * as React from "react"
import Link from "next/link"
import type { ColumnDef } from "@tanstack/react-table"

import { BusinessStatusBadge } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import type { ProductPublicationRow } from "@/features/product-publications/types"
import { formatDateTime } from "@/lib/datetime"

export function usePublicationListColumns(onPreview: (id: string) => void) {
    return React.useMemo<ColumnDef<ProductPublicationRow>[]>(
        () => [
            {
                id: "sku",
                header: "SKU / 商品",
                meta: { label: "SKU / 商品", width: "reference" },
                cell: ({ row }) => (
                    <div className="min-w-[12rem] max-w-[16rem]">
                        <div className="truncate text-sm font-medium">
                            <span className="num">{row.original.skuCode}</span>
                        </div>
                        <div className="truncate text-sm">
                            {row.original.productName}
                        </div>
                        <div className="truncate text-xs text-muted-foreground">
                            {row.original.specification}
                            <span className="mx-1">·</span>
                            <span className="num">
                                {row.original.publicationCode}
                            </span>
                        </div>
                    </div>
                ),
            },
            {
                id: "mall",
                header: "目标商城",
                meta: { label: "目标商城", width: "default" },
                cell: ({ row }) => (
                    <span className="text-sm">
                        {row.original.targetMallName}
                    </span>
                ),
            },
            {
                id: "acked",
                header: "商城生效版",
                meta: { label: "商城生效版", width: "status" },
                cell: ({ row }) =>
                    row.original.currentAckedRevisionNo != null ? (
                        <span className="num text-sm">
                            r{row.original.currentAckedRevisionNo}
                        </span>
                    ) : (
                        <span className="text-xs text-muted-foreground">
                            尚未生效
                        </span>
                    ),
            },
            {
                id: "latest",
                header: "最新发布版",
                meta: { label: "最新发布版", width: "status" },
                cell: ({ row }) => (
                    <div className="text-sm">
                        {row.original.latestRevisionNo != null ? (
                            <span className="num">
                                r{row.original.latestRevisionNo}
                            </span>
                        ) : (
                            "—"
                        )}
                        {row.original.hasPendingConfirmation ? (
                            <Badge variant="outline" className="ml-1 text-2xs">
                                待确认
                            </Badge>
                        ) : null}
                    </div>
                ),
            },
            {
                id: "offering",
                header: "固定供给",
                meta: { label: "固定供给", width: "default" },
                cell: ({ row }) => (
                    <div className="min-w-0 text-sm">
                        <div className="truncate">
                            {row.original.fixedOffering.supplierName}
                        </div>
                        <div className="truncate text-xs text-muted-foreground">
                            {row.original.fixedOffering.availabilityLabel}
                        </div>
                    </div>
                ),
            },
            {
                id: "price",
                header: "含税销售价",
                meta: {
                    label: "含税销售价",
                    width: "amount",
                    align: "end",
                    numeric: true,
                },
                cell: ({ row }) => (
                    <span className="num text-sm">
                        {row.original.salesPriceGross
                            ? `¥${row.original.salesPriceGross}`
                            : "—"}
                    </span>
                ),
            },
            {
                id: "pubStatus",
                header: "发布状态",
                meta: { label: "发布状态", width: "status" },
                cell: ({ row }) => (
                    <BusinessStatusBadge
                        context="list"
                        label={row.original.publicationStatusLabel}
                        tone={row.original.publicationStatusTone}
                    />
                ),
            },
            {
                id: "delivery",
                header: "商城接收",
                meta: { label: "商城接收", width: "status" },
                cell: ({ row }) =>
                    row.original.latestDelivery ? (
                        <div>
                            <BusinessStatusBadge
                                context="list"
                                label={row.original.latestDelivery.statusLabel}
                                tone={row.original.latestDelivery.statusTone}
                            />
                            {row.original.latestDelivery.errorSummary ? (
                                <div className="mt-0.5 max-w-[10rem] truncate text-tiny text-destructive">
                                    {row.original.latestDelivery.errorSummary}
                                </div>
                            ) : null}
                        </div>
                    ) : (
                        <span className="text-xs text-muted-foreground">—</span>
                    ),
            },
            {
                id: "ackAt",
                header: "商城确认时间",
                meta: {
                    label: "商城确认时间",
                    width: "default",
                    numeric: true,
                },
                cell: ({ row }) => (
                    <span className="num text-xs text-muted-foreground">
                        {row.original.latestDelivery?.mallAckAt
                            ? formatDateTime(
                                  row.original.latestDelivery.mallAckAt,
                                  "monthDay",
                                  "passthrough",
                              )
                            : "—"}
                    </span>
                ),
            },
            {
                id: "owner",
                header: "负责人",
                meta: { label: "负责人", width: "default" },
                cell: ({ row }) => (
                    <span className="text-sm">{row.original.ownerLabel}</span>
                ),
            },
            {
                id: "actions",
                header: "操作",
                meta: { label: "操作", width: "default", align: "end" },
                cell: ({ row }) => (
                    <div className="flex justify-end gap-1">
                        <Button
                            type="button"
                            variant="ghost"
                            size="xs"
                            onClick={() =>
                                onPreview(row.original.publicationId)
                            }
                        >
                            预览
                        </Button>
                        <Button
                            type="button"
                            variant="outline"
                            size="xs"
                            render={
                                <Link
                                    href={`/commerce/publications/${encodeURIComponent(row.original.publicationId)}`}
                                />
                            }
                        >
                            打开
                        </Button>
                    </div>
                ),
            },
        ],
        [onPreview],
    )
}
