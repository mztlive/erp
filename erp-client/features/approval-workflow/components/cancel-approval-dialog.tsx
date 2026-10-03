"use client"

import * as React from "react"

import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"

import { approvalConflictMessage, isApprovalConflict } from "../api"
import { documentIsEditableDraft } from "../api/document-cancel"
import { createApprovalIdempotencyKey } from "../idempotency"
import { useCancelApprovalMutation, useCancelBlockedMutation } from "../queries"
import { reasonFormSchema } from "../schema"
import type { ApprovalCommandView } from "../types"
import { getErrorMessage } from "@/lib/api/errors"
import { classifyFormalCommandError } from "@/lib/formal-command"

/**
 * 撤回审批或取消受阻审批。
 *
 * 撤回走业务单据资源接口；受阻取消走专用端口。两者都不得调用通用任务关闭。
 */
export function CancelApprovalDialog({
    open,
    onOpenChange,
    mode,
    instanceId,
    documentType,
    documentId,
    documentVersion,
    currentRoundNo = 1,
    reviseRejected = false,
    currentNodeName,
    afterStatusLabel,
    expectedInstanceVersion,
    expectedExecutionVersion,
    expectedTaskVersion,
    emergency = false,
    onApplied,
    id = "governance-approval-cancel-dialog",
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    mode: "withdraw" | "cancel-blocked"
    instanceId: string
    documentType?: string
    documentId?: string
    documentVersion?: number
    currentRoundNo?: number
    reviseRejected?: boolean
    currentNodeName?: string
    afterStatusLabel: string
    expectedInstanceVersion: string
    expectedExecutionVersion: string
    expectedTaskVersion?: string
    emergency?: boolean
    onApplied?: (view: ApprovalCommandView) => void
    id?: string
}) {
    const cancelApproval = useCancelApprovalMutation()
    const cancelBlocked = useCancelBlockedMutation(instanceId)
    const pending = cancelApproval.isPending || cancelBlocked.isPending
    const [uncertain, setUncertain] = React.useState(false)
    const frozenWithdraw = React.useRef<
        Parameters<typeof cancelApproval.mutateAsync>[0] | null
    >(null)
    const frozenBlocked = React.useRef<
        Parameters<typeof cancelBlocked.mutateAsync>[0] | null
    >(null)
    const submissionInFlight = React.useRef(false)
    const [idempotencyKey, setIdempotencyKey] = React.useState("")
    const [conflictMessage, setConflictMessage] = React.useState<string | null>(
        null,
    )

    const form = useAppForm({
        defaultValues: { reason: "" },
        validators: {
            onChange: reasonFormSchema,
        },
        onSubmit: async ({ value }) => {
            if (submissionInFlight.current) return
            submissionInFlight.current = true
            const wasUncertain = uncertain
            let checkingUnknown = false
            try {
                const withdraw = frozenWithdraw.current ?? {
                    documentType: documentType ?? "",
                    documentId: documentId ?? "",
                    documentVersion: documentVersion ?? 0,
                    instanceId,
                    currentRoundNo,
                    request: {
                        reason: value.reason,
                        expected_instance_version: expectedInstanceVersion,
                        expected_execution_version: expectedExecutionVersion,
                        expected_task_version: expectedTaskVersion ?? null,
                        idempotency_key: idempotencyKey,
                    },
                }
                if (mode === "withdraw") frozenWithdraw.current = withdraw
                const blocked = frozenBlocked.current ?? {
                    reason: value.reason,
                    expected_instance_version: expectedInstanceVersion,
                    expected_execution_version: expectedExecutionVersion,
                    expected_task_version: expectedTaskVersion ?? null,
                    idempotency_key: idempotencyKey,
                }
                if (mode === "cancel-blocked") frozenBlocked.current = blocked
                if (wasUncertain && mode === "withdraw") {
                    checkingUnknown = true
                    if (
                        await documentIsEditableDraft(
                            withdraw.documentType,
                            withdraw.documentId,
                        )
                    ) {
                        frozenWithdraw.current = null
                        setUncertain(false)
                        onOpenChange(false)
                        onApplied?.({
                            instanceId,
                            currentRoundNo,
                            instanceStatus: "CANCELLED",
                            subjectStatus: "draft",
                            outcome: "APPLIED",
                        })
                        return
                    }
                    checkingUnknown = false
                }
                const view =
                    mode === "withdraw"
                        ? await cancelApproval.mutateAsync(withdraw)
                        : await cancelBlocked.mutateAsync(blocked)
                frozenWithdraw.current = null
                frozenBlocked.current = null
                setUncertain(false)
                onOpenChange(false)
                onApplied?.(view)
            } catch (error) {
                const unknown =
                    wasUncertain ||
                    checkingUnknown ||
                    classifyFormalCommandError(error) === "unknown"
                setUncertain(unknown)
                if (!unknown) {
                    frozenWithdraw.current = null
                    frozenBlocked.current = null
                }
                if (isApprovalConflict(error)) {
                    setConflictMessage(approvalConflictMessage(error))
                    return
                }
                setConflictMessage(
                    unknown
                        ? "处理结果待确认，请使用本次操作重试；确认前不要修改输入或重复撤回。"
                        : getErrorMessage(error, "撤回未完成，请刷新后重试。"),
                )
            } finally {
                submissionInFlight.current = false
            }
        },
    })

    React.useEffect(() => {
        if (!open) return
        frozenWithdraw.current = null
        frozenBlocked.current = null
        setUncertain(false)
        form.reset({
            reason: reviseRejected ? "按驳回意见修改原单后重新提交" : "",
        })
        setIdempotencyKey(
            createApprovalIdempotencyKey(
                mode === "withdraw" ? "cancel" : "cancel-blocked",
                instanceId,
            ),
        )
        setConflictMessage(null)
    }, [form, instanceId, mode, open, reviseRejected])

    const title =
        reviseRejected && mode === "withdraw"
            ? "修改原单"
            : mode === "withdraw"
              ? emergency
                  ? "应急撤回审批"
                  : "撤回审批"
              : "取消受阻审批"

    return (
        <Dialog
            open={open}
            onOpenChange={(next) => {
                if (!submissionInFlight.current && !pending && !uncertain)
                    onOpenChange(next)
            }}
        >
            <DialogContent
                closeButtonId={`${id}-close`}
                showCloseButton={!pending && !uncertain}
            >
                <DialogHeader>
                    <DialogTitle>{title}</DialogTitle>
                    <DialogDescription>
                        当前节点：{currentNodeName ?? "—"}。撤回后单据将回到
                        {afterStatusLabel}。
                        {reviseRejected
                            ? "原单编号与审批记录保留，修改后需重新提交审批。"
                            : null}
                        {mode === "cancel-blocked"
                            ? "此操作不可恢复，不会改派或继续推进。"
                            : null}
                        {emergency
                            ? "你正在代原提交人撤回，系统会记录应急代办身份。"
                            : null}
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="space-y-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <form.AppField
                        name="reason"
                        children={(field) => (
                            <field.TextareaField
                                id={`${id}-reason`}
                                label="原因"
                                required
                                disabled={pending || uncertain}
                            />
                        )}
                    />
                    {conflictMessage ? (
                        <p className="text-sm text-destructive">
                            {conflictMessage}
                        </p>
                    ) : null}
                    <DialogFooter>
                        <Button
                            id={`${id}-cancel`}
                            type="button"
                            variant="outline"
                            disabled={pending || uncertain}
                            onClick={() => onOpenChange(false)}
                        >
                            取消
                        </Button>
                        <form.AppForm>
                            <form.SubmitButton
                                loading={pending}
                                id={`${id}-submit`}
                                label={
                                    uncertain
                                        ? "核对撤回结果"
                                        : reviseRejected && mode === "withdraw"
                                          ? "撤回并修改原单"
                                          : mode === "withdraw"
                                            ? "确认撤回"
                                            : "确认取消"
                                }
                                disabled={pending}
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
