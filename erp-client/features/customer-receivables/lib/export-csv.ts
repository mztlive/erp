import type { CustomerAccountsListView } from "./types"

function csvEscape(value: string | number | null | undefined): string {
    const text = value == null ? "" : String(value)
    return `"${text.replaceAll('"', '""')}"`
}

export function buildAccountsCsv(data: CustomerAccountsListView): string {
    let header: string[] = []
    let rows: (string | number | null | undefined)[][] = []

    if (data.view === "receivable") {
        header = [
            "销售单",
            "往来主体",
            "经营客户",
            "到期日",
            "应收总额",
            "净已收",
            "开放应收",
            "净已开票",
            "可开票",
            "状态",
        ]
        rows = data.receivables.map((row) => [
            row.salesOrderNo,
            row.counterpartyPartyName,
            row.customerName,
            row.dueDate,
            row.grossTotal,
            row.settledTotal,
            row.openTotal,
            row.invoicedTotal,
            row.openInvoiceableTotal,
            row.statusLabel,
        ])
    } else if (data.view === "receipt") {
        header = [
            "回款单号",
            "往来主体",
            "到账时间",
            "到账金额",
            "净已分配",
            "未分配",
            "状态",
        ]
        rows = data.receipts.map((row) => [
            row.receiptNo,
            row.counterpartyPartyName,
            row.receivedAt,
            row.amount,
            row.allocatedTotal,
            row.unallocatedAmount,
            row.statusLabel,
        ])
    } else if (data.view === "sales_invoice") {
        header = [
            "发票号码",
            "代码",
            "种类",
            "开票日期",
            "含税",
            "不含税",
            "税额",
            "净已分配",
            "未分配",
            "状态",
        ]
        rows = data.invoices.map((row) => [
            row.invoiceNo,
            row.invoiceCode ?? "",
            row.invoiceKindLabel,
            row.invoiceDate,
            row.grossAmount,
            row.netAmount,
            row.taxAmount,
            row.allocatedTotal,
            row.unallocatedAmount,
            row.statusLabel,
        ])
    } else {
        header = ["轨道", "单号", "供应商", "记录金额", "未分配余额"]
        rows = [
            ...data.unallocated.receipts.map((row) => [
                "回款",
                row.receiptNo,
                row.counterpartyPartyName,
                row.amount,
                row.unallocatedAmount,
            ]),
            ...data.unallocated.invoices.map((row) => [
                "销项发票",
                row.invoiceNo,
                row.counterpartyPartyName,
                row.grossAmount,
                row.unallocatedAmount,
            ]),
        ]
    }

    return [
        header.map(csvEscape).join(","),
        ...rows.map((row) => row.map(csvEscape).join(",")),
    ].join("\r\n")
}

export function downloadCsv(fileName: string, content: string): void {
    const blob = new Blob(["\uFEFF" + content], {
        type: "text/csv;charset=utf-8",
    })
    const url = URL.createObjectURL(blob)
    const link = document.createElement("a")
    link.href = url
    link.download = fileName
    document.body.appendChild(link)
    link.click()
    link.remove()
    URL.revokeObjectURL(url)
}
