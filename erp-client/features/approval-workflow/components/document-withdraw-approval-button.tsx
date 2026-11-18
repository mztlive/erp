"use client"

import * as React from "react"
import { useRouter } from "@/lib/navigation"

import { RequiredMark } from "@/components/ui/required-mark"
import {
    AlertDialog,
    AlertDialogAction,
    AlertDialogCancel,
    AlertDialogContent,
    AlertDialogDescription,
    AlertDialogFooter,
    AlertDialogHeader,
    AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Button } from "@/components/ui/button"
import { Textarea } from "@/components/ui/textarea"
import { documentIsEditableDraft } from "@/features/approval-workflow/api/document-cancel"
import { getErrorPresentation } from "@/lib/api/errors"
import { classifyFormalCommandError } from "@/lib/formal-command"

type WithdrawGate = {
    enabled: boolean
    reason?: string
}

type WithdrawResult = {
    status: "succeeded" | "unknown" | "blocked"
    title: string
    description: string
    reference?: string
}

/**
 * 销售单与采购单页头「撤回审批」。
 *
 * 可见性、权限闸门、命令内容和稳定 id 由调用方提供。
 * 未知结果沿用同一次命令重试；确认单据已回到草稿时不再提交。
 */
export function DocumentWithdrawApprovalButton<TCommand>({
    visible,
    permitted,
    pending,
    gate,
    reviseRejected,
    documentNoun,
    ids,
    createIdempotencyKey,
    buildCommand,
    documentType,
    documentIdOf,
    submit,
    successHref,
    reference,
    onResult,
}: {
    visible: boolean
    permitted: boolean
    pending: boolean
    gate: WithdrawGate
    reviseRejected: boolean
    documentNoun: string
    ids: {
        trigger: string
        reason: string
        cancel: string
        confirm: string
    }
    createIdempotencyKey: () => string
    buildCommand: (input: {
        reason: string
        idempotencyKey: string
    }) => TCommand
    documentType: string
    documentIdOf: (command: TCommand) => string
    submit: (command: TCommand) => Promise<unknown>
    successHref?: string
    reference?: string
    onResult?: (result: WithdrawResult) => void
}) {
    const router = useRouter()
    const [open, setOpen] = React.useState(false)
    const [reason, setReason] = React.useState("")
    const [idempotencyKey, setIdempotencyKey] = React.useState("")
    const [confirmError, setConfirmError] = React.useState<string | null>(null)
    const [uncertain, setUncertain] = React.useState(false)
    const [checking, setChecking] = React.useState(false)
    const commandInFlight = React.useRef(false)
    const pendingCommand = React.useRef<TCommand | null>(null)
    const busy = pending || checking

    if (!visible || !permitted) return null

    return (
        <>
            <Button
                id={ids.trigger}
                type="button"
                size="sm"
                variant="outline"
                disabled={!gate.enabled}
                title={gate.reason}
                onClick={() => {
                    pendingCommand.current = null
                    setUncertain(false)
                    setReason(
                        reviseRejected ? "按驳回意见修改原单后重新提交" : "",
                    )
                    setConfirmError(null)
                    setIdempotencyKey(createIdempotencyKey())
                    setOpen(true)
                }}
            >
                {reviseRejected ? "修改原单" : "撤回审批"}
            </Button>
            <AlertDialog
                open={open}
                onOpenChange={(next) => {
                    if (!pendingCommand.current && !uncertain && !busy)
                        setOpen(next)
                }}
            >
                <AlertDialogContent className="sm:max-w-md">
                    <AlertDialogHeader>
                        <AlertDialogTitle>
                            {reviseRejected ? "修改原单" : "撤回审批"}
                        </AlertDialogTitle>
                        <AlertDialogDescription>
                            撤回后，{documentNoun}将回到草稿。
                            {reviseRejected
                                ? `原${documentNoun}编号和审批记录保留，修改后重新提交审批。`
                                : null}
                        </AlertDialogDescription>
                    </AlertDialogHeader>
                    <div className="space-y-2">
                        <label
                            htmlFor={ids.reason}
                            className="text-sm font-medium"
                        >
                            撤回原因
                            <RequiredMark />
                        </label>
                        <Textarea
                            id={ids.reason}
                            value={reason}
                            onChange={(event) => setReason(event.target.value)}
                            placeholder="请输入撤回原因"
                            rows={3}
                            disabled={busy || uncertain}
                        />
                        {confirmError ? (
                            <p
                                className="text-sm text-destructive"
                                role="alert"
                            >
                                {confirmError}
                            </p>
                        ) : null}
                    </div>
                    <AlertDialogFooter>
                        <AlertDialogCancel
                            id={ids.cancel}
                            disabled={busy || uncertain}
                        >
                            取消
                        </AlertDialogCancel>
                        <AlertDialogAction
                            loading={busy}
                            id={ids.confirm}
                            disabled={busy || !reason.trim()}
                            onClick={(event) => {
                                event.preventDefault()
                                if (commandInFlight.current) return
                                commandInFlight.current = true
                                setChecking(true)
                                setConfirmError(null)
                                const command =
                                    pendingCommand.current ??
                                    buildCommand({
                                        reason: reason.trim(),
                                        idempotencyKey,
                                    })
                                pendingCommand.current = command
                                let checkingUnknown = false
                                void (async () => {
                                    if (uncertain) {
                                        checkingUnknown = true
                                        if (
                                            await documentIsEditableDraft(
                                                documentType,
                                                documentIdOf(command),
                                            )
                                        )
                                            return
                                        checkingUnknown = false
                                    }
                                    return submit(command)
                                })()
                                    .then(() => {
                                        pendingCommand.current = null
                                        setUncertain(false)
                                        setOpen(false)
                                        if (successHref)
                                            router.push(successHref)
                                        onResult?.({
                                            status: "succeeded",
                                            title: "审批已撤回",
                                            description:
                                                "已撤回当前审批，单据回到可编辑草稿。",
                                            reference,
                                        })
                                    })
                                    .catch((error: unknown) => {
                                        const unknown =
                                            uncertain ||
                                            checkingUnknown ||
                                            classifyFormalCommandError(
                                                error,
                                            ) === "unknown"
                                        setUncertain(unknown)
                                        if (!unknown)
                                            pendingCommand.current = null
                                        const failure = getErrorPresentation(
                                            error,
                                            "撤回审批未完成，请刷新后重试。",
                                        )
                                        setConfirmError(failure.description)
                                        onResult?.({
                                            status: unknown
                                                ? "unknown"
                                                : "blocked",
                                            title: unknown
                                                ? "处理结果待确认"
                                                : failure.title,
                                            description: unknown
                                                ? "请使用本次操作重试；确认前不要修改输入或重复撤回。"
                                                : failure.description,
                                            reference,
                                        })
                                    })
                                    .finally(() => {
                                        commandInFlight.current = false
                                        setChecking(false)
                                    })
                            }}
                        >
                            {busy
                                ? "撤回中"
                                : uncertain
                                  ? "核对撤回结果"
                                  : reviseRejected
                                    ? "撤回并修改原单"
                                    : "确认撤回"}
                        </AlertDialogAction>
                    </AlertDialogFooter>
                </AlertDialogContent>
            </AlertDialog>
        </>
    )
}
