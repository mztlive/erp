import type { Workbook } from "exceljs"
import type { InquiryCustomerWorkbookProduct } from "@/lib/inquiry-workbook"
import {
    addDeliveryChecks,
    colorDeliveryChecks,
    INQUIRY_DELIVERY_CHECK_HEADERS,
    INQUIRY_DELIVERY_HELPER_HEADERS,
} from "@/lib/inquiry-delivery-checks"

export const INQUIRY_POOL_TABLE = "InquirySelection"
export const INQUIRY_POOL_LINK_HEADER = "配送关联"
export const INQUIRY_POOL_DELIVERY_HEADERS = [
    "商品名称",
    "SKU 编号",
    "SPU 编号",
    "收件人",
    "联系电话",
    "省",
    "市",
    "区县",
    "详细地址",
    "配送数量",
    "要求交付日期",
    "交付条件",
] as const

/** 固定关联键随选品表行一起排序；配送地址不会因增减选品重新分配。 */
export function inquiryPoolDeliveryFormula(key: string, field: string): string {
    const escaped = key.replaceAll('"', '""')
    const lookup = (column: string) =>
        `INDEX(${INQUIRY_POOL_TABLE}[${column}],MATCH("${escaped}",${INQUIRY_POOL_TABLE}[${INQUIRY_POOL_LINK_HEADER}],0))`
    return `IF(${lookup("需求数量")}="","",${lookup(field)})`
}

export function appendPoolDeliverySheet(
    workbook: Workbook,
    products: readonly InquiryCustomerWorkbookProduct[],
) {
    const sheet = workbook.addWorksheet("配送需求", {
        views: [{ state: "frozen", ySplit: 1, xSplit: 3 }],
    })
    const widths = [
        30, 24, 24, 18, 22, 16, 16, 16, 38, 16, 20, 28, 16, 16, 16, 24, 16, 24,
    ]
    const headers = [
        ...INQUIRY_POOL_DELIVERY_HEADERS,
        ...INQUIRY_DELIVERY_CHECK_HEADERS,
        ...INQUIRY_DELIVERY_HELPER_HEADERS,
    ]
    sheet.addRow(headers)
    sheet.getRow(1).font = { bold: true, color: { argb: "FFFFFFFF" } }
    sheet.getRow(1).height = 30
    headers.forEach((_, index) => {
        sheet.getColumn(index + 1).width = widths[index]
        sheet.getColumn(index + 1).numFmt = "@"
        sheet.getCell(1, index + 1).fill = {
            type: "pattern",
            pattern: "solid",
            fgColor: { argb: "FF334155" },
        }
    })
    sheet.getColumn(17).hidden = true
    sheet.getColumn(18).hidden = true
    for (const product of products) {
        if (product.inquiry_supported === false) continue
        if (!product.delivery_key)
            throw new Error("商品配送关联缺失，请重新导出")
        const row = sheet.addRow(Array<string>(18).fill(""))
        row.height = 28
        row.alignment = { vertical: "top", wrapText: true }
        row.eachCell((cell) => {
            cell.border = {
                top: {
                    style: "medium",
                    color: {
                        argb: "FF94A3B8",
                    },
                },
            }
        })
        for (const [column, field] of [
            [1, "商品名称"],
            [2, "SKU 编号"],
            [3, "SPU 编号"],
            [10, "需求数量"],
        ] as const) {
            row.getCell(column).value = {
                formula: inquiryPoolDeliveryFormula(
                    product.delivery_key,
                    field,
                ),
                result: "",
            }
            row.getCell(column).fill = {
                type: "pattern",
                pattern: "solid",
                fgColor: { argb: "FFF2F2F2" },
            }
        }
        // 配送数量允许手动拆分，用输入底色区分于只读商品资料。
        row.getCell(10).fill = {
            type: "pattern",
            pattern: "solid",
            fgColor: { argb: "FFFFFBEB" },
        }
        row.getCell(10).dataValidation = {
            type: "custom",
            allowBlank: true,
            formulae: [`AND(ISNUMBER(1*J${row.number}),1*J${row.number}>0)`],
            showInputMessage: true,
            promptTitle: "填写本地址配送数量",
            prompt: "一个地址填一行。多地址时复制该商品整行到表末，并将原行及新增行的数量分别改为实际分配值。红色超量、黄色不足、绿色数量一致。",
            showErrorMessage: true,
            errorTitle: "请填写有效数量",
            error: "配送数量须大于零；不使用的配送行请清空。",
            errorStyle: "stop",
        }
        row.getCell(4).dataValidation = {
            type: "custom",
            formulae: ["TRUE"],
            allowBlank: true,
            showInputMessage: true,
            promptTitle: "一个收件地址填写一行",
            prompt: "每个商品默认一行。需要多个地址时，复制该商品整行到表末，分别填写收件信息和配送数量。",
        }
        addDeliveryChecks(sheet, row.number)
    }
    sheet.autoFilter = {
        from: { row: 1, column: 1 },
        to: { row: sheet.rowCount, column: 16 },
    }
    colorDeliveryChecks(sheet)
    workbook.calcProperties.fullCalcOnLoad = true
}
