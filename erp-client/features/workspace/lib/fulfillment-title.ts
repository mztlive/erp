/**
 * 工作台履约任务标题。丢掉「草稿」和内部 id，优先露出销售/采购单号。
 */

import { displayText } from "@/features/fulfillment-operations/lib/readable-label"
import {
    OPERATION_TYPE_LABEL,
    type FulfillmentOperation,
} from "@/features/fulfillment-operations/types"

const DRAFT_STATUS_SUFFIX = /\s*·\s*(草稿|已发货|已签收|已冲正|待过账|已过账)$/

const OPAQUE_ID =
    /^(?:[0-9a-f]{24,}|[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})$/i

const PREFIXED_OPAQUE_ID = /^(?:DLV|FH|GRN|SF|DN|PR|ED)-[0-9a-f]{24,}$/i

const SOURCE_NUMBER = /(?:销售单|采购单)\s+(\S+)/

function isOpaqueToken(value: string): boolean {
    const trimmed = value.trim()
    return OPAQUE_ID.test(trimmed) || PREFIXED_OPAQUE_ID.test(trimmed)
}

function looksLikeDocumentNumber(value: string): boolean {
    const trimmed = value.trim()
    if (!trimmed || isOpaqueToken(trimmed)) return false
    return /[A-Za-z]/.test(trimmed) && /\d/.test(trimmed)
}

/**
 * 工作台列表第一行的扫读单号。
 *
 * 「供应商直发 · 销售单 SO-1」→「SO-1」；
 * 只有作业类型或内部 id 时回退作业类型，不把 UUID 摆上列表。
 */
export function fulfillmentListNumber(
    label: string | undefined,
    fallbackTypeLabel: string,
): string {
    const title = fulfillmentObjectTitle(label, fallbackTypeLabel)
    const source = title.match(SOURCE_NUMBER)?.[1]
    if (source && looksLikeDocumentNumber(source)) return source
    const tokens = title.split(/\s+/).filter((token) => token !== "·")
    const last = tokens[tokens.length - 1]
    if (last && looksLikeDocumentNumber(last)) return last
    return title
}

/**
 * 详情标题。去掉草稿状态和内部 id，保留作业类型与来源单号。
 */
export function fulfillmentObjectTitle(
    label: string | undefined,
    fallbackTypeLabel: string,
): string {
    const raw = label?.trim()
    if (!raw) return fallbackTypeLabel
    const withoutStatus = raw.replace(DRAFT_STATUS_SUFFIX, "").trim()
    const cleaned = withoutStatus
        .split(/\s+/)
        .filter((token) => !isOpaqueToken(token))
        .join(" ")
        .replace(/\s*·\s*/g, " · ")
        .replace(/^·\s*|\s*·$/g, "")
        .trim()
    return cleaned || fallbackTypeLabel
}

/**
 * 作业面标题：有来源单号时用作业类型 + 销售单号，否则清洗工作项标签。
 */
export function fulfillmentTaskTitle(
    item: {
        objectTitle: string
        workItemTypeLabel: string
    },
    operation?: Pick<FulfillmentOperation, "operationType" | "source">,
): string {
    if (operation) {
        const salesOrderNo = displayText(operation.source.salesOrderNo)
        const typeLabel = OPERATION_TYPE_LABEL[operation.operationType]
        return salesOrderNo ? `${typeLabel} · ${salesOrderNo}` : typeLabel
    }
    return fulfillmentObjectTitle(item.objectTitle, item.workItemTypeLabel)
}
