import type { Page } from "@/lib/api"
import { createApiError } from "@/lib/api/errors"
import {
    loadReceivables,
    loadReceipts,
    loadSalesInvoices,
} from "@/features/customer-receivables/api/loaders"
import {
    projectReceivable,
    projectReceipt,
    projectInvoice,
} from "@/features/customer-receivables/api/mappers"
import type { SalesOrderDetailView } from "./sales-orders"

/** 汇总本单应收前读取所有页；分页中断必须失败，禁止将部分记录当作完整余额。 */
export async function collectOrderPages<T>(
    load: (page: number) => Promise<Page<T>>,
): Promise<T[]> {
    const items: T[] = []
    for (let page = 1; ; page++) {
        const result = await load(page)
        if (
            result.page !== page ||
            (!result.items.length && items.length < result.total)
        ) {
            throw createApiError({
                kind: "Parse",
                message: "记录读取不完整，请刷新后重试。",
                retryable: true,
            })
        }
        items.push(...result.items)
        if (items.length >= result.total) return items
    }
}

export async function fetchOrderReceivables(order: SalesOrderDetailView) {
    const accounts = await collectOrderPages((page) =>
        loadReceivables({
            view: "receivable",
            salesOrderId: order.id,
            page,
            pageSize: 100,
        }),
    )
    return accounts.map((account) => ({
        ...projectReceivable(account),
        businessType:
            order.nature === "card_voucher"
                ? ("card" as const)
                : ("physical_service" as const),
        businessTypeLabel:
            order.nature === "card_voucher" ? "卡券" : "实物与服务",
    }))
}

export async function fetchOrderReceipts(
    order: SalesOrderDetailView,
    page: number,
) {
    const result = await loadReceipts({
        view: "receipt",
        salesOrderId: order.id,
        page,
        pageSize: 20,
    })
    return {
        ...result,
        items: result.items.map((row) =>
            projectReceipt(row, {
                counterpartyPartyName: order.settlementEntity,
                customerName: order.customerName,
            }),
        ),
    }
}

export async function fetchOrderInvoices(
    order: SalesOrderDetailView,
    page: number,
) {
    const result = await loadSalesInvoices({
        view: "sales_invoice",
        salesOrderId: order.id,
        page,
        pageSize: 20,
    })
    return {
        ...result,
        items: result.items.map((row) =>
            projectInvoice(row, {
                counterpartyPartyName: order.settlementEntity,
                customerName: order.customerName,
            }),
        ),
    }
}
