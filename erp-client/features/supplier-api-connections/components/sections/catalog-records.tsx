"use client"
import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"
import type { UseQueryResult } from "@tanstack/react-query"
import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessTableFrame,
    DataTable,
} from "@/components/business"
import {
    CATALOG_JOB_LABEL,
    type CatalogJob,
    type CatalogResult,
} from "../../api/catalog"
import { formatDateTime } from "@/lib/datetime"
export function CatalogRecords({
    query,
}: {
    query: UseQueryResult<CatalogResult, Error>
}) {
    const columns = React.useMemo<ColumnDef<CatalogJob>[]>(
        () => [
            {
                accessorKey: "created_at",
                header: "开始时间",
                cell: ({ row }) =>
                    formatDateTime(
                        new Date(row.original.created_at * 1000).toISOString(),
                        "default",
                    ),
            },
            { accessorKey: "job_no", header: "任务编号" },
            {
                accessorKey: "status",
                header: "状态",
                cell: ({ row }) =>
                    row.original.stalled
                        ? "已中断，可继续"
                        : CATALOG_JOB_LABEL[row.original.status],
            },
            { accessorKey: "observed_count", header: "读取规格数" },
            { accessorKey: "pages", header: "已处理页数" },
            {
                accessorKey: "error_summary",
                header: "失败原因",
                cell: ({ row }) => row.original.error_summary ?? "—",
            },
        ],
        [],
    )
    return (
        <BusinessTableFrame
            title="商品同步记录"
            description="最近 20 次同步。重新同步会更新读取结果，保留已完成的公司商品匹配。"
            table={
                <DataTable
                    id="supplier-api-connections-catalog-records"
                    data={query.data?.jobs ?? []}
                    columns={columns}
                    getRowId={(row) => row.job_id}
                    rowCount={query.data?.jobs.length ?? 0}
                    manualPagination={false}
                    loading={query.isPending}
                    caption="商品同步记录"
                    layout="flush"
                    errorState={
                        query.isError ? (
                            <BusinessFailureState
                                title="同步记录加载失败"
                                description={query.error.message}
                                onRetry={() => void query.refetch()}
                            />
                        ) : undefined
                    }
                    emptyState={
                        <BusinessEmptyState
                            kind="no-data"
                            title="暂无同步记录"
                            description="在商品同步页开始同步后，这里会显示实际执行结果。"
                        />
                    }
                />
            }
        />
    )
}
