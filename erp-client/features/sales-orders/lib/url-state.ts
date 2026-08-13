import type {
    SalesOrderCloseFilter,
    SalesOrderCollectionFilter,
    SalesOrderCommercialStatusFilter,
    SalesOrderFulfillmentFilter,
    SalesOrderInvoiceFilter,
    SalesOrderNatureFilter,
    SalesOrderOriginFilter,
    SalesOrderReviewStatusFilter,
    SalesOrderSummaryFilter,
} from "@/features/sales-orders/filter-orders"
import {
    SALES_ORDER_CLOSE_OPTIONS,
    SALES_ORDER_COLLECTION_OPTIONS,
    SALES_ORDER_COMMERCIAL_STATUS_OPTIONS,
    SALES_ORDER_FULFILLMENT_OPTIONS,
    SALES_ORDER_INVOICE_OPTIONS,
    SALES_ORDER_REVIEW_STATUS_OPTIONS,
} from "@/features/sales-orders/filter-orders"
import { createUrlStateCodec } from "@/lib/url-state"

export type SalesOrdersUrlState = {
    search?: string
    customerId?: string
    contractId?: string
    createdBy?: string
    nature: SalesOrderNatureFilter
    summary: SalesOrderSummaryFilter
    origin: SalesOrderOriginFilter
    commercialStatus: SalesOrderCommercialStatusFilter
    reviewStatus: SalesOrderReviewStatusFilter
    fulfillment: SalesOrderFulfillmentFilter
    collection: SalesOrderCollectionFilter
    invoice: SalesOrderInvoiceFilter
    closeStatus: SalesOrderCloseFilter
    createdFrom?: string
    createdTo?: string
    page: number
    pageSize: number
    sort?: string
    dir?: "asc" | "desc"
}

const NATURES: SalesOrderNatureFilter[] = [
    "all",
    "physical_service",
    "card_voucher",
]

const SUMMARIES: SalesOrderSummaryFilter[] = [
    "all",
    "mine",
    "createdByMe",
    "exception",
]

const ORIGINS: SalesOrderOriginFilter[] = ["all", "erp", "mall"]

const COMMERCIAL_STATUSES: SalesOrderCommercialStatusFilter[] = [
    "all",
    ...SALES_ORDER_COMMERCIAL_STATUS_OPTIONS.map((option) => option.value),
]

const REVIEW_STATUSES: SalesOrderReviewStatusFilter[] = [
    "all",
    ...SALES_ORDER_REVIEW_STATUS_OPTIONS.map((option) => option.value),
]

const FULFILLMENT_STATUSES: SalesOrderFulfillmentFilter[] = [
    "all",
    ...SALES_ORDER_FULFILLMENT_OPTIONS.map((option) => option.value),
]

const COLLECTION_STATUSES: SalesOrderCollectionFilter[] = [
    "all",
    ...SALES_ORDER_COLLECTION_OPTIONS.map((option) => option.value),
]

const INVOICE_STATUSES: SalesOrderInvoiceFilter[] = [
    "all",
    ...SALES_ORDER_INVOICE_OPTIONS.map((option) => option.value),
]

const CLOSE_STATUSES: SalesOrderCloseFilter[] = [
    "all",
    ...SALES_ORDER_CLOSE_OPTIONS.map((option) => option.value),
]

const DIRECTIONS = ["asc", "desc"] as const
const BUSINESS_DATE_PATTERN = /^\d{4}-\d{2}-\d{2}$/

function parseBusinessDate(raw: string | null): string | undefined {
    if (!raw || !BUSINESS_DATE_PATTERN.test(raw)) return undefined
    const [year, month, day] = raw.split("-").map(Number)
    const date = new Date(Date.UTC(year, month - 1, day))
    return date.getUTCFullYear() === year &&
        date.getUTCMonth() === month - 1 &&
        date.getUTCDate() === day
        ? raw
        : undefined
}

const MANAGED_QUERY_KEYS = [
    "q",
    "search",
    "customerId",
    "contractId",
    "createdBy",
    "nature",
    "businessType",
    "summary",
    "origin",
    "status",
    "statusCode",
    "commercialStatus",
    "reviewStatus",
    "fulfillment",
    "collection",
    "invoice",
    "closeStatus",
    "createdFrom",
    "createdTo",
    "page",
    "pageSize",
    "sort",
    "dir",
] as const

const codec = createUrlStateCodec<SalesOrdersUrlState>([
    {
        key: "q",
        name: "search",
        type: "string",
        trim: true,
        aliases: ["search"],
    },
    { key: "customerId", type: "string", trim: true },
    { key: "contractId", type: "string", trim: true },
    { key: "createdBy", type: "string", trim: true },
    {
        key: "nature",
        type: "enum",
        values: NATURES,
        defaultValue: "all",
        aliases: ["businessType"],
        normalize: (raw) =>
            raw === "voucher"
                ? "card_voucher"
                : raw === "goods_service"
                  ? "physical_service"
                  : raw,
    },
    { key: "summary", type: "enum", values: SUMMARIES, defaultValue: "all" },
    { key: "origin", type: "enum", values: ORIGINS, defaultValue: "all" },
    {
        key: "commercialStatus",
        type: "enum",
        values: COMMERCIAL_STATUSES,
        defaultValue: "all",
    },
    {
        key: "reviewStatus",
        type: "enum",
        values: REVIEW_STATUSES,
        defaultValue: "all",
    },
    {
        key: "fulfillment",
        type: "enum",
        values: FULFILLMENT_STATUSES,
        defaultValue: "all",
    },
    {
        key: "collection",
        type: "enum",
        values: COLLECTION_STATUSES,
        defaultValue: "all",
    },
    {
        key: "invoice",
        type: "enum",
        values: INVOICE_STATUSES,
        defaultValue: "all",
    },
    {
        key: "closeStatus",
        type: "enum",
        values: CLOSE_STATUSES,
        defaultValue: "all",
    },
    {
        key: "createdFrom",
        type: "custom",
        parse: (get) => parseBusinessDate(get("createdFrom")),
        build: (value) =>
            typeof value === "string" ? parseBusinessDate(value) : undefined,
    },
    {
        key: "createdTo",
        type: "custom",
        parse: (get) => parseBusinessDate(get("createdTo")),
        build: (value) =>
            typeof value === "string" ? parseBusinessDate(value) : undefined,
    },
    { key: "page", type: "number", defaultValue: 1 },
    { key: "pageSize", type: "number", defaultValue: 20, min: 1, max: 100 },
    { key: "sort", type: "string" },
    { key: "dir", type: "enum", values: DIRECTIONS },
])

export function parseSalesOrdersSearchParams(
    searchParams: URLSearchParams | { get(name: string): string | null },
): SalesOrdersUrlState {
    const parsed = codec.parse(searchParams)
    const [createdFrom, createdTo] =
        parsed.createdFrom &&
        parsed.createdTo &&
        parsed.createdFrom > parsed.createdTo
            ? [parsed.createdTo, parsed.createdFrom]
            : [parsed.createdFrom, parsed.createdTo]

    if (parsed.summary === "mine") {
        return {
            ...parsed,
            createdBy: undefined,
            commercialStatus: "all",
            reviewStatus: "all",
            createdFrom,
            createdTo,
        }
    }
    if (parsed.summary === "createdByMe") {
        return {
            ...parsed,
            createdBy: undefined,
            createdFrom,
            createdTo,
        }
    }
    if (parsed.summary === "exception") {
        return {
            ...parsed,
            commercialStatus: "all",
            reviewStatus: "all",
            createdFrom,
            createdTo,
        }
    }
    return { ...parsed, createdFrom, createdTo }
}

export function buildSalesOrdersSearchParams(
    state: SalesOrdersUrlState,
): string {
    return codec.build(state)
}

export function mergeSalesOrdersSearchParams(
    searchParams: { toString(): string },
    state: SalesOrdersUrlState,
): string {
    const merged = new URLSearchParams(searchParams.toString())
    for (const key of MANAGED_QUERY_KEYS) merged.delete(key)
    const managed = new URLSearchParams(
        buildSalesOrdersSearchParams(state).replace(/^\?/, ""),
    )
    for (const [key, value] of managed) merged.append(key, value)
    const query = merged.toString()
    return query ? `?${query}` : ""
}

export function normalizedSalesOrdersSearchParams(
    searchParams: {
        getAll(name: string): string[]
        toString(): string
    },
    state: SalesOrdersUrlState,
): string | undefined {
    const actual = new URLSearchParams()
    for (const key of MANAGED_QUERY_KEYS) {
        for (const value of searchParams.getAll(key)) actual.append(key, value)
    }
    actual.sort()

    const canonical = new URLSearchParams(
        buildSalesOrdersSearchParams(state).replace(/^\?/, ""),
    )
    canonical.sort()
    if (actual.toString() === canonical.toString()) return undefined

    return mergeSalesOrdersSearchParams(searchParams, state)
}
