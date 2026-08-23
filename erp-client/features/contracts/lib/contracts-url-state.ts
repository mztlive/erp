import { createUrlStateCodec } from "@/lib/url-state"

import type { ContractMetricFilter } from "@/features/contracts/lib/filter-contracts"

/** URL 契约：q（旧 search 别名只读兼容）/metric/page/pageSize/sort/dir/customerId。 */
const CONTRACT_METRIC_VALUES: ContractMetricFilter[] = [
    "all",
    "effective",
    "expiring_30d",
    "expired",
    "terminated",
]

const CONTRACTS_URL_FIELDS = [
    { key: "q", type: "string", trim: true, aliases: ["search"] as const },
    {
        key: "metric",
        type: "enum",
        values: CONTRACT_METRIC_VALUES,
        defaultValue: "all",
    },
    { key: "page", type: "number", defaultValue: 1 },
    { key: "pageSize", type: "number", defaultValue: 20, min: 1, max: 100 },
    { key: "sort", type: "string" },
    { key: "dir", type: "enum", values: ["asc", "desc"] as const },
    { key: "customerId", type: "string" },
    { key: "settlementPartyId", type: "string" },
    { key: "owner", type: "string" },
    { key: "upload", type: "enum", values: ["1"] as const },
] as const

export type ContractsUrlState = {
    q?: string
    metric: ContractMetricFilter
    page: number
    pageSize: number
    sort?: string
    dir?: "asc" | "desc"
    customerId?: string
    settlementPartyId?: string
    owner?: string
    upload?: "1"
}

export const contractsUrlCodec =
    createUrlStateCodec<ContractsUrlState>(CONTRACTS_URL_FIELDS)
