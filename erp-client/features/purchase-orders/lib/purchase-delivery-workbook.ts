import type { Worksheet } from "exceljs"

import { fullDeliveryAddress } from "@/lib/delivery-address"
import { compactFixed } from "@/lib/fixed-decimal"
import { createWorkbook, downloadWorkbook, workbookBlob } from "@/lib/xlsx"
import {
    buildPurchaseDeliveryPlanSummary,
    purchaseDeliveryWorkbookFileName,
    type PurchaseDeliveryPlanLine,
} from "./purchase-delivery-plan"

export type PurchaseDeliveryWorkbookInput = Readonly<{
    documentLabel?: string
    supplierName: string
    lines: readonly PurchaseDeliveryPlanLine[]
}>

const DETAIL_HEADERS = [
    "序号",
    "商品名称",
    "规格",
    "收货人",
    "联系电话",
    "收货地区",
    "详细地址",
    "完整地址",
    "数量",
    "单位",
    "预计交期",
] as const

const DETAIL_WIDTHS = [8, 28, 22, 14, 17, 24, 38, 48, 12, 8, 14] as const

const SUMMARY_HEADERS = [
    "序号",
    "商品名称",
    "规格",
    "采购数量",
    "单位",
    "已分配数量",
    "地址条数",
    "预计交期",
] as const

const SUMMARY_WIDTHS = [8, 30, 22, 12, 8, 12, 12, 14] as const

function styleSheetHeader(
    sheet: Worksheet,
    headers: readonly string[],
    widths: readonly number[],
) {
    sheet.addRow([...headers])
    sheet.columns.forEach((column, index) => {
        column.width = widths[index]
        column.numFmt = "@"
        column.font = { name: "Microsoft YaHei", size: 11 }
    })
    const headerRow = sheet.getRow(1)
    headerRow.height = 24
    headerRow.font = { name: "Microsoft YaHei", size: 11, bold: true }
    headerRow.alignment = { vertical: "middle" }
    headerRow.eachCell((cell) => {
        cell.fill = {
            type: "pattern",
            pattern: "solid",
            fgColor: { argb: "FFF1F3F5" },
        }
    })
    sheet.views = [{ state: "frozen", ySplit: 1, showGridLines: true }]
}

async function buildWorkbook(input: PurchaseDeliveryWorkbookInput) {
    const summary = buildPurchaseDeliveryPlanSummary(input.lines)
    const workbook = await createWorkbook()

    const detailSheet = workbook.addWorksheet("发货地址明细")
    styleSheetHeader(detailSheet, DETAIL_HEADERS, DETAIL_WIDTHS)
    detailSheet.autoFilter = { from: "A1", to: "K1" }

    let rowSequence = 0
    for (const group of summary.itemGroups) {
        if (group.allocations.length === 0) {
            rowSequence += 1
            detailSheet.addRow([
                String(rowSequence),
                group.itemName,
                group.itemSku ?? "",
                "",
                "",
                "",
                "未指定交付地址，履约前补齐",
                "未指定交付地址，履约前补齐",
                group.quantity,
                group.unit,
                group.expectedDeliveryDate ?? "",
            ])
            continue
        }
        for (const row of group.allocations) {
            rowSequence += 1
            detailSheet.addRow([
                String(rowSequence),
                group.itemName,
                group.itemSku ?? "",
                row.recipient,
                row.phone,
                row.region_name ?? "",
                row.address,
                fullDeliveryAddress(row),
                compactFixed(row.quantity),
                group.unit,
                group.expectedDeliveryDate ?? "",
            ])
        }
    }

    const summarySheet = workbook.addWorksheet("商品汇总")
    styleSheetHeader(summarySheet, SUMMARY_HEADERS, SUMMARY_WIDTHS)
    summary.itemGroups.forEach((group, index) => {
        summarySheet.addRow([
            String(index + 1),
            group.itemName,
            group.itemSku ?? "",
            group.quantity,
            group.unit,
            group.allocatedTotal,
            String(group.allocations.length),
            group.expectedDeliveryDate ?? "",
        ])
    })

    const infoSheet = workbook.addWorksheet("单据说明")
    infoSheet.getColumn(1).width = 20
    infoSheet.getColumn(2).width = 60
    infoSheet.getColumn(1).font = {
        name: "Microsoft YaHei",
        size: 11,
        bold: true,
    }
    infoSheet.getColumn(2).font = { name: "Microsoft YaHei", size: 11 }
    infoSheet.addRow(["供应商", input.supplierName])
    if (input.documentLabel) {
        infoSheet.addRow(["采购单标识", input.documentLabel])
    }
    infoSheet.addRow(["商品项数", String(summary.itemGroups.length)])
    infoSheet.addRow(["地址分配条数", String(summary.totalRows)])
    infoSheet.addRow(["独立收货地址数", String(summary.uniqueAddressCount)])
    infoSheet.addRow([
        "使用说明",
        "本表仅包含本采购单对应的收件安排，联系电话已设置为文本格式，可直接用于打单发货。",
    ])

    return workbook
}

/** 生成对供应商发货地址明细 Excel（.xlsx）Blob，不含内部销售客户与成本信息。 */
export async function createPurchaseDeliveryWorkbookBlob(
    input: PurchaseDeliveryWorkbookInput,
): Promise<Blob> {
    return workbookBlob(await buildWorkbook(input))
}

/** 直接下载对供应商发货地址明细 Excel 文件。 */
export async function downloadPurchaseDeliveryWorkbook(
    input: PurchaseDeliveryWorkbookInput,
    fileName?: string,
): Promise<void> {
    await downloadWorkbook(
        await buildWorkbook(input),
        fileName ?? purchaseDeliveryWorkbookFileName(input.supplierName),
        {
            linkId: "purchase-order-delivery-workbook-file",
            revokeDelayMs: 60_000,
        },
    )
}
