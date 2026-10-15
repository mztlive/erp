import { createUrlStateCodec } from "@/lib/url-state"

export const requestFilterFields = [
    ["request_no", "申请单号", "申请单号"],
    ["supplier_id", "供应商", "供应商名称"],
    ["payer_party_id", "我方主体", "我方主体名称"],
    ["applicant_name", "申请人", "申请人姓名"],
    ["purchase_order_no", "采购单号", "采购单号"],
] as const

export type RequestFilters = Record<
    (typeof requestFilterFields)[number][0],
    string
>
export const emptyRequestFilters: RequestFilters = {
    request_no: "",
    supplier_id: "",
    payer_party_id: "",
    applicant_name: "",
    purchase_order_no: "",
}

export const requestSortFields = [
    "request_no",
    "supplier",
    "applicant",
    "amount",
    "paid",
    "unpaid",
    "date",
    "status",
] as const
export type RequestSort = (typeof requestSortFields)[number]
export type RequestListQuery = Partial<RequestFilters> & {
    view: "effective" | "draft"
    page: number
    page_size: number
    sort_by?: RequestSort
    sort_desc?: boolean
}
export type RequestListUrl = Omit<RequestListQuery, "view"> & {
    view: "effective" | "available" | "draft"
}

export const requestListCodec = createUrlStateCodec<RequestListUrl>([
    {
        key: "view",
        type: "enum",
        values: ["effective", "available", "draft"],
        defaultValue: "effective",
    },
    ...requestFilterFields.map(([key]) => ({
        key,
        type: "string" as const,
        trim: true,
    })),
    { key: "page", type: "number", defaultValue: 1, min: 1 },
    { key: "page_size", type: "number", defaultValue: 50, min: 1, max: 100 },
    { key: "sort_by", type: "enum", values: requestSortFields },
    { key: "sort_desc", type: "boolean", defaultValue: false },
])
