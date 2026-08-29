/**
 * W06 客户验收 — 进度推导、批次草稿、提交校验。
 * 无 React 依赖。分配合计必须等于通过数量（与后端守恒一致）。
 */

import type { ValidationIssue } from "@/components/business"
import {
    FULFILLMENT_TYPE_LABEL,
    type AcceptanceDraftLine,
    type AcceptanceEligibleFact,
    type AcceptanceOverallResult,
    type AcceptanceSalesLineGroup,
} from "@/features/sales-orders/lib/acceptance-types"

export type FormalResultState =
    | {
          kind: "post"
          status: "succeeded" | "unknown" | "failed"
          title: string
          description: string
          reference?: string
          facts: Array<{ label: string; value: string }>
          remainingEligibleCount?: number
          hasException?: boolean
      }
    | {
          kind: "reverse"
          status: "succeeded" | "failed"
          title: string
          description: string
          reference?: string
          facts: Array<{ label: string; value: string }>
      }

export type AcceptanceStuckKind = "done" | "accept" | "deliver" | "exception"

export type AcceptanceLineProgress = {
    salesOrderLineId: string
    lineNo: number
    itemSnapshot: string
    unitCode: string
    requiredQuantity: string
    deliveredQuantity: string
    acceptedQuantity: string
    pendingQuantity: string
    pendingFacts: AcceptanceEligibleFact[]
    stuckKind: AcceptanceStuckKind
    stuckLabel: string
}

export type AcceptanceOrderProgress = {
    lines: AcceptanceLineProgress[]
    requiredQuantity: string
    deliveredQuantity: string
    acceptedQuantity: string
    pendingQuantity: string
    unitCode: string | null
    pendingFactCount: number
}

export type AcceptanceBatchDraft = {
    fact: AcceptanceEligibleFact
    qty: string
    result: AcceptanceOverallResult
    exceptionQty: string
    reason: string
}

export type AcceptanceBatchSelection = Map<string, AcceptanceBatchDraft>

export function parseQty(value: string): number {
    const n = Number(value)
    return Number.isFinite(n) ? n : 0
}

export function formatQty(value: string | number): string {
    const n = typeof value === "number" ? value : Number(value)
    if (!Number.isFinite(n)) return "0"
    if (Number.isInteger(n)) return String(n)
    return String(n)
}

export function qtyWithUnit(
    quantity: string | number,
    unitCode: string,
): string {
    const qty = formatQty(quantity)
    return unitCode ? `${qty} ${unitCode}` : qty
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

export function passQuantity(draft: AcceptanceBatchDraft): number {
    const qty = parseQty(draft.qty)
    if (draft.result === "PASS") return qty
    return Math.max(0, qty - parseQty(draft.exceptionQty))
}

export function exceptionQuantity(draft: AcceptanceBatchDraft): number {
    if (draft.result === "PASS") return 0
    return parseQty(draft.exceptionQty)
}

export function defaultBatchDraft(
    fact: AcceptanceEligibleFact,
): AcceptanceBatchDraft {
    return {
        fact,
        qty: fact.eligibleQuantity,
        result: "PASS",
        exceptionQty: "0",
        reason: "",
    }
}

export function deriveOverall(
    drafts: Iterable<AcceptanceBatchDraft>,
): AcceptanceOverallResult {
    let hasReject = false
    let hasShort = false
    let hasServiceFail = false
    for (const draft of drafts) {
        if (draft.result === "SERVICE_FAIL") hasServiceFail = true
        if (draft.result === "REJECT") hasReject = true
        if (draft.result === "SHORT") hasShort = true
    }
    if (hasServiceFail) return "SERVICE_FAIL"
    if (hasReject) return "REJECT"
    if (hasShort) return "SHORT"
    return "PASS"
}

export function buildDraftLines(
    selected: AcceptanceBatchSelection,
): AcceptanceDraftLine[] {
    const bySalesLine = new Map<
        string,
        {
            accepted: number
            short: number
            rejected: number
            serviceFail: boolean
            reasons: string[]
            allocations: AcceptanceDraftLine["allocations"]
        }
    >()

    for (const draft of selected.values()) {
        const lineId = draft.fact.salesOrderLineId
        const current = bySalesLine.get(lineId) ?? {
            accepted: 0,
            short: 0,
            rejected: 0,
            serviceFail: false,
            reasons: [],
            allocations: [],
        }
        const passed = passQuantity(draft)
        const exception = exceptionQuantity(draft)
        current.accepted += passed
        if (draft.result === "SHORT") current.short += exception
        if (draft.result === "REJECT" || draft.result === "SERVICE_FAIL") {
            current.rejected += exception
        }
        if (draft.result === "SERVICE_FAIL") current.serviceFail = true
        if (draft.reason.trim()) current.reasons.push(draft.reason.trim())
        if (passed > 0) {
            current.allocations.push({
                fulfillmentLineId: draft.fact.fulfillmentLineId,
                fulfillmentFactType: draft.fact.fulfillmentFactType,
                allocatedQuantity: formatQty(passed),
            })
        }
        bySalesLine.set(lineId, current)
    }

    const lines: AcceptanceDraftLine[] = []
    for (const [salesOrderLineId, line] of bySalesLine) {
        lines.push({
            salesOrderLineId,
            acceptedQuantity: formatQty(line.accepted),
            shortQuantity: formatQty(line.short),
            rejectedQuantity: formatQty(line.rejected),
            reason: line.reasons.join("；"),
            serviceFail: line.serviceFail,
            allocations: line.allocations,
        })
    }
    return lines
}

export function collectValidationIssues(
    selected: AcceptanceBatchSelection,
): ValidationIssue[] {
    const issues: ValidationIssue[] = []
    if (selected.size === 0) {
        issues.push({
            id: "no-source",
            label: "交付批次",
            message: "请至少选择一条待验收的交付记录",
            targetId: "acceptance-register-list",
        })
        return issues
    }

    for (const draft of selected.values()) {
        const qty = parseQty(draft.qty)
        const eligible = parseQty(draft.fact.eligibleQuantity)
        const factId = draft.fact.fulfillmentLineId
        if (qty <= 0) {
            issues.push({
                id: `qty-zero-${factId}`,
                label: draft.fact.fulfillmentNo,
                message: "本次数量必须大于 0",
                targetId: `batch-qty-${factId}`,
            })
        }
        if (qty > eligible) {
            issues.push({
                id: `qty-cap-${factId}`,
                label: draft.fact.fulfillmentNo,
                message: `不能超过待验 ${formatQty(eligible)} ${draft.fact.unitCode}`,
                targetId: `batch-qty-${factId}`,
            })
        }
        if (draft.result !== "PASS") {
            const exception = parseQty(draft.exceptionQty)
            if (exception <= 0) {
                issues.push({
                    id: `exc-zero-${factId}`,
                    label: draft.fact.fulfillmentNo,
                    message: "请填写短少或拒收数量",
                    targetId: `batch-exc-${factId}`,
                })
            }
            if (exception > qty) {
                issues.push({
                    id: `exc-cap-${factId}`,
                    label: draft.fact.fulfillmentNo,
                    message: "短少或拒收不能超过本次数量",
                    targetId: `batch-exc-${factId}`,
                })
            }
            if (!draft.reason.trim()) {
                issues.push({
                    id: `reason-${factId}`,
                    label: draft.fact.fulfillmentNo,
                    message: "短少、拒收或服务不通过时原因必填",
                    targetId: `batch-reason-${factId}`,
                })
            }
        }
    }

    const lines = buildDraftLines(selected)
    for (const line of lines) {
        if (parseQty(line.acceptedQuantity) <= 0) {
            issues.push({
                id: `line-pass-${line.salesOrderLineId}`,
                label: "通过数量",
                message:
                    "整批短少或拒收时通过数量为 0，不能过账。请减少短少/拒收数量，或先不勾选本批、另开退货处理。",
                targetId: "acceptance-register-list",
            })
        }
        if (line.allocations.length === 0) {
            issues.push({
                id: `line-alloc-${line.salesOrderLineId}`,
                label: "通过数量",
                message: "至少要有一件通过，才能记到对应交付批次上。",
                targetId: "acceptance-register-list",
            })
        }
    }
    return issues
}

function sumFactQty(
    facts: AcceptanceEligibleFact[],
    pick: (fact: AcceptanceEligibleFact) => string,
): number {
    return facts.reduce((sum, fact) => sum + parseQty(pick(fact)), 0)
}

function stuckForLine(input: {
    required: number
    delivered: number
    accepted: number
    pending: number
    pendingFacts: AcceptanceEligibleFact[]
}): { kind: AcceptanceStuckKind; label: string } {
    const { required, delivered, accepted, pending, pendingFacts } = input
    if (pending > 0) {
        if (pendingFacts.length === 1) {
            const fact = pendingFacts[0]
            return {
                kind: "accept",
                label: `${typeLabel(fact)} ${fact.fulfillmentNo} 待验 ${formatQty(fact.eligibleQuantity)}`,
            }
        }
        const types = uniqueTypes(pendingFacts)
        return {
            kind: "accept",
            label: `待验 ${pendingFacts.length} 批${types ? ` · ${types}` : ""}`,
        }
    }
    if (delivered + 1e-9 < required) {
        return {
            kind: "deliver",
            label: `还差 ${formatQty(required - delivered)} 未交付`,
        }
    }
    if (accepted + 1e-9 < required) {
        return { kind: "exception", label: "已交付未全部通过" }
    }
    return { kind: "done", label: "已完成" }
}

function typeLabel(fact: AcceptanceEligibleFact): string {
    return FULFILLMENT_TYPE_LABEL[fact.fulfillmentFactType]
}

function uniqueTypes(facts: AcceptanceEligibleFact[]): string {
    return [...new Set(facts.map(typeLabel))].join("/")
}

export function buildLineProgress(
    group: AcceptanceSalesLineGroup,
): AcceptanceLineProgress {
    const pendingFacts = group.fulfillmentFacts.filter(
        (fact) => parseQty(fact.eligibleQuantity) > 0,
    )
    const required = parseQty(group.requiredQuantity)
    const delivered = sumFactQty(
        group.fulfillmentFacts,
        (fact) => fact.netSuccessfulQuantity,
    )
    const accepted = sumFactQty(
        group.fulfillmentFacts,
        (fact) => fact.netAcceptedAllocatedQuantity,
    )
    const pending = sumFactQty(pendingFacts, (fact) => fact.eligibleQuantity)
    const stuck = stuckForLine({
        required,
        delivered,
        accepted,
        pending,
        pendingFacts,
    })
    return {
        salesOrderLineId: group.salesOrderLineId,
        lineNo: group.lineNo,
        itemSnapshot: group.itemSnapshot,
        unitCode: group.unitCode,
        requiredQuantity: formatQty(group.requiredQuantity),
        deliveredQuantity: formatQty(delivered),
        acceptedQuantity: formatQty(accepted),
        pendingQuantity: formatQty(pending),
        pendingFacts,
        stuckKind: stuck.kind,
        stuckLabel: stuck.label,
    }
}

export function buildOrderProgress(
    salesLines: AcceptanceSalesLineGroup[],
): AcceptanceOrderProgress {
    const lines = salesLines
        .map(buildLineProgress)
        .sort((a, b) => a.lineNo - b.lineNo)
    const units = new Set(lines.map((line) => line.unitCode).filter(Boolean))
    const unitCode = units.size === 1 ? ([...units][0] ?? null) : null
    const sum = (pick: (line: AcceptanceLineProgress) => string) =>
        formatQty(
            lines.reduce((total, line) => total + parseQty(pick(line)), 0),
        )
    return {
        lines,
        requiredQuantity: sum((line) => line.requiredQuantity),
        deliveredQuantity: sum((line) => line.deliveredQuantity),
        acceptedQuantity: sum((line) => line.acceptedQuantity),
        pendingQuantity: sum((line) => line.pendingQuantity),
        unitCode,
        pendingFactCount: lines.reduce(
            (count, line) => count + line.pendingFacts.length,
            0,
        ),
    }
}

export function pendingFactsOf(
    salesLines: AcceptanceSalesLineGroup[],
): AcceptanceEligibleFact[] {
    return salesLines.flatMap((line) =>
        line.fulfillmentFacts.filter(
            (fact) => parseQty(fact.eligibleQuantity) > 0,
        ),
    )
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
