"use client"

import * as React from "react"

import { getErrorMessage } from "@/lib/api/errors"
import { type ResultState } from "@/components/business/feedback"
import { useAppForm } from "@/components/form"
import {
    usePreviewAccessChangeMutation,
    useSubmitAccessChangeMutation,
} from "@/features/access-audit/hooks/queries"
import { changeReasonSchema } from "@/features/access-audit/lib/change-reason-schema"
import { accessChangeResultState } from "@/features/access-audit/pages/lib/outcome-state"
import type {
    AccessChangeCommand,
    AccessChangeOutcome,
    AccessImpactPreview,
} from "@/features/access-audit/types"

type AccessChangeFlowInput = {
    setActionError: React.Dispatch<React.SetStateAction<string | null>>
    setLastResult: React.Dispatch<React.SetStateAction<ResultState>>
}

/**
 * 授权变更预览/提交流程状态：预览弹层、影响结果、复核原因表单与提交。
 * 从页面 hook 拆出，保证页面 hook 只负责列表与筛选。
 */
function useAccessChangeFlow({
    setActionError,
    setLastResult,
}: AccessChangeFlowInput) {
    const [changeOpen, setChangeOpen] = React.useState(false)
    const [pendingCommand, setPendingCommand] =
        React.useState<AccessChangeCommand | null>(null)
    const [impact, setImpact] = React.useState<AccessImpactPreview | null>(null)
    const idempotencyRef = React.useRef<string | null>(null)

    const previewMutation = usePreviewAccessChangeMutation()
    const submitMutation = useSubmitAccessChangeMutation()

    const form = useAppForm({
        defaultValues: {
            reasonCode: "SECURITY_OPS",
            comment: "",
        },
        validators: {
            onChange: changeReasonSchema,
        },
        onSubmit: async () => {
            // 确认在影响预览 Dialog 内提交
        },
    })

    const startChange = React.useCallback(
        async (command: AccessChangeCommand) => {
            setActionError(null)
            setLastResult(null)
            setImpact(null)
            idempotencyRef.current = null
            try {
                const preview = await previewMutation.mutateAsync(command)
                setPendingCommand(command)
                setImpact(preview)
                form.reset()
                setChangeOpen(true)
            } catch (err) {
                setActionError(getErrorMessage(err, "影响预览失败"))
            }
        },
        [previewMutation, form, setActionError, setLastResult],
    )

    const applyOutcome = React.useCallback(
        (outcome: AccessChangeOutcome) => {
            setLastResult(accessChangeResultState(outcome))
        },
        [setLastResult],
    )

    const confirmChange = React.useCallback(async () => {
        if (!pendingCommand || !impact) return
        if (impact.submissionBlocker) {
            applyOutcome({
                outcome: "REJECTED",
                code: impact.submissionBlocker.code,
                message: impact.submissionBlocker.message,
                actionBlockers: [impact.submissionBlocker],
            })
            setChangeOpen(false)
            return
        }

        if (!idempotencyRef.current) {
            idempotencyRef.current = `w19-${pendingCommand.action}-${Date.now()}`
        }
        const values = form.state.values
        const command: AccessChangeCommand = {
            ...pendingCommand,
            reasonCode: values.reasonCode,
            comment: values.comment?.trim() || undefined,
            idempotencyKey: idempotencyRef.current,
        }
        try {
            const outcome = await submitMutation.mutateAsync(command)
            applyOutcome(outcome)
            setChangeOpen(false)
            setPendingCommand(null)
            setImpact(null)
        } catch (err) {
            setActionError(getErrorMessage(err, "提交失败"))
        }
    }, [pendingCommand, impact, form, submitMutation, applyOutcome, setActionError])

    return {
        form,
        changeOpen,
        setChangeOpen,
        pendingCommand,
        setPendingCommand,
        impact,
        setImpact,
        submitMutation,
        startChange,
        applyOutcome,
        confirmChange,
    }
}

export { useAccessChangeFlow }
