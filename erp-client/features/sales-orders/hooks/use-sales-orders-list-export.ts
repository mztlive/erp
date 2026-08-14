import * as React from "react"

import { fetchSalesOrders } from "@/features/sales-orders/api/sales-orders"
import type { SalesOrdersListQuery } from "@/features/sales-orders/api/contracts"
import { useCreateSalesOrderExportJobMutation } from "@/features/sales-orders/hooks/queries"
import {
    buildSalesOrdersListCsv,
    type SalesOrdersListExportJob,
} from "@/features/sales-orders/lib/sales-orders-list-csv"

/**
 * 列表 CSV 导出：创建后台导出任务、拉取当前筛选全集、生成文件并触发下载。
 * 行数通过列表 total 传参，避免页面重复读取查询结果。
 */
export function useSalesOrdersListExport(
    query: SalesOrdersListQuery,
    total: number,
) {
    const exportMutation = useCreateSalesOrderExportJobMutation()
    const [exportJob, setExportJob] =
        React.useState<SalesOrdersListExportJob | null>(null)

    const exportCsv = React.useCallback(async () => {
        if (total === 0) return
        const job = await exportMutation.mutateAsync({ rowCount: total })
        const all = await fetchSalesOrders({
            ...query,
            page: 1,
            pageSize: total,
        })
        const now = new Date()
        const { fileName, content } = buildSalesOrdersListCsv(all.items, now)
        setExportJob({
            jobId: job.jobId,
            rowCount: job.rowCount,
            downloadLabel: fileName,
            exportedAt: now.toISOString(),
            fileName,
        })

        const url = URL.createObjectURL(
            new Blob([content], { type: "text/csv;charset=utf-8" }),
        )
        const anchor = document.createElement("a")
        anchor.href = url
        anchor.download = fileName
        anchor.click()
        URL.revokeObjectURL(url)
    }, [exportMutation, query, total])

    return { exportJob, exportCsv, isExporting: exportMutation.isPending }
}
