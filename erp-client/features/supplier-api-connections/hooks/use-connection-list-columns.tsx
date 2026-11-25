"use client"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessStatusBadge,
    TableEmptyCell,
    TableIdentityCell,
    TableRowActions,
    TableTextCell,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import type { ConnectionListItem } from "@/features/supplier-api-connections/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { formatDateTime } from "@/lib/datetime"
import { freshnessText } from "@/lib/ui-text"

export function useConnectionListColumns(
    onOpen: ((connectionId: string) => void) | undefined,
    onOpenCatalog: ((connectionId: string) => void) | undefined,
) {
    return React.useMemo<ColumnDef<ConnectionListItem>[]>(() => {
        const columns: ColumnDef<ConnectionListItem>[] = [
            {
                id: "identity",
                accessorFn: (row) => row.connectionCode,
                header: "连接身份",
                meta: { label: "连接身份", width: "flex" },
                cell: ({ row }) => {
                    const { connectionCode, connectionId, supplier } =
                        row.original
                    return (
                        <TableIdentityCell
                            primary={supplier.name}
                            primaryTitle={supplier.name}
                            secondary={
                                onOpen ? (
                                    <Button
                                        id={`supplier-api-connections-list-row-${toAutomationIdSegment(connectionId)}-open-code`}
                                        type="button"
                                        variant="link"
                                        size="xs"
                                        className="h-auto max-w-full border-0 px-0 font-normal focus-visible:underline"
                                        aria-label={`打开连接 ${connectionCode}`}
                                        onClick={(event) => {
                                            event.stopPropagation()
                                            onOpen(connectionId)
                                        }}
                                    >
                                        <span className="truncate">
                                            {connectionCode}
                                        </span>
                                    </Button>
                                ) : (
                                    connectionCode
                                )
                            }
                            secondaryTitle={connectionCode}
                            numericSecondary
                        />
                    )
                },
            },
            {
                id: "connectorType",
                accessorFn: (row) => row.connectorTypeLabel,
                header: "接口类型",
                meta: { label: "接口类型", width: "status" },
                cell: ({ row }) => (
                    <TableTextCell
                        value={row.original.connectorTypeLabel}
                        clamp={1}
                    />
                ),
            },
            {
                id: "status",
                accessorFn: (row) => row.statusLabel,
                header: "状态",
                meta: { label: "状态", width: "status" },
                cell: ({ row }) => (
                    <div className="flex flex-col items-start gap-1">
                        <BusinessStatusBadge
                            context="list"
                            label={row.original.statusLabel}
                            tone={row.original.statusTone}
                            appearance={
                                row.original.status === "ENABLED"
                                    ? "quiet"
                                    : "badge"
                            }
                        />
                        <TableTextCell
                            value={row.original.nextStep}
                            clamp={2}
                            className="max-w-table-column-status text-xs whitespace-normal text-muted-foreground"
                        />
                    </div>
                ),
            },
            {
                id: "capabilities",
                accessorFn: (row) => row.capabilitySummary,
                header: "能力摘要",
                meta: { label: "能力摘要", width: "reference" },
                cell: ({ row }) => (
                    <div className="flex min-w-0 flex-col">
                        <TableTextCell
                            value={row.original.capabilitySummary}
                            clamp={1}
                        />
                        <span className="text-xs text-muted-foreground">
                            连接级 · 非商品级
                        </span>
                    </div>
                ),
            },
            {
                id: "health",
                accessorFn: (row) => row.healthLabel,
                header: "健康",
                meta: { label: "健康", width: "default" },
                cell: ({ row }) => (
                    <div className="flex flex-col items-start gap-0.5">
                        <BusinessStatusBadge
                            context="list"
                            label={row.original.healthLabel}
                            tone={row.original.healthTone}
                            appearance={
                                row.original.healthResult === "SUCCESS"
                                    ? "quiet"
                                    : "badge"
                            }
                        />
                        {row.original.lastHealthAt ? (
                            <TableTextCell
                                value={formatDateTime(
                                    row.original.lastHealthAt,
                                    "default",
                                )}
                                numeric
                                className="text-xs text-muted-foreground"
                            />
                        ) : (
                            <TableEmptyCell />
                        )}
                    </div>
                ),
            },
            {
                id: "catalog",
                accessorFn: (row) => row.catalogLabel,
                header: freshnessText.catalogSyncAt,
                meta: { label: freshnessText.catalogSyncAt, width: "default" },
                cell: ({ row }) => (
                    <TableTextCell
                        value={row.original.catalogLabel}
                        clamp={1}
                    />
                ),
            },
            {
                id: "nextStep",
                accessorFn: (row) => row.nextStep,
                header: "下一步",
                meta: { label: "下一步", width: "reference" },
                cell: ({ row }) => (
                    <TableTextCell
                        value={row.original.nextStep}
                        clamp={2}
                        className="whitespace-normal"
                    />
                ),
            },
            {
                id: "owners",
                header: "业务/技术",
                meta: { label: "业务/技术负责人", width: "default" },
                cell: ({ row }) => {
                    const { businessOwner, technicalOwner } = row.original
                    if (!businessOwner && !technicalOwner)
                        return <TableEmptyCell />
                    return (
                        <TableTextCell
                            value={`${businessOwner || "—"} / ${technicalOwner || "—"}`}
                            clamp={1}
                        />
                    )
                },
            },
        ]

        if (onOpen && onOpenCatalog) {
            columns.push({
                id: "actions",
                header: "操作",
                meta: { label: "操作", width: "default", align: "end" },
                enableSorting: false,
                cell: ({ row }) => {
                    const { connectionCode, connectionId } = row.original
                    const segment = toAutomationIdSegment(connectionId)
                    return (
                        <TableRowActions
                            moreId={`supplier-api-connections-list-row-${segment}-more`}
                            moreLabel={`${connectionCode} 更多操作`}
                            maxInline={2}
                            actions={[
                                {
                                    id: `supplier-api-connections-list-row-${segment}-open-catalog`,
                                    label: "商品库",
                                    onClick: () => onOpenCatalog(connectionId),
                                },
                                {
                                    id: `supplier-api-connections-list-row-${segment}-open-settings`,
                                    label: "平台设置",
                                    onClick: () => onOpen(connectionId),
                                },
                            ]}
                        />
                    )
                },
            })
        }

        return columns
    }, [onOpen, onOpenCatalog])
}
