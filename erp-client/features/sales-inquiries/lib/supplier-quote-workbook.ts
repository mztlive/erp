import ExcelJS from "exceljs"
import JSZip from "jszip"
import { inquiryDate, inquiryDateInput } from "./format"
import {
    supplierQuoteFileNames,
    type SupplierQuoteGroup,
    type SupplierQuoteSource,
} from "./supplier-quote-export"
import type { SupplierQuoteImage } from "./supplier-quote-images"

export const SUPPLIER_QUOTE_VERSION = "supplier-inquiry-quote-v4"
export const SUPPLIER_QUOTE_HEADERS = [
    "主图",
    "商品名称",
    "规格",
    "SKU 编号",
    "供应商货号",
    "单位",
    "需求数量",
    "要求交付日",
    "交付要求",
    "配送要求",
    "含税单价（元）",
    "税率（%）",
    "可供数量",
    "运费（元）",
    "报价备注",
]
const WIDTHS = [14, 28, 24, 19, 22, 10, 15, 16, 30, 24, 18, 13, 15, 16, 30]
const ARCHIVE_MAX_BYTES = 50 * 1024 * 1024

function configureSheet(
    sheet: ExcelJS.Worksheet,
    title: string,
    meta: string,
    headers: string[],
    widths: number[],
    instruction: string,
) {
    sheet.views = [{ state: "frozen", ySplit: 4, xSplit: 2 }]
    sheet.properties.defaultRowHeight = 24
    for (const [index, text] of [title, meta, instruction].entries()) {
        sheet.mergeCells(index + 1, 1, index + 1, headers.length)
        const cell = sheet.getCell(index + 1, 1)
        cell.value = text
        cell.font = {
            name: "微软雅黑",
            size: index === 0 ? 18 : 11,
            bold: index === 0,
            color: { argb: "FF243746" },
        }
        cell.alignment = { vertical: "middle", wrapText: true }
        sheet.getRow(index + 1).height = index === 0 ? 34 : 30
    }
    headers.forEach((header, index) => {
        const column = sheet.getColumn(index + 1)
        column.width = widths[index] ?? 22
        column.numFmt = "@"
        const cell = sheet.getCell(4, index + 1)
        cell.value = header
        cell.font = {
            name: "微软雅黑",
            size: 11,
            bold: true,
            color: { argb: "FFFFFFFF" },
        }
        cell.fill = {
            type: "pattern",
            pattern: "solid",
            fgColor: { argb: "FF334E68" },
        }
        cell.alignment = { vertical: "middle", wrapText: true }
    })
    sheet.getRow(4).height = 32
    sheet.autoFilter = {
        from: { row: 4, column: 1 },
        to: { row: 4, column: headers.length },
    }
    sheet.pageSetup = {
        orientation: "landscape",
        paperSize: 9,
        printTitlesRow: "1:4",
    }
}

/** ExcelJS 字符串写入 shared string，外部文本不创建公式；十进制数量保留原精度。 */
function addRow(
    sheet: ExcelJS.Worksheet,
    values: (string | number | null)[],
    height: number,
    inputFrom?: number,
) {
    const row = sheet.addRow(values)
    const textLines = values.map((value, index) => {
        const width = Math.max(1, (sheet.getColumn(index + 1).width ?? 22) - 2)
        return String(value ?? "")
            .split("\n")
            .reduce((count, text) => {
                const size = Array.from(text).reduce(
                    (sum, character) =>
                        sum + (character.charCodeAt(0) > 255 ? 2 : 1),
                    0,
                )
                return count + Math.max(1, Math.ceil(size / width))
            }, 0)
    })
    row.height = Math.min(
        409,
        Math.max(height, Math.max(...textLines) * 15 + 12),
    )
    for (let column = 1; column <= values.length; column += 1) {
        const cell = row.getCell(column)
        cell.font = { name: "微软雅黑", size: 11, color: { argb: "FF243746" } }
        cell.alignment = { vertical: "middle", wrapText: true }
        cell.border = { bottom: { style: "hair", color: { argb: "FFD9E2EC" } } }
        if (inputFrom && column >= inputFrom) {
            cell.fill = {
                type: "pattern",
                pattern: "solid",
                fgColor: { argb: "FFFFF2CC" },
            }
            cell.numFmt = [11, 12, 13, 14].includes(column) ? "0.######" : "@"
        }
    }
    return row
}

export async function buildSupplierQuoteWorkbook(
    source: SupplierQuoteSource,
    group: SupplierQuoteGroup,
    images: ReadonlyMap<string, SupplierQuoteImage>,
): Promise<Uint8Array> {
    const workbook = new ExcelJS.Workbook()
    workbook.creator = "采购询价"
    workbook.subject = SUPPLIER_QUOTE_VERSION
    const title = `${group.supplierName}报价表`
    const meta = `询价单号：${source.inquiryNo}　需求版本：${source.requirementVersion}　报价截止：${inquiryDate(source.deadlineAt)}`
    const sheet = workbook.addWorksheet("报价表")
    configureSheet(
        sheet,
        title,
        meta,
        [...SUPPLIER_QUOTE_HEADERS, "回导标识（请勿修改）"],
        WIDTHS,
        "请填写黄色区域：价格按本行单位报价，税率填写百分数（如 13）；报价即确认按要求交付日交付，无法按期交付请联系采购；运费填写本行总额，已含运费填写 0；无法供货时可供数量填写 0 并说明原因；整行排序须包含隐藏列。",
    )
    const imageIds = new Map<string, number>()
    for (const [index, line] of group.lines.entries()) {
        if (line.issue) throw new Error(`${line.name}：${line.issue}`)
        const image = line.imageAssetId
            ? images.get(line.imageAssetId)
            : undefined
        if (line.imageAssetId && !image)
            throw new Error(`${line.name}的图片未能完整读取，请重试`)
        const addresses = line.regions.reduce(
            (sum, region) => sum + region.addresses,
            0,
        )
        const row = addRow(
            sheet,
            [
                image ? "" : "暂无图片",
                line.name,
                line.specification,
                line.skuCode,
                line.supplierCodes.filter(Boolean).join("、"),
                line.unit,
                line.quantity,
                inquiryDateInput(line.dueAt) || "未设置",
                line.deliveryTerms,
                `第 ${index + 1} 项：${
                    addresses
                        ? `${line.regions.length} 个地区，${addresses} 个地址；详见配送需求`
                        : "按交付要求"
                }`,
                ...Array.from(
                    { length: SUPPLIER_QUOTE_HEADERS.length - 10 },
                    () => null,
                ),
                JSON.stringify({ lineId: line.lineId, skuId: line.skuId }),
            ],
            66,
            11,
        )
        if (image && line.imageAssetId) {
            let imageId = imageIds.get(line.imageAssetId)
            if (imageId === undefined) {
                imageId = workbook.addImage({
                    base64: image.base64,
                    extension: "jpeg",
                })
                imageIds.set(line.imageAssetId, imageId)
            }
            sheet.addImage(imageId, {
                tl: { col: 0.1, row: row.number - 1 + 0.08 },
                ext: { width: image.width, height: image.height },
                editAs: "oneCell",
            })
        }
    }
    sheet.getColumn(SUPPLIER_QUOTE_HEADERS.length + 1).hidden = true
    const identity = workbook.addWorksheet("回导信息", { state: "veryHidden" })
    identity.getCell("A1").value = SUPPLIER_QUOTE_VERSION
    identity.getCell("A2").value = source.inquiryId
    identity.getCell("A3").value = source.requirementVersion
    identity.getCell("A4").value = source.feedbackRound
    identity.getCell("A5").value = group.supplierId
    identity.getCell("A6").value = group.supplierName
    appendDeliverySheet(workbook, group, meta)
    return new Uint8Array(await workbook.xlsx.writeBuffer())
}

function appendDeliverySheet(
    workbook: ExcelJS.Workbook,
    group: SupplierQuoteGroup,
    meta: string,
) {
    if (!group.lines.some((line) => line.regions.length)) return
    const sheet = workbook.addWorksheet("配送需求")
    configureSheet(
        sheet,
        `${group.supplierName}配送需求`,
        meta,
        [
            "询价项次",
            "SKU 编号",
            "商品名称",
            "规格",
            "配送地区",
            "地址数量",
            "配送数量",
            "单位",
            "要求交付日",
            "交付要求",
        ],
        [14, 19, 28, 24, 32, 12, 15, 10, 16, 36],
        "询价项次对应报价表配送要求中的项次。配送数量按客户需求原样列出，商品询价数量以报价表为准；运费口径或配送条件有疑问请联系采购确认。",
    )
    group.lines.forEach((line, index) => {
        for (const region of line.regions)
            addRow(
                sheet,
                [
                    String(index + 1),
                    line.skuCode,
                    line.name,
                    line.specification,
                    region.name,
                    region.addresses,
                    region.quantity,
                    line.unit,
                    inquiryDateInput(line.dueAt) || "未设置",
                    line.deliveryTerms,
                ],
                42,
            )
    })
}

/** 全部工作簿成功后才生成唯一 ZIP，避免下载部分成功的供应商文件包。 */
export async function buildSupplierQuoteArchive(
    source: SupplierQuoteSource,
    groups: readonly SupplierQuoteGroup[],
    images: ReadonlyMap<string, SupplierQuoteImage>,
): Promise<Blob> {
    if (!groups.length) throw new Error("请选择至少一家有询价商品的供应商")
    if (groups.length > 200)
        throw new Error("一次最多生成 200 家供应商的报价表，请减少所选供应商")
    const zip = new JSZip()
    const names = supplierQuoteFileNames(groups)
    let bytes = 0
    for (const [index, group] of groups.entries()) {
        const workbook = await buildSupplierQuoteWorkbook(source, group, images)
        bytes += workbook.byteLength
        if (bytes > ARCHIVE_MAX_BYTES)
            throw new Error("报价表文件包超过 50 MiB，请减少所选商品或供应商")
        zip.file(names[index]!, workbook)
    }
    const archive = await zip.generateAsync({
        type: "blob",
        mimeType: "application/zip",
    })
    if (archive.size > ARCHIVE_MAX_BYTES)
        throw new Error("报价表文件包超过 50 MiB，请减少所选商品或供应商")
    return archive
}
