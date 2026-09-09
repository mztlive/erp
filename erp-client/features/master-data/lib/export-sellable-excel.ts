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
    24,
    14,
    12,
    10,
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
    canvas.width = bitmap.width
    canvas.height = bitmap.height
    const context = canvas.getContext("2d")
    if (!context) {
        bitmap.close()
        throw new Error("无法转换商品主图")
    }
    context.drawImage(bitmap, 0, 0)
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
    if (extension) {
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

export async function buildSellableItemsExcelFile(input: {
    rows: readonly MasterDataListItem[]
    filterSnapshotLabel: string
    fileLabel: string
}): Promise<void> {
    const workbook = new ExcelJS.Workbook()
    workbook.creator = "公司商品池"
    const sheet = workbook.addWorksheet("选品", {
        views: [{ state: "frozen", ySplit: 2, xSplit: 0 }],
    })
    sheet.mergeCells(1, 1, 1, SELLABLE_EXCEL_HEADERS.length)
    sheet.getCell(1, 1).value =
        `筛选条件=${input.filterSnapshotLabel}。说明=按勾选导出；主图在单元格内；不含无权查看的敏感信息。`
    sheet.getCell(1, 1).alignment = { wrapText: true, vertical: "middle" }
    sheet.getRow(1).height = 28

    const headerRow = sheet.getRow(2)
    SELLABLE_EXCEL_HEADERS.forEach((header, index) => {
        const cell = headerRow.getCell(index + 1)
        cell.value = header
        cell.font = { bold: true }
    })
    COLUMN_WIDTHS.forEach((width, index) => {
        sheet.getColumn(index + 1).width = width
    })

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
            item.marketPrice,
            item.supplyRegions,
            item.supplierLabel,
            item.productKind,
            item.baseUnit,
            item.barcode,
        ]
        values.forEach((value, columnIndex) => {
            excelRow.getCell(columnIndex + 1).value = value
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

    const buffer = await workbook.xlsx.writeBuffer()
    const bytes = new Uint8Array(buffer as ArrayBuffer)
    downloadBlob(
        new Blob([bytes], {
            type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        }),
        `基础资料-${input.fileLabel}.xlsx`,
    )
}
