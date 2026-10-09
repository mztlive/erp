import { Workbook } from "exceljs"
import type { PublicReceiptView } from "../types"
import type { ReceiptContent } from "./receipt"

/** 仅导出当前公开回执，不包含访问链接、内部商品编号或内部价格。 */
export async function downloadReceipt(
    receipt: PublicReceiptView,
    content: ReceiptContent,
) {
    const workbook = new Workbook()
    const sheet = workbook.addWorksheet("选品确认回执")
    sheet.columns = [
        { width: 30 },
        { width: 34 },
        { width: 46 },
        ...(content.byQuantity ? [{ width: 16 }, { width: 20 }] : []),
    ]
    const columnCount = sheet.columns.length
    const addHeading = (text: string, size = 12) => {
        const row = sheet.addRow([text])
        sheet.mergeCells(row.number, 1, row.number, columnCount)
        row.font = { name: "微软雅黑", size, bold: true }
        row.height = 30
        return row
    }
    addHeading("选品确认回执", 18)
    addHeading(receipt.customer_name).height = 44
    sheet.addRow(["方案编号", receipt.proposal_no])
    sheet.addRow(["提交时间", content.submittedAt])
    sheet.addRow(["状态", "选品已提交"])
    if (receipt.recipient) {
        sheet.addRow(["收件人", receipt.recipient.name])
        sheet.addRow(["联系电话", receipt.recipient.phone])
        sheet.addRow([
            "收件地址",
            [
                receipt.recipient.province,
                receipt.recipient.city,
                receipt.recipient.district,
                receipt.recipient.address,
            ].join(" "),
        ])
    }
    sheet.addRow([])
    const header = sheet.addRow([
        "商品名称",
        "规格",
        "套餐明细",
        ...(content.byQuantity ? ["数量（份）", "金额（含税，元）"] : []),
    ])
    header.font = { name: "微软雅黑", bold: true }
    header.height = 28
    for (const item of content.items) {
        sheet.addRow([
            item.name,
            item.specification
                .map((spec) => `${spec.name}：${spec.value}`)
                .join(" / "),
            item.members
                .map((member) =>
                    [
                        member.name,
                        member.specification
                            .map((spec) => `${spec.name}：${spec.value}`)
                            .join(" / "),
                        `1 ${member.unit}`,
                    ]
                        .filter(Boolean)
                        .join(" · "),
                )
                .join("\n"),
            ...(content.byQuantity
                ? [item.quantity ?? "—", item.amount ?? "—"]
                : []),
        ])
    }
    if (content.byQuantity) {
        const total = sheet.addRow([
            "合计（含税）",
            "",
            "",
            content.quantity ?? "—",
            content.total ?? "—",
        ])
        total.font = { name: "微软雅黑", bold: true }
        total.height = 28
    }
    sheet.addRow([])
    sheet.addRow(["选品结果已保存。如需调整，请联系销售。"])
    sheet.mergeCells(sheet.rowCount, 1, sheet.rowCount, columnCount)
    for (const notice of content.notices) {
        sheet.addRow([notice])
        sheet.mergeCells(sheet.rowCount, 1, sheet.rowCount, columnCount)
    }
    sheet.eachRow((row) => {
        row.eachCell((cell) => {
            cell.numFmt = "@"
            cell.alignment = { vertical: "middle", wrapText: true }
        })
    })
    sheet.views = [{ state: "frozen", ySplit: header.number }]
    const buffer = await workbook.xlsx.writeBuffer()
    const url = URL.createObjectURL(
        new Blob([new Uint8Array(buffer)], {
            type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        }),
    )
    const link = document.createElement("a")
    link.id = "sales-selection-receipt-download-file"
    link.href = url
    link.download = `选品清单-${receipt.proposal_no.replace(/[^\p{L}\p{N}_-]/gu, "_")}.xlsx`
    document.body.appendChild(link)
    link.click()
    link.remove()
    setTimeout(() => URL.revokeObjectURL(url), 30_000)
}
