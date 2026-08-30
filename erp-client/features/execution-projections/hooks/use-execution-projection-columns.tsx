"use client"

import * as React from "react"
import Link from "next/link"
import type { ColumnDef } from "@tanstack/react-table"

import { BusinessStatusBadge } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { w29Href } from "@/features/execution-projections/lib/url-state"
import {
    LATENCY_LABEL,
    SOURCE_LABEL,
    type ExecutionProjectionRow,
} from "@/features/execution-projections/types"
import { openWorkspaceLabel } from "@/lib/ui-text"

export type ProjectionRowCommandAction =
    | {
          kind: "QUERY_RESULT"
          row: ExecutionProjectionRow
          objectVersion: string
      }
    | {
          kind: "RETRY"
          row: ExecutionProjectionRow
          objectVersion: string
      }

export function useExecutionProjectionColumns(options: {
    replaceParams: (patch: Record<string, string | null | undefined>) => void
    commandPending: boolean
    onRowCommand: (action: ProjectionRowCommandAction) => void
}): ColumnDef<ExecutionProjectionRow>[] {
    const { replaceParams, commandPending, onRowCommand } = options

    return React.useMemo(
        () => [
            {
                id: "select",
                header: ({ table }) => (
                    <Checkbox
                        aria-label="全选本页可选项"
                        checked={table.getIsAllPageRowsSelected()}
                        indeterminate={
                            table.getIsSomePageRowsSelected() &&
                            !table.getIsAllPageRowsSelected()
                        }
                        onCheckedChange={(value) =>
                            table.toggleAllPageRowsSelected(Boolean(value))
                        }
                    />
                ),
                cell: ({ row }) => (
                    <Checkbox
                        aria-label={`选择 ${row.original.salesOrderNo}`}
                        checked={row.getIsSelected()}
                        onCheckedChange={(value) =>
                            row.toggleSelected(Boolean(value))
                        }
                        onClick={(e) => e.stopPropagation()}
                    />
                ),
                meta: { label: "选择", width: "status" },
                enableSorting: false,
            },
            {
                id: "salesOrder",
                accessorKey: "salesOrderNo",
                header: "销售单",
                meta: { label: "销售单", width: "default" },
                cell: ({ row }) => (
                    <div className="min-w-[9rem]">
                        <div className="num text-sm font-medium">
                            {row.original.salesOrderNo}
                        </div>
                        <div className="truncate text-xs text-muted-foreground">
                            {row.original.customerLabel}
                        </div>
                    </div>
                ),
            },
            {
                id: "source",
                header: "来源",
                meta: { label: "来源", width: "default" },
                cell: ({ row }) => (
                    <Badge
                        variant={
                            row.original.projectionSource ===
                            "MIGRATION_BASELINE"
                                ? "warning"
                                : "secondary"
                        }
                    >
                        {SOURCE_LABEL[row.original.projectionSource]}
                    </Badge>
                ),
            },
            {
                id: "mall",
                accessorKey: "targetMallName",
                header: "商城",
                meta: { label: "商城", width: "default" },
                cell: ({ row }) => (
                    <span className="text-sm">
                        {row.original.targetMallName}
                    </span>
                ),
            },
            {
                id: "delivery",
                header: "接收状态",
                meta: { label: "接收状态", width: "status" },
                cell: ({ row }) => (
                    <div className="flex flex-col gap-1">
                        <BusinessStatusBadge
                            context="list"
                            label={row.original.delivery.statusLabel}
                            tone={row.original.delivery.statusTone}
                        />
                        {row.original.latencyBand === "over_sla" ? (
                            <span className="text-tiny text-warning-foreground">
                                {LATENCY_LABEL.over_sla}
                            </span>
                        ) : row.original.latencyBand === "near_sla" ? (
                            <span className="text-tiny text-muted-foreground">
                                {LATENCY_LABEL.near_sla}
                            </span>
                        ) : null}
                    </div>
                ),
            },
            {
                id: "acked",
                header: "商城已确认版",
                meta: { label: "商城已确认版", width: "status", numeric: true },
                cell: ({ row }) => (
                    <span className="num text-sm">
                        {row.original.currentAckedRevisionNo != null
                            ? `v${row.original.currentAckedRevisionNo}`
                            : "尚未确认"}
                    </span>
                ),
            },
            {
                id: "attempt",
                header: "最近尝试",
                meta: { label: "最近尝试", width: "default" },
                cell: ({ row }) => (
                    <div className="text-xs">
                        <div className="num">
                            {row.original.delivery.attemptCount} 次
                        </div>
                        <div className="text-muted-foreground">
                            {row.original.delivery.lastAttemptAt ?? "—"}
                        </div>
                    </div>
                ),
            },
            {
                id: "error",
                header: "失败原因",
                meta: { label: "失败原因", width: "default" },
                cell: ({ row }) => (
                    <div className="max-w-[12rem]">
                        {row.original.reconciliationStatus ===
                        "VERSION_MISMATCH" ? (
                            <Badge variant="warning" className="mb-1">
                                版本差异
                            </Badge>
                        ) : null}
                        <span className="line-clamp-2 text-xs text-muted-foreground">
                            {row.original.delivery.errorSummary ?? "—"}
                        </span>
                    </div>
                ),
            },
            {
                id: "actions",
                header: "操作",
                meta: { label: "操作", width: "default", align: "end" },
                cell: ({ row }) => {
                    const r = row.original
                    const canQuery = r.allowedActions.includes("QUERY_RESULT")
                    const canRetry = r.allowedActions.includes("RETRY")
                    const canEscalate = r.allowedActions.includes("ESCALATE")
                    return (
                        <div
                            role="toolbar"
                            tabIndex={-1}
                            aria-label={`${r.projectionNo} 行操作`}
                            className="flex min-w-[11rem] flex-wrap justify-end gap-1"
                            onClick={(e) => e.stopPropagation()}
                            onKeyDown={(e) => e.stopPropagation()}
                        >
                            <Button
                                type="button"
                                size="xs"
                                variant="outline"
                                onClick={() =>
                                    replaceParams({
                                        projectionId: r.projectionId,
                                        revision: null,
                                    })
                                }
                            >
                                打开
                            </Button>
                            <Button
                                type="button"
                                size="xs"
                                variant="outline"
                                render={
                                    <Link
                                        href={`/sales/orders/${r.salesOrderId}?section=collaboration`}
                                    />
                                }
                            >
                                销售单
                            </Button>
                            {canQuery ? (
                                <Button
                                    type="button"
                                    size="xs"
                                    disabled={commandPending}
                                    onClick={() =>
                                        onRowCommand({
                                            kind: "QUERY_RESULT",
                                            row: r,
                                            objectVersion: r.objectVersion,
                                        })
                                    }
                                >
                                    查询结果
                                </Button>
                            ) : null}
                            {canRetry ? (
                                <Button
                                    type="button"
                                    size="xs"
                                    variant="outline"
                                    disabled={commandPending}
                                    onClick={() =>
                                        onRowCommand({
                                            kind: "RETRY",
                                            row: r,
                                            objectVersion: r.objectVersion,
                                        })
                                    }
                                >
                                    重试
                                </Button>
                            ) : null}
                            {canEscalate ||
                            r.reconciliationStatus === "VERSION_MISMATCH" ||
                            r.delivery.workItemId ? (
                                <Button
                                    type="button"
                                    size="xs"
                                    variant="outline"
                                    render={
                                        <Link
                                            href={w29Href(
                                                r.delivery.workItemId,
                                                r.delivery.errorTaskId,
                                            )}
                                        />
                                    }
                                >
                                    {openWorkspaceLabel("W29")}
                                </Button>
                            ) : null}
                        </div>
                    )
                },
            },
        ],
        [replaceParams, commandPending, onRowCommand],
    )
}
