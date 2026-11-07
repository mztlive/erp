import type { CellValue, Workbook } from "exceljs"
import { multiplyFixed, compactFixed } from "@/lib/fixed-decimal"
import { downloadWorkbook, loadWorkbookFromFile } from "@/lib/xlsx"

export const SUPPLIER_IMPORT_HEADERS = [
    "供应商编号",
    "供应商全称",
    "联系人",
    "联系方式",
    "对公银行账号",
    "开户行",
    "供应商地址",
    "公司签约主体",
    "公司付款主体",
    "结算方式",
    "经营类目",
    "合同编号",
    "合同有效期",
    "合同文件",
    "授权书文件",
    "授权书有效期",
    "食品经营许可证",
    "供应商法人身份证",
    "发票类型",
    "发票税点",
    "供应商合作期初评分",
    "供应商评级",
    "供应商合作中评分",
] as const
export type SupplierImportRow = {
    row_number: number
    cells: string[]
    party_no: string
    supplier_no: string
    effective_from: string
    parse_errors: string[]
}
export type SupplierImportResult = {
    row_number: number
    name: string
    status: "succeeded" | "skipped" | "failed" | "uncertain"
    message: string
    supplier_id: string | null
    supplier_no: string | null
}

const businessCode = (prefix: string) =>
    `${prefix}-${Date.now().toString(36).toUpperCase()}${crypto.randomUUID().replaceAll("-", "").slice(0, 6).toUpperCase()}`
const businessDate = () =>
    new Intl.DateTimeFormat("en-CA", {
        timeZone: "Asia/Shanghai",
        year: "numeric",
        month: "2-digit",
        day: "2-digit",
    }).format(new Date())

const UNRESOLVED_CELL_PREFIX = "【待修正】"
const UNRESOLVED_ERRORS_HEADER = "待处理读取错误（修正后清空）"

/** 仅保留可见文本，不把原公式或错误对象写回可执行单元格。 */
const rejectedCellText = (value: CellValue) =>
    UNRESOLVED_CELL_PREFIX +
    (typeof value === "object" ? JSON.stringify(value) : String(value))

/** 文本标识保持前导零；拒绝精度已经丢失的数字账号和公式。 */
export const supplierCellText = (
    value: CellValue,
    column: number,
    format: string,
): string => {
    if (value == null) return ""
    if (
        typeof value === "string" &&
        value.trim().startsWith(UNRESOLVED_CELL_PREFIX)
    )
        throw new Error("请修正原单元格内容并移除【待修正】标记")
    if (value instanceof Date) {
        if (Number.isNaN(value.valueOf())) throw new Error("单元格日期无效")
        return value.toISOString().slice(0, 10)
    }
    if (typeof value === "number") {
        if (!Number.isFinite(value)) throw new Error("单元格数值无效")
        if (
            [0, 3, 4].includes(column) &&
            (!Number.isSafeInteger(value) ||
                String(Math.trunc(value)).length > 15)
        )
            throw new Error(
                "编号、电话或银行账号超过数字精度，请在 Excel 中改为文本后重新填写",
            )
        if (column === 19 && format.includes("%"))
            return `${compactFixed(multiplyFixed(String(value), "100", { leftMaxScale: 6, rightMaxScale: 0, outputScale: 4 }))}%`
        return String(value)
    }
    if (typeof value === "object") {
        if ("formula" in value || "sharedFormula" in value)
            throw new Error("导入单元格不能包含公式，请粘贴为值")
        if ("richText" in value)
            return value.richText
                .map((part) => part.text)
                .join("")
                .trim()
        if ("hyperlink" in value) return value.hyperlink
        throw new Error("单元格包含错误或不支持的内容")
    }
    return String(value).trim()
}

/** 校验完整表头，保留 Excel 原行号与局部解析错误。 */
export const supplierWorkbookRows = (
    workbook: Workbook,
): SupplierImportRow[] => {
    const sheet = workbook.worksheets.find(
        (sheet) => sheet.getCell(1, 2).text.trim() === "供应商全称",
    )
    if (!sheet) throw new Error("未找到供应商模板，请保留第 1 行表头")
    SUPPLIER_IMPORT_HEADERS.forEach((name, index) => {
        if (sheet.getCell(1, index + 1).text.trim() !== name)
            throw new Error(
                `第 ${index + 1} 列应为“${name}”，请使用原供应商模板`,
            )
    })
    const imageRows = new Set(
        sheet
            .getImages()
            .map((image) => Math.floor(image.range.tl.nativeRow) + 1),
    )
    const rows: SupplierImportRow[] = []
    sheet.eachRow({ includeEmpty: false }, (row, number) => {
        if (number === 1) return
        const errors: string[] = []
        if (
            sheet.getCell(1, 26).text === UNRESOLVED_ERRORS_HEADER &&
            row.getCell(26).text.trim()
        )
            errors.push(row.getCell(26).text)
        const cells = SUPPLIER_IMPORT_HEADERS.map((name, index) => {
            try {
                return supplierCellText(
                    row.getCell(index + 1).value,
                    index,
                    row.getCell(index + 1).numFmt ?? "",
                )
            } catch (error) {
                errors.push(
                    `${name}：${error instanceof Error ? error.message : "读取失败"}`,
                )
                const value = row.getCell(index + 1).value
                return typeof value === "string" &&
                    value.trim().startsWith(UNRESOLVED_CELL_PREFIX)
                    ? value
                    : rejectedCellText(value)
            }
        })
        if (cells.every((value) => !value) && !errors.length) return
        if (imageRows.has(number))
            errors.push("该行包含浮动图片，需通过供应商附件上传登记")
        rows.push({
            row_number: number,
            cells,
            party_no: businessCode("PTY"),
            supplier_no: businessCode("SUP"),
            effective_from: businessDate(),
            parse_errors: errors,
        })
    })
    if (!rows.length || rows.length > 500)
        throw new Error("每次导入应包含 1–500 行供应商数据")
    return rows
}

export const readSupplierFile = async (file: File) => {
    if (!file.name.toLowerCase().endsWith(".xlsx"))
        throw new Error("请选择 .xlsx 格式的供应商模板")
    if (file.size > 10 * 1024 * 1024) throw new Error("文件不能超过 10 MB")
    return supplierWorkbookRows(await loadWorkbookFromFile(file))
}

/** 失败清单含原数据，保存为文本单元格，避免公式执行和长账号精度丢失。 */
export const supplierFailuresWorkbook = async (
    rows: SupplierImportRow[],
    results: SupplierImportResult[],
) => {
    const { Workbook } = await import("exceljs")
    const workbook = new Workbook()
    const sheet = workbook.addWorksheet("失败行")
    sheet.addRow([
        ...SUPPLIER_IMPORT_HEADERS,
        "原始行号",
        "失败原因",
        UNRESOLVED_ERRORS_HEADER,
    ])
    for (const result of results.filter((row) => row.status === "failed")) {
        const row = rows.find((row) => row.row_number === result.row_number)
        if (row)
            sheet.addRow([
                ...row.cells,
                String(row.row_number),
                [...row.parse_errors, result.message].join("；"),
                row.parse_errors.join("；"),
            ])
    }
    sheet.columns.forEach((column) => {
        column.width = 22
        column.numFmt = "@"
    })
    sheet.getRow(1).font = { bold: true }
    return workbook
}

/** 生成与导入解析器共用表头的空白模板，说明不进入数据行。 */
export const supplierTemplateWorkbook = async () => {
    const { Workbook } = await import("exceljs")
    const workbook = new Workbook()
    const sheet = workbook.addWorksheet("供应商")
    sheet.addRow([...SUPPLIER_IMPORT_HEADERS])
    sheet.columns.forEach((column) => {
        column.width = 24
        column.numFmt = "@"
    })
    sheet.getRow(1).font = { bold: true }
    sheet.views = [{ state: "frozen", ySplit: 1 }]
    const guide = workbook.addWorksheet("填写说明")
    for (const instruction of [
        "在「供应商」工作表从第 2 行起填写，每行一个供应商，每次 1–500 行，文件不超过 10 MB。保留第 1 行的全部表头和列顺序。",
        "供应商全称、结算方式、发票类型必须填写。公司签约主体和公司付款主体至少填写一项，使用系统中已维护的公司全称、简称或别名。",
        "联系人与联系方式必须同时填写；对公银行账号与开户行必须同时填写。",
        "结算方式填写：预付款、现结、周结、月结、季结、半年结或年结。发票类型填写：专票、普票或电子发票。",
        "编号、联系方式和银行账号按文本填写，保留前导零。所有单元格只填写值，不使用公式。",
        "合同文件、授权书文件、授权书有效期、食品经营许可证、供应商法人身份证列留空，导入后在供应商资料中上传和登记；不要在模板中嵌入图片或填写文件名。",
        "合同有效期按 YYYY-MM-DD 填写，并同时填写合同编号；不明确的有效期留空。",
        "提交后在「后台任务」查看进度和逐行结果，下载待处理行并修正后重新导入。",
    ]) {
        guide.addRow([instruction])
    }
    guide.getColumn(1).width = 100
    guide.getColumn(1).alignment = { wrapText: true, vertical: "middle" }
    guide.eachRow((row) => {
        row.height = 48
    })
    return workbook
}

/** 下载可直接填写的供应商导入模板。 */
export const downloadSupplierTemplate = async () => {
    await downloadSupplierWorkbook(
        await supplierTemplateWorkbook(),
        "供应商导入模板.xlsx",
    )
}

/** 下载仅包含失败行的工作簿。 */
export const downloadSupplierFailures = async (
    rows: SupplierImportRow[],
    results: SupplierImportResult[],
) => {
    const workbook = await supplierFailuresWorkbook(rows, results)
    await downloadSupplierWorkbook(workbook, "供应商导入失败行.xlsx")
}

/** 保存供应商工作簿并释放临时下载地址。 */
const downloadSupplierWorkbook = async (
    workbook: Workbook,
    filename: string,
) => {
    await downloadWorkbook(workbook, filename, { revokeDelayMs: 1000 })
}
