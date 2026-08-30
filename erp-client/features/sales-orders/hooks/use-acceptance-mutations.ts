"use client"

import * as React from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"

import {
    postCustomerAcceptanceWorkspace,
    reverseCustomerAcceptanceWorkspace,
} from "@/features/sales-orders/api/acceptance"
import { salesOrderKeys } from "@/features/sales-orders/hooks/queries"
import { workItemKeys } from "@/features/work-items/queries"
import {
    OVERALL_RESULT_LABEL,
    type AcceptanceOverallResult,
} from "@/features/sales-orders/lib/acceptance-types"
import type { FormalResultState } from "@/features/sales-orders/lib/acceptance-model"
import { resultText } from "@/lib/ui-text"

/**
 * 验收登记 / 冲正与结果反馈。成功后失效验收与销售单详情缓存。
 */
export function useAcceptanceMutations({
    salesOrderId,
    idempotencyKey,
    submittedOverallRef,
    onPostSucceeded,
    onReverseSucceeded,
}: {
    salesOrderId: string
    idempotencyKey: string
    submittedOverallRef: React.RefObject<AcceptanceOverallResult>
    onPostSucceeded: (payload: {
        remainingEligibleCount: number
        acceptanceNo: string
    }) => void
    onReverseSucceeded: () => void
}) {
    const queryClient = useQueryClient()
    const [formalResult, setFormalResult] =
        React.useState<FormalResultState | null>(null)

    const postMutation = useMutation({
        meta: { suppressErrorToast: true },
        mutationFn: postCustomerAcceptanceWorkspace,
        onSuccess: async (result) => {
            if (result.status === "succeeded") {
                const overall = submittedOverallRef.current
                const remaining =
                    result.remainingEligibleCount === 0
                        ? "本单待验已清零。"
                        : result.remainingEligibleQuantityLabel
                          ? `本单还有 ${result.remainingEligibleQuantityLabel} 待验。`
                          : `本单还有 ${result.remainingEligibleCount} 批待验。`
                const exceptionHint =
                    overall === "PASS"
                        ? remaining
                        : `${remaining} ${result.factOnlyNotice}`
                setFormalResult({
                    kind: "post",
                    status: "succeeded",
                    title: "客户验收已登记",
                    description: exceptionHint,
                    reference: result.acceptanceNo,
                    remainingEligibleCount: result.remainingEligibleCount,
                    hasException: overall !== "PASS",
                    facts: [
                        {
                            label: "本次结果",
                            value: OVERALL_RESULT_LABEL[overall],
                        },
                        {
                            label: "待验",
                            value:
                                result.remainingEligibleCount === 0
                                    ? "已清零"
                                    : result.remainingEligibleQuantityLabel ||
                                      `${result.remainingEligibleCount} 批`,
                        },
                    ],
                })
                await queryClient.invalidateQueries({
                    queryKey: salesOrderKeys.acceptanceRoot(salesOrderId),
                })
                await queryClient.invalidateQueries({
                    queryKey: salesOrderKeys.detail(salesOrderId),
                })
                await queryClient.invalidateQueries({
                    queryKey: workItemKeys.all,
                })
                onPostSucceeded({
                    remainingEligibleCount: result.remainingEligibleCount,
                    acceptanceNo: result.acceptanceNo,
                })
                return
            } else if (result.status === "unknown") {
                setFormalResult({
                    kind: "post",
                    status: "unknown",
                    title: resultText.unknown,
                    description: `${result.message} 未确认前不按成功处理；请用原任务号查询后再决定是否重试。`,
                    facts: [
                        {
                            label: resultText.originalTaskNo,
                            value: result.idempotencyKey,
                        },
                    ],
                })
            } else {
                setFormalResult({
                    kind: "post",
                    status: "failed",
                    title: "验收登记失败",
                    description: result.message,
                    facts: [],
                })
            }
            await queryClient.invalidateQueries({
                queryKey: salesOrderKeys.acceptanceRoot(salesOrderId),
            })
            await queryClient.invalidateQueries({
                queryKey: salesOrderKeys.detail(salesOrderId),
            })
            await queryClient.invalidateQueries({ queryKey: workItemKeys.all })
        },
        onError: () => {
            setFormalResult({
                kind: "post",
                status: "unknown",
                title: resultText.unknown,
                description:
                    "请求超时或网络中断，结果未确认；请查询最终结果或重试，避免重复登记。",
                facts: [
                    { label: resultText.originalTaskNo, value: idempotencyKey },
                ],
            })
        },
    })

    const reverseMutation = useMutation({
        mutationFn: reverseCustomerAcceptanceWorkspace,
        onSuccess: async (result) => {
            if (result.status === "succeeded") {
                setFormalResult({
                    kind: "reverse",
                    status: "succeeded",
                    title: "误录验收已冲正",
                    description: `已新增反向验收记录；原验收 ${result.originalAcceptanceNo} 保留可追溯。`,
                    reference: result.reverseAcceptanceNo,
                    facts: [
                        {
                            label: "原验收单号",
                            value: result.originalAcceptanceNo,
                        },
                        {
                            label: "冲正单号",
                            value: result.reverseAcceptanceNo,
                        },
                    ],
                })
                onReverseSucceeded()
            } else {
                setFormalResult({
                    kind: "reverse",
                    status: "failed",
                    title: "冲正失败",
                    description: result.message,
                    facts: [],
                })
            }
            await queryClient.invalidateQueries({
                queryKey: salesOrderKeys.acceptanceRoot(salesOrderId),
            })
            await queryClient.invalidateQueries({
                queryKey: salesOrderKeys.detail(salesOrderId),
            })
            await queryClient.invalidateQueries({ queryKey: workItemKeys.all })
        },
    })

    return {
        postMutation,
        reverseMutation,
        formalResult,
        setFormalResult,
    }
}
