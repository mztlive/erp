"use client"

import * as React from "react"
import type { UseMutationResult } from "@tanstack/react-query"

import { getErrorMessage } from "@/lib/api/errors"
import type {
    CompleteSupplierOrderTaskInput,
    CompleteSupplierOrderTaskResult,
    FormalActionResponse,
    SupplierOrderDetailView,
} from "@/features/supplier-orders/types"
import type { CommandIdentity } from "@/features/supplier-orders/hooks/use-supplier-order-center-identity"
import type { SupplierOrderCenterResult } from "@/features/supplier-orders/hooks/use-supplier-order-center-actions"

type SupplierOrderCenterTaskActionsInput = {
    detail: SupplierOrderDetailView | undefined
    completionEvidence?: NonNullable<
        SupplierOrderDetailView["lastInvestigation"]
    >
    refetch: () => Promise<unknown>
    setResult: React.Dispatch<
        React.SetStateAction<SupplierOrderCenterResult | null>
    >
    completeTaskMutation: UseMutationResult<
        FormalActionResponse<CompleteSupplierOrderTaskResult>,
        Error,
        CompleteSupplierOrderTaskInput
    >
    commandIdentity: (kind: string, objectId: string) => CommandIdentity
    forgetCommandIdentity: (key: string) => void
}

/** 任务级命令：依据已核实的终态结果确认完成。 */
export function useSupplierOrderCenterTaskActions(
    input: SupplierOrderCenterTaskActionsInput,
) {
    const {
        detail,
        completionEvidence,
        refetch,
        setResult,
        completeTaskMutation,
        commandIdentity,
        forgetCommandIdentity,
    } = input

    const [completeOpen, setCompleteOpen] = React.useState(false)

    async function handleCompleteTask() {
        const workItem = detail?.workItem
        const evidence = detail?.lastInvestigation ?? completionEvidence
        if (
            !detail ||
            !workItem ||
            evidence?.outcome !== "VERIFIED_TERMINAL" ||
            !evidence.verifiedSupplierActionResultId ||
            !evidence.verifiedResolution
        ) {
            setCompleteOpen(false)
            setResult({
                status: "blocked",
                title: "尚不能完成任务",
                description:
                    "可验证处理结果、供应商动作记录或固定结论尚未齐备，任务保持待处理。",
            })
            return
        }
        const identity = commandIdentity("complete", workItem.workItemId)
        try {
            const response = await completeTaskMutation.mutateAsync({
                workItemId: workItem.workItemId,
                expectedTaskVersion: workItem.taskVersion,
                expectedSubjectVersion: workItem.subjectVersion,
                decision: {
                    type: "CONFIRM_VERIFIED_TERMINAL_RESULT",
                    orderId: detail.order.id,
                    expectedOrderLockVersion: detail.order.lockVersion,
                    verifiedSupplierActionResultId:
                        evidence.verifiedSupplierActionResultId,
                    resolution: evidence.verifiedResolution,
                },
                idempotencyKey: identity.idempotencyKey,
            })
            if (response.status !== "unknown") {
                forgetCommandIdentity(identity.key)
            }
            setCompleteOpen(false)
            await refetch()
            setResult({
                status:
                    response.status === "succeeded"
                        ? "succeeded"
                        : response.status === "blocked"
                          ? "blocked"
                          : response.status === "unknown"
                            ? "unknown"
                            : "rejected",
                title:
                    response.status === "succeeded"
                        ? "任务已完成"
                        : "任务未完成",
                description: response.message,
                reference: response.reference,
            })
        } catch (error) {
            setResult({
                status: "rejected",
                title: "任务完成未提交",
                description: getErrorMessage(error, "提交失败，请刷新后重试"),
            })
        }
    }

    return {
        completeOpen,
        setCompleteOpen,
        handleCompleteTask,
    }
}
