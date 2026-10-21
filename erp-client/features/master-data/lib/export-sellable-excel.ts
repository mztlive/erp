import ExcelJS from "exceljs"

import { fetchFileAssetPreviewBlob } from "@/features/file-assets/api"
import type { MasterDataListItem } from "@/features/master-data/types"
import {
    SELLABLE_EXCEL_HEADERS,
    downloadBlob,
    excelImageExtension,
    mapWithConcurrency,
    toSellableExcelRow,
    type SellableExcelImageExtension,
} from "@/features/master-data/lib/sellable-excel-rows"

import {
    appendInquiryDeliverySheets,
    INQUIRY_IMPORT_MAX_BYTES,
    INQUIRY_POOL_TEMPLATE_VERSION,
    INQUIRY_WORKBOOK_MIME,
} from "@/lib/inquiry-workbook"

import { INQUIRY_POOL_TABLE } from "@/lib/inquiry-pool-delivery"

const IMAGE_COL_WIDTH = 14
const IMAGE_SIZE_PX = 76
const DATA_ROW_HEIGHT = 62
const IMAGE_FETCH_CONCURRENCY = 4

const COLUMN_WIDTHS = [
    IMAGE_COL_WIDTH,
    28,
    22,
    16,
    16,
    14,
    14,
    14,
    14,
    24,
    14,
    12,
    10,
    16,
    16,
    16,
    32,
    26,
    16,
] as const

type ExcelImagePayload = Readonly<{
    base64: string
    extension: SellableExcelImageExtension
}>

function blobToDataUrl(blob: Blob): Promise<string> {
    return new Promise((resolve, reject) => {
        const reader = new FileReader()
        reader.onload = () => resolve(String(reader.result ?? ""))
        reader.onerror = () =>
            reject(reader.error ?? new Error("读取商品主图失败"))
        reader.readAsDataURL(blob)
    })
}

async function rasterizeToJpeg(blob: Blob): Promise<ExcelImagePayload> {
    const bitmap = await createImageBitmap(blob)
    const canvas = document.createElement("canvas")
    const scale = Math.min(1, 152 / Math.max(bitmap.width, bitmap.height))
    canvas.width = Math.max(1, Math.round(bitmap.width * scale))
    canvas.height = Math.max(1, Math.round(bitmap.height * scale))
    const context = canvas.getContext("2d")
    if (!context) {
        bitmap.close()
        throw new Error("无法转换商品主图")
    }
    context.fillStyle = "#ffffff"
    context.fillRect(0, 0, canvas.width, canvas.height)
    context.drawImage(bitmap, 0, 0, canvas.width, canvas.height)
    bitmap.close()
    return {
        base64: canvas.toDataURL("image/jpeg", 0.86),
        extension: "jpeg",
    }
}

async function toExcelImage(
    blob: Blob,
    fileName?: string,
): Promise<ExcelImagePayload> {
    const extension = excelImageExtension(blob.type, fileName)
    if (extension && blob.size <= 12 * 1024) {
        return { base64: await blobToDataUrl(blob), extension }
    }
    return rasterizeToJpeg(blob)
}

async function loadRowImage(
    assetId: string | undefined,
): Promise<ExcelImagePayload | undefined> {
    if (!assetId) return undefined
    try {
        const blob = await fetchFileAssetPreviewBlob(assetId)
        return await toExcelImage(blob)
    } catch {
        return undefined
    }
}

export async function buildSellableItemsWorkbook(input: {
    rows: readonly MasterDataListItem[]
    filterSnapshotLabel: string
    fileLabel: string
}): Promise<Blob> {
    if (input.rows.length > 2000)
        throw new Error("一次最多导出 2000 件商品，请缩小筛选范围后重试")
    const workbook = new ExcelJS.Workbook()
    workbook.creator = "公司商品池"
    workbook.subject = INQUIRY_POOL_TEMPLATE_VERSION
    const sheet = workbook.addWorksheet("选品", {
        views: [{ state: "frozen", ySplit: 2, xSplit: 0 }],
    })
    sheet.mergeCells(1, 1, 1, SELLABLE_EXCEL_HEADERS.length)
    sheet.getCell(1, 1).value =
        `筛选条件=${input.filterSnapshotLabel}。填写需求数量即表示选中商品，并在配送需求页填写数量及地址。发送客户前请自行处理内部信息。`
    sheet.getCell(1, 1).alignment = { wrapText: true, vertical: "middle" }
    sheet.getRow(1).height = 28

    const headerRow = sheet.getRow(2)
    SELLABLE_EXCEL_HEADERS.forEach((header, index) => {
        const cell = headerRow.getCell(index + 1)
        cell.value = header
        cell.font = { bold: true }
        if (index >= 14 && index <= 16)
            cell.fill = {
                type: "pattern",
                pattern: "solid",
                fgColor: { argb: "FFFFF2CC" },
            }
    })
    COLUMN_WIDTHS.forEach((width, index) => {
        sheet.getColumn(index + 1).width = width
    })

    sheet.getColumn(19).hidden = true
    sheet.getColumn(20).hidden = true
    const mapped = input.rows.map(toSellableExcelRow)
    const images = await mapWithConcurrency(
        mapped,
        IMAGE_FETCH_CONCURRENCY,
        (item) => loadRowImage(item.imageAssetId),
    )

    mapped.forEach((item, index) => {
        const excelRowNumber = index + 3
        const excelRow = sheet.getRow(excelRowNumber)
        excelRow.height = DATA_ROW_HEIGHT
        excelRow.alignment = { vertical: "middle", wrapText: true }
        const values = [
            images[index] ? "" : "无主图",
            item.name,
            item.specification,
            item.skuNo,
            item.productNo,
            item.salesPrice,
            item.bulkPrice,
            item.marketPrice,
            item.bulkMinQuantity,
            item.supplierCodes,
            item.supplierLabel,
            item.productKind,
            item.baseUnit,
            item.barcode,
            "",
            "",
            "",
            input.rows[index]?.productKind === "VOUCHER"
                ? "卡券不适用销售询价"
                : "填写需求数量后参与询价",
            input.rows[index]?.sellableItem?.baseUnitCode ?? "",
            `line-${index + 1}`,
        ]
        values.forEach((value, columnIndex) => {
            const cell = excelRow.getCell(columnIndex + 1)
            cell.numFmt = "@"
            cell.value = value
        })
        const image = images[index]
        if (!image) return
        const imageId = workbook.addImage({
            base64: image.base64,
            extension: image.extension,
        })
        sheet.addImage(imageId, {
            tl: { col: 0, row: excelRowNumber - 1 },
            ext: { width: IMAGE_SIZE_PX, height: IMAGE_SIZE_PX },
            editAs: "oneCell",
        })
    })

    sheet.addTable({
        name: INQUIRY_POOL_TABLE,
        ref: "A2",
        headerRow: true,
        totalsRow: false,
        style: { theme: "TableStyleLight1", showRowStripes: false },
        columns: SELLABLE_EXCEL_HEADERS.map((name) => ({
            name,
            filterButton: true,
        })),
        rows: input.rows.map((_, index) =>
            SELLABLE_EXCEL_HEADERS.map(
                (_, column) =>
                    sheet.getRow(index + 3).getCell(column + 1).value,
            ),
        ),
    })

    appendInquiryDeliverySheets(
        workbook,
        input.rows.map((row, index) => ({
            sku_code: row.stableNo,
            delivery_key: `line-${index + 1}`,
            inquiry_supported: row.productKind !== "VOUCHER",
            product_name: row.name,
            specification: row.sellableItem?.specificationLabel ?? "",
            unit: row.sellableItem?.baseUnitCode ?? "",
        })),
        true,
    )
    const buffer = await workbook.xlsx.writeBuffer()
    const blob = new Blob([new Uint8Array(buffer as ArrayBuffer)], {
        type: INQUIRY_WORKBOOK_MIME,
    })
    if (blob.size > INQUIRY_IMPORT_MAX_BYTES)
        throw new Error("导出文件超过询价可导入的 5 MiB，请减少商品数量后重试")
    return blob
}

export async function buildSellableItemsExcelFile(input: {
    rows: readonly MasterDataListItem[]
    filterSnapshotLabel: string
    fileLabel: string
}): Promise<void> {
    downloadBlob(
        await buildSellableItemsWorkbook(input),
        `基础资料-${input.fileLabel}.xlsx`,
    )
}
