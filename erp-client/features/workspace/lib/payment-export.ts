import type { PaymentExportRow } from "../api/payment-export"
import { formatFixedDisplay } from "@/lib/fixed-decimal"

const money = (value: string) =>
    formatFixedDisplay(value, {
        maxScale: 2,
        minimumFractionDigits: 2,
        maximumFractionDigits: 2,
        useGrouping: false,
    })

/** 金额、账号与联行号使用精确文本单元格，不经浮点数或公式解析。 */
export async function buildPaymentWorkbook(rows: readonly PaymentExportRow[]) {
    const { Workbook } = await import("exceljs")
    const workbook = new Workbook()
    const sheet = workbook.addWorksheet("待执行采购付款", {
        views: [{ state: "frozen", ySplit: 1 }],
    })
    const headers = [
        "付款申请号",
        "付款主体公司名称",
        "付款金额（元，剩余待付）",
        "付款账期",
        "希望付款日",
        "收款人全称",
        "收款银行账号",
        "联行号",
        "开户行",
        "备注",
        "供应商名称",
        "付款方式",
        "申请金额（元）",
        "已付金额（元）",
        "已关闭金额（元）",
        "来源采购单",
    ]
    sheet.columns = headers.map((header, index) => ({
        header,
        width: [28, 34, 26, 36, 18, 34, 30, 20, 40, 48, 34, 20, 22, 22, 22, 40][
            index
        ],
        style: { numFmt: "@", alignment: { vertical: "top", wrapText: true } },
    }))
    const details = workbook.addWorksheet("采购付款明细", {
        views: [{ state: "frozen", ySplit: 1 }],
    })
    details.columns = [
        "付款申请号",
        "来源采购单",
        "付款主体公司名称",
        "付款账期",
        "本行待付金额（元）",
        "申请金额（元）",
        "已付金额（元）",
        "已关闭金额（元）",
    ].map((header) => ({
        header,
        width: 28,
        style: { numFmt: "@", alignment: { vertical: "top", wrapText: true } },
    }))
    for (const { request, recipient } of rows) {
        const bank = request.execution_method.type === "bank_transfer"
        if (
            bank &&
            (!recipient?.account_number ||
                !recipient.account_name ||
                !recipient.bank_name)
        )
            throw new Error(
                `${request.request_no} 缺少完整收款资料，请刷新后重新导出`,
            )
        sheet.addRow([
            request.request_no,
            request.our_company_name,
            money(request.unpaid_amount),
            [
                ...new Set(request.lines.map((line) => line.payment_term_name)),
            ].join("；"),
            request.desired_payment_date,
            recipient?.account_name ?? request.supplier_name,
            recipient?.account_number ?? "不适用",
            bank ? recipient?.bank_routing_number || "未填写" : "不适用",
            bank
                ? [recipient?.bank_name, recipient?.bank_branch_name]
                      .filter(Boolean)
                      .filter((name, index, all) => all.indexOf(name) === index)
                      .join(" · ")
                : "不适用",
            request.description,
            request.supplier_name,
            bank ? "银行转账" : "供应商预存款抵扣",
            money(request.total_amount),
            money(request.paid_amount),
            money(request.closed_amount),
            request.lines.map((line) => line.purchase_order_no).join("；"),
        ])
        for (const line of request.lines)
            details.addRow([
                request.request_no,
                line.purchase_order_no,
                request.our_company_name,
                line.payment_term_name,
                money(line.unpaid_amount),
                money(line.requested_amount),
                money(line.paid_amount),
                money(line.closed_amount),
            ])
    }
    for (const tab of [sheet, details]) {
        tab.getRow(1).font = { bold: true, color: { argb: "FFFFFFFF" } }
        tab.getRow(1).fill = {
            type: "pattern",
            pattern: "solid",
            fgColor: { argb: "FF334155" },
        }
        tab.getRow(1).height = 32
        tab.autoFilter = {
            from: { row: 1, column: 1 },
            to: { row: tab.rowCount, column: tab.columnCount },
        }
    }
    return new Blob([new Uint8Array(await workbook.xlsx.writeBuffer())], {
        type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    })
}
