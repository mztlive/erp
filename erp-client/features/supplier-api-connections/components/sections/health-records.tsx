"use client"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessEmptyState,
    BusinessStatusBadge,
    BusinessTableFrame,
    DataTable,
} from "@/components/business"
import type { HealthRecordView } from "@/features/supplier-api-connections/types"
import { formatDateTime } from "@/lib/datetime"

export function HealthRecords({ records }: { records: HealthRecordView[] }) {
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
                header: "检查结果",
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
                                className="text-xs text-destructive"
                                role="status"
                            >
                                自动重试已停止
                            </div>
                        ) : null}
                        {row.original.result === "UNKNOWN" ? (
                            <div
                                className="text-xs text-warning-soft-foreground"
                                role="status"
                            >
                                结果待确认
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
                header: "说明",
                meta: { label: "摘要" },
                cell: ({ row }) => (
                    <span className="text-sm text-muted-foreground">
                        {row.original.errorSummary ?? "—"}
                    </span>
                ),
            },
        ],
        [],
    )

    return (
        <div className="space-y-3">
            <h3 className="text-lg font-semibold">检查记录</h3>
            <BusinessTableFrame
                title="检查记录"
                description="保留每次检查结果；可通过列设置查看检查类型、耗时等信息。"
                table={
                    <DataTable
                        id="supplier-api-connections-health-table"
                        data={records}
                        columns={columns}
                        defaultColumnVisibility={{
                            type: false,
                            latency: false,
                            job: false,
                            trace: false,
                        }}
                        getRowId={(r) => r.recordId}
                        rowCount={records.length}
                        caption="健康检查记录"
                        layout="flush"
                        manualPagination={false}
                        emptyState={
                            <BusinessEmptyState
                                kind="no-data"
                                className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                                title="暂无健康记录"
                                description="在连接设置中点击“测试连接”，结果会记录在本页。"
                            />
                        }
                    />
                }
            />
        </div>
    )
}
