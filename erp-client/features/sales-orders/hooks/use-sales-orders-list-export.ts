import { useMutation } from "@tanstack/react-query"
import type { SalesOrdersListQuery } from "../api/contracts"
import { salesOrderExportQuery } from "../api/sales-orders-list"
import { runDataTransfer, downloadTransfer } from "@/lib/data-transfer"
import { toast } from "@/components/ui/toast"
import { getErrorMessage } from "@/lib/api/errors"
export const useSalesOrdersListExport = (
    query: SalesOrdersListQuery,
    total: number,
) => {
    const mutation = useMutation({
        mutationFn: async () => {
            const result = await runDataTransfer(
                "sales_orders",
                salesOrderExportQuery(query),
            )
            downloadTransfer(result)
            return {
                jobId: result.jobId,
                rowCount:
                    typeof result.metadata?.row_count === "number"
                        ? result.metadata.row_count
                        : 0,
                downloadLabel: result.file_name,
                exportedAt: new Date().toISOString(),
                fileName: result.file_name,
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
