/**
 * W06 客户验收 — 进度推导、批次草稿、提交校验。
 * 无 React 依赖。分配合计必须等于通过数量（与后端守恒一致）。
 */

import type { ValidationIssue } from "@/components/business"
import {
    FULFILLMENT_TYPE_LABEL,
    type AcceptanceBatchDecision,
    type AcceptanceDraftLine,
    type AcceptanceEligibleFact,
    type AcceptanceOverallResult,
    type AcceptanceSalesLineGroup,
} from "@/features/sales-orders/lib/acceptance-types"
import {
    clampZeroFixed,
    compactFixed,
    compareDecimal,
    normalizeFixed,
    subtractFixed,
    sumFixed,
} from "@/lib/fixed-decimal"

const QUANTITY_SCALE = 6

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

export function compareQty(left: string, right: string): -1 | 0 | 1 {
    try {
        return compareDecimal(left, right, QUANTITY_SCALE)
    } catch {
        return 0
    }
}

export function isPositiveQty(value: string): boolean {
    return compareQty(value, "0") > 0
}

export function formatQty(value: string): string {
    try {
        return compactFixed(
            normalizeFixed(value, {
                maxScale: QUANTITY_SCALE,
                outputScale: QUANTITY_SCALE,
                allowNegative: true,
            }),
        )
    } catch {
        return "0"
    }
}

function sumQty(values: readonly string[]): string {
    return compactFixed(
        sumFixed(values, {
            maxScale: QUANTITY_SCALE,
            outputScale: QUANTITY_SCALE,
            allowNegative: true,
        }),
    )
}

export function qtyWithUnit(quantity: string, unitCode: string): string {
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

export function passQuantity(draft: AcceptanceBatchDraft): string {
    if (draft.result === "PASS") return formatQty(draft.qty)
    const remaining = subtractFixed(draft.qty, draft.exceptionQty, {
        maxScale: QUANTITY_SCALE,
        outputScale: QUANTITY_SCALE,
    })
    return compactFixed(
        clampZeroFixed(remaining, {
            maxScale: QUANTITY_SCALE,
            outputScale: QUANTITY_SCALE,
        }),
    )
}

export function exceptionQuantity(draft: AcceptanceBatchDraft): string {
    if (draft.result === "PASS") return "0"
    return formatQty(draft.exceptionQty)
}

export function isSinglePiece(quantity: string): boolean {
    return isPositiveQty(quantity) && compareQty(quantity, "1") <= 0
}

export function hasFilledException(draft: AcceptanceBatchDraft): boolean {
    return draft.result !== "PASS" && isPositiveQty(draft.exceptionQty)
}

export function applyResultChange(
    draft: AcceptanceBatchDraft,
    result: AcceptanceOverallResult,
): AcceptanceBatchDraft {
    if (result === "PASS") {
        return { ...draft, result, exceptionQty: "0", reason: "" }
    }
    return {
        ...draft,
        result,
        exceptionQty: isPositiveQty(draft.exceptionQty)
            ? draft.exceptionQty
            : formatQty(draft.qty),
    }
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

export function pendingAsPassSelection(
    salesLines: AcceptanceSalesLineGroup[],
): AcceptanceBatchSelection {
    const next: AcceptanceBatchSelection = new Map()
    for (const fact of pendingFactsOf(salesLines)) {
        next.set(fact.fulfillmentLineId, defaultBatchDraft(fact))
    }
    return next
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
            accepted: string[]
            short: string[]
            rejected: string[]
            serviceFail: boolean
            reasons: string[]
            allocations: AcceptanceDraftLine["allocations"]
        }
    >()

    for (const draft of selected.values()) {
        const lineId = draft.fact.salesOrderLineId
        const current = bySalesLine.get(lineId) ?? {
            accepted: [],
            short: [],
            rejected: [],
            serviceFail: false,
            reasons: [],
            allocations: [],
        }
        const passed = passQuantity(draft)
        const exception = exceptionQuantity(draft)
        current.accepted.push(passed)
        if (draft.result === "SHORT") current.short.push(exception)
        if (draft.result === "REJECT" || draft.result === "SERVICE_FAIL") {
            current.rejected.push(exception)
        }
        if (draft.result === "SERVICE_FAIL") current.serviceFail = true
        if (draft.reason.trim()) current.reasons.push(draft.reason.trim())
        if (isPositiveQty(passed)) {
            current.allocations.push({
                fulfillmentLineId: draft.fact.fulfillmentLineId,
                fulfillmentFactType: draft.fact.fulfillmentFactType,
                allocatedQuantity: passed,
            })
        }
        bySalesLine.set(lineId, current)
    }

    const lines: AcceptanceDraftLine[] = []
    for (const [salesOrderLineId, line] of bySalesLine) {
        lines.push({
            salesOrderLineId,
            acceptedQuantity: sumQty(line.accepted),
            shortQuantity: sumQty(line.short),
            rejectedQuantity: sumQty(line.rejected),
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
        const factId = draft.fact.fulfillmentLineId
        if (!isPositiveQty(draft.qty)) {
            issues.push({
                id: `qty-zero-${factId}`,
                label: draft.fact.fulfillmentNo,
                message: "本次数量必须大于 0",
                targetId: `batch-qty-${factId}`,
            })
        }
        if (compareQty(draft.qty, draft.fact.eligibleQuantity) > 0) {
            issues.push({
                id: `qty-cap-${factId}`,
                label: draft.fact.fulfillmentNo,
                message: `不能超过待验 ${formatQty(draft.fact.eligibleQuantity)} ${draft.fact.unitCode}`,
                targetId: `batch-qty-${factId}`,
            })
        }
        if (draft.result !== "PASS") {
            if (!isPositiveQty(draft.exceptionQty)) {
                issues.push({
                    id: `exc-zero-${factId}`,
                    label: draft.fact.fulfillmentNo,
                    message: "请填写短少或拒收数量",
                    targetId: `batch-exc-${factId}`,
                })
            }
            if (compareQty(draft.exceptionQty, draft.qty) > 0) {
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
        if (!isPositiveQty(line.acceptedQuantity)) {
            const sample = [...selected.values()].find(
                (draft) =>
                    draft.fact.salesOrderLineId === line.salesOrderLineId,
            )
            const name = sample?.fact.itemSnapshot ?? "本批"
            issues.push({
                id: `line-pass-${line.salesOrderLineId}`,
                label: name,
                message: `${name}整件短少或拒收不能记入验收。请点「本次不验」后另开退货处理，或多件时减少短少数量并保留通过。`,
                targetId: "acceptance-register-list",
            })
        }
    }
    return issues
}

function sumFactQty(
    facts: AcceptanceEligibleFact[],
    pick: (fact: AcceptanceEligibleFact) => string,
): string {
    return sumQty(facts.map(pick))
}

function stuckForLine(input: {
    required: string
    delivered: string
    accepted: string
    pending: string
    pendingFacts: AcceptanceEligibleFact[]
}): { kind: AcceptanceStuckKind; label: string } {
    const { required, delivered, accepted, pending, pendingFacts } = input
    if (isPositiveQty(pending)) {
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
    if (compareQty(delivered, required) < 0) {
        const remaining = subtractFixed(required, delivered, {
            maxScale: QUANTITY_SCALE,
            outputScale: QUANTITY_SCALE,
        })
        return {
            kind: "deliver",
            label: `还差 ${formatQty(remaining)} 未交付`,
        }
    }
    if (compareQty(accepted, required) < 0) {
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
    const pendingFacts = group.fulfillmentFacts.filter((fact) =>
        isPositiveQty(fact.eligibleQuantity),
    )
    const required = formatQty(group.requiredQuantity)
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
        requiredQuantity: required,
        deliveredQuantity: delivered,
        acceptedQuantity: accepted,
        pendingQuantity: pending,
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
        sumQty(lines.map(pick))
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
        line.fulfillmentFacts.filter((fact) =>
            isPositiveQty(fact.eligibleQuantity),
        ),
    )
}

/** 服务明细只能通过或不通过；商品明细可短少或拒收。 */
export function resultDecisionsForFact(
    fact: AcceptanceEligibleFact,
): AcceptanceOverallResult[] {
    if (fact.fulfillmentFactType === "SERVICE") {
        return ["PASS", "SERVICE_FAIL"]
    }
    return ["PASS", "SHORT", "REJECT"]
}

/** 每批可选结果，含本次不验。 */
export function batchDecisionsForFact(
    fact: AcceptanceEligibleFact,
): AcceptanceBatchDecision[] {
    return ["SKIP", ...resultDecisionsForFact(fact)]
}

export function registerableSalesLines(
    salesLines: readonly AcceptanceSalesLineGroup[],
): AcceptanceSalesLineGroup[] {
    return salesLines.filter((line) => pendingFactsOf([line]).length > 0)
}

export function lineAcceptanceHint(
    facts: readonly AcceptanceEligibleFact[],
): string {
    const pending = facts.filter((fact) => isPositiveQty(fact.eligibleQuantity))
    if (pending.length === 0) return ""
    const allService = pending.every(
        (fact) => fact.fulfillmentFactType === "SERVICE",
    )
    const allGoods = pending.every(
        (fact) => fact.fulfillmentFactType !== "SERVICE",
    )
    if (allService) return "服务明细只能记通过或不通过。"
    if (allGoods) return "商品明细可记通过、短少或拒收。"
    return "本行含商品与服务，按每批选择结果。"
}

export function summarizeLineDecisions(
    facts: readonly AcceptanceEligibleFact[],
    selected: AcceptanceBatchSelection,
): string {
    const pending = facts.filter((fact) => isPositiveQty(fact.eligibleQuantity))
    if (pending.length === 0) return "无待验"
    let skip = 0
    let pass = 0
    let exception = 0
    for (const fact of pending) {
        const draft = selected.get(fact.fulfillmentLineId)
        if (!draft) skip += 1
        else if (draft.result === "PASS") pass += 1
        else exception += 1
    }
    if (skip === pending.length) return "本次不验"
    if (pass === pending.length) return "全部通过"
    if (exception === pending.length) return "含异常"
    const parts: string[] = []
    if (pass > 0) parts.push(`通过 ${pass}`)
    if (exception > 0) parts.push(`异常 ${exception}`)
    if (skip > 0) parts.push(`不验 ${skip}`)
    return parts.join(" · ")
}

export function exceptionQuantityLabel(
    result: AcceptanceOverallResult,
): string {
    if (result === "SHORT") return "短少数量"
    if (result === "SERVICE_FAIL") return "不通过数量"
    return "拒收数量"
}

export type AcceptanceConfirmLine = {
    fulfillmentLineId: string
    itemLabel: string
    fulfillmentLabel: string
    resultText: string
    resultTone: "success" | "warning" | "destructive"
    reason?: string
}

const OPAQUE_ID =
    /^(?:[0-9a-f]{24,}|[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})$/i

const PREFIXED_OPAQUE_ID =
    /^(?:DLV|FH|GRN|SF|DN|PR|ED|PO|SO|CG)-[0-9a-f]{24,}$/i

/**
 * 确认层回显：每个已勾选批次拆成品名、交付方式和结果。
 * 内部单号不上屏；不验批次不在 selected 里，不会出现。
 */
export function acceptanceConfirmLines(
    selected: AcceptanceBatchSelection,
): AcceptanceConfirmLine[] {
    return [...selected.values()]
        .sort((left, right) => {
            if (left.fact.lineNo !== right.fact.lineNo) {
                return left.fact.lineNo - right.fact.lineNo
            }
            return left.fact.itemSnapshot.localeCompare(
                right.fact.itemSnapshot,
                "zh",
            )
        })
        .map((draft) => {
            const reason = draft.reason.trim()
            return {
                fulfillmentLineId: draft.fact.fulfillmentLineId,
                itemLabel: draft.fact.itemSnapshot,
                fulfillmentLabel: visibleFulfillmentLabel(draft.fact),
                resultText: acceptanceConfirmResultText(draft),
                resultTone: acceptanceConfirmResultTone(draft.result),
                reason: reason || undefined,
            }
        })
}

function visibleFulfillmentLabel(fact: AcceptanceEligibleFact): string {
    const type = FULFILLMENT_TYPE_LABEL[fact.fulfillmentFactType]
    const documentNo = fact.fulfillmentNo.trim()
    if (
        !documentNo ||
        OPAQUE_ID.test(documentNo) ||
        PREFIXED_OPAQUE_ID.test(documentNo)
    ) {
        return type
    }
    return `${type} ${documentNo}`
}

function acceptanceConfirmResultTone(
    result: AcceptanceOverallResult,
): AcceptanceConfirmLine["resultTone"] {
    if (result === "PASS") return "success"
    if (result === "SHORT") return "warning"
    return "destructive"
}

function acceptanceConfirmResultText(draft: AcceptanceBatchDraft): string {
    const unit = draft.fact.unitCode
    if (draft.result === "PASS") {
        return `通过 ${qtyWithUnit(passQuantity(draft), unit)}`
    }
    const exceptionWord =
        draft.result === "SHORT"
            ? "短少"
            : draft.result === "SERVICE_FAIL"
              ? "不通过"
              : "拒收"
    const exception = qtyWithUnit(exceptionQuantity(draft), unit)
    const passed = passQuantity(draft)
    if (!isPositiveQty(passed)) {
        return `${exceptionWord} ${exception}`
    }
    return `${exceptionWord} ${exception}、通过 ${qtyWithUnit(passed, unit)}`
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
