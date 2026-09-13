import { useMutation } from "@tanstack/react-query"
import { fetchSalesOrders } from "@/features/sales-orders/api/sales-orders"
import type { SalesOrdersListQuery } from "@/features/sales-orders/api/contracts"
import { buildSalesOrdersListCsv } from "@/features/sales-orders/lib/sales-orders-list-csv"
import { collectExportPages, downloadListCsv } from "@/lib/list-export"
import { toast } from "@/components/ui/toast"
import { getErrorMessage } from "@/lib/api/errors"

/** 用已应用条件逐页查询并下载；失败时不输出部分结果，也不登记虚构后台任务。 */
export const useSalesOrdersListExport = (
    query: SalesOrdersListQuery,
    total: number,
) => {
    const mutation = useMutation({
        mutationFn: async () => {
            const items = await collectExportPages(
                (page, pageSize) =>
                    fetchSalesOrders({ ...query, page, pageSize }),
                (row) => row.id,
            )
            const now = new Date()
            const { fileName, content } = buildSalesOrdersListCsv(items, now)
            downloadListCsv(content, fileName)
            return {
                jobId: "",
                rowCount: items.length,
                downloadLabel: fileName,
                exportedAt: now.toISOString(),
                fileName,
            }
        },
        onError: (error) =>
            toast.add({
                title: "导出失败",
                description: getErrorMessage(error, "请重新查询后重试"),
                type: "error",
            }),
    })
    return {
        exportJob: mutation.data ?? null,
        exportCsv: () => {
            if (total > 0) mutation.mutate()
        },
        isExporting: mutation.isPending,
    }
}
