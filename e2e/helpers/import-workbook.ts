import { createRequire } from "node:module"
import fs from "node:fs/promises"
import path from "node:path"

import type { CellValue, Workbook } from "../../erp-client/node_modules/exceljs"

// 使用前端实际安装的工作簿实现，避免在 E2E 再维护一份 Excel 依赖。
const requireClient = createRequire(
    new URL("../../erp-client/package.json", import.meta.url),
)
const { Workbook: ExcelWorkbook } = requireClient(
    "exceljs",
) as typeof import("../../erp-client/node_modules/exceljs")

/** 按真实模板生成测试工作簿，编号与金额保持文本单元格。 */
export async function writeImportWorkbook(
    outputPath: string,
    sheetName: string,
    headers: readonly string[],
    rows: readonly (readonly CellValue[])[],
): Promise<string> {
    const workbook = new ExcelWorkbook()
    const sheet = workbook.addWorksheet(sheetName)
    sheet.addRow([...headers])
    for (const row of rows) sheet.addRow([...row])
    sheet.columns.forEach((column) => {
        column.numFmt = "@"
        column.width = 22
    })
    await fs.mkdir(path.dirname(outputPath), { recursive: true })
    await workbook.xlsx.writeFile(outputPath)
    return outputPath
}

/** 读取浏览器下载产物，供断言原行号、失败原因和文本安全性。 */
export async function readImportWorkbook(inputPath: string): Promise<Workbook> {
    const workbook = new ExcelWorkbook()
    await workbook.xlsx.readFile(inputPath)
    return workbook
}
