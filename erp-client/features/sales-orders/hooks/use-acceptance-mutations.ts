"use client"

import * as React from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"

import {
    postCustomerAcceptanceWorkspace,
    reverseCustomerAcceptanceWorkspace,
    saveCustomerAcceptanceDraft,
} from "@/features/sales-orders/api/acceptance"
import { salesOrderKeys } from "@/features/sales-orders/hooks/queries"
import {
    OVERALL_RESULT_LABEL,
    type AcceptanceOverallResult,
} from "@/features/sales-orders/lib/acceptance-types"
import type { FormalResultState } from "@/features/sales-orders/lib/acceptance-model"
import { resultText } from "@/lib/ui-text"

/**
 * 验收工作台的三个变更（保存草稿 / 登记验收 / 冲正）与结果反馈状态。
 * 成功后失效验收与销售单详情缓存；结果文案与拆分前完全一致。
 */
export function useAcceptanceMutations({
    salesOrderId,
    idempotencyKey,
    submittedOverallRef,
    setDraftSavedAt,
    onPostSucceeded,
    onReverseSucceeded,
}: {
    salesOrderId: string
    idempotencyKey: string
    /** 提交瞬间的总体结果快照（含服务不通过），用于结果反馈不被服务端降级。 */
    submittedOverallRef: React.RefObject<AcceptanceOverallResult>
    setDraftSavedAt: (updatedAt: string | null) => void
    /** 登记成功后的状态复位（清空来源/行结果、重置表单与幂等键等）。 */
    onPostSucceeded: () => void
    /** 冲正成功后关闭冲正对话框并清空理由。 */
    onReverseSucceeded: () => void
}) {
    const queryClient = useQueryClient()
    const [formalResult, setFormalResult] =
        React.useState<FormalResultState | null>(null)

    const saveDraftMutation = useMutation({
        mutationFn: saveCustomerAcceptanceDraft,
        onSuccess: async (draft) => {
            setDraftSavedAt(draft.updatedAt)
            await queryClient.invalidateQueries({
                queryKey: salesOrderKeys.acceptanceRoot(salesOrderId),
            })
        },
    })

    const postMutation = useMutation({
        mutationFn: postCustomerAcceptanceWorkspace,
        onSuccess: async (result) => {
            if (result.status === "succeeded") {
                const overall = submittedOverallRef.current
                setFormalResult({
                    kind: "post",
                    status: "succeeded",
                    title: "客户验收已登记",
                    description: `${result.factOnlyNotice} 还剩待验收 ${result.remainingEligibleCount} 批 · 约 ${result.remainingEligibleQuantityLabel}。`,
                    reference: result.acceptanceNo,
                    facts: [
                        {
                            label: "总体结果",
                            value: OVERALL_RESULT_LABEL[overall],
                        },
                        {
                            label: "剩余待验收",
                            value: `${result.remainingEligibleCount} 批`,
                        },
                        {
                            label: "履约轨",
                            value:
                                result.remainingEligibleCount === 0
                                    ? "待验收已清零"
                                    : result.remainingEligibleQuantityLabel
                                      ? `仍待验收 ${result.remainingEligibleCount} 批 · ${result.remainingEligibleQuantityLabel}`
                                      : `仍待验收 ${result.remainingEligibleCount} 批`,
                        },
                        {
                            label: "下一步",
                            value:
                                overall === "PASS"
                                    ? "系统按履约进度自动判断结案"
                                    : "销售协同处理验收异常",
                        },
                    ],
                })
                onPostSucceeded()
            } else if (result.status === "unknown") {
                setFormalResult({
                    kind: "post",
                    status: "unknown",
                    title: resultText.unknown,
                    description: `${result.message} 未确认前不关闭草稿、不按成功处理；可用原提交编号查询。`,
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
        },
    })

    return {
        saveDraftMutation,
        postMutation,
        reverseMutation,
        formalResult,
        setFormalResult,
    }
}
