"use client"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"

import { BusinessStatusBadge } from "@/components/business"
import { Button } from "@/components/ui/button"
import type { ConnectionListItem } from "@/features/supplier-api-connections/types"
import { formatDateTime } from "@/lib/datetime"
import { freshnessText } from "@/lib/ui-text"

export function useConnectionListColumns(
    onOpen: (connectionId: string) => void,
) {
    return React.useMemo<ColumnDef<ConnectionListItem>[]>(
        () => [
            {
                id: "identity",
                accessorFn: (row) => row.connectionCode,
                header: "连接身份",
                meta: { label: "连接身份", width: "reference" },
                cell: ({ row }) => {
                    const r = row.original
                    return (
                        <div className="min-w-0 py-0.5">
                            <Button
                                type="button"
                                variant="link"
                                size="xs"
                                className="num h-auto justify-start px-0 font-medium"
                                aria-label={`打开连接 ${r.connectionCode}`}
                                onClick={() => onOpen(r.connectionId)}
                            >
                                {r.connectionCode}
                            </Button>
                            <div className="truncate text-xs text-muted-foreground">
                                {r.supplier.name}
                            </div>
                        </div>
                    )
                },
            },
            {
                id: "environment",
                accessorFn: (row) => row.environmentLabel,
                header: "环境",
                meta: { label: "环境", width: "status" },
                cell: ({ row }) => {
                    const env = row.original.environment
                    const isProd = env === "PRODUCTION"
                    return (
                        <span
                            className={
                                isProd
                                    ? "text-sm font-medium text-destructive"
                                    : "text-sm text-muted-foreground"
                            }
                            aria-label={`环境：${row.original.environmentLabel}${
                                isProd ? "（生产环境）" : ""
                            }`}
                        >
                            {row.original.environmentLabel}
                            {isProd ? (
                                <span className="sr-only">生产环境</span>
                            ) : null}
                        </span>
                    )
                },
            },
            {
                id: "status",
                accessorFn: (row) => row.statusLabel,
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
                id: "capabilities",
                accessorFn: (row) => row.capabilitySummary,
                header: "能力摘要",
                meta: { label: "能力摘要" },
                cell: ({ row }) => (
                    <div className="max-w-[14rem]">
                        <div className="truncate text-sm">
                            {row.original.capabilitySummary}
                        </div>
                        <div className="text-tiny text-muted-foreground">
                            连接级 · 非商品级
                        </div>
                    </div>
                ),
            },
            {
                id: "health",
                accessorFn: (row) => row.healthLabel,
                header: "健康",
                meta: { label: "健康", width: "status" },
                cell: ({ row }) => (
                    <div className="space-y-0.5">
                        <BusinessStatusBadge
                            context="list"
                            label={row.original.healthLabel}
                            tone={row.original.healthTone}
                        />
                        <div className="text-tiny text-muted-foreground">
                            {formatDateTime(
                                row.original.lastHealthAt,
                                "default",
                            )}
                        </div>
                    </div>
                ),
            },
            {
                id: "catalog",
                accessorFn: (row) => row.catalogLabel,
                header: freshnessText.catalogSyncAt,
                meta: { label: freshnessText.catalogSyncAt },
                cell: ({ row }) => (
                    <span className="text-sm">{row.original.catalogLabel}</span>
                ),
            },
            {
                id: "nextStep",
                accessorFn: (row) => row.nextStep,
                header: "下一步",
                meta: { label: "下一步" },
                cell: ({ row }) => (
                    <span className="line-clamp-2 text-sm text-muted-foreground">
                        {row.original.nextStep}
                    </span>
                ),
            },
            {
                id: "owners",
                accessorFn: (row) =>
                    `${row.businessOwner ?? "—"} / ${row.technicalOwner ?? "—"}`,
                header: "业务/技术",
                meta: { label: "业务/技术负责人" },
                cell: ({ row }) => (
                    <span className="text-xs text-muted-foreground">
                        {row.original.businessOwner ?? "—"} /{" "}
                        {row.original.technicalOwner ?? "—"}
                    </span>
                ),
            },
            {
                id: "actions",
                header: "操作",
                meta: { label: "操作", width: "status" },
                enableSorting: false,
                cell: ({ row }) => (
                    <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        onClick={() => onOpen(row.original.connectionId)}
                    >
                        打开
                    </Button>
                ),
            },
        ],
        [onOpen],
    )
}
