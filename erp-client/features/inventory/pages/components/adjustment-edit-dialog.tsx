"use client"

import * as React from "react"
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
import { Alert, AlertDescription } from "@/components/ui/alert"
import { AdjustmentApprovalArea } from "../../components/adjustment-approval-area"
import type { EditableAdjustment } from "../../api/adjustment-edit"
import { useEditableAdjustmentQuery } from "../../hooks/use-adjustment-edit"
import {
    useResolveAdjustmentUnknownMutation,
    useSubmitAdjustmentMutation,
} from "../../hooks/queries"
import { adjustSchema } from "../../lib/presentation"
import { REASON_TYPE_OPTIONS } from "../../types"
import { submitAdjustment } from "../../api/adjustment"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { getErrorMessage } from "@/lib/api/errors"
import {
    classifyFormalCommandError,
    FormalCommandKeyLedger,
    type FormalCommandIdentity,
} from "@/lib/formal-command"

type SubmitInput = Omit<
    Parameters<typeof submitAdjustment>[0],
    "idempotencyKey"
>
const editSchema = adjustSchema.omit({ quantity: true }).extend({
    lines: z
        .array(
            z.object({
                lineId: z.string().min(1),
                quantity: adjustSchema.shape.quantity,
                direction: z.enum(["increase", "decrease"]),
            }),
        )
        .min(1),
})

/** 编辑撤回后的原调整单，所有原行与单据身份均保留。 */
export function AdjustmentEditDialog({
    id,
    onClose,
    onSubmitted,
}: {
    id: string
    onClose: () => void
    onSubmitted: () => void
}) {
    const query = useEditableAdjustmentQuery(id)
    const [busy, setBusy] = React.useState(false)
    const prefix = `inventory-adjustment-edit-${toAutomationIdSegment(id)}`
    return (
        <Dialog
            open
            onOpenChange={(open) => {
                if (!open && !busy) onClose()
            }}
        >
            <DialogContent
                closeButtonId={`${prefix}-close`}
                showCloseButton={!busy}
                className="max-h-[90dvh] overflow-y-auto sm:max-w-xl"
            >
                <DialogHeader>
                    <DialogTitle>
                        修改库存调整单
                        {query.data ? ` · ${query.data.adjustmentNo}` : ""}
                    </DialogTitle>
                    <DialogDescription>
                        修改原单后重新提交审批，审批通过后调整库存。
                    </DialogDescription>
                </DialogHeader>
                {query.isPending ? (
                    <p className="text-sm text-muted-foreground">
                        正在加载原草稿…
                    </p>
                ) : query.data ? (
                    <>
                        {query.isError ? (
                            <p
                                role="alert"
                                className="text-sm text-destructive"
                            >
                                {getErrorMessage(
                                    query.error,
                                    "刷新原草稿失败，当前输入已保留。",
                                )}
                            </p>
                        ) : null}
                        <AdjustmentEditForm
                            draft={query.data}
                            prefix={prefix}
                            onBusyChange={setBusy}
                            onClose={onClose}
                            onSubmitted={onSubmitted}
                            onRefresh={() => query.refetch()}
                        />
                    </>
                ) : query.isError ? (
                    <div className="space-y-3">
                        <p role="alert" className="text-sm text-destructive">
                            {getErrorMessage(
                                query.error,
                                "加载原草稿失败，请重试。",
                            )}
                        </p>
                        <LoadingButton
                            id={`${prefix}-reload`}
                            type="button"
                            variant="outline"
                            loading={query.isFetching}
                            onClick={() => void query.refetch()}
                        >
                            重新加载
                        </LoadingButton>
                    </div>
                ) : null}
            </DialogContent>
        </Dialog>
    )
}

function AdjustmentEditForm({
    draft,
    prefix,
    onBusyChange,
    onClose,
    onSubmitted,
    onRefresh,
}: {
    draft: EditableAdjustment
    prefix: string
    onBusyChange: (busy: boolean) => void
    onClose: () => void
    onSubmitted: () => void
    onRefresh: () => Promise<unknown>
}) {
    const submit = useSubmitAdjustmentMutation()
    const resolve = useResolveAdjustmentUnknownMutation()
    const ledger = React.useRef(new FormalCommandKeyLedger())
    const inFlight = React.useRef(false)
    const [unknown, setUnknown] =
        React.useState<FormalCommandIdentity<SubmitInput> | null>(null)
    const [error, setError] = React.useState<string | null>(null)
    const defaults = React.useMemo(
        () => ({
            reasonType: draft.reasonType,
            note: draft.note,
            occurredAt: draft.occurredAt,
            lines: draft.lines.map(({ lineId, quantity, direction }) => ({
                lineId,
                quantity,
                direction,
            })),
        }),
        [draft],
    )
    const pending = submit.isPending || resolve.isPending
    const locked = pending || Boolean(unknown)

    async function execute(command: FormalCommandIdentity<SubmitInput>) {
        if (inFlight.current) return
        inFlight.current = true
        setError(null)
        try {
            const result = await submit.mutateAsync({
                ...command.payload,
                idempotencyKey: command.idempotencyKey,
            })
            ledger.current.settle(
                "submit",
                result.status === "failed" ? "failed" : result.status,
            )
            if (result.status === "succeeded") {
                setUnknown(null)
                onSubmitted()
                onClose()
            } else if (result.status === "unknown") {
                setUnknown(command)
                setError(result.message)
            } else {
                setUnknown(null)
                setError(result.message)
            }
        } catch (cause) {
            const outcome = classifyFormalCommandError(cause)
            ledger.current.settle("submit", outcome)
            setUnknown(outcome === "unknown" ? command : null)
            setError(getErrorMessage(cause, "提交未完成，请重试。"))
        } finally {
            inFlight.current = false
        }
    }

    const form = useAppForm({
        defaultValues: defaults,
        validators: { onChange: editSchema },
        onSubmit: async ({ value }) => {
            if (locked || inFlight.current || !draft.approval.submitCommand)
                return
            const first = draft.lines[0]
            const command = ledger.current.acquire(
                "submit",
                `inventory:${draft.id}:submit`,
                {
                    stockAdjustmentId: draft.id,
                    submitCommand: draft.approval.submitCommand,
                    balanceId: first.balanceId,
                    lineId: first.lineId,
                    expectedBalanceLockVersion: first.balanceVersion,
                    reasonType: value.reasonType,
                    reasonTypeLabel:
                        REASON_TYPE_OPTIONS.find(
                            (option) => option.value === value.reasonType,
                        )?.label ?? "库存调整",
                    direction: value.lines[0].direction,
                    quantity: value.lines[0].quantity.trim(),
                    lineUpdates: value.lines.map((line) => ({
                        ...line,
                        quantity: line.quantity.trim(),
                    })),
                    balanceVersions: [
                        ...new Map(
                            draft.lines.map((line) => [
                                line.balanceId,
                                {
                                    balanceId: line.balanceId,
                                    expectedVersion: line.balanceVersion,
                                },
                            ]),
                        ).values(),
                    ],
                    note: value.note.trim(),
                    occurredAt: value.occurredAt,
                },
            )
            await execute(command)
        },
    })
    const loadedDefaults = React.useRef(defaults)
    React.useEffect(() => {
        if (!locked && loadedDefaults.current !== defaults) {
            form.reset(defaults)
            loadedDefaults.current = defaults
        }
    }, [defaults, form, locked])
    React.useEffect(() => {
        onBusyChange(locked)
        return () => onBusyChange(false)
    }, [locked, onBusyChange])

    async function resolveUnknown() {
        if (!unknown || pending) return
        setError(null)
        try {
            const result = await resolve.mutateAsync({
                stockAdjustmentId: draft.id,
                expectedSubjectVersion:
                    unknown.payload.submitCommand.expectedSubjectVersion,
                idempotencyKey: unknown.idempotencyKey,
            })
            if (result.status === "succeeded") {
                ledger.current.settle("submit", "succeeded")
                setUnknown(null)
                onSubmitted()
                onClose()
            } else setError(result.message)
        } catch (cause) {
            setError(getErrorMessage(cause, "暂无法确认提交结果，请稍后重试。"))
        }
    }

    return (
        <form
            className="space-y-4"
            onSubmit={(event) => {
                event.preventDefault()
                void form.handleSubmit()
            }}
        >
            <p className="text-sm font-medium">
                {draft.warehouseName} · {draft.lines.length} 项明细
            </p>
            {error ? (
                <Alert variant={unknown ? "default" : "destructive"}>
                    <AlertDescription>{error}</AlertDescription>
                </Alert>
            ) : null}
            <fieldset disabled={locked} className="space-y-4">
                <form.AppField
                    name="reasonType"
                    children={(field) => (
                        <field.SelectField
                            id={`${prefix}-reason-type`}
                            label="原因类型"
                            required
                            disabled={locked}
                            allowClear={false}
                            options={REASON_TYPE_OPTIONS.filter(
                                (option) => option.value !== "OTHER",
                            )}
                            onValueChange={(value) => {
                                const direction =
                                    value === "COUNT_GAIN"
                                        ? "increase"
                                        : "decrease"
                                form.setFieldValue(
                                    "lines",
                                    form.state.values.lines.map((line) => ({
                                        ...line,
                                        direction,
                                    })),
                                )
                            }}
                        />
                    )}
                />
                {draft.lines.map((line, index) => (
                    <div
                        key={line.lineId}
                        className="space-y-2 rounded-lg border p-3"
                    >
                        <div className="text-sm font-medium">
                            {line.skuName}{" "}
                            <span className="num text-muted-foreground">
                                {line.skuCode}
                            </span>
                        </div>
                        <p className="text-xs text-muted-foreground">
                            账面现存 {line.onHand} · 可用 {line.available}
                        </p>
                        <form.AppField
                            name={`lines[${index}].quantity`}
                            children={(field) => (
                                <field.TextField
                                    id={`${prefix}-line-${toAutomationIdSegment(line.lineId)}-quantity`}
                                    label="调整数量"
                                    required
                                    disabled={locked}
                                />
                            )}
                        />
                    </div>
                ))}
                <form.AppField
                    name="occurredAt"
                    children={(field) => (
                        <field.DateTimeField
                            id={`${prefix}-occurred-at`}
                            label="业务发生时间"
                            required
                            disabled={locked}
                        />
                    )}
                />
                <form.AppField
                    name="note"
                    children={(field) => (
                        <field.TextareaField
                            id={`${prefix}-note`}
                            label="原因说明"
                            required
                            disabled={locked}
                        />
                    )}
                />
                <AdjustmentApprovalArea
                    phase="confirm"
                    approval={draft.approval}
                />
            </fieldset>
            {unknown ? (
                <div className="flex flex-wrap gap-2">
                    <LoadingButton
                        id={`${prefix}-resolve`}
                        type="button"
                        variant="outline"
                        disabled={pending}
                        loading={resolve.isPending}
                        onClick={() => void resolveUnknown()}
                    >
                        查询提交结果
                    </LoadingButton>
                    <LoadingButton
                        id={`${prefix}-retry`}
                        type="button"
                        variant="outline"
                        disabled={pending}
                        loading={submit.isPending}
                        onClick={() => void execute(unknown)}
                    >
                        使用本次操作重试
                    </LoadingButton>
                </div>
            ) : null}
            <DialogFooter>
                <Button
                    id={`${prefix}-cancel`}
                    type="button"
                    variant="outline"
                    disabled={locked}
                    onClick={onClose}
                >
                    取消
                </Button>
                <Button
                    id={`${prefix}-refresh`}
                    type="button"
                    variant="outline"
                    disabled={locked}
                    onClick={() => void onRefresh()}
                >
                    重新加载草稿
                </Button>
                <form.AppForm>
                    <form.SubmitButton
                        id={`${prefix}-submit`}
                        label="保存并提交审批"
                        disabled={locked}
                        loading={submit.isPending}
                    />
                </form.AppForm>
            </DialogFooter>
        </form>
    )
}
