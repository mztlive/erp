"use client"

import * as React from "react"
import { useResubmissionReason } from "@/features/approval-workflow/hooks/use-resubmission-reason"
import {
    SalesChangeBlockers,
    SalesChangeDraftFields,
    SalesChangeTotalSummary,
} from "./sales-change-draft-fields"
import { salesChangeDraftSchema } from "../lib/sales-change-draft-schema"
import {
    useSalesChangeDraftQuery,
    useSalesChangeDraftCommands,
} from "../hooks/use-sales-change-draft"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import {
    Dialog,
    DialogBody,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { SubmissionRouteConfirmation } from "@/features/approval-workflow/components/submission-route-confirmation"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"
import {
    type SalesChangeDraft,
    type SaveSalesChangeDraft,
} from "../api/sales-change-draft"
import { useSubmitSalesChangeOrderMutation } from "../hooks/queries"
import {
    salesChangeDraftValues,
    salesChangeSavedMatches,
    salesChangeTargetLines,
    type SalesChangeDraftValues,
} from "../lib/sales-change-draft"
import type { SalesOrderNature } from "../types"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { classifyFormalCommandError } from "@/lib/formal-command"

/** 编辑原销售变更内容；提交继续消费同一变更单 ID。 */
export function SalesChangeOrderEditDialog({
    open,
    onOpenChange,
    salesOrderId,
    changeOrderId,
    nature,
    approval,
    onApplied,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    salesOrderId: string
    changeOrderId: string
    nature: SalesOrderNature
    approval?: DocumentApprovalView
    onApplied: () => void
}) {
    const [busy, setBusy] = React.useState(false)
    const busyRef = React.useRef(false)
    const setCommandBusy = React.useCallback((next: boolean) => {
        busyRef.current = next
        setBusy(next)
    }, [])
    const query = useSalesChangeDraftQuery(changeOrderId, open)
    const id = `sales-change-edit-${toAutomationIdSegment(changeOrderId)}`
    return (
        <Dialog
            open={open}
            onOpenChange={(next) => !busyRef.current && onOpenChange(next)}
        >
            <DialogContent
                size="2xl"
                scroll="body"
                showCloseButton={!busy}
                closeButtonId={`${id}-close`}
            >
                <DialogHeader className="bg-popover text-left">
                    <DialogTitle>销售改单</DialogTitle>
                    <DialogDescription>
                        修改后填写变更原因并提交审批。审批通过前仍按当前销售版本执行，已有回款、开票和交付记录保留。
                        关闭不会放弃本次改单；未保存的内容请先点「保存修改」。
                    </DialogDescription>
                </DialogHeader>
                {query.isPending ? (
                    <DialogBody>
                        <p className="text-xs text-muted-foreground">
                            正在读取原单…
                        </p>
                    </DialogBody>
                ) : query.isError && !query.data ? (
                    <DialogBody className="space-y-3">
                        <p
                            role="alert"
                            className="text-body-compact text-destructive"
                        >
                            {getErrorMessage(query.error)}
                        </p>
                        <Button
                            id={`${id}-reload`}
                            type="button"
                            variant="outline"
                            onClick={() => void query.refetch()}
                        >
                            重新读取
                        </Button>
                    </DialogBody>
                ) : query.data ? (
                    <SalesChangeDraftForm
                        key={changeOrderId}
                        draft={query.data}
                        id={id}
                        salesOrderId={salesOrderId}
                        changeOrderId={changeOrderId}
                        nature={nature}
                        approval={approval}
                        onBusyChange={setCommandBusy}
                        onApplied={onApplied}
                        onClose={() => onOpenChange(false)}
                        onRefresh={async () => {
                            const result = await query.refetch()
                            if (result.isError) throw result.error
                            if (!result.data) throw new Error("原单读取失败")
                            return result.data
                        }}
                    />
                ) : null}
            </DialogContent>
        </Dialog>
    )
}

type EditIntent = {
    shouldSubmit: boolean
    resubmissionReason?: string
    original: SalesChangeDraft
    values: SalesChangeDraftValues
    save: SaveSalesChangeDraft
    submit?: { version: number; idempotencyKey: string; contentHash: string }
}

function SalesChangeDraftForm({
    draft,
    id,
    salesOrderId,
    changeOrderId,
    nature,
    approval,
    onBusyChange,
    onApplied,
    onClose,
    onRefresh,
}: {
    draft: SalesChangeDraft
    id: string
    salesOrderId: string
    changeOrderId: string
    nature: SalesOrderNature
    approval?: DocumentApprovalView
    onBusyChange: (busy: boolean) => void
    onApplied: () => void
    onClose: () => void
    onRefresh: () => Promise<SalesChangeDraft>
}) {
    const submitMutation = useSubmitSalesChangeOrderMutation()
    const commands = useSalesChangeDraftCommands(changeOrderId)
    const [pending, setPending] = React.useState(false)
    const [saved, setSaved] = React.useState(false)
    const shouldSubmit = React.useRef(true)
    const [uncertain, setUncertain] = React.useState(false)
    const [error, setError] = React.useState<string | null>(null)
    const rework = useResubmissionReason({
        open: true,
        required:
            draft.status === "REJECTED" ||
            approval?.instance?.status === "REJECTED",
        id: `${id}-resubmission-reason`,
        disabled: pending || uncertain,
    })
    const intent = React.useRef<EditIntent | null>(null)
    const inFlight = React.useRef(false)
    React.useEffect(() => {
        onBusyChange(pending || uncertain)
    }, [onBusyChange, pending, uncertain])
    const execute = async (command: EditIntent, recoverSave: boolean) => {
        if (inFlight.current) return
        inFlight.current = true
        onBusyChange(true)
        setPending(true)
        setError(null)
        setSaved(false)
        let checkingUnknown = false
        try {
            if (!command.submit) {
                let saved: SalesChangeDraft
                if (recoverSave) {
                    checkingUnknown = true
                    const current = await commands.read()
                    checkingUnknown = false
                    if (
                        salesChangeSavedMatches(
                            current,
                            command.values,
                            command.original,
                        )
                    )
                        saved = current
                    else if (
                        current.version === command.save.expected_version &&
                        current.working_copy_version ===
                            command.save.expected_working_copy_version
                    )
                        saved = await commands.save(command.save)
                    else
                        throw new Error(
                            "原单内容已变化，本次保存未能确认，请重新读取后核对",
                        )
                } else saved = await commands.save(command.save)
                if (!command.shouldSubmit) {
                    intent.current = null
                    setUncertain(false)
                    setSaved(true)
                    return
                }
                command.submit = {
                    version: saved.version,
                    contentHash: saved.content_hash,
                    idempotencyKey: `sales-change-edit:${changeOrderId}:${crypto.randomUUID()}`,
                }
            }
            if (recoverSave && command.submit) {
                checkingUnknown = true
                const submitted = await commands.matches(
                    command.submit.version,
                    command.submit.contentHash,
                )
                checkingUnknown = false
                if (submitted) {
                    intent.current = null
                    setUncertain(false)
                    onApplied()
                    onClose()
                    return
                }
            }
            await submitMutation.mutateAsync({
                salesChangeOrderId: changeOrderId,
                salesOrderId,
                nature,
                version: command.submit.version,
                resubmissionReason: command.resubmissionReason,
                idempotencyKey: command.submit.idempotencyKey,
            })
            intent.current = null
            setUncertain(false)
            onApplied()
            onClose()
        } catch (cause) {
            const unknown =
                recoverSave ||
                checkingUnknown ||
                classifyFormalCommandError(cause) === "unknown"
            setUncertain(unknown)
            setError(
                unknown
                    ? "处理结果待确认。请保留本次输入，使用下方按钮查询保存结果或重试同一提交。"
                    : getErrorMessage(cause, "原单修改或提交未完成"),
            )
            if (!unknown) {
                intent.current = null
                void onRefresh().catch(() => undefined)
            }
        } finally {
            inFlight.current = false
            setPending(false)
        }
    }
    const form = useAppForm({
        defaultValues: salesChangeDraftValues(draft),
        validators: { onChange: salesChangeDraftSchema },
        onSubmit: async ({ value }) => {
            if (pending || uncertain || (shouldSubmit.current && !rework.valid))
                return
            try {
                const command: EditIntent = {
                    shouldSubmit: shouldSubmit.current,
                    resubmissionReason: rework.reason,
                    original: draft,
                    values: structuredClone(value),
                    save: {
                        expected_version: draft.version,
                        expected_working_copy_version:
                            draft.working_copy_version,
                        reason: value.reason.trim(),
                        terms: value.terms,
                        business_remark: value.remark.trim() || null,
                        lines: salesChangeTargetLines(draft, value),
                    },
                }
                intent.current = command
                await execute(command, false)
            } catch (cause) {
                setError(getErrorMessage(cause, "请核对变更明细"))
            }
        },
    })
    const loadedVersion = React.useRef(
        `${draft.version}:${draft.working_copy_version}`,
    )
    React.useEffect(() => {
        const version = `${draft.version}:${draft.working_copy_version}`
        if (!pending && !uncertain && loadedVersion.current !== version) {
            form.reset(salesChangeDraftValues(draft))
            loadedVersion.current = version
        }
    }, [draft, form, pending, uncertain])
    const disabled = pending || uncertain
    return (
        <form
            className="flex min-h-0 flex-1 flex-col overflow-hidden"
            onSubmit={(event) => {
                event.preventDefault()
                shouldSubmit.current = true
                void form.handleSubmit()
            }}
        >
            <DialogBody className="space-y-6">
                <SalesChangeDraftFields
                    form={form}
                    id={id}
                    draft={draft}
                    changeOrderId={changeOrderId}
                    disabled={disabled}
                    onError={setError}
                />
                {rework.field}
                <SubmissionRouteConfirmation
                    definition={approval?.definition}
                />
                {saved ? (
                    <p role="status" className="text-xs text-muted-foreground">
                        修改已保存，尚未重新提交审批。
                    </p>
                ) : null}
                {error ? (
                    <p
                        role="alert"
                        className="text-body-compact text-destructive"
                    >
                        {error}
                    </p>
                ) : null}
            </DialogBody>
            <DialogFooter className="flex-col bg-popover sm:flex-row sm:items-center sm:justify-between">
                <SalesChangeTotalSummary
                    form={form}
                    baseAmount={draft.base_gross_amount}
                />
                <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:gap-3">
                    {uncertain ? null : (
                        <SalesChangeBlockers form={form} id={id} />
                    )}
                    <div className="flex flex-col-reverse gap-2 sm:flex-row">
                        <Button
                            id={`${id}-cancel`}
                            type="button"
                            variant="outline"
                            disabled={disabled}
                            onClick={onClose}
                        >
                            关闭
                        </Button>
                        {!uncertain ? (
                            <LoadingButton
                                id={`${id}-save`}
                                type="button"
                                variant="outline"
                                disabled={pending}
                                loading={pending && !shouldSubmit.current}
                                onClick={() => {
                                    shouldSubmit.current = false
                                    void form.handleSubmit()
                                }}
                            >
                                保存修改
                            </LoadingButton>
                        ) : null}
                        {uncertain ? (
                            <LoadingButton
                                id={`${id}-recover`}
                                type="button"
                                loading={pending}
                                disabled={pending}
                                onClick={() => {
                                    if (intent.current)
                                        void execute(intent.current, true)
                                }}
                            >
                                核对并重试本次操作
                            </LoadingButton>
                        ) : (
                            <form.AppForm>
                                <form.SubmitButton
                                    id={`${id}-submit`}
                                    label="提交改单"
                                    loading={pending}
                                    disabled={pending || !rework.valid}
                                />
                            </form.AppForm>
                        )}
                    </div>
                </div>
            </DialogFooter>
        </form>
    )
}
