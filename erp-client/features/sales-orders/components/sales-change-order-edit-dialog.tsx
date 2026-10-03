"use client"

import * as React from "react"
import { useQuery } from "@tanstack/react-query"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { SubmissionRouteConfirmation } from "@/features/approval-workflow/components/submission-route-confirmation"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"
import {
    fetchSalesChangeDraft,
    saveSalesChangeDraft,
    salesChangeSubmissionMatches,
    type SalesChangeDraft,
    type SaveSalesChangeDraft,
} from "../api/sales-change-draft"
import { useSubmitSalesChangeOrderMutation } from "../hooks/queries"
import {
    nonnegativeSalesQuantity,
    positiveSalesDecimal,
    salesChangeDraftValues,
    salesChangeSavedMatches,
    salesChangeTargetLines,
    validSalesTaxRate,
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
    const query = useQuery({
        queryKey: ["sales-orders", "change-draft", changeOrderId],
        queryFn: () => fetchSalesChangeDraft(changeOrderId),
        enabled: open,
        staleTime: 0,
        refetchOnWindowFocus: false,
        refetchOnReconnect: false,
    })
    const id = `sales-change-edit-${toAutomationIdSegment(changeOrderId)}`
    return (
        <Dialog
            open={open}
            onOpenChange={(next) => !busyRef.current && onOpenChange(next)}
        >
            <DialogContent
                className="max-h-[85dvh] overflow-y-auto sm:max-w-2xl"
                showCloseButton={!busy}
                closeButtonId={`${id}-close`}
            >
                <DialogHeader>
                    <DialogTitle>修改销售变更原单</DialogTitle>
                    <DialogDescription>
                        保留原变更单、销售单关联和全部明细。保存修改后重新提交审批。
                    </DialogDescription>
                </DialogHeader>
                {query.isPending ? (
                    <p className="text-sm text-muted-foreground">
                        正在读取原单…
                    </p>
                ) : query.isError && !query.data ? (
                    <div className="space-y-3">
                        <p role="alert" className="text-sm text-destructive">
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
                    </div>
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
    const [pending, setPending] = React.useState(false)
    const [uncertain, setUncertain] = React.useState(false)
    const [error, setError] = React.useState<string | null>(null)
    const intent = React.useRef<EditIntent | null>(null)
    const inFlight = React.useRef(false)
    React.useEffect(() => {
        onBusyChange(pending || uncertain)
    }, [onBusyChange, pending, uncertain])
    const schema = React.useMemo(
        () =>
            z.object({
                reason: z
                    .string()
                    .trim()
                    .min(1, "请填写变更原因")
                    .max(512, "变更原因最多512个字符"),
                remark: z.string().max(512, "业务备注最多512个字符"),
                lines: z.array(
                    z.object({
                        quantity: z
                            .string()
                            .refine(
                                nonnegativeSalesQuantity,
                                "数量须大于或等于0，最多6位小数",
                            ),
                        unitPrice: z
                            .string()
                            .refine(
                                (value) => positiveSalesDecimal(value, 4),
                                "含税单价须大于0，最多4位小数",
                            ),
                        taxRate: z
                            .string()
                            .refine(
                                validSalesTaxRate,
                                "税率应在0至1之间，最多6位小数",
                            ),
                        faceValue: z
                            .string()
                            .refine(
                                (value) => positiveSalesDecimal(value, 2),
                                "面额须大于0，最多2位小数",
                            ),
                        cardCount: z
                            .string()
                            .refine(
                                (value) =>
                                    /^[1-9]\d*$/.test(value) &&
                                    BigInt(value) <= BigInt(4294967295),
                                "卡张数须为有效正整数",
                            ),
                    }),
                ),
            }),
        [],
    )
    const execute = async (command: EditIntent, recoverSave: boolean) => {
        if (inFlight.current) return
        inFlight.current = true
        onBusyChange(true)
        setPending(true)
        setError(null)
        let checkingUnknown = false
        try {
            if (!command.submit) {
                let saved: SalesChangeDraft
                if (recoverSave) {
                    checkingUnknown = true
                    const current = await fetchSalesChangeDraft(changeOrderId)
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
                        saved = await saveSalesChangeDraft(
                            changeOrderId,
                            command.save,
                        )
                    else
                        throw new Error(
                            "原单内容已变化，本次保存未能确认，请重新读取后核对",
                        )
                } else
                    saved = await saveSalesChangeDraft(
                        changeOrderId,
                        command.save,
                    )
                command.submit = {
                    version: saved.version,
                    contentHash: saved.content_hash,
                    idempotencyKey: `sales-change-edit:${changeOrderId}:${crypto.randomUUID()}`,
                }
            }
            if (recoverSave && command.submit) {
                checkingUnknown = true
                const submitted = await salesChangeSubmissionMatches(
                    changeOrderId,
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
        validators: { onChange: schema },
        onSubmit: async ({ value }) => {
            if (pending || uncertain) return
            try {
                const command: EditIntent = {
                    original: draft,
                    values: structuredClone(value),
                    save: {
                        expected_version: draft.version,
                        expected_working_copy_version:
                            draft.working_copy_version,
                        reason: value.reason.trim(),
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
            className="space-y-4"
            onSubmit={(event) => {
                event.preventDefault()
                void form.handleSubmit()
            }}
        >
            <form.AppField name="reason">
                {(field) => (
                    <field.TextareaField
                        id={`${id}-reason`}
                        label="变更原因"
                        required
                        disabled={disabled}
                    />
                )}
            </form.AppField>
            {draft.lines.map((line, index) => (
                <div
                    key={line.sales_order_line_id}
                    className="space-y-3 rounded-md border p-3"
                >
                    <p className="font-medium">
                        {line.line_no}. {line.item_name_snapshot}
                    </p>
                    <p className="text-sm text-muted-foreground">
                        {line.spec_snapshot ?? "—"} ·{" "}
                        {line.unit_snapshot ?? "—"}
                    </p>
                    <div className="grid gap-3 sm:grid-cols-2">
                        {line.goods ? (
                            <form.AppField name={`lines[${index}].quantity`}>
                                {(field) => (
                                    <field.TextField
                                        id={`${id}-${line.line_no}-quantity`}
                                        label="数量"
                                        inputMode="decimal"
                                        required
                                        disabled={disabled}
                                    />
                                )}
                            </form.AppField>
                        ) : (
                            <>
                                <form.AppField
                                    name={`lines[${index}].faceValue`}
                                >
                                    {(field) => (
                                        <field.TextField
                                            id={`${id}-${line.line_no}-face`}
                                            label="单卡面额"
                                            inputMode="decimal"
                                            required
                                            disabled={disabled}
                                        />
                                    )}
                                </form.AppField>
                                <form.AppField
                                    name={`lines[${index}].cardCount`}
                                >
                                    {(field) => (
                                        <field.TextField
                                            id={`${id}-${line.line_no}-cards`}
                                            label="卡张数"
                                            inputMode="numeric"
                                            required
                                            disabled={disabled}
                                        />
                                    )}
                                </form.AppField>
                            </>
                        )}
                        <form.AppField name={`lines[${index}].unitPrice`}>
                            {(field) => (
                                <field.TextField
                                    id={`${id}-${line.line_no}-price`}
                                    label="含税成交单价"
                                    inputMode="decimal"
                                    required
                                    disabled={disabled}
                                />
                            )}
                        </form.AppField>
                        <form.AppField name={`lines[${index}].taxRate`}>
                            {(field) => (
                                <field.TextField
                                    id={`${id}-${line.line_no}-tax`}
                                    label="销项税率（如13%填0.13）"
                                    inputMode="decimal"
                                    required
                                    disabled={disabled}
                                />
                            )}
                        </form.AppField>
                    </div>
                </div>
            ))}
            <form.AppField name="remark">
                {(field) => (
                    <field.TextareaField
                        id={`${id}-remark`}
                        label="业务备注"
                        disabled={disabled}
                    />
                )}
            </form.AppField>
            <SubmissionRouteConfirmation definition={approval?.definition} />
            {error ? (
                <p role="alert" className="text-sm text-destructive">
                    {error}
                </p>
            ) : null}
            <DialogFooter>
                <Button
                    id={`${id}-cancel`}
                    type="button"
                    variant="outline"
                    disabled={disabled}
                    onClick={onClose}
                >
                    取消
                </Button>
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
                            label="保存并提交审批"
                            loading={pending}
                            disabled={pending}
                        />
                    </form.AppForm>
                )}
            </DialogFooter>
        </form>
    )
}
