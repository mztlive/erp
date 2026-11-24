import type { Cell, Worksheet } from "exceljs"
import {
    regionPath,
    type RegionIndex,
} from "@/features/regions/lib/region-index"
import { clipboardRows } from "@/lib/clipboard-rows"
import { compactFixed } from "@/lib/fixed-decimal"
import { createWorkbook, loadWorkbookFromFile, workbookBlob } from "@/lib/xlsx"
import type { SalesOrderDraftLineInput } from "../types"
import {
    addressColumns,
    allocationBaseline,
    isWorkbookProduct,
    productIdentity,
    productLabel,
    workbookScope,
    type DeliveryImportIssue,
    type DeliveryImportRow,
    type DeliveryImportSource,
    type DeliveryWorkbookContext,
} from "./sales-delivery-import"

const sheetName = "收货安排"
const metadataName = "收货安排记录"
const legacyVersion = "sales-delivery-arrangements-v1"
const version = "sales-delivery-arrangements-v2"
const addressHeaders = addressColumns.map(([name]) => name)
const headers = [
    "明细编号",
    "商品名称",
    "规格",
    "单位",
    "销售数量",
    ...addressHeaders,
    "原地址编号",
]
const productDisplayColumnCount = 3
const legacyHeaders = [...headers.slice(0, 3), "类型", ...headers.slice(3)]
const maxRows = 20_000
const maxFileBytes = 5 * 1024 * 1024
const productHeaders = [
    "商品明细",
    "商品名称",
    "商品",
    "SKU",
    "SKU编码",
    "商品编码",
]
const mixedProductsMessage =
    "名单包含多种商品。请按商品分别导入，或使用本单导出的表格"

function setColumns(
    sheet: Worksheet,
    headers: readonly string[],
    widths: number[],
) {
    sheet.addRow([...headers])
    sheet.columns.forEach((column, index) => {
        column.width = widths[index]
        column.numFmt = "@"
        column.font = { name: "Microsoft YaHei", size: 11 }
    })
    sheet.getRow(1).height = 24
    sheet.getRow(1).font = { name: "Microsoft YaHei", size: 11, bold: true }
    sheet.getRow(1).alignment = { vertical: "middle" }
    sheet.views = [{ state: "frozen", ySplit: 1, showGridLines: false }]
}

function productCells(
    line: SalesOrderDraftLineInput,
    index: number,
    unitName: string,
) {
    return [
        String(index + 1),
        line.name.trim(),
        line.specification?.trim() ?? "",
        unitName,
    ]
}

export async function createSalesDeliveryWorkbook(
    context: DeliveryWorkbookContext,
    regions: RegionIndex,
) {
    const workbook = await createWorkbook()
    const sheet = workbook.addWorksheet(sheetName)
    setColumns(
        sheet,
        headers,
        [12, 30, 23, 10, 14, 16, 20, 16, 17, 17, 46, 14, 14],
    )
    sheet.views = [
        { state: "frozen", xSplit: 2, ySplit: 1, showGridLines: false },
    ]
    sheet.getColumn(headers.length).hidden = true
    sheet.autoFilter = { from: "A1", to: "L1" }
    sheet.getColumn(7).alignment = { horizontal: "left" }
    sheet.getRow(1).eachCell((cell, index) => {
        cell.fill = {
            type: "pattern",
            pattern: "solid",
            fgColor: { argb: index <= 5 ? "FFF1F3F5" : "FFDDEDEB" },
        }
    })
    const notes = workbook.addWorksheet("填写说明")
    notes.getColumn(1).width = 100
    notes.addRow([
        "填写收货人至配送数量。同一商品需要多个地址时，复制该商品整行后填写。",
    ])
    notes.addRow([
        "更改原地址时直接编辑所在行。导回后按商品替换本表填写的地址，删除的地址会在预览中列出。",
    ])
    notes.addRow([
        "整项商品跳过时删除该商品的全部行，或使用没有填写地址的原始空行。",
    ])
    notes.addRow([
        "联系电话使用文本格式。各地址的配送数量合计须等于该商品的销售数量。",
    ])
    if (!context.scope.startsWith("draft:")) {
        notes.addRow([
            "新建页面导出的文件请在当前页面导回。需要稍后导入时，先保存并打开草稿，再从草稿导出。",
        ])
    }
    notes.getColumn(1).font = { name: "Microsoft YaHei", size: 11 }
    const metadata = workbook.addWorksheet(metadataName, {
        state: "veryHidden",
    })
    metadata.addRow([version, await workbookScope(context)])
    const products = context.lines
        .map((line, index) => ({ line, index }))
        .filter(({ line }) => isWorkbookProduct(line))
    if (!products.length) throw new Error("请先选择商品")
    const baselines = await Promise.all(
        products.map(({ line }) => allocationBaseline(line)),
    )
    products.forEach(({ line, index }, productIndex) => {
        const cells = productCells(
            line,
            index,
            context.unitNames?.get(line.unit.trim()) ?? "",
        )
        const quantity = compactFixed(line.quantity)
        metadata.addRow([
            cells[0],
            line.rowKey,
            productIdentity(line),
            baselines[productIndex],
            ...cells.slice(1),
        ])
        const allocations = line.deliveryAllocations ?? []
        if (!allocations.length) sheet.addRow([...cells, quantity])
        allocations.forEach((allocation, addressIndex) => {
            const [province, city, district] = regionPath(
                regions,
                allocation.region_code,
            )
            sheet.addRow([
                ...cells,
                quantity,
                allocation.recipient,
                allocation.phone,
                province?.name ?? "",
                city?.name ?? "",
                district?.name ?? "",
                allocation.address,
                compactFixed(allocation.quantity),
                String(addressIndex + 1),
            ])
        })
    })
    return workbookBlob(workbook)
}

function readCell(cell: Cell, allowNumber = false): string {
    const value = cell.value
    if (value == null) return ""
    if (typeof value === "string") return value.trim()
    if (allowNumber && typeof value === "number" && Number.isFinite(value))
        return String(value)
    if (typeof value === "object" && "richText" in value)
        return value.richText
            .map((part) => part.text)
            .join("")
            .trim()
    if (typeof value === "object" && "hyperlink" in value)
        return value.text.trim()
    throw new Error("请填写文本或数值，公式请先粘贴为值")
}

function readAddressCells(getCell: (index: number) => string) {
    const errors: string[] = []
    const cells = addressHeaders.map((header, index) => {
        try {
            return getCell(index)
        } catch (error) {
            errors.push(`${header}：${(error as Error).message}`)
            return ""
        }
    })
    return { cells, errors }
}

function readProductIdentity(identity: string, legacy: boolean) {
    if (!legacy) return identity
    const parts: unknown = JSON.parse(identity)
    // V1 的第四项是明细类型标记。新旧表格均通过稳定行键和 SKU 身份定位商品。
    if (
        !Array.isArray(parts) ||
        parts.length !== 4 ||
        typeof parts[3] !== "boolean"
    )
        throw new Error("文件中的商品明细记录损坏，请重新导出")
    return JSON.stringify(parts.slice(0, 3))
}

function readExportedRows(
    sheet: Worksheet,
    metadata: Worksheet,
    context: DeliveryWorkbookContext,
    legacy: boolean,
): DeliveryImportSource {
    const expectedHeaders = legacy ? legacyHeaders : headers
    const displayCount = productDisplayColumnCount + (legacy ? 1 : 0)
    if (
        expectedHeaders.some(
            (header, index) =>
                readCell(sheet.getRow(1).getCell(index + 1)) !== header,
        )
    )
        throw new Error("表头与本单导出文件不一致，请保留原列并重新导入")
    const products = new Map<
        string,
        { lineKey: string; identity: string; display: string[] }
    >()
    const baselines = new Map<string, string>()
    metadata.eachRow((row, index) => {
        if (index === 1) return
        const [id, lineKey, identity, baseline, ...display] = Array.from(
            { length: 4 + displayCount },
            (_, column) => readCell(row.getCell(column + 1)),
        )
        if (
            !id ||
            !lineKey ||
            !identity ||
            !baseline ||
            products.has(id) ||
            baselines.has(lineKey)
        )
            throw new Error("文件中的商品明细记录损坏，请重新导出")
        products.set(id, {
            lineKey,
            identity: readProductIdentity(identity, legacy),
            display,
        })
        baselines.set(lineKey, baseline)
    })
    const current = new Map(context.lines.map((line) => [line.rowKey, line]))
    const rows: DeliveryImportRow[] = []
    sheet.eachRow((row, index) => {
        if (index === 1) return
        const { cells, errors } = readAddressCells((column) =>
            readCell(
                row.getCell(column + displayCount + 3),
                column === 1 || column === 6,
            ),
        )
        if (!errors.length && !cells.some(Boolean)) return
        let lineKey = ""
        let label = ""
        let addressId = ""
        try {
            const id = readCell(row.getCell(1), true)
            const product = products.get(id)
            const line = product && current.get(product.lineKey)
            label = [id, readCell(row.getCell(2))].filter(Boolean).join(" · ")
            if (
                !product ||
                !line ||
                !isWorkbookProduct(line) ||
                product.identity !== productIdentity(line)
            )
                throw new Error(
                    "商品明细已删除、更换或无法识别，请重新导出或复制完整商品行",
                )
            if (
                product.display.some(
                    (value, position) =>
                        readCell(row.getCell(position + 2)) !== value,
                )
            )
                throw new Error(
                    "商品资料与明细编号不一致，请复制导出文件中的完整商品行",
                )
            lineKey = line.rowKey
            label = [id, ...product.display.slice(0, 2)]
                .filter(Boolean)
                .join(" · ")
            addressId = readCell(row.getCell(expectedHeaders.length))
        } catch (error) {
            errors.push((error as Error).message)
        }
        rows.push({ row: index, lineKey, label, cells, addressId, errors })
    })
    return { rows, baselines }
}

function findAddressColumns(header: string[]) {
    const normalized = header.map((name) => name.replace(/\s+/g, ""))
    return addressColumns.map((aliases) =>
        normalized.findIndex((name) => aliases.some((alias) => alias === name)),
    )
}

function validateHeader(columns: number[]) {
    const missing = addressHeaders
        .slice(0, -1)
        .filter((_, index) => columns[index] < 0)
    if (missing.length)
        throw new Error(
            `表头缺少${missing.join("、")}。请补齐收货地址各列后重新导入`,
        )
}

function requireProduct(context: DeliveryWorkbookContext, lineKey: string) {
    const index = context.lines.findIndex(
        (line) => line.rowKey === lineKey && isWorkbookProduct(line),
    )
    if (index < 0) throw new Error("请先选择名单对应的商品明细")
    return productLabel(context.lines[index], index)
}

function readCustomerSheet(
    sheet: Worksheet,
    context: DeliveryWorkbookContext,
    lineKey: string,
): DeliveryImportSource {
    const label = requireProduct(context, lineKey)
    const header = sheet.getRow(1)
    const names = Array.from({ length: header.cellCount }, (_, index) =>
        readCell(header.getCell(index + 1)),
    )
    const columns = findAddressColumns(names)
    validateHeader(columns)
    const rows: DeliveryImportRow[] = []
    const productColumn = names.findIndex((name) =>
        productHeaders.includes(name),
    )
    const productNames = new Set<string>()
    sheet.eachRow((row, index) => {
        if (index === 1) return
        const { cells, errors } = readAddressCells((column) =>
            columns[column] < 0
                ? ""
                : readCell(
                      row.getCell(columns[column] + 1),
                      column === 1 || column === 6,
                  ),
        )
        if (!errors.length && !cells.some(Boolean)) return
        if (productColumn >= 0) {
            const productName = readCell(row.getCell(productColumn + 1), true)
            if (productName) productNames.add(productName)
        }
        rows.push({ row: index, lineKey, label, cells, errors })
    })
    if (productNames.size > 1) throw new Error(mixedProductsMessage)
    return { rows }
}

export async function readSalesDeliveryWorkbook(
    file: File,
    context: DeliveryWorkbookContext,
    target: string,
) {
    if (!/\.xlsx$/i.test(file.name) || file.size > maxFileBytes)
        throw new Error("请选择不超过 5 MB 的 .xlsx 文件")
    const workbook = await loadWorkbookFromFile(file).catch(() => {
        throw new Error("无法读取 Excel，请将文件另存为 .xlsx 后重试")
    })
    const metadata = workbook.getWorksheet(metadataName)
    const sheet = metadata
        ? workbook.getWorksheet(sheetName)
        : workbook.worksheets[0]
    if (!sheet) throw new Error("Excel 没有可读取的工作表")
    if (
        sheet.rowCount > maxRows + 1 ||
        sheet.columnCount > 100 ||
        (metadata && metadata.rowCount > maxRows + 1)
    )
        throw new Error("工作表最多支持 20,000 行地址、100 列，请清理多余行列")
    if (metadata) {
        const format = readCell(metadata.getCell("A1"))
        if (
            (format !== version && format !== legacyVersion) ||
            readCell(metadata.getCell("B1")) !== (await workbookScope(context))
        )
            throw new Error("此文件不属于当前销售单或客户，请重新导出收货安排")
        return readExportedRows(
            sheet,
            metadata,
            context,
            format === legacyVersion,
        )
    }
    if (
        readCell(sheet.getCell("A1")) === headers[0] ||
        readCell(sheet.getCell(headers.length)) === headers.at(-1) ||
        readCell(sheet.getCell(legacyHeaders.length)) === legacyHeaders.at(-1)
    )
        throw new Error("导出文件缺少商品明细记录，请从当前销售单重新导出")
    return readCustomerSheet(sheet, context, target)
}

export function readPastedDeliveryRows(
    text: string,
    context: DeliveryWorkbookContext,
    target: string,
): DeliveryImportSource {
    if (text.length > 100_000)
        throw new Error("粘贴内容超过 10 万字符，请改用 Excel 文件")
    const label = requireProduct(context, target)
    const rows = clipboardRows(text)
    if (!rows.length) throw new Error("请粘贴至少一行地址")
    const names = rows[0].cells.map((value) => value.trim())
    const columns = findAddressColumns(names)
    const hasHeader = columns.some((index) => index >= 0)
    if (hasHeader) validateHeader(columns)
    if (rows.length > maxRows + (hasHeader ? 1 : 0))
        throw new Error("一次最多解析 20,000 行地址")
    const productColumn = hasHeader
        ? names.findIndex(
              (name) => productHeaders.includes(name) || name === headers[0],
          )
        : -1
    if (
        productColumn >= 0 &&
        new Set(
            rows
                .slice(1)
                .map((row) => row.cells[productColumn]?.trim())
                .filter(Boolean),
        ).size > 1
    )
        throw new Error(mixedProductsMessage)
    return {
        rows: rows.slice(hasHeader ? 1 : 0).map((row) => {
            const { cells, errors } = readAddressCells((index) =>
                (row.cells[hasHeader ? columns[index] : index] ?? "").trim(),
            )
            if (!hasHeader && (row.cells.length < 6 || row.cells.length > 7))
                errors.push(
                    "请按收货人、联系电话、省、市、区县、详细地址、配送数量粘贴 6 或 7 列",
                )
            return { row: row.line, lineKey: target, label, cells, errors }
        }),
    }
}

export async function createSalesDeliveryIssueWorkbook(
    issues: DeliveryImportIssue[],
) {
    const workbook = await createWorkbook()
    const sheet = workbook.addWorksheet("问题行")
    setColumns(
        sheet,
        ["原表行号", "商品明细", ...addressHeaders, "问题"],
        [13, 27, 16, 20, 15, 15, 15, 42, 14, 60],
    )
    for (const issue of issues)
        sheet.addRow([issue.row, issue.label, ...issue.cells, issue.message])
    return workbookBlob(workbook)
}
