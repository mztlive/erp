import type { Cell, Workbook } from "exceljs"
import type {
    CustomerImportResultRow,
    CustomerImportRow,
} from "@/features/customers/api/import"
import { downloadWorkbook, loadWorkbookFromFile } from "@/lib/xlsx"

const SHEET_NAME = "客户"
const MAX_ROWS = 1000
export const CUSTOMER_IMPORT_MAX_BYTES = 5 * 1024 * 1024
const columns = [
    { key: "legal_name", label: "客户名称", alias: "法定名称", width: 34 },
    { key: "short_name", label: "客户简称", alias: "简称", width: 22 },
    {
        key: "unified_credit_code",
        label: "统一社会信用代码",
        alias: "信用代码",
        width: 26,
    },
] as const

function textCell(cell: Cell): string | null {
    if (cell.isMerged) return null
    const value = cell.value
    if (value == null) return ""
    if (typeof value === "string") return value.trim()
    if (typeof value === "object" && "richText" in value)
        return cell.text.trim()
    return null
}

export async function readCustomerImportWorkbook(file: File) {
    if (!file.name.toLowerCase().endsWith(".xlsx"))
        throw new Error("请选择 .xlsx 格式的 Excel 文件")
    if (file.size > CUSTOMER_IMPORT_MAX_BYTES)
        throw new Error("文件不能超过 5 MB")

    const workbook = await loadWorkbookFromFile(file).catch(() => {
        throw new Error("无法读取 Excel，请另存为 .xlsx 后重新选择")
    })
    const sheet = workbook.getWorksheet(SHEET_NAME) ?? workbook.worksheets[0]
    if (!sheet) throw new Error("Excel 没有工作表")
    if (sheet.rowCount > 10_001)
        throw new Error("工作表超过 10000 行，请删除多余空白行后重试")

    const positions = new Map<(typeof columns)[number]["key"], number>()
    sheet.getRow(1).eachCell((cell, columnNumber) => {
        const header = textCell(cell)
        const definition = columns.find(
            (column) => column.label === header || column.alias === header,
        )
        if (!definition) return
        if (positions.has(definition.key))
            throw new Error(`「${definition.label}」列重复，请保留一列`)
        positions.set(definition.key, columnNumber)
    })
    if (!positions.has("legal_name"))
        throw new Error("第一行需要包含「客户名称」，可下载模板填写")

    const rows: CustomerImportRow[] = []
    sheet.eachRow((row, rowNumber) => {
        if (rowNumber === 1) return
        const errors: string[] = []
        let invalidName = false
        const values = columns.map(({ key, label }) => {
            const column = positions.get(key)
            if (column === undefined) return ""
            const cell = row.getCell(column)
            const text = textCell(cell)
            if (text !== null) return text
            invalidName ||= key === "legal_name"
            errors.push(`「${label}」请填写文本`)
            return ""
        })
        const [legalName = "", shortName = "", creditCode = ""] = values
        if (!legalName && !shortName && !creditCode && !errors.length) return
        if (!legalName && !invalidName) errors.push("客户名称不能为空")
        if ([...legalName].length > 200)
            errors.push("客户名称不能超过 200 个字符")
        if ([...shortName].length > 100)
            errors.push("客户简称不能超过 100 个字符")
        if (creditCode && !/^[a-z0-9]{18}$/i.test(creditCode))
            errors.push("统一社会信用代码必须是 18 位字母或数字")
        rows.push({
            row_number: rowNumber,
            legal_name: legalName,
            short_name: shortName || undefined,
            unified_credit_code: creditCode || undefined,
            error: errors.join("；") || undefined,
        })
        if (rows.length > MAX_ROWS)
            throw new Error("每个文件最多导入 1000 行客户资料")
    })
    if (!rows.length) throw new Error("请在表头下填写至少一行客户资料")
    return rows
}

function customerSheet(workbook: Workbook, includeResults = false) {
    const sheet = workbook.addWorksheet(SHEET_NAME, {
        views: [{ state: "frozen", ySplit: 1 }],
    })
    sheet.addRow([
        ...columns.map(({ label }) => label),
        ...(includeResults ? ["原 Excel 行号", "处理结果", "处理说明"] : []),
    ])
    columns.forEach(({ width }, index) => {
        const column = sheet.getColumn(index + 1)
        column.width = width
        column.numFmt = "@"
    })
    sheet.getRow(1).font = { bold: true }
    return sheet
}

export async function downloadCustomerImportTemplate() {
    const { Workbook } = await import("exceljs")
    const workbook = new Workbook()
    customerSheet(workbook)
    const guide = workbook.addWorksheet("填写说明")
    guide.getColumn(1).width = 90
    for (const text of [
        "在「客户」工作表填写，每行一个客户，最多 1000 行。",
        "客户名称填写企业法定全称。客户简称、统一社会信用代码可留空。",
        "统一社会信用代码按文本填写，须为 18 位字母或数字。请先将公式转换为文本。",
        "新客户默认启用，负责销售为提交人，客户编号自动生成。",
        "遇到已有客户或已有企业主体，请核对后在客户中心处理。",
        "提交后在「后台任务」查看结果，可按原 Excel 行号定位并下载对应行。",
    ])
        guide.addRow([text])
    await downloadWorkbook(workbook, "客户导入模板.xlsx")
}

export async function downloadCustomerImportResults(
    rows: CustomerImportResultRow[],
    label: string,
    jobNo: string,
) {
    const { Workbook } = await import("exceljs")
    const workbook = new Workbook()
    const sheet = customerSheet(workbook, true)
    sheet.getColumn(4).width = 16
    sheet.getColumn(5).width = 14
    sheet.getColumn(6).width = 64
    for (const row of rows) {
        sheet.addRow([
            row.legal_name,
            row.short_name ?? "",
            row.unified_credit_code ?? "",
            row.row_number,
            label,
            row.message,
        ])
    }
    await downloadWorkbook(workbook, `客户导入-${jobNo}-${label}.xlsx`)
}
