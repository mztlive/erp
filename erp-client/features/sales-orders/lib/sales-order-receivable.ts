import type { StatusTone } from "@/components/ui/status-badge"
import type {
    AllocationLine,
    ReceivableAccountRow,
    ReceiptRow,
    SalesInvoiceRow,
} from "@/features/customer-receivables/types"
import { sumFixed } from "@/lib/fixed-decimal"

export type OrderReceivableDocument = {
    id: string
    documentType: string
    documentNumber: string
    statusLabel: string
    statusTone: StatusTone
    amount: string
    amountLabel: string
    owner: string
}

/**
 * 成交含税金额减去已回款，得到本单待回款。
 *
 * @param gross 销售单含税成交金额
 * @param received 已回款含税金额
 * @returns 待回款十进制字符串；非法输入时回退成交金额
 */
export function remainingReceivableAmount(
    gross: string,
    received: string,
): string {
    try {
        return sumFixed([gross, `-${received}`], {
            maxScale: 2,
            outputScale: 2,
            allowNegative: true,
        })
    } catch {
        return gross
    }
}

/**
 * 本单应收子账及其分录的稳定身份，用来判断回款/发票是否核到本单。
 *
 * @param accounts 本单应收子账
 * @returns 子账 ID 与分录 ID 集合
 */
export function receivableTargetIds(
    accounts: readonly {
        accountId: string
        entries: readonly { entryId: string }[]
    }[],
): Set<string> {
    const ids = new Set<string>()
    for (const account of accounts) {
        ids.add(account.accountId)
        for (const entry of account.entries) {
            ids.add(entry.entryId)
        }
    }
    return ids
}

/**
 * 已过账核销里，落到指定应收身份的净金额。
 *
 * @param allocations 回款或发票上的核销明细
 * @param targetIds 本单应收子账/分录 ID
 * @returns 核到本单的含税净额；没有任何命中时为 `0.00`
 */
export function amountAllocatedToTargets(
    allocations: readonly Pick<
        AllocationLine,
        "targetId" | "amountGross" | "action" | "isPosted"
    >[],
    targetIds: ReadonlySet<string>,
): string {
    const parts: string[] = []
    for (const allocation of allocations) {
        if (!allocation.isPosted || !targetIds.has(allocation.targetId)) {
            continue
        }
        parts.push(
            allocation.action === "REVERSE"
                ? `-${allocation.amountGross}`
                : allocation.amountGross,
        )
    }
    if (parts.length === 0) {
        return "0.00"
    }
    return sumFixed(parts, {
        maxScale: 2,
        outputScale: 2,
        allowNegative: true,
    })
}

/**
 * 把本单应收子账投影成对象中心关联单据行。
 *
 * @param accounts 本单应收子账
 * @returns 关联单据投影
 */
export function mapOrderReceivableAccounts(
    accounts: readonly Pick<
        ReceivableAccountRow,
        | "accountId"
        | "accountSeq"
        | "salesOrderNo"
        | "statusLabel"
        | "statusTone"
        | "openTotal"
        | "counterpartyPartyName"
    >[],
): OrderReceivableDocument[] {
    return accounts.map((account) => ({
        id: account.accountId,
        documentType: "应收子账",
        documentNumber: `${account.salesOrderNo} · 子账 #${account.accountSeq}`,
        statusLabel: account.statusLabel,
        statusTone: account.statusTone,
        amount: account.openTotal,
        amountLabel: "开放应收（含税）",
        owner: account.counterpartyPartyName,
    }))
}

/**
 * 把核到本单的回款投影成对象中心关联单据行。金额只计本单核销净额。
 *
 * @param receipts 已按本单过滤的回款
 * @param targetIds 本单应收身份
 * @returns 关联单据投影
 */
export function mapOrderReceipts(
    receipts: readonly Pick<
        ReceiptRow,
        | "receiptId"
        | "receiptNo"
        | "statusLabel"
        | "statusTone"
        | "counterpartyPartyName"
        | "allocations"
    >[],
    targetIds: ReadonlySet<string>,
): OrderReceivableDocument[] {
    return receipts.map((receipt) => ({
        id: receipt.receiptId,
        documentType: "客户回款",
        documentNumber: receipt.receiptNo,
        statusLabel: receipt.statusLabel,
        statusTone: receipt.statusTone,
        amount: amountAllocatedToTargets(receipt.allocations, targetIds),
        amountLabel: "核到本单（含税）",
        owner: receipt.counterpartyPartyName,
    }))
}

/**
 * 把核到本单的销项发票投影成对象中心关联单据行。金额只计本单核销净额。
 *
 * @param invoices 已按本单过滤的销项发票
 * @param targetIds 本单应收身份
 * @returns 关联单据投影
 */
export function mapOrderInvoices(
    invoices: readonly Pick<
        SalesInvoiceRow,
        | "invoiceId"
        | "invoiceNo"
        | "invoiceKindLabel"
        | "statusLabel"
        | "statusTone"
        | "counterpartyPartyName"
        | "allocations"
    >[],
    targetIds: ReadonlySet<string>,
): OrderReceivableDocument[] {
    return invoices.map((invoice) => ({
        id: invoice.invoiceId,
        documentType: `销项发票 · ${invoice.invoiceKindLabel}`,
        documentNumber: invoice.invoiceNo,
        statusLabel: invoice.statusLabel,
        statusTone: invoice.statusTone,
        amount: amountAllocatedToTargets(invoice.allocations, targetIds),
        amountLabel: "核到本单（含税）",
        owner: invoice.counterpartyPartyName,
    }))
}
