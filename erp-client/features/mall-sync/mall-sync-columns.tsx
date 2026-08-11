"use client"

import * as React from "react"
import Link from "next/link"
import type { ReadonlyURLSearchParams } from "next/navigation"
import type { ColumnDef } from "@tanstack/react-table"

import { BusinessStatusBadge } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import type {
    MallSnapshotRow,
    MallSyncJobRow,
    MappingTaskView,
    ReconciliationDifference,
} from "@/features/mall-sync/types"
import { formatDateTime } from "@/lib/datetime"
import { freshnessText, versionText } from "@/lib/ui-text"

type PatchUrl = (
    patch: Record<string, string | null | undefined>,
    options?: { replace?: boolean },
) => void

type MallSyncColumnsInput = {
    patchUrl: PatchUrl
    searchParams: ReadonlyURLSearchParams
}

function useMallSyncColumns({ patchUrl, searchParams }: MallSyncColumnsInput) {
    const jobColumns = React.useMemo<ColumnDef<MallSyncJobRow>[]>(
        () => [
            {
                id: "jobNo",
                accessorFn: (r) => r.jobNo,
                header: "任务号",
                cell: ({ row }) => (
                    <button
                        type="button"
                        className="text-left text-sm font-medium text-primary hover:underline"
                        onClick={() =>
                            patchUrl({
                                view: "jobs",
                                jobId: row.original.jobId,
                            })
                        }
                    >
                        {row.original.jobNo}
                    </button>
                ),
            },
            {
                id: "type",
                accessorFn: (r) => r.jobTypeLabel,
                header: "类型",
                cell: ({ row }) => (
                    <span className="text-sm">{row.original.jobTypeLabel}</span>
                ),
            },
            {
                id: "status",
                header: "状态",
                cell: ({ row }) => (
                    <BusinessStatusBadge
                        context="list"
                        label={row.original.statusLabel}
                        tone={row.original.statusTone}
                    />
                ),
            },
            {
                id: "counts",
                header: "页 / 条 / 错",
                meta: { align: "end", numeric: true },
                cell: ({ row }) => (
                    <span className="num text-sm">
                        {row.original.pageCount}/{row.original.itemCount}/
                        {row.original.errorCount}
                    </span>
                ),
            },
            {
                id: "wm",
                header: freshnessText.syncProgress,
                cell: ({ row }) => (
                    <span className="text-sm text-muted-foreground">
                        {row.original.watermarkAdvanced ? "已推进" : "未推进"}
                    </span>
                ),
            },
            {
                id: "started",
                header: "开始",
                cell: ({ row }) => (
                    <span className="text-sm tabular-nums">
                        {formatDateTime(row.original.startedAt, "default")}
                    </span>
                ),
            },
        ],
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [searchParams],
    )

    const snapshotColumns = React.useMemo<ColumnDef<MallSnapshotRow>[]>(
        () => [
            {
                id: "order",
                header: "商城销售单号",
                cell: ({ row }) => (
                    <button
                        type="button"
                        className="font-mono text-sm text-primary hover:underline"
                        onClick={() =>
                            patchUrl({
                                view: "snapshots",
                                snapshotId: row.original.snapshotId,
                            })
                        }
                    >
                        {row.original.externalOrderNo}
                    </button>
                ),
            },
            {
                id: "status",
                header: "商城状态",
                cell: ({ row }) => (
                    <span className="text-sm">
                        {row.original.sourceStatusLabel}
                    </span>
                ),
            },
            {
                id: "mapping",
                header: "数据映射状态",
                cell: ({ row }) => (
                    <Badge variant="outline">
                        {row.original.mappingStatusLabel}
                    </Badge>
                ),
            },
            {
                id: "hash",
                header: versionText.dataVersion,
                cell: ({ row }) => (
                    <span className="font-mono text-xs text-muted-foreground">
                        {row.original.contentHashShort}
                    </span>
                ),
            },
            {
                id: "applied",
                header: "ERP 版本",
                cell: ({ row }) =>
                    row.original.appliedSalesOrderNo ? (
                        <Link
                            href={`/sales/orders/${row.original.appliedSalesOrderId}`}
                            className="text-sm text-primary hover:underline"
                        >
                            {row.original.appliedSalesOrderNo}
                        </Link>
                    ) : (
                        <span className="text-sm text-muted-foreground">
                            未形成
                        </span>
                    ),
            },
        ],
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [searchParams],
    )

    const mappingColumns = React.useMemo<ColumnDef<MappingTaskView>[]>(
        () => [
            {
                id: "order",
                header: "来源单号",
                cell: ({ row }) => (
                    <button
                        type="button"
                        className="font-mono text-sm text-primary hover:underline"
                        onClick={() =>
                            patchUrl({
                                view: "mapping",
                                mappingTaskId: row.original.mappingTaskId,
                                workItemId:
                                    row.original.ownerRoutingState ===
                                    "CONFIGURED"
                                        ? row.original.workItem.workItemId
                                        : null,
                            })
                        }
                    >
                        {row.original.externalOrderNo}
                    </button>
                ),
            },
            {
                id: "type",
                header: "映射类型",
                cell: ({ row }) => (
                    <span className="text-sm">
                        {row.original.mappingTypeLabel}
                    </span>
                ),
            },
            {
                id: "mapStatus",
                header: "映射状态",
                cell: ({ row }) => (
                    <BusinessStatusBadge
                        context="list"
                        label={row.original.mappingTaskStatusLabel}
                        tone={
                            row.original.mappingTaskStatus === "RESOLVED"
                                ? "success"
                                : row.original.mappingTaskStatus === "PENDING"
                                  ? "warning"
                                  : "neutral"
                        }
                    />
                ),
            },
            {
                id: "reapply",
                header: "重新归集",
                cell: ({ row }) =>
                    row.original.reapplyOperation ? (
                        <BusinessStatusBadge
                            context="list"
                            label={row.original.reapplyOperation.statusLabel}
                            tone={
                                row.original.reapplyOperation.status ===
                                "SUCCEEDED"
                                    ? "success"
                                    : row.original.reapplyOperation.status ===
                                        "UNKNOWN"
                                      ? "destructive"
                                      : "info"
                            }
                        />
                    ) : (
                        <span className="text-sm text-muted-foreground">
                            未开始
                        </span>
                    ),
            },
            {
                id: "owner",
                header: "责任",
                cell: ({ row }) =>
                    row.original.ownerRoutingState === "MISSING" ? (
                        <Badge variant="destructive">待责任配置</Badge>
                    ) : (
                        <span className="text-sm">
                            {row.original.ownerRoleLabel}
                        </span>
                    ),
            },
            {
                id: "wi",
                header: "待办",
                cell: ({ row }) =>
                    row.original.ownerRoutingState === "CONFIGURED" ? (
                        <span className="text-sm">
                            {row.original.workItem.statusLabel}
                        </span>
                    ) : (
                        <span className="text-sm text-muted-foreground">
                            无
                        </span>
                    ),
            },
        ],
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [searchParams],
    )

    const diffColumns = React.useMemo<ColumnDef<ReconciliationDifference>[]>(
        () => [
            {
                id: "order",
                header: "来源单号",
                cell: ({ row }) => (
                    <button
                        type="button"
                        className="font-mono text-sm text-primary hover:underline"
                        onClick={() =>
                            patchUrl({
                                view: "reconciliation",
                                differenceId: row.original.differenceId,
                            })
                        }
                    >
                        {row.original.externalOrderNo}
                    </button>
                ),
            },
            {
                id: "type",
                header: "差异类型",
                cell: ({ row }) => (
                    <span className="text-sm">
                        {row.original.differenceTypeLabel}
                    </span>
                ),
            },
            {
                id: "fp",
                header: versionText.dataVersion,
                cell: ({ row }) => (
                    <span className="font-mono text-xs text-muted-foreground">
                        {row.original.sourceFingerprintShort ?? "—"}
                        {row.original.erpFingerprintShort
                            ? ` ↔ ${row.original.erpFingerprintShort}`
                            : ""}
                    </span>
                ),
            },
            {
                id: "status",
                header: "状态",
                cell: ({ row }) => (
                    <BusinessStatusBadge
                        context="list"
                        label={row.original.statusLabel}
                        tone={row.original.statusTone}
                    />
                ),
            },
        ],
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [searchParams],
    )

    return { diffColumns, jobColumns, mappingColumns, snapshotColumns }
}

export { useMallSyncColumns }
