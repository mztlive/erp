import { createUrlStateCodec } from "@/lib/url-state"

export const candidateFilterFields = [
    ["purchase_order_no", "采购单号", "采购单号"],
    ["purchase_summary", "采购内容", "商品名称或采购内容"],
    ["supplier_id", "供应商", "供应商名称"],
    ["payer_party_id", "我方主体", "我方主体名称"],
] as const
export type CandidateFilters = Record<
    (typeof candidateFilterFields)[number][0],
    string
>
export const emptyCandidateFilters: CandidateFilters = {
    purchase_order_no: "",
    purchase_summary: "",
    supplier_id: "",
    payer_party_id: "",
}
export const candidateSortFields = [
    "purchase_order_no",
    "supplier_name",
    "payment_term_name",
    "total_amount",
    "paid_amount",
    "reserved_amount",
    "available_amount",
] as const
export type CandidateListQuery = Partial<CandidateFilters> & {
    page: number
    page_size: number
    sort_by?: (typeof candidateSortFields)[number]
    sort_desc?: boolean
}
export const candidateListCodec = createUrlStateCodec<CandidateListQuery>([
    ...candidateFilterFields.map(([name]) => ({
        key: `candidate_${name}`,
        name,
        type: "string" as const,
        trim: true,
    })),
    {
        key: "candidate_page",
        name: "page",
        type: "number",
        defaultValue: 1,
        min: 1,
    },
    {
        key: "candidate_page_size",
        name: "page_size",
        type: "number",
        defaultValue: 50,
        min: 1,
        max: 100,
    },
    {
        key: "candidate_sort_by",
        name: "sort_by",
        type: "enum",
        values: candidateSortFields,
    },
    {
        key: "candidate_sort_desc",
        name: "sort_desc",
        type: "boolean",
        defaultValue: false,
    },
])
