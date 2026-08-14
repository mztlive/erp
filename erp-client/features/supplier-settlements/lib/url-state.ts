import type {
    DifferenceType,
    SettlementSection,
    SettlementView,
} from "@/features/supplier-settlements/types"
import { SECTIONS } from "@/features/supplier-settlements/types"
import { createUrlStateCodec } from "@/lib/url-state"

export type SettlementsUrlState = {
    view: SettlementView
    supplierId?: string
    periodFrom?: string
    periodTo?: string
    status?: string
    differenceType?: DifferenceType
    q?: string
    page: number
    preview?: string
    statementId?: string
    workItemId?: string
    queueContextId?: string
    from?: string
    section: SettlementSection
    returnTo?: string
    /** 差异工作台选中项锚定（刷新/分享不丢上下文） */
    diff?: string
}

const VIEW_VALUES = [
    "pending",
    "prepared_by_me",
    "review_by_me",
    "confirmed",
] as const
const DIFF_VALUES = [
    "MISSING_ORDER",
    "DUPLICATE",
    "AMOUNT",
    "REFUND",
    "STATUS",
] as const

const codec = createUrlStateCodec<SettlementsUrlState>([
    { key: "view", type: "enum", values: VIEW_VALUES, defaultValue: "pending" },
    {
        key: "supplier",
        name: "supplierId",
        type: "string",
        aliases: ["supplierId"],
    },
    { key: "periodFrom", type: "string", aliases: ["period"] },
    { key: "periodTo", type: "string" },
    { key: "status", type: "string" },
    { key: "differenceType", type: "enum", values: DIFF_VALUES },
    { key: "q", type: "string", trim: true },
    { key: "page", type: "number", defaultValue: 1 },
    { key: "preview", type: "string" },
    { key: "statementId", type: "string", aliases: ["id"] },
    { key: "workItemId", type: "string" },
    { key: "queueContextId", type: "string" },
    { key: "from", type: "string" },
    {
        key: "section",
        type: "enum",
        values: SECTIONS,
        defaultValue: "overview",
        buildWhen: (value, state) =>
            value !== "overview" && Boolean(state.statementId),
    },
    { key: "returnTo", type: "string" },
    { key: "diff", type: "string" },
])

export const parseSettlementsSearchParams = codec.parse
export const buildSettlementsSearchParams = codec.build
