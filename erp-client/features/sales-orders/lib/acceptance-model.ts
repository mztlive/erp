/**
 * W06 客户验收 — 工作台纯逻辑（数量解析、结果推导、草稿组装、校验）。
 * 无 React 依赖，供 hooks 与组件共用；行为与拆分前完全一致。
 */

import type { ValidationIssue } from "@/components/business"
import type {
    AcceptanceDraftLine,
    AcceptanceEligibleFact,
    AcceptanceOverallResult,
    AcceptanceSalesLineGroup,
} from "@/features/sales-orders/lib/acceptance-types"

export type LineResultState = {
    acceptedQuantity: string
    shortQuantity: string
    rejectedQuantity: string
    reason: string
    /** 服务不通过：将结果写入 rejected 并标注 */
    serviceFail: boolean
    /** 用户手工改过「通过数量」后不再被分配数自动覆盖 */
    acceptedManual: boolean
}

export type FormalResultState =
    | {
          kind: "post"
          status: "succeeded" | "unknown" | "failed"
          title: string
          description: string
          reference?: string
          facts: Array<{ label: string; value: string }>
      }
    | {
          kind: "reverse"
          status: "succeeded" | "failed"
          title: string
          description: string
          reference?: string
          facts: Array<{ label: string; value: string }>
      }

/** 已选履约批次：履约行 id → 事实 + 本次分配数量 */
export type AcceptanceFactSelection = Map<
    string,
    { fact: AcceptanceEligibleFact; qty: string }
>

export function emptyLineResult(): LineResultState {
    return {
        acceptedQuantity: "0",
        shortQuantity: "0",
        rejectedQuantity: "0",
        reason: "",
        serviceFail: false,
        acceptedManual: false,
    }
}

export function parseQty(value: string): number {
    const n = Number(value)
    return Number.isFinite(n) ? n : 0
}

export function todayLocalDateTimeInput(): string {
    const d = new Date()
    const pad = (n: number) => String(n).padStart(2, "0")
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`
}

export function formatOccurredAt(iso: string): string {
    try {
        return new Intl.DateTimeFormat("zh-CN", {
            dateStyle: "medium",
            timeStyle: "short",
            timeZone: "Asia/Shanghai",
        }).format(new Date(iso))
    } catch {
        return iso
    }
}

export function deriveOverall(lines: LineResultState[]): AcceptanceOverallResult {
    let hasReject = false
    let hasShort = false
    let hasServiceFail = false
    for (const line of lines) {
        if (line.serviceFail) hasServiceFail = true
        if (parseQty(line.rejectedQuantity) > 0) hasReject = true
        if (parseQty(line.shortQuantity) > 0) hasShort = true
    }
    if (hasServiceFail) return "SERVICE_FAIL"
    if (hasReject) return "REJECT"
    if (hasShort) return "SHORT"
    return "PASS"
}

export function buildDraftLines(
    selected: AcceptanceFactSelection,
    lineResults: Map<string, LineResultState>,
): AcceptanceDraftLine[] {
    const bySalesLine = new Map<
        string,
        Array<{ fulfillmentLineId: string; allocatedQuantity: string }>
    >()
    for (const [id, entry] of selected) {
        const list = bySalesLine.get(entry.fact.salesOrderLineId) ?? []
        list.push({
            fulfillmentLineId: id,
            allocatedQuantity: entry.qty,
        })
        bySalesLine.set(entry.fact.salesOrderLineId, list)
    }

    const lines: AcceptanceDraftLine[] = []
    for (const [salesOrderLineId, allocations] of bySalesLine) {
        const result = lineResults.get(salesOrderLineId) ?? emptyLineResult()
        lines.push({
            salesOrderLineId,
            acceptedQuantity: result.acceptedQuantity || "0",
            shortQuantity: result.shortQuantity || "0",
            rejectedQuantity: result.rejectedQuantity || "0",
            reason: result.reason,
            serviceFail: result.serviceFail,
            allocations,
        })
    }
    return lines
}

export function collectValidationIssues(
    selected: AcceptanceFactSelection,
    lineResults: Map<string, LineResultState>,
): ValidationIssue[] {
    const issues: ValidationIssue[] = []
    if (selected.size === 0) {
        issues.push({
            id: "no-source",
            label: "履约来源",
            message: "请至少选择一条可验收履约记录",
            targetId: "acceptance-fact-pool",
        })
        return issues
    }

    const lines = buildDraftLines(selected, lineResults)
    for (const line of lines) {
        const accepted = parseQty(line.acceptedQuantity)
        const short = parseQty(line.shortQuantity)
        const rejected = parseQty(line.rejectedQuantity)
        const total = accepted + short + rejected
        const alloc = line.allocations.reduce(
            (s, a) => s + parseQty(a.allocatedQuantity),
            0,
        )
        if (total <= 0) {
            issues.push({
                id: `line-empty-${line.salesOrderLineId}`,
                label: "验收数量",
                message: "通过、短少与拒收合计须大于 0",
                targetId: `line-result-${line.salesOrderLineId}`,
            })
        }
        if (Math.abs(total - alloc) > 1e-9) {
            issues.push({
                id: `line-balance-${line.salesOrderLineId}`,
                label: "数量守恒",
                message: `结果合计 ${total} 与分配合计 ${alloc} 不一致`,
                targetId: `line-result-${line.salesOrderLineId}`,
            })
        }
        if ((short > 0 || rejected > 0) && !line.reason.trim()) {
            issues.push({
                id: `line-reason-${line.salesOrderLineId}`,
                label: "客户反馈",
                message: "短少、拒收或服务不通过时原因必填",
                targetId: `line-reason-${line.salesOrderLineId}`,
            })
        }
        for (const allocItem of line.allocations) {
            const fact = selected.get(allocItem.fulfillmentLineId)?.fact
            if (!fact) continue
            if (
                parseQty(allocItem.allocatedQuantity) >
                parseQty(fact.eligibleQuantity)
            ) {
                issues.push({
                    id: `alloc-cap-${allocItem.fulfillmentLineId}`,
                    label: fact.fulfillmentNo,
                    message: `分配不可超过净可验收 ${fact.eligibleQuantity} ${fact.unitCode}`,
                    targetId: `alloc-qty-${allocItem.fulfillmentLineId}`,
                })
            }
            if (parseQty(allocItem.allocatedQuantity) <= 0) {
                issues.push({
                    id: `alloc-zero-${allocItem.fulfillmentLineId}`,
                    label: fact.fulfillmentNo,
                    message: "分配数量必须大于 0",
                    targetId: `alloc-qty-${allocItem.fulfillmentLineId}`,
                })
            }
        }
    }
    return issues
}

export function autoFillLineResult(
    salesOrderLineId: string,
    selected: AcceptanceFactSelection,
    prev: LineResultState | undefined,
): LineResultState {
    let allocSum = 0
    for (const entry of selected.values()) {
        if (entry.fact.salesOrderLineId === salesOrderLineId) {
            allocSum += parseQty(entry.qty)
        }
    }
    if (
        prev &&
        (parseQty(prev.shortQuantity) > 0 ||
            parseQty(prev.rejectedQuantity) > 0 ||
            prev.serviceFail)
    ) {
        return prev
    }
    // 用户手工改过「通过数量」后，调整分配数不再静默覆盖（P1-5）。
    if (prev?.acceptedManual) return prev
    return {
        acceptedQuantity: String(allocSum),
        shortQuantity: prev?.shortQuantity ?? "0",
        rejectedQuantity: prev?.rejectedQuantity ?? "0",
        reason: prev?.reason ?? "",
        serviceFail: prev?.serviceFail ?? false,
        acceptedManual: false,
    }
}

export function buildFactIndex(
    salesLines: AcceptanceSalesLineGroup[],
): Map<string, AcceptanceEligibleFact> {
    const factIndex = new Map<string, AcceptanceEligibleFact>()
    for (const line of salesLines) {
        for (const fact of line.fulfillmentFacts) {
            factIndex.set(fact.fulfillmentLineId, fact)
        }
    }
    return factIndex
}
