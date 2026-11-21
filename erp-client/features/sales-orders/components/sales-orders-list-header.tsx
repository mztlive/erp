import { FormalActionResult } from "@/components/business"
import type { SalesOrdersListExportJob } from "@/features/sales-orders/lib/sales-orders-list-csv"

export function SalesOrdersListExportResult({
    exportJob,
}: {
    exportJob: SalesOrdersListExportJob
}) {
    return (
        <FormalActionResult
            status="succeeded"
            title="导出完成"
            description={`已生成 CSV 文件，共 ${exportJob.rowCount} 行，仅包含当前筛选结果；导出后金额与状态以列表页最新数据为准。`}
            facts={[
                {
                    label: "文件",
                    value: exportJob.fileName,
                },
                {
                    label: "行数",
                    value: String(exportJob.rowCount),
                },
                {
                    label: "导出时间",
                    value: new Date(exportJob.exportedAt).toLocaleString(
                        "zh-CN",
                    ),
                },
            ]}
        />
    )
}
