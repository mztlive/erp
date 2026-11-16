import type { Cell, Workbook } from "exceljs"
import type {
    BrandImportResult,
    BrandImportRow,
} from "@/features/master-data/api/brand-import"
import { downloadWorkbook, loadWorkbookFromFile } from "@/lib/xlsx"

/** 表头按名称匹配，允许只提供品牌名称一列。 */
export async function readBrandFile(file: File): Promise<BrandImportRow[]> {
    if (!file.name.toLowerCase().endsWith(".xlsx"))
        throw new Error("请选择 .xlsx 格式的 Excel 文件")
    if (file.size > 5 * 1024 * 1024) throw new Error("文件不能超过 5 MB")
    const workbook = await loadWorkbookFromFile(file)
    const sheet = workbook.getWorksheet("品牌") ?? workbook.worksheets[0]
    if (!sheet) throw new Error("文件没有工作表")
    if (sheet.rowCount > 10001)
        throw new Error("工作表超过 10000 行，请删除多余空白行后重试")
    let nameColumn = 0
    let codeColumn = 0
    sheet.getRow(1).eachCell((cell, column) => {
        const header = cell.text.trim()
        if (["品牌名称", "名称"].includes(header)) {
            if (nameColumn) throw new Error("品牌名称列重复，请保留一列")
            nameColumn = column
        }
        if (["品牌代码", "代码"].includes(header)) {
            if (codeColumn) throw new Error("品牌代码列重复，请保留一列")
            codeColumn = column
        }
    })
    if (!nameColumn)
        throw new Error("第一行需要包含「品牌名称」列，请下载模板填写")
    const rows: BrandImportRow[] = []
    sheet.eachRow((row, rowNumber) => {
        if (rowNumber === 1) return
        const nameCell = row.getCell(nameColumn)
        const codeCell = codeColumn ? row.getCell(codeColumn) : undefined
        const name = nameCell.text.trim()
        const brand_code = codeCell?.text.trim() ?? ""
        if (
            !name &&
            !brand_code &&
            !unsupportedCell(nameCell) &&
            (!codeCell || !unsupportedCell(codeCell))
        )
            return
        const errors: string[] = []
        if (!name) errors.push("品牌名称不能为空")
        if ([...name].length > 128) errors.push("品牌名称不能超过 128 个字符")
        if ([...brand_code].length > 64)
            errors.push("品牌代码不能超过 64 个字符")
        for (const cell of [nameCell, codeCell]) {
            if (cell && unsupportedCell(cell))
                errors.push("请将公式、错误值转换为文本后导入")
        }
        rows.push({
            row_number: rowNumber,
            name,
            brand_code,
            error: errors.join("；") || undefined,
        })
    })
    if (!rows.length || rows.length > 1000)
        throw new Error("每个文件应包含 1–1000 行品牌")
    return rows
}

function unsupportedCell(cell: Cell) {
    const value = cell.value
    return (
        value !== null &&
        typeof value === "object" &&
        ("formula" in value ||
            "sharedFormula" in value ||
            "error" in value ||
            value instanceof Date)
    )
}

async function downloadBrandWorkbook(workbook: Workbook, name: string) {
    await downloadWorkbook(workbook, name, { revokeDelayMs: 1000 })
}

export async function downloadBrandTemplate() {
    const { Workbook } = await import("exceljs")
    const workbook = new Workbook()
    const sheet = workbook.addWorksheet("品牌")
    sheet.addRow(["品牌名称", "品牌代码"])
    sheet.columns.forEach((column) => {
        column.width = 32
        column.numFmt = "@"
    })
    sheet.getRow(1).font = { bold: true }
    const guide = workbook.addWorksheet("填写说明")
    for (const text of [
        "在「品牌」工作表填写，每行一个品牌，最多 1000 行。",
        "品牌名称必填；品牌代码选填，留空自动生成。",
        "新品牌默认启用，Logo 可在导入后补充。",
        "相同名称或代码的已有品牌会核对后跳过，不修改已有资料；名称和代码冲突的行导入失败。",
        "提交成功后可关闭页面，在「后台任务」查看进度和逐行结果；网络响应超时未收到确认时，请在原窗口点击「重新确认」。",
        "修改失败行后重新导入即可。请将公式转换为文本。",
    ])
        guide.addRow([text])
    guide.getColumn(1).width = 100
    await downloadBrandWorkbook(workbook, "品牌导入模板.xlsx")
}

export async function downloadBrandFailures(
    rows: BrandImportRow[],
    results: BrandImportResult[],
) {
    const { Workbook } = await import("exceljs")
    const workbook = new Workbook()
    const sheet = workbook.addWorksheet("品牌")
    sheet.addRow(["品牌名称", "品牌代码", "原始行号", "失败原因"])
    const failures = new Map(
        results
            .filter((result) => result.status === "failed")
            .map((result) => [result.row_number, result.message]),
    )
    for (const row of rows) {
        const error = row.error || failures.get(row.row_number)
        if (error)
            sheet.addRow([
                row.name,
                row.brand_code,
                String(row.row_number),
                error,
            ])
    }
    sheet.columns.forEach((column) => {
        column.width = 32
        column.numFmt = "@"
    })
    sheet.getRow(1).font = { bold: true }
    await downloadBrandWorkbook(workbook, "品牌导入失败行.xlsx")
}
