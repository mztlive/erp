"use client"

import * as React from "react"

import type { ResultState } from "@/components/business/feedback"
import { useAppForm } from "@/components/form"
import { getErrorMessage } from "@/lib/api/errors"
import { resultText } from "@/lib/ui-text"
import {
    useCreateAdjustmentDraftMutation,
    useResolveAdjustmentUnknownMutation,
    useSubmitAdjustmentMutation,
} from "@/features/inventory/hooks/queries"
import {
    adjustSchema,
    localNowInput,
} from "@/features/inventory/lib/presentation"
import { REASON_TYPE_OPTIONS } from "@/features/inventory/types"
import type {
    AdjustmentReasonType,
    StockAdjustmentApprovalView,
    StockAdjustmentSubmitCommand,
    StockBalanceRow,
} from "@/features/inventory/types"

export type AdjustmentMeta = {
    stockAdjustmentId: string
    lineId: string
    balanceId: string
    warehouseName: string
    skuCode: string
    skuName: string
    baseUnit: string
    onHand: string
    available: string
    adjustmentNo: string
    segregationNote: string
    approval?: StockAdjustmentApprovalView
}

export type AdjustmentPendingPayload = {
    stockAdjustmentId: string
    submitCommand: StockAdjustmentSubmitCommand
    lineId: string
    balanceId: string
    expectedBalanceLockVersion: string
    reasonType: AdjustmentReasonType
    reasonTypeLabel: string
    direction: "increase" | "decrease"
    quantity: string
    note: string
    occurredAt: string
    idempotencyKey: string
    forceUnknown?: boolean
}

/**
 * 只拼接服务端已给出的当前节点/审批人，缺失字段省略。
 */
export const formatSubmittedResult = (
    outcome: {
        adjustmentNo: string
        currentNodeLabel?: string
        nextResponsible?: string
    },
    suffix?: string,
): string => {
    const parts = [`单号 ${outcome.adjustmentNo}`]
    if (outcome.currentNodeLabel) {
        parts.push(`当前节点：${outcome.currentNodeLabel}`)
    }
    if (outcome.nextResponsible) {
        parts.push(`当前审批人：${outcome.nextResponsible}`)
    }
    if (suffix) parts.push(suffix.replace(/。$/, ""))
    return `${parts.join("。")}。`
}

export interface AdjustmentWorkflowInput {
    isPhoneNarrow: boolean
    /** 发起前记录焦点行（详情/调整关闭后恢复）。 */
    onFocusRestore: (balanceId: string) => void
    /** 创建草稿成功后关闭余额预览。 */
    onPreviewClose: () => void
}

export function useAdjustmentWorkflow({
    isPhoneNarrow,
    onFocusRestore,
    onPreviewClose,
}: AdjustmentWorkflowInput) {
    const createDraftMutation = useCreateAdjustmentDraftMutation()
    const submitMutation = useSubmitAdjustmentMutation()
    const resolveUnknownMutation = useResolveAdjustmentUnknownMutation()

    const [, setAdjustBalanceId] = React.useState<string | null>(null)
    const [adjustDraftId, setAdjustDraftId] = React.useState<string | null>(
        null,
    )
    const [adjustLockVersion, setAdjustLockVersion] = React.useState("")
    const [adjustMeta, setAdjustMeta] = React.useState<AdjustmentMeta | null>(
        null,
    )
    const [confirmOpen, setConfirmOpen] = React.useState(false)
    const [lastResult, setLastResult] = React.useState<ResultState>(null)
    const [actionError, setActionError] = React.useState<string | null>(null)
    const [pendingPayload, setPendingPayload] =
        React.useState<AdjustmentPendingPayload | null>(null)

    const idempotencyRef = React.useRef<string | null>(null)

    const form = useAppForm({
        defaultValues: {
            reasonType: "COUNT_LOSS" as AdjustmentReasonType,
            quantity: "",
            note: "",
            occurredAt: localNowInput(),
        },
        validators: {
            onChange: adjustSchema,
        },
        onSubmit: async () => {
            setConfirmOpen(true)
        },
    })

    const closeAdjustment = React.useCallback(() => {
        setAdjustDraftId(null)
        setAdjustBalanceId(null)
        setAdjustMeta(null)
        setConfirmOpen(false)
        setPendingPayload(null)
    }, [])

    const startAdjustment = React.useCallback(
        async (row: StockBalanceRow) => {
            if (isPhoneNarrow) {
                setActionError(
                    "窄屏（移动端）仅支持只读查询；库存调整请在桌面完成。",
                )
                return
            }
            if (!row.allowedActions.includes("CREATE_ADJUSTMENT")) {
                setActionError(
                    row.actionBlockers.find(
                        (b) => b.action === "CREATE_ADJUSTMENT",
                    )?.message ?? "当前不允许发起库存调整",
                )
                return
            }
            onFocusRestore(row.balanceId)
            setActionError(null)
            setLastResult(null)
            idempotencyRef.current = null
            try {
                const draft = await createDraftMutation.mutateAsync({
                    balanceId: row.balanceId,
                    balanceLockVersion: row.lockVersion,
                    warehouseId: row.warehouseId,
                    warehouseName: row.warehouseName,
                    skuId: row.skuId,
                    skuCode: row.skuCode,
                    skuName: row.skuName,
                    baseUnit: row.baseUnit,
                })
                setAdjustBalanceId(row.balanceId)
                setAdjustDraftId(draft.stockAdjustmentId)
                setAdjustLockVersion(draft.balanceLockVersion)
                setAdjustMeta({
                    stockAdjustmentId: draft.stockAdjustmentId,
                    lineId: draft.lineId,
                    balanceId: draft.balanceId,
                    warehouseName: draft.warehouseName,
                    skuCode: draft.skuCode,
                    skuName: draft.skuName,
                    baseUnit: draft.baseUnit,
                    onHand: row.onHandQuantity,
                    available: row.availableQuantity,
                    adjustmentNo: draft.adjustmentNo,
                    segregationNote: draft.segregationNote,
                    approval: draft.approval,
                })
                form.reset()
                form.setFieldValue("reasonType", draft.reasonType)
                form.setFieldValue("quantity", draft.quantity)
                form.setFieldValue("note", draft.note)
                form.setFieldValue(
                    "occurredAt",
                    draft.occurredAt.slice(0, 16) || localNowInput(),
                )
                onPreviewClose()
            } catch (err) {
                setActionError(
                    getErrorMessage(err, "创建调整草稿失败，请稍后重试"),
                )
            }
        },
        [
            createDraftMutation,
            form,
            isPhoneNarrow,
            onFocusRestore,
            onPreviewClose,
        ],
    )

    const doSubmit = React.useCallback(async () => {
        if (!adjustDraftId || !adjustMeta) return
        const submitCommand = adjustMeta.approval?.submitCommand
        if (
            !submitCommand ||
            !adjustMeta.approval?.allowedActions.includes("SUBMIT")
        ) {
            setActionError("当前调整单不能提交，请关闭后重新发起。")
            setConfirmOpen(false)
            return
        }
        const values = form.state.values
        const reason =
            REASON_TYPE_OPTIONS.find((r) => r.value === values.reasonType) ??
            REASON_TYPE_OPTIONS[1]
        if (!idempotencyRef.current) {
            idempotencyRef.current = `w10-adj-${adjustDraftId}-${Date.now()}`
        }
        const payload: AdjustmentPendingPayload = {
            stockAdjustmentId: adjustDraftId,
            submitCommand,
            lineId: adjustMeta.lineId,
            balanceId: adjustMeta.balanceId,
            expectedBalanceLockVersion: adjustLockVersion,
            reasonType: values.reasonType,
            reasonTypeLabel: reason.label,
            direction: reason.direction,
            quantity: values.quantity.trim(),
            note: values.note.trim(),
            occurredAt: values.occurredAt,
            idempotencyKey: idempotencyRef.current,
        }
        setPendingPayload(payload)
        setActionError(null)
        const result = await submitMutation.mutateAsync(payload)
        if (result.status === "succeeded") {
            setLastResult({
                status: "succeeded",
                title: "调整已提交审批",
                description: formatSubmittedResult(
                    result.outcome,
                    "余额尚未变化，审批通过后由系统更新。",
                ),
                reference: result.outcome.reference,
            })
            setConfirmOpen(false)
            closeAdjustment()
            return
        }
        if (result.status === "unknown") {
            setLastResult({
                status: "unknown",
                title: resultText.unknown,
                description: result.message,
                reference: result.idempotencyKey,
                pendingIdempotencyKey: result.idempotencyKey,
            })
            setConfirmOpen(false)
            return
        }
        if (
            result.code === "VERSION_CONFLICT" &&
            result.latestLockVersion != null
        ) {
            setAdjustLockVersion(result.latestLockVersion)
            setActionError(result.message)
            setConfirmOpen(false)
            return
        }
        setActionError(result.message)
        setConfirmOpen(false)
    }, [
        adjustDraftId,
        adjustMeta,
        adjustLockVersion,
        form.state.values,
        submitMutation,
        closeAdjustment,
    ])

    const resolveLastUnknown = React.useCallback(async () => {
        if (!lastResult?.pendingIdempotencyKey || !pendingPayload) return
        const r = await resolveUnknownMutation.mutateAsync({
            idempotencyKey: lastResult.pendingIdempotencyKey,
            stockAdjustmentId: pendingPayload.stockAdjustmentId,
            expectedSubjectVersion:
                pendingPayload.submitCommand.expectedSubjectVersion,
            expectedBalanceLockVersion:
                pendingPayload.expectedBalanceLockVersion,
        })
        if (r.status === "succeeded") {
            setLastResult({
                status: "succeeded",
                title: "调整已提交审批",
                description: formatSubmittedResult(r.outcome),
                reference: r.outcome.reference,
            })
            closeAdjustment()
        } else if (r.status === "unknown") {
            setLastResult({
                status: "unknown",
                title: "仍在查询最终结果",
                description: r.message,
                reference: r.idempotencyKey,
                pendingIdempotencyKey: r.idempotencyKey,
            })
        } else {
            setActionError(r.message)
        }
    }, [
        lastResult?.pendingIdempotencyKey,
        pendingPayload,
        resolveUnknownMutation,
        closeAdjustment,
    ])

    return {
        form,
        adjustDraftId,
        adjustMeta,
        confirmOpen,
        setConfirmOpen,
        lastResult,
        actionError,
        pendingPayload,
        isCreating: createDraftMutation.isPending,
        isSubmitting: submitMutation.isPending,
        isResolving: resolveUnknownMutation.isPending,
        startAdjustment,
        closeAdjustment,
        doSubmit,
        resolveLastUnknown,
    }
}

export type AdjustmentFormApi = ReturnType<typeof useAdjustmentWorkflow>["form"]
