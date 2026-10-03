import {
    BATCH_LIMIT,
    EXTRA_FIELDS,
    GRID_FIELDS,
    type BatchRow,
    type TextFieldKey,
} from "./batch-supply"
import { SupplyBatchValidationError } from "./batch-supply-error"

export const IMPORT_COLUMNS = [
    "公司 SKU 编号",
    ...GRID_FIELDS.map((field) => field.label),
    ...EXTRA_FIELDS.map((field) => field.label),
    "可供状态",
    "可供数量",
]
export const IMPORT_KEYS: (TextFieldKey | "skuCode" | "availabilityStatus")[] =
    [
        "skuCode",
        ...GRID_FIELDS.map((field) => field.key),
        ...EXTRA_FIELDS.map((field) => field.key),
        "availabilityStatus",
        "availableQuantity",
    ]
export type ImportRow = Partial<Record<(typeof IMPORT_KEYS)[number], string>>

/** Excel 剪贴板和 CSV 共享解析器，保留引号内换行和制表符。 */
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
export function importRows(matrix: string[][]): ImportRow[] {
    if (matrix.length < 2)
        throw new SupplyBatchValidationError("文件没有数据，请按模板填写后导入")
    const headers = matrix[0].map((value) => value.trim())
    for (const name of IMPORT_COLUMNS.slice(0, 3))
        if (!headers.includes(name))
            throw new SupplyBatchValidationError(
                `缺少列「${name}」，请使用供给模板`,
            )
    if (new Set(headers).size !== headers.length)
        throw new SupplyBatchValidationError("文件存在重复列名，请检查表头")
    if (matrix.length - 1 > BATCH_LIMIT)
        throw new SupplyBatchValidationError(
            `每批最多 ${BATCH_LIMIT} 行，请拆分文件`,
        )
    return matrix
        .slice(1)
        .map((cells) =>
            Object.fromEntries(
                IMPORT_COLUMNS.map((name, i) => [
                    IMPORT_KEYS[i],
                    headers.includes(name)
                        ? (cells[headers.indexOf(name)] ?? "").trim()
                        : "",
                ]),
            ),
        )
}
export async function readSupplyFile(file: File): Promise<ImportRow[]> {
    if (file.size > 5 * 1024 * 1024)
        throw new SupplyBatchValidationError("文件不能超过 5 MB")
    if (/\.csv$/i.test(file.name))
        return importRows(parseDelimited(await file.text(), ","))
    if (!/\.xlsx$/i.test(file.name))
        throw new SupplyBatchValidationError("请选择 .xlsx 或 UTF-8 CSV 文件")
    const { Workbook } = await import("exceljs")
    const workbook = new Workbook()
    await workbook.xlsx.load(await file.arrayBuffer())
    const sheet = workbook.getWorksheet("供给配置") ?? workbook.worksheets[0]
    if (!sheet || sheet.rowCount > BATCH_LIMIT + 1 || sheet.columnCount > 40)
        throw new SupplyBatchValidationError(
            `每批最多 ${BATCH_LIMIT} 行，请使用供给模板`,
        )
    const matrix: string[][] = []
    sheet.eachRow((row) => {
        const cells: string[] = []
        for (let i = 1; i <= sheet.columnCount; i++) {
            const cell = row.getCell(i)
            if (cell.formula)
                throw new SupplyBatchValidationError(
                    `第 ${row.number} 行包含公式，请先粘贴为值`,
                )
            cells.push(
                cell.value instanceof Date
                    ? cell.value.toISOString().slice(0, 10)
                    : cell.text,
            )
        }
        if (cells.some((value) => value.trim())) matrix.push(cells)
    })
    return importRows(matrix)
}
export async function downloadSupplyTemplate() {
    const { Workbook } = await import("exceljs")
    const workbook = new Workbook()
    const sheet = workbook.addWorksheet("供给配置")
    sheet.addRow(IMPORT_COLUMNS)
    sheet.columns.forEach((column) => {
        column.width = 24
        column.numFmt = "@"
    })
    sheet.getRow(1).font = { bold: true }
    const notes = workbook.addWorksheet("填写说明")
    for (const line of [
        "先在弹窗选择一个供应商；每批最多 100 行，只匹配已有公司 SKU。",
        "公司 SKU 编号和供应商订货编码按文本填写，保留前导零。",
        "两种含税价、税率、起订量、可供区域、生效日期均须明确填写。税率填百分数，例如 13。",
        "多个区域用顿号分隔；日期格式 YYYY-MM-DD；失效日期留空表示长期有效。",
        "可供状态：可供、不可供、停止供应、数据已过期；可供数量留空表示未提供，0 表示明确为零。",
        "导入仅加入待配置表格，校验并提交后才创建供给；不会覆盖已有供给。",
        "不支持公式，请粘贴为值后导入。",
    ])
        notes.addRow([line])
    notes.getColumn(1).width = 110
    const blob = new Blob([await workbook.xlsx.writeBuffer()], {
        type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    })
    downloadBlob(blob, "供应商供给配置模板.xlsx")
}
export function downloadBlob(blob: Blob, name: string) {
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
