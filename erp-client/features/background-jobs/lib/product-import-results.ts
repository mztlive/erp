import { downloadWorkbook } from "@/lib/xlsx"
import { IMPORT_RESULT_LABELS } from "../labels"
import type { BackgroundJobItemStatus, ProductImportResults } from "../api"

/** 用文本保留编码、条码和金额原值，源图片公式也按文本写入。 */
export async function downloadProductImportResults(
    results: ProductImportResults,
    status: BackgroundJobItemStatus,
    jobNo: string,
) {
    const { Workbook } = await import("exceljs")
    const workbook = new Workbook()
    const sheet = workbook.addWorksheet("逐行结果", {
        views: [{ state: "frozen", ySplit: 1 }],
    })
    sheet.addRow(["原 Excel 行号", "处理结果", "处理说明", ...results.headers])
    for (const row of results.rows) {
        sheet.addRow([
            row.source_row_no,
            row.outcome_unknown
                ? "结果待核对"
                : status === "failed"
                  ? "执行失败"
                  : IMPORT_RESULT_LABELS[status],
            row.result_summary ?? "",
            ...row.cells,
        ])
    }
    sheet.columns.forEach((column) => {
        column.width = 24
    })
    sheet.getColumn(3).width = 52
    sheet.getRow(1).font = { bold: true }
    await downloadWorkbook(
        workbook,
        `商品导入-${jobNo}-${IMPORT_RESULT_LABELS[status]}.xlsx`,
    )
}
