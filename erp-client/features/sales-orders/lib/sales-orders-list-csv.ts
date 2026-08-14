import type { SalesOrderListItem } from "@/features/sales-orders/types"
import { NATURE_LABEL, ORIGIN_LABEL } from "@/features/sales-orders/lib/labels"

export type SalesOrdersListExportJob = {
    jobId: string
    rowCount: number
    downloadLabel: string
    exportedAt: string
    fileName: string
}

/**
 * 列表导出 CSV（含 BOM），结构与列定义一致：
 * 销售单号,客户,合同,业务性质,状态,创建来源,成交金额（含税）,负责人,提交时间。
 */
export function buildSalesOrdersListCsv(
    orders: SalesOrderListItem[],
    now: Date,
): { fileName: string; content: string } {
    const pad = (n: number) => String(n).padStart(2, "0")
    const datePart = `${now.getFullYear()}${pad(now.getMonth() + 1)}${pad(now.getDate())}`
    const timePart = `${pad(now.getHours())}${pad(now.getMinutes())}`
    const fileName = `销售单列表_${datePart}_${timePart}.csv`

    const quote = (value: string) => `"${value.replaceAll('"', '""')}"`
    const rows = orders.map((order) =>
        [
            order.documentNumber,
            order.customerName,
            order.contractNumber,
            NATURE_LABEL[order.nature],
            order.primaryStatus.label,
            ORIGIN_LABEL[order.originSystem],
            order.amountGross,
            order.ownerName,
            order.submittedAt,
        ]
            .map((value) => quote(String(value)))
            .join(","),
    )
    const csv = [
        `# 导出时间 ${now.toLocaleString("zh-CN")}；仅包含当前筛选结果，金额以列表页最新数据为准。`,
        "销售单号,客户,合同,业务性质,状态,创建来源,成交金额（含税）,负责人,提交时间",
        ...rows,
    ].join("\n")
    return { fileName, content: `\uFEFF${csv}` }
}
