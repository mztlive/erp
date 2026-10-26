import type { BatchRow } from "./batch-supply"
import { SupplyBatchValidationError } from "./batch-supply-error"

/** 读取 Excel 剪贴板的表格文本，保留引号内换行和制表符。 */
export function parseDelimited(text: string, delimiter = "\t"): string[][] {
    const rows: string[][] = []
    let row: string[] = [],
        cell = "",
        quoted = false
    const source = text.replace(/^\uFEFF/, "")
    for (let i = 0; i < source.length; i++) {
        const char = source[i]
        if (char === '"' && (quoted || cell.length === 0)) {
            if (quoted && source[i + 1] === '"') {
                cell += '"'
                i++
            } else quoted = !quoted
        } else if (
            !quoted &&
            (char === delimiter || char === "\n" || char === "\r")
        ) {
            row.push(cell)
            cell = ""
            if (char !== delimiter) {
                rows.push(row)
                row = []
                if (char === "\r" && source[i + 1] === "\n") i++
            }
        } else cell += char
    }
    if (quoted)
        throw new SupplyBatchValidationError(
            "粘贴内容的引号未闭合，请检查原表格",
        )
    row.push(cell)
    rows.push(row)
    return rows.filter((cells) => cells.some((value) => value.trim()))
}
function downloadBlob(blob: Blob, name: string) {
    const url = URL.createObjectURL(blob)
    const link = document.createElement("a")
    link.href = url
    link.download = name
    link.click()
    setTimeout(() => URL.revokeObjectURL(url), 1000)
}
export function downloadFailedRows(rows: BatchRow[]) {
    const matrix = [
        ["公司 SKU 编号", "商品名称", "供应商订货编码", "处理状态", "说明"],
        ...rows
            .filter((row) => row.status !== "SUCCEEDED")
            .map((row) => [
                row.skuCode,
                row.skuName,
                row.supplierSkuCode,
                row.status === "UNKNOWN" ? "待确认结果" : "未完成",
                row.message,
            ]),
    ]
    const csv = matrix
        .map((row) =>
            row
                .map(
                    (cell) =>
                        `"${(/^[=+@-]/.test(cell) ? "'" : "") + cell.replaceAll('"', '""')}"`,
                )
                .join(","),
        )
        .join("\r\n")
    downloadBlob(
        new Blob(["\uFEFF" + csv], { type: "text/csv;charset=utf-8" }),
        "供给未完成明细.csv",
    )
}
