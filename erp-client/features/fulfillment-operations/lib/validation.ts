/**
 * W09 提交前的客户端校验与确认说明。
 * 页面检查只用于提前拦截明显错误；最终以服务端复核为准。
 */

import type { ValidationIssue } from "@/components/business"
import type {
    FulfillmentDraft,
    FulfillmentFormalOutcome,
    FulfillmentOperation,
    ReceiptDraftLine,
} from "@/features/fulfillment-operations/types"
import {
    FACT_TYPE_LABEL,
    FORMAL_STATUS_LABEL,
    RESULT_LABEL,
    SERVICE_RESULT_LABEL,
} from "@/features/fulfillment-operations/types"
import {
    clampZeroFixed,
    compactFixed,
    compareDecimal,
    subtractFixed,
    sumFixed,
} from "@/lib/fixed-decimal"

const QUANTITY_SCALE = 6

function compareQuantity(left: string, right: string): -1 | 0 | 1 | null {
    try {
        return compareDecimal(left, right, QUANTITY_SCALE)
    } catch {
        return null
    }
}

function isPositiveQuantity(value: string): boolean {
    return compareQuantity(value, "0") === 1
}

function quantityExceeds(left: string, right: string): boolean {
    return compareQuantity(left, right) === 1
}

function quantityTotal(values: readonly string[]): string {
    try {
        return compactFixed(
            sumFixed(
                values.map((value) => value || "0"),
                {
                    maxScale: QUANTITY_SCALE,
                    outputScale: QUANTITY_SCALE,
                },
            ),
        )
    } catch {
        return "—"
    }
}

export function cloneDraft(draft: FulfillmentDraft): FulfillmentDraft {
    return structuredClone(draft)
}

/**
 * 合格数量跟着「到货 − 不合格」走。
 * 一线最常见的是「全收全合格」和「收 N 坏 R」两种，这样只要动一到两个框。
 * 用户直接改合格数量时不再回算，避免和手工修正打架。
 */
export function withDerivedQualified(line: ReceiptDraftLine): ReceiptDraftLine {
    try {
        const difference = subtractFixed(
            line.receivedQuantity,
            line.rejectedQuantity,
            { maxScale: QUANTITY_SCALE, outputScale: QUANTITY_SCALE },
        )
        return {
            ...line,
            qualifiedQuantity: compactFixed(
                clampZeroFixed(difference, {
                    maxScale: QUANTITY_SCALE,
                    outputScale: QUANTITY_SCALE,
                }),
            ),
        }
    } catch {
        return line
    }
}

export function clientValidation(
    operation: FulfillmentOperation,
    draft: FulfillmentDraft,
): ValidationIssue[] {
    const issues: ValidationIssue[] = []
    if (draft.type !== operation.operationType) {
        issues.push({
            id: "type-mismatch",
            label: "单据类型",
            message: "这条草稿和当前单据对不上",
        })
        return issues
    }
    if (operation.gate.state === "BLOCKED" && draft.type !== "WAREHOUSE_SHIP") {
        issues.push({
            id: "gate",
            label: "先款条件",
            message: operation.gate.message,
            targetId: "prepayment-gate",
        })
    }

    if (draft.type === "RECEIPT") {
        draft.lines.forEach((line, i) => {
            if (!isPositiveQuantity(line.receivedQuantity)) {
                issues.push({
                    id: `recv-${i}`,
                    label: "到货数量",
                    message: "必须大于 0",
                    targetId: `receipt-recv-${i}`,
                })
            }
            const qualityTotal = quantityTotal([
                line.qualifiedQuantity,
                line.rejectedQuantity,
            ])
            if (
                qualityTotal !== "—" &&
                quantityExceeds(qualityTotal, line.receivedQuantity)
            ) {
                issues.push({
                    id: `qty-sum-${i}`,
                    label: "质量数量",
                    message: "合格 + 不合格不得超过到货",
                    targetId: `receipt-qual-${i}`,
                })
            }
            if (!line.qualityResult) {
                issues.push({
                    id: `recv-qr-${i}`,
                    label: "质量结果",
                    message: "请选择质量结果",
                    targetId: `receipt-qr-${i}`,
                })
            }
        })
    }
    if (draft.type === "WAREHOUSE_SHIP") {
        if (!draft.carrier.trim()) {
            issues.push({
                id: "carrier",
                label: "承运方",
                message: "必填",
                targetId: "ship-carrier",
            })
        }
        if (!draft.trackingNo.trim()) {
            issues.push({
                id: "tracking",
                label: "物流单号",
                message: "必填",
                targetId: "ship-tracking",
            })
        }
        draft.lines.forEach((line, i) => {
            const src = operation.lines.find(
                (l) => l.salesOrderLineId === line.salesOrderLineId,
            )
            const cap = src?.reservedQuantity ?? src?.remainingQuantity ?? "0"
            if (!isPositiveQuantity(line.quantity)) {
                issues.push({
                    id: `ship-qty-${i}`,
                    label: "发货数量",
                    message: "必须大于 0",
                    targetId: `ship-qty-${i}`,
                })
            } else if (quantityExceeds(line.quantity, cap)) {
                issues.push({
                    id: `ship-cap-${i}`,
                    label: "发货数量",
                    message: `不能超过为这单留的 ${cap}`,
                    targetId: `ship-qty-${i}`,
                })
            }
            if (!line.stockReservationId) {
                issues.push({
                    id: `ship-rsv-${i}`,
                    label: "留货",
                    message: "找不到为这单留的货，先联系仓储确认",
                })
            }
        })
    }
    if (draft.type === "SUPPLIER_DIRECT") {
        if (!draft.carrier.trim()) {
            issues.push({
                id: "d-carrier",
                label: "承运方",
                message: "必填",
                targetId: "direct-carrier",
            })
        }
        if (!draft.trackingNo.trim()) {
            issues.push({
                id: "d-tracking",
                label: "物流单号",
                message: "必填",
                targetId: "direct-tracking",
            })
        }
    }
    if (draft.type === "ELECTRONIC") {
        if (!draft.result) {
            issues.push({
                id: "el-result",
                label: "履约结果",
                message: "请选择履约结果",
                targetId: "el-result",
            })
        }
        if (!draft.recipientMasked.trim()) {
            issues.push({
                id: "el-recipient",
                label: "交付对象",
                message: "交付对象不能为空",
            })
        }
        draft.lines.forEach((line, i) => {
            const src = operation.lines.find(
                (l) => l.salesOrderLineId === line.salesOrderLineId,
            )
            const cap = src?.remainingQuantity ?? "0"
            if (!isPositiveQuantity(line.quantity)) {
                issues.push({
                    id: `el-qty-${i}`,
                    label: "交付数量",
                    message: "必须大于 0",
                    targetId: `el-qty-${i}`,
                })
            } else if (
                isPositiveQuantity(cap) &&
                quantityExceeds(line.quantity, cap)
            ) {
                issues.push({
                    id: `el-cap-${i}`,
                    label: "交付数量",
                    message: `不能超过剩余可交付 ${cap}`,
                    targetId: `el-qty-${i}`,
                })
            }
        })
    }
    if (draft.type === "SERVICE") {
        if (draft.result !== "SUCCESS" && draft.result !== "FAILURE") {
            issues.push({
                id: "svc-result",
                label: "履约结果",
                message: "请选择成功或失败",
                targetId: "svc-result",
            })
        }
        if (!draft.serviceLocation.trim()) {
            issues.push({
                id: "svc-loc",
                label: "服务地点",
                message: "必填",
                targetId: "service-loc",
            })
        }
        if (!draft.startedAt || !draft.endedAt) {
            issues.push({
                id: "svc-time-req",
                label: "服务时间",
                message: "开始与结束时间都要填",
                targetId: "service-start",
            })
        }
        if (
            draft.endedAt &&
            draft.startedAt &&
            draft.endedAt < draft.startedAt
        ) {
            issues.push({
                id: "svc-time",
                label: "服务时间",
                message: "结束不得早于开始",
                targetId: "service-start",
            })
        }
        if (
            !draft.completionNote.trim() ||
            draft.completionNote.trim().length < 4
        ) {
            issues.push({
                id: "svc-note",
                label: "完成说明",
                message: "至少 4 个字",
                targetId: "service-note",
            })
        }
        if (!draft.evidenceFile && !draft.evidenceAttachmentId.trim()) {
            issues.push({
                id: "svc-evidence",
                label: "图片凭证",
                message: "请上传现场图片凭证",
                targetId: "service-evidence",
            })
        } else if (draft.evidenceFile) {
            const type = draft.evidenceFile.type
            if (
                type !== "image/jpeg" &&
                type !== "image/png" &&
                type !== "image/webp"
            ) {
                issues.push({
                    id: "svc-evidence-type",
                    label: "图片凭证",
                    message: "仅支持 JPG、PNG 或 WebP",
                    targetId: "service-evidence",
                })
            }
            if (draft.evidenceFile.size > 5 * 1024 * 1024) {
                issues.push({
                    id: "svc-evidence-size",
                    label: "图片凭证",
                    message: "图片不能超过 5 MB",
                    targetId: "service-evidence",
                })
            }
        }
        draft.lines.forEach((line, i) => {
            if (!isPositiveQuantity(line.quantity)) {
                issues.push({
                    id: `svc-qty-${i}`,
                    label: "服务数量",
                    message: "必须大于 0",
                    targetId: `svc-qty-${i}`,
                })
            }
        })
    }
    return issues
}

export function buildPostedFacts(outcome: FulfillmentFormalOutcome) {
    const facts: { label: string; value: string }[] = [
        {
            label: "记录类型",
            value: FACT_TYPE_LABEL[outcome.factType],
        },
        { label: "记录编号", value: outcome.factNo },
        {
            label: "当前状态",
            value:
                FORMAL_STATUS_LABEL[outcome.formalStatus] ??
                outcome.formalStatus,
        },
        { label: "库存会怎么变", value: outcome.inventoryImpactSummary },
    ]
    if (outcome.inventoryDelta.length > 0) {
        facts.push({
            label: "库存流水",
            value: outcome.inventoryDelta
                .map(
                    (d) =>
                        `${d.warehouseLabel} · ${d.skuLabel} ${d.direction === "INCREASE" ? "+" : "−"}${d.quantity}`,
                )
                .join("；"),
        })
    }
    if (outcome.reservationDelta.length > 0) {
        facts.push({
            label: "留货变化",
            value: outcome.reservationDelta
                .map(
                    (d) =>
                        `${d.action === "CREATE" ? "留出" : "用掉"} ${d.quantity}`,
                )
                .join("；"),
        })
    }
    facts.push({
        label: "还剩多少没处理",
        value:
            outcome.remainingByLine
                .map((l) => `${l.itemName} ${l.quantity}`)
                .join("；") || "0",
    })
    facts.push({
        label: "接下来",
        value: outcome.acceptanceNextStep,
    })
    return facts
}

/**
 * 确认弹窗的一句话说明。明细、锁定字段和下一责任人不再重复列出。
 */
export function confirmDescription(draft: FulfillmentDraft): string {
    const suffix = "确认后不能改。"
    if (draft.type === "RECEIPT") {
        const qual = quantityTotal(
            draft.lines.map((line) => line.qualifiedQuantity),
        )
        const rej = quantityTotal(
            draft.lines.map((line) => line.rejectedQuantity),
        )
        return isPositiveQuantity(rej)
            ? `合格 ${qual} 入库存并留货，不合格 ${rej} 不入库。${suffix}`
            : `合格 ${qual} 入库存并留货。${suffix}`
    }
    if (draft.type === "WAREHOUSE_SHIP") {
        const qty = quantityTotal(draft.lines.map((line) => line.quantity))
        return `发出 ${qty}，扣库存并核销留货。${suffix}`
    }
    if (draft.type === "SUPPLIER_DIRECT") {
        return `供应商直发给客户，不走自有仓库。${suffix}`
    }
    if (draft.type === "ELECTRONIC") {
        return `交付结果：${RESULT_LABEL[draft.result]}。不动库存。${suffix}`
    }
    return `服务结果：${SERVICE_RESULT_LABEL[draft.result || "SUCCESS"]}。已附现场图片凭证，不动库存。${suffix}`
}
