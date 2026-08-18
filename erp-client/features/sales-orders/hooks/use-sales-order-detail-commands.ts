"use client"

import {
    prepareProcurementRejectionResolution,
    prepareStartSalesChangeOrder,
    type ResolveProcurementRejectionPayload,
    type SalesOrderDetailView,
    type StartSalesChangeOrderPayload,
} from "@/features/sales-orders/api/sales-orders"
import {
    useResolveProcurementRejectionMutation,
    useStartSalesChangeOrderMutation,
} from "@/features/sales-orders/hooks/queries"
import type { SalesOrderDetailActionResult } from "@/features/sales-orders/lib/sales-order-detail-model"
import { getErrorPresentation } from "@/lib/api/errors"
import {
    classifyFormalCommandError,
    type FormalCommandKeyLedger,
} from "@/lib/formal-command"

const parseEvidenceReferenceIds = (value: string): string[] => [
    ...new Set(
        value
            .split(/[\s,，;；]+/)
            .map((item) => item.trim())
            .filter(Boolean),
    ),
]

/**
 * 采购驳回的两种处置（作废 / 申请低毛利承接）共用同一账本槽位与同一 mutation，
 * 保证结果未知时只能用原操作重试。
 */
export function useSalesOrderDetailRejectionResolution() {
    const mutation = useResolveProcurementRejectionMutation()

    const voidAfterRejection = async ({
        order,
        commandLedger,
        onResult,
        reason,
    }: {
        order: SalesOrderDetailView
        commandLedger: FormalCommandKeyLedger
        onResult: (next: SalesOrderDetailActionResult) => void
        reason: string
    }) => {
        let command = commandLedger.peek<ResolveProcurementRejectionPayload>(
            "procurement-rejection-resolution",
        )
        if (command && command.payload.action !== "VOID_AFTER_REJECTION") {
            onResult({
                status: "unknown",
                title: "处理结果待确认",
                description: "另一项处理的结果仍待确认，请先使用原操作重试。",
                reference: order.documentNumber,
            })
            throw new Error("另一项处理的结果仍待确认，请先使用原操作重试。")
        }
        try {
            if (!command) {
                const payload = await prepareProcurementRejectionResolution({
                    salesOrderId: order.id,
                    action: "VOID_AFTER_REJECTION",
                    voidReasonCode: "SALES_DECISION_NOT_TO_PROCEED",
                    comment: reason,
                })
                command = commandLedger.acquire(
                    "procurement-rejection-resolution",
                    `sales:${order.id}:procurement-rejection:void`,
                    payload,
                )
            }
            if (!command) return
            const outcome = await mutation.mutateAsync({
                ...command.payload,
                idempotencyKey: command.idempotencyKey,
            })
            commandLedger.settle(
                "procurement-rejection-resolution",
                "succeeded",
            )
            onResult({
                status: "rejected",
                title: "本单已作废",
                description: outcome.detail,
                reference: outcome.reference,
            })
        } catch (error) {
            const settlement = command
                ? classifyFormalCommandError(error)
                : "failed"
            commandLedger.settle("procurement-rejection-resolution", settlement)
            const failure = getErrorPresentation(
                error,
                "作废未完成，请刷新后重试。",
            )
            onResult({
                status: settlement === "unknown" ? "unknown" : "blocked",
                title:
                    settlement === "unknown" ? "处理结果待确认" : failure.title,
                description:
                    settlement === "unknown"
                        ? "当前原因已保留，请使用本次操作重试。"
                        : failure.description,
                reference: order.documentNumber,
            })
            throw error
        }
    }

    const requestLowMargin = async ({
        order,
        commandLedger,
        onResult,
        reason,
        evidence,
    }: {
        order: SalesOrderDetailView
        commandLedger: FormalCommandKeyLedger
        onResult: (next: SalesOrderDetailActionResult) => void
        reason: string
        evidence: string
    }) => {
        const evidenceReferenceIds = parseEvidenceReferenceIds(evidence)
        if (!reason.trim()) throw new Error("请填写低毛利承接理由")
        if (evidenceReferenceIds.length === 0)
            throw new Error("请至少填写一项已登记证据 ID")
        let command = commandLedger.peek<ResolveProcurementRejectionPayload>(
            "procurement-rejection-resolution",
        )
        if (
            command &&
            command.payload.action !== "REQUEST_LOW_MARGIN_ACCEPTANCE"
        ) {
            onResult({
                status: "unknown",
                title: "处理结果待确认",
                description: "另一项处理的结果仍待确认，请先使用原操作重试。",
                reference: order.documentNumber,
            })
            throw new Error("另一项处理的结果仍待确认，请先使用原操作重试。")
        }
        try {
            if (!command) {
                const payload = await prepareProcurementRejectionResolution({
                    salesOrderId: order.id,
                    action: "REQUEST_LOW_MARGIN_ACCEPTANCE",
                    lowMarginAcceptanceReason: reason.trim(),
                    evidenceReferenceIds,
                })
                command = commandLedger.acquire(
                    "procurement-rejection-resolution",
                    `sales:${order.id}:procurement-rejection:low-margin`,
                    payload,
                )
            }
            if (!command) return
            const outcome = await mutation.mutateAsync({
                ...command.payload,
                idempotencyKey: command.idempotencyKey,
            })
            commandLedger.settle(
                "procurement-rejection-resolution",
                "succeeded",
            )
            onResult({
                status: "succeeded",
                title: "已申请低毛利承接",
                description: outcome.detail,
                reference: outcome.reference,
                nextResponsible: "销售上级",
            })
        } catch (error) {
            const settlement = command
                ? classifyFormalCommandError(error)
                : "failed"
            commandLedger.settle("procurement-rejection-resolution", settlement)
            const failure = getErrorPresentation(
                error,
                "承接申请未提交，请刷新后重试。",
            )
            onResult({
                status: settlement === "unknown" ? "unknown" : "blocked",
                title:
                    settlement === "unknown" ? "处理结果待确认" : failure.title,
                description:
                    settlement === "unknown"
                        ? "当前输入已保留，请使用本次操作重试。"
                        : failure.description,
                reference: order.documentNumber,
            })
            throw error
        }
    }

    return {
        voidAfterRejection,
        requestLowMargin,
        isPending: mutation.isPending,
    }
}

export function useSalesOrderDetailStartChange() {
    const mutation = useStartSalesChangeOrderMutation()

    const startChange = async ({
        order,
        commandLedger,
        onResult,
    }: {
        order: SalesOrderDetailView
        commandLedger: FormalCommandKeyLedger
        onResult: (next: SalesOrderDetailActionResult) => void
    }) => {
        const isCard = order.nature === "card_voucher"
        let command =
            commandLedger.peek<StartSalesChangeOrderPayload>("start-change")
        try {
            if (!command) {
                const payload = await prepareStartSalesChangeOrder({
                    salesOrderId: order.id,
                    baseRevisionNo: order.version,
                    nature: order.nature,
                })
                command = commandLedger.acquire(
                    "start-change",
                    `sales:${order.id}:change`,
                    payload,
                )
            }
            if (!command) return
            const change = await mutation.mutateAsync({
                ...command.payload,
                idempotencyKey: command.idempotencyKey,
            })
            commandLedger.settle("start-change", "succeeded")
            onResult({
                status: "succeeded",
                title: "改单已创建",
                description: `已进入「${change.statusLabel}」。当前版本对客户仍然有效。`,
                reference: change.id,
                nextResponsible: isCard ? "运营与财务" : "采购与财务",
            })
        } catch (error) {
            const settlement = command
                ? classifyFormalCommandError(error)
                : "failed"
            commandLedger.settle("start-change", settlement)
            const failure = getErrorPresentation(
                error,
                "改单未创建，请刷新后重试。",
            )
            onResult({
                status: settlement === "unknown" ? "unknown" : "blocked",
                title:
                    settlement === "unknown" ? "处理结果待确认" : failure.title,
                description:
                    settlement === "unknown"
                        ? "请使用本次操作重试；确认前不要重复创建改单。"
                        : failure.description,
                reference: order.documentNumber,
            })
            throw error
        }
    }

    return {
        startChange,
        isPending: mutation.isPending,
    }
}
