"use client"

import {
    type SalesOrderDetailView,
    type StartSalesChangeOrderPayload,
} from "@/features/sales-orders/api/sales-orders"
import { useStartSalesChangeOrderMutation } from "@/features/sales-orders/hooks/queries"
import type { SalesOrderDetailActionResult } from "@/features/sales-orders/lib/sales-order-detail-model"
import { getErrorPresentation } from "@/lib/api/errors"
import {
    classifyFormalCommandError,
    type FormalCommandKeyLedger,
} from "@/lib/formal-command"

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
        let command =
            commandLedger.peek<StartSalesChangeOrderPayload>("start-change")
        try {
            if (!command) {
                const payload: StartSalesChangeOrderPayload = {
                    salesOrderId: order.id,
                    baseRevisionNo: order.currentRevisionNo ?? 0,
                    nature: order.nature,
                }
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
                nextResponsible:
                    change.approval?.instance?.currentAssigneeName ??
                    change.approval?.instance?.currentAssignee,
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
