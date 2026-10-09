import { Workbook, type Worksheet } from "exceljs"

import { formatBookInstant } from "@/features/sales-selection/lib/presentation"
import type {
    BookSelectionDetailView,
    BookVoucherView,
} from "@/features/sales-selection/types"

function prepareSheet(sheet: Worksheet, headings: readonly string[]) {
    const header = sheet.addRow([...headings])
    header.font = { name: "微软雅黑", bold: true }
    header.height = 28
    sheet.views = [{ state: "frozen", ySplit: 1 }]
    sheet.autoFilter = {
        from: { row: 1, column: 1 },
        to: { row: 1, column: headings.length },
    }
}

async function downloadWorkbook(
    workbook: Workbook,
    customerName: string,
    label: string,
    downloadId: string,
) {
    workbook.eachSheet((sheet) => {
        sheet.eachRow((row) => {
            row.eachCell((cell) => {
                cell.numFmt = "@"
                cell.alignment = { vertical: "middle", wrapText: true }
            })
        })
    })
    const buffer = await workbook.xlsx.writeBuffer()
    const url = URL.createObjectURL(
        new Blob([new Uint8Array(buffer)], {
            type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        }),
    )
    const link = document.createElement("a")
    link.id = downloadId
    link.href = url
    link.download = `${label}-${customerName.replace(/[^\p{L}\p{N}_-]/gu, "_") || "客户"}.xlsx`
    document.body.appendChild(link)
    link.click()
    link.remove()
    setTimeout(() => URL.revokeObjectURL(url), 30_000)
}

/** 导出可分发的业务提货码，不包含选品册密码和内部编号。 */
export async function downloadBookVouchers(
    customerName: string,
    vouchers: readonly BookVoucherView[],
) {
    const workbook = new Workbook()
    const sheet = workbook.addWorksheet("提货码")
    sheet.columns = [{ width: 30 }, { width: 18 }]
    prepareSheet(sheet, ["提货码", "选品状态"])
    for (const voucher of vouchers) {
        sheet.addRow([
            voucher.voucher_code,
            voucher.submitted ? "已提交" : "待选品",
        ])
    }
    await downloadWorkbook(
        workbook,
        customerName,
        "提货码",
        "sales-selection-vouchers-download-file",
    )
}

/** 每个已提交人的每种商品一行，逐行保留完整收件信息。 */
export async function downloadBookSelectionDetails(
    customerName: string,
    selections: readonly BookSelectionDetailView[],
) {
    const workbook = new Workbook()
    const sheet = workbook.addWorksheet("选品明细")
    sheet.columns = [
        { width: 26 },
        { width: 26 },
        { width: 24 },
        { width: 16 },
        { width: 20 },
        { width: 16 },
        { width: 16 },
        { width: 16 },
        { width: 42 },
        { width: 30 },
        { width: 30 },
        { width: 16 },
        { width: 12 },
        { width: 18 },
        { width: 18 },
        { width: 20 },
    ]
    prepareSheet(sheet, [
        "提货码",
        "方案编号",
        "提交时间",
        "收件人",
        "手机号",
        "省",
        "市",
        "区",
        "详细地址",
        "商品名称",
        "规格",
        "数量",
        "单位",
        "单价（含税，元）",
        "金额（含税，元）",
        "个人合计（含税，元）",
    ])
    for (const selection of selections) {
        for (const item of selection.items) {
            sheet.addRow([
                selection.voucher_code ?? "—",
                selection.proposal_no,
                formatBookInstant(selection.submitted_at),
                selection.recipient?.name ?? "—",
                selection.recipient?.phone ?? "—",
                selection.recipient?.province ?? "—",
                selection.recipient?.city ?? "—",
                selection.recipient?.district ?? "—",
                selection.recipient?.address ?? "—",
                item.name,
                (item.specification ?? [])
                    .map((spec) => `${spec.name}：${spec.value}`)
                    .join(" / "),
                item.quantity?.toString() ?? "—",
                item.unit ?? "—",
                item.unit_price,
                item.line_amount ?? "—",
                selection.total_amount ?? "—",
            ])
        }
    }
    await downloadWorkbook(
        workbook,
        customerName,
        "选品明细",
        "sales-selection-details-download-file",
    )
}
