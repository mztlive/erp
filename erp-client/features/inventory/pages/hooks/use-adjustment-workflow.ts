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
import type { DocumentApprovalView } from "@/features/approval-workflow/types"
import type {
    AdjustmentReasonType,
    StockBalanceRow,
} from "@/features/inventory/types"

export type AdjustmentMeta = {
    stockAdjustmentId: string
    warehouseName: string
    skuCode: string
    skuName: string
    baseUnit: string
    onHand: string
    available: string
    adjustmentNo: string
    editVersion: number
    segregationNote: string
    approval?: DocumentApprovalView
}

export type AdjustmentPendingPayload = {
    stockAdjustmentId: string
    expectedBalanceLockVersion: number
    seedBalanceLockVersion: number
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
    const [adjustLockVersion, setAdjustLockVersion] = React.useState<number>(0)
    const [adjustSeedLock, setAdjustSeedLock] = React.useState<number>(0)
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
                })
                setAdjustBalanceId(row.balanceId)
                setAdjustDraftId(draft.stockAdjustmentId)
                setAdjustLockVersion(draft.balanceLockVersion)
                setAdjustSeedLock(row.lockVersion)
                setAdjustMeta({
                    stockAdjustmentId: draft.stockAdjustmentId,
                    warehouseName: draft.warehouseName,
                    skuCode: draft.skuCode,
                    skuName: draft.skuName,
                    baseUnit: draft.baseUnit,
                    onHand: row.onHandQuantity,
                    available: row.availableQuantity,
                    adjustmentNo: draft.adjustmentNo,
                    editVersion: draft.editVersion,
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
                setActionError(getErrorMessage(err, "创建调整草稿失败"))
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
        const values = form.state.values
        const reason =
            REASON_TYPE_OPTIONS.find((r) => r.value === values.reasonType) ??
            REASON_TYPE_OPTIONS[1]
        if (!idempotencyRef.current) {
            idempotencyRef.current = `w10-adj-${adjustDraftId}-${Date.now()}`
        }
        const payload: AdjustmentPendingPayload = {
            stockAdjustmentId: adjustDraftId,
            expectedBalanceLockVersion: adjustLockVersion,
            seedBalanceLockVersion: adjustSeedLock,
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
        adjustSeedLock,
        form.state.values,
        submitMutation,
        closeAdjustment,
    ])

    const resolveLastUnknown = React.useCallback(async () => {
        if (!lastResult?.pendingIdempotencyKey) return
        const r = await resolveUnknownMutation.mutateAsync({
            idempotencyKey: lastResult.pendingIdempotencyKey,
            stockAdjustmentId: pendingPayload?.stockAdjustmentId,
            expectedBalanceLockVersion:
                pendingPayload?.expectedBalanceLockVersion,
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
        isSubmitting: submitMutation.isPending,
        isResolving: resolveUnknownMutation.isPending,
        startAdjustment,
        closeAdjustment,
        doSubmit,
        resolveLastUnknown,
    }
}

export type AdjustmentFormApi = ReturnType<typeof useAdjustmentWorkflow>["form"]
