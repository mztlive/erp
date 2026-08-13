/**
 * W09 提交前的客户端校验与影响预览。
 * 页面检查只用于提前拦截明显错误；最终以服务端复核为准。
 */

import type { ValidationIssue } from "@/components/business"
import type {
    FulfillmentDraft,
    FulfillmentFormalOutcome,
    FulfillmentTask,
    ReceiptDraftLine,
} from "@/features/fulfillment-operations/types"
import {
    FACT_TYPE_LABEL,
    FORMAL_STATUS_LABEL,
    RESULT_LABEL,
} from "@/features/fulfillment-operations/types"

export function cloneDraft(draft: FulfillmentDraft): FulfillmentDraft {
    return structuredClone(draft)
}

/**
 * 合格数量跟着「到货 − 不合格」走。
 * 一线最常见的是「全收全合格」和「收 N 坏 R」两种，这样只要动一到两个框。
 * 用户直接改合格数量时不再回算，避免和手工修正打架。
 */
export function withDerivedQualified(line: ReceiptDraftLine): ReceiptDraftLine {
    const recv = Number(line.receivedQuantity)
    const rej = Number(line.rejectedQuantity)
    if (!Number.isFinite(recv) || !Number.isFinite(rej)) return line
    return { ...line, qualifiedQuantity: String(Math.max(0, recv - rej)) }
}

export function clientValidation(
    task: FulfillmentTask,
    draft: FulfillmentDraft,
): ValidationIssue[] {
    const issues: ValidationIssue[] = []
    if (draft.type !== task.operationType) {
        issues.push({
            id: "type-mismatch",
            label: "任务类型",
            message: "这条草稿和当前任务对不上",
        })
        return issues
    }
    if (task.gate.state === "BLOCKED" && draft.type !== "WAREHOUSE_SHIP") {
        issues.push({
            id: "gate",
            label: "先款条件",
            message: task.gate.message,
            targetId: "prepayment-gate",
        })
    }

    if (draft.type === "RECEIPT") {
        draft.lines.forEach((line, i) => {
            const recv = Number(line.receivedQuantity)
            const qual = Number(line.qualifiedQuantity)
            const rej = Number(line.rejectedQuantity)
            if (!(recv > 0)) {
                issues.push({
                    id: `recv-${i}`,
                    label: "到货数量",
                    message: "必须大于 0",
                    targetId: `receipt-recv-${i}`,
                })
            }
            if (qual + rej > recv + 1e-9) {
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
            const qty = Number(line.quantity)
            const src = task.lines.find(
                (l) => l.salesOrderLineId === line.salesOrderLineId,
            )
            const cap = Number(
                src?.reservedQuantity ?? src?.remainingQuantity ?? 0,
            )
            if (!(qty > 0)) {
                issues.push({
                    id: `ship-qty-${i}`,
                    label: "发货数量",
                    message: "必须大于 0",
                    targetId: `ship-qty-${i}`,
                })
            } else if (qty > cap + 1e-9) {
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
            const qty = Number(line.quantity)
            const src = task.lines.find(
                (l) => l.salesOrderLineId === line.salesOrderLineId,
            )
            const cap = Number(src?.remainingQuantity ?? 0)
            if (!(qty > 0)) {
                issues.push({
                    id: `el-qty-${i}`,
                    label: "交付数量",
                    message: "必须大于 0",
                    targetId: `el-qty-${i}`,
                })
            } else if (cap > 0 && qty > cap + 1e-9) {
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
        if (!draft.result) {
            issues.push({
                id: "svc-result",
                label: "履约结果",
                message: "请选择履约结果",
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
                targetId: "service-ended",
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
        draft.lines.forEach((line, i) => {
            const qty = Number(line.quantity)
            if (!(qty > 0)) {
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

export function impactPreview(
    task: FulfillmentTask,
    draft: FulfillmentDraft,
): string[] {
    if (draft.type === "RECEIPT") {
        const qual = draft.lines.reduce(
            (s, l) => s + Number(l.qualifiedQuantity || 0),
            0,
        )
        const rej = draft.lines.reduce(
            (s, l) => s + Number(l.rejectedQuantity || 0),
            0,
        )
        return [
            `合格 ${qual} 入库存，并按对应的销售单留货`,
            `不合格 ${rej} 不入库，也不留货`,
            "不影响客户验收，验收由销售另外登记",
        ]
    }
    if (draft.type === "WAREHOUSE_SHIP") {
        const qty = draft.lines.reduce((s, l) => s + Number(l.quantity || 0), 0)
        return [
            `发出 ${qty}：用掉为这单留的货，库存相应减少`,
            "客户签收不等于验收通过，验收由销售另外登记",
        ]
    }
    if (draft.type === "SUPPLIER_DIRECT") {
        return [
            "记一笔供应商直接发客户",
            "不动自己仓库的库存",
            "接下来由销售登记客户验收",
        ]
    }
    if (draft.type === "ELECTRONIC") {
        return [
            `交付结果：${RESULT_LABEL[draft.result]}`,
            "不动库存。填「失败」也不能再改，要重做得新开一条",
            "成功后由销售登记客户验收",
        ]
    }
    return [
        `服务结果：${RESULT_LABEL[draft.result]}`,
        "记一笔服务完成，不动库存",
        "成功后由销售登记客户验收",
    ]
}
