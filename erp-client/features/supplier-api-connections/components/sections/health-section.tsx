"use client"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessEmptyState,
    BusinessStatusBadge,
    BusinessTableFrame,
    DataTable,
} from "@/components/business"
import type {
    ConnectionCenterView,
    HealthRecordView,
} from "@/features/supplier-api-connections/types"
import { formatDateTime } from "@/lib/datetime"

export function HealthSection({
    records,
    last,
}: {
    records: HealthRecordView[]
    last?: ConnectionCenterView["lastHealth"]
}) {
    const columns = React.useMemo<ColumnDef<HealthRecordView>[]>(
        () => [
            {
                id: "at",
                accessorFn: (r) => r.at,
                header: "时间",
                meta: { label: "时间" },
                cell: ({ row }) => (
                    <span className="text-sm">
                        {formatDateTime(row.original.at, "default")}
                    </span>
                ),
            },
            {
                id: "type",
                accessorFn: (r) => r.checkType,
                header: "检查类型",
                meta: { label: "检查类型" },
            },
            {
                id: "result",
                header: "结果",
                meta: { label: "结果", width: "status" },
                cell: ({ row }) => (
                    <div className="space-y-0.5">
                        <BusinessStatusBadge
                            context="list"
                            label={row.original.resultLabel}
                            tone={row.original.resultTone}
                        />
                        {row.original.autoRetryStopped ? (
                            <div
                                className="text-tiny text-destructive"
                                role="status"
                            >
                                自动重试已停止
                            </div>
                        ) : null}
                        {row.original.result === "UNKNOWN" ? (
                            <div
                                className="text-tiny text-warning-soft-foreground"
                                role="status"
                            >
                                结果未知 · 不按失败播报
                            </div>
                        ) : null}
                    </div>
                ),
            },
            {
                id: "latency",
                header: "耗时",
                meta: { label: "耗时", numeric: true },
                cell: ({ row }) => (
                    <span className="num text-sm">
                        {row.original.latencyMs != null
                            ? `${row.original.latencyMs} ms`
                            : "—"}
                    </span>
                ),
            },
            {
                id: "job",
                header: "任务号",
                meta: { label: "任务号" },
                cell: ({ row }) => (
                    <span className="font-mono text-xs">
                        {row.original.jobNo ?? "—"}
                    </span>
                ),
            },
            {
                id: "trace",
                header: "追踪号",
                meta: { label: "追踪号" },
                cell: ({ row }) => (
                    <span className="font-mono text-xs">
                        {row.original.traceId ?? "—"}
                    </span>
                ),
            },
            {
                id: "summary",
                header: "摘要",
                meta: { label: "摘要" },
                cell: ({ row }) => (
                    <span className="text-xs text-muted-foreground">
                        {row.original.errorSummary ?? "—"}
                    </span>
                ),
            },
        ],
        [],
    )

    return (
        <div className="space-y-3">
            {last ? (
                <p className="text-sm text-muted-foreground">
                    最近：{formatDateTime(last.at, "default")} ·{" "}
                    {last.resultLabel}
                    {last.autoRetryStopped ? " · 自动重试已停止" : ""}
                </p>
            ) : null}
            <BusinessTableFrame
                title="健康检查记录"
                description="不展示原始密钥与敏感消息内容；结果未知单独文字说明"
                table={
                    <DataTable
                        data={records}
                        columns={columns}
                        getRowId={(r) => r.recordId}
                        rowCount={records.length}
                        caption="健康检查记录"
                        density="compact"
                        layout="flush"
                        manualPagination={false}
                        emptyState={
                            <BusinessEmptyState
                                kind="no-data"
                                className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                                title="暂无健康记录"
                                description="技术角色可在页头执行健康检查，结果会记录在本页。"
                            />
                        }
                    />
                }
            />
        </div>
    )
}
