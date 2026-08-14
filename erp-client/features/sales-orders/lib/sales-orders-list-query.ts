import type { SalesOrdersListQuery } from "@/features/sales-orders/api/contracts"
import type { SalesOrdersUrlState } from "@/features/sales-orders/lib/url-state"

export const SORT_COLUMN_TO_FIELD: Record<
    string,
    NonNullable<SalesOrdersListQuery["sortBy"]>
> = {
    document: "documentNumber",
    contract: "contractNumber",
    amount: "amountGross",
    owner: "ownerName",
    submittedAt: "submittedAt",
}

const BUSINESS_TIME_ZONE_OFFSET_SECONDS = 8 * 60 * 60

function businessDateBoundary(
    value: string | undefined,
    endOfDay: boolean,
): number | undefined {
    if (!value) return undefined
    const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value)
    if (!match) return undefined
    const [, year, month, day] = match
    const utcSeconds =
        Date.UTC(
            Number(year),
            Number(month) - 1,
            Number(day),
            endOfDay ? 23 : 0,
            endOfDay ? 59 : 0,
            endOfDay ? 59 : 0,
        ) / 1000
    const seconds = utcSeconds - BUSINESS_TIME_ZONE_OFFSET_SECONDS
    return Number.isFinite(seconds) ? seconds : undefined
}

function businessDateStart(value?: string): number | undefined {
    return businessDateBoundary(value, false)
}

function businessDateEnd(value?: string): number | undefined {
    return businessDateBoundary(value, true)
}

export function buildSalesOrdersListQuery(
    url: SalesOrdersUrlState,
    currentUserId: string,
): SalesOrdersListQuery {
    return {
        page: url.page,
        pageSize: url.pageSize,
        search: url.search,
        customerId: url.customerId,
        contractId: url.contractId,
        createdBy: url.createdBy,
        nature: url.nature,
        summary: url.summary,
        currentUserId,
        origin: url.origin,
        commercialStatus: url.commercialStatus,
        reviewStatus: url.reviewStatus,
        fulfillment: url.fulfillment,
        collection: url.collection,
        invoice: url.invoice,
        closeStatus: url.closeStatus,
        createdFrom: businessDateStart(url.createdFrom),
        createdTo: businessDateEnd(url.createdTo),
        sortBy: url.sort ? SORT_COLUMN_TO_FIELD[url.sort] : undefined,
        sortDir: url.dir,
    }
}

/** 需要登录人身份的视图（待我处理/我创建的）在身份就绪前不发起查询。 */
export function salesOrdersListIdentityReady(
    url: SalesOrdersUrlState,
    currentUserId: string,
): boolean {
    return (
        (url.summary !== "mine" && url.summary !== "createdByMe") ||
        Boolean(currentUserId)
    )
}
