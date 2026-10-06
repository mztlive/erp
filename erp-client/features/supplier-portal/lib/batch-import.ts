import type { PortalBatchMode } from "../types"
import {
    batchColumns,
    batchModeLabels,
    editBatchValues,
    isBatchRowLocked,
    newBatchRow,
    PORTAL_BATCH_BYTES,
    PORTAL_BATCH_LIMIT,
    type BatchEditorRow,
    type BatchField,
    type BatchValues,
} from "./batch-model"

/** 剪贴板和CSV保留引号内换行、分隔符和双引号。 */
export function parsePortalDelimited(
    text: string,
    delimiter = "\t",
): string[][] {
    const rows: string[][] = []
    let cells: string[] = [],
        value = "",
        quoted = false
    const source = text.replace(/^\uFEFF/, "")
    for (let position = 0; position < source.length; position++) {
        const char = source[position]
        if (char === '"' && (quoted || value === "")) {
            if (quoted && source[position + 1] === '"') {
                value += '"'
                position++
            } else quoted = !quoted
        } else if (
            !quoted &&
            (char === delimiter || char === "\r" || char === "\n")
        ) {
            cells.push(value)
            value = ""
            if (char !== delimiter) {
                rows.push(cells)
                cells = []
                if (char === "\r" && source[position + 1] === "\n") position++
            }
        } else value += char
    }
    if (quoted) throw new Error("表格内容的引号未闭合，请检查后重新粘贴")
    cells.push(value)
    rows.push(cells)
    return rows.filter((row) => row.some((cell) => cell.trim()))
}
const headerKey = (value: string) => value.trim().replace(/\s/g, "")
function requiredColumns(mode: PortalBatchMode): BatchField[] {
    if (mode === "availability")
        return ["skuNo", "orderingCode", "availability", "reason"]
    const common: BatchField[] = [
        "orderingCode",
        "dropshipPrice",
        "bulkPrice",
        "taxPercentage",
        "minimumQuantity",
        "regions",
        "validFrom",
    ]
    return mode === "new_product"
        ? [
              "groupCode",
              "productName",
              "productKind",
              "brandRaw",
              "categoryRaw",
              "skuName",
              "unitRaw",
              ...common,
          ]
        : ["skuNo", "reason", ...common]
}
export function importPortalMatrix(
    matrix: string[][],
    mode: PortalBatchMode,
): BatchEditorRow[] {
    if (matrix.length < 2) throw new Error("请包含模板表头和至少一条数据")
    if (matrix.length - 1 > PORTAL_BATCH_LIMIT)
        throw new Error("每批最多100条数据，请拆分后导入")
    const columns = batchColumns(mode)
    const headers = matrix[0].map(headerKey)
    if (new Set(headers).size !== headers.length)
        throw new Error("表头存在重复列，请核对模板")
    for (const header of headers)
        if (!columns.some((item) => headerKey(item.label) === header))
            throw new Error(
                `列「${matrix[0][headers.indexOf(header)]}」不属于本次操作，请下载对应模板`,
            )
    for (const key of requiredColumns(mode)) {
        const target = columns.find((item) => item.key === key)
        if (target && !headers.includes(headerKey(target.label)))
            throw new Error(
                `缺少「${target.label}」列，请使用${batchModeLabels[mode]}模板`,
            )
    }
    return matrix.slice(1).map((cells) => {
        if (cells.length > headers.length)
            throw new Error("部分数据超出表头列数，请核对分隔符和引号")
        const values = Object.fromEntries(
            columns
                .filter((item) => headers.includes(headerKey(item.label)))
                .map((item) => [
                    item.key,
                    (
                        cells[headers.indexOf(headerKey(item.label))] ?? ""
                    ).trim(),
                ]),
        ) as Partial<BatchValues>
        return newBatchRow(values)
    })
}
export function importPortalText(
    text: string,
    mode: PortalBatchMode,
): BatchEditorRow[] {
    if (new TextEncoder().encode(text).byteLength > PORTAL_BATCH_BYTES)
        throw new Error("粘贴内容不能超过5 MB，请拆分表格")
    return importPortalMatrix(
        parsePortalDelimited(
            text,
            text.split(/\r?\n/, 1)[0].includes("\t") ? "\t" : ",",
        ),
        mode,
    )
}
export async function readPortalBatchFile(
    file: File,
    mode: PortalBatchMode,
): Promise<BatchEditorRow[]> {
    if (file.size > PORTAL_BATCH_BYTES)
        throw new Error("文件不能超过5 MB，请拆分表格")
    if (/\.csv$/i.test(file.name)) {
        let source: string
        try {
            source = new TextDecoder("utf-8", { fatal: true }).decode(
                await file.arrayBuffer(),
            )
        } catch {
            throw new Error(
                "CSV必须使用UTF-8编码，请在Excel中另存为UTF-8 CSV后重试",
            )
        }
        return importPortalMatrix(parsePortalDelimited(source, ","), mode)
    }
    if (!/\.xlsx$/i.test(file.name))
        throw new Error("请选择Excel .xlsx或UTF-8 CSV文件")
    const { Workbook, ValueType } = await import("exceljs")
    const workbook = new Workbook()
    await workbook.xlsx.load(await file.arrayBuffer())
    const sheet =
        workbook.getWorksheet(batchModeLabels[mode]) ?? workbook.worksheets[0]
    if (!sheet) throw new Error("文件没有工作表，请按模板填写")
    if (
        sheet.rowCount > PORTAL_BATCH_LIMIT + 1 ||
        sheet.columnCount > batchColumns(mode).length
    )
        throw new Error("表格超出模板范围；每批最多100条数据，请清理多余行列")
    const matrix: string[][] = []
    sheet.eachRow((row) => {
        const cells: string[] = []
        for (let index = 1; index <= sheet.columnCount; index++) {
            const cell = row.getCell(index)
            if (cell.type === ValueType.Formula || cell.formula)
                throw new Error(`第${row.number}行含有公式，请先粘贴为值再导入`)
            cells.push(
                cell.value instanceof Date
                    ? cell.value.toISOString().slice(0, 10)
                    : cell.text,
            )
        }
        if (cells.some((cell) => cell.trim())) matrix.push(cells)
    })
    return importPortalMatrix(matrix, mode)
}
/** 单元格多格粘贴整次检查范围与锁定行，拒绝截断输入。 */
export function pastePortalCells(
    rows: BatchEditorRow[],
    rowId: string,
    field: BatchField,
    text: string,
    mode: PortalBatchMode,
): BatchEditorRow[] {
    if (new TextEncoder().encode(text).byteLength > PORTAL_BATCH_BYTES)
        throw new Error("粘贴内容不能超过5 MB")
    const matrix = parsePortalDelimited(text)
    const columns = batchColumns(mode)
    const startRow = rows.findIndex((row) => row.rowId === rowId)
    const startColumn = columns.findIndex((item) => item.key === field)
    if (
        startRow < 0 ||
        startColumn < 0 ||
        startRow + matrix.length > rows.length ||
        matrix.some((cells) => startColumn + cells.length > columns.length)
    )
        throw new Error(
            "粘贴超出当前表格范围，请先添加足够行数并核对模板列顺序",
        )
    if (rows.slice(startRow, startRow + matrix.length).some(isBatchRowLocked))
        throw new Error("粘贴范围包含已完成或待确认资料，请选择可编辑行")
    const updates = new Map<string, Partial<BatchValues>>()
    matrix.forEach((cells, index) =>
        updates.set(
            rows[startRow + index].rowId,
            Object.fromEntries(
                cells.map((value, offset) => [
                    columns[startColumn + offset].key,
                    value.trim(),
                ]),
            ),
        ),
    )
    return editBatchValues(rows, updates, mode)
}
function downloadBlob(blob: Blob, name: string, mode: PortalBatchMode) {
    const url = URL.createObjectURL(blob)
    const link = document.createElement("a")
    link.id = `supplier-portal-batch-${mode}-download-file`
    link.href = url
    link.download = name
    link.click()
    setTimeout(() => URL.revokeObjectURL(url), 1000)
}
export async function downloadPortalBatchTemplate(mode: PortalBatchMode) {
    const { Workbook } = await import("exceljs")
    const workbook = new Workbook()
    const sheet = workbook.addWorksheet(batchModeLabels[mode])
    sheet.addRow(batchColumns(mode).map((item) => item.label))
    sheet.columns.forEach((item) => {
        item.width = 24
        item.numFmt = "@"
    })
    sheet.getRow(1).font = { bold: true }
    const notes = workbook.addWorksheet("填写说明")
    const common = [
        "每批最多100条SKU或供给数据，文件不超过5 MB。此模板只用于标题对应的操作。",
        "编号、订货编码、条码按文本填写，保留前导零；供应商订货编码须使用自己的真实编码。",
        "价格、数量、税率按十进制数字填写；税率填百分数，如13；可供数量空白表示未提供，0表示明确为零。",
        "多个可供区域用顿号分隔；日期使用YYYY-MM-DD；失效日期留空表示长期有效。",
        "只接受UTF-8 CSV；Excel公式须先粘贴为值。导入和校验不会创建申请或更新可供。",
    ]
    const specific =
        mode === "new_product"
            ? [
                  "每行一个SKU，同一商品的所有行使用同一商品组编号；商品名称、类型、品牌、分类、型号及说明在组内必须一致。",
                  "商品类型：实物、虚拟、线下服务、卡券。品牌和分类原始值允许待内部匹配，但不能缺失；未知品牌与确实无品牌须如实区分。",
                  "原始单位须包含计量及包装含义。规格属性按规格名=取值填写，多个用分号分隔。",
                  "新品按整商品组保存草稿；每组成功后可打开原草稿补充图片并提交采购确认，无需重新建单。保存草稿不会自动进入审核。",
                  "图片不随表格导入，保存后在原草稿上传。提交并通过采购确认后，新建SKU由内部定价并上架。",
              ]
            : mode === "availability"
              ? [
                    "必须同时精确填写自己的公司SKU编号和供应商订货编码。可供状态仅允许有货或临时缺货。",
                    "提交后直接更新可供情况；恢复有货不会解除采购暂停、停止合作等限制。",
                ]
              : [
                    "已有商品报价只匹配采购向本供应商开放的公司SKU；已有供给调价只匹配自己的供给。",
                    "报价的可供状态仅允许有货或临时缺货。申请原因必填。",
                    "提交成功表示申请已创建、待采购确认；确认前价格及当前条款继续有效。",
                ]
    for (const note of [...common, ...specific]) notes.addRow([note])
    notes.getColumn(1).width = 110
    downloadBlob(
        new Blob([await workbook.xlsx.writeBuffer()], {
            type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        }),
        `${batchModeLabels[mode]}模板.xlsx`,
        mode,
    )
}
/** CSV单元格引号转义后增加公式防护，保留未完成的全部原始输入。 */
export function downloadPortalBatchErrors(
    rows: BatchEditorRow[],
    mode: PortalBatchMode,
) {
    const columns = batchColumns(mode)
    const matrix = [
        columns.map((item) => item.label).concat("处理结果", "说明"),
        ...rows
            .filter((row) => row.status !== "SUCCEEDED")
            .map((row) => [
                ...columns.map((item) => row.values[item.key]),
                row.status === "UNKNOWN" ? "待确认结果" : "未完成",
                row.message,
            ]),
    ]
    const csv = matrix
        .map((cells) =>
            cells
                .map(
                    (value) =>
                        `"${(formulaLike(value) ? "'" : "") + value.replaceAll('"', '""')}"`,
                )
                .join(","),
        )
        .join("\r\n")
    downloadBlob(
        new Blob([`\uFEFF${csv}`], { type: "text/csv;charset=utf-8" }),
        `${batchModeLabels[mode]}未完成明细.csv`,
        mode,
    )
}
function formulaLike(value: string): boolean {
    let position = 0
    while (
        position < value.length &&
        (value.charCodeAt(position) < 32 || /\s/.test(value[position]))
    )
        position++
    return /^[=+@-]/.test(value.slice(position))
}
