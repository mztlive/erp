"use client"

import * as React from "react"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
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
import { FieldGroup } from "@/components/ui/field"
import { SubmissionRouteConfirmation } from "@/features/approval-workflow/components/submission-route-confirmation"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"
import type { PurchaseChangeDraft } from "../api/purchase-change-draft"
import type { SubmitPurchaseChangeInput } from "../api/purchase-order-commands"
import { useSubmitPurchaseChangeMutation } from "../hooks/queries"
import {
    usePurchaseChangeDraftQuery,
    usePurchaseChangeSubmissionQuery,
} from "../hooks/use-purchase-change-draft"
import type { PurchaseOrderDetailResult } from "../hooks/use-purchase-order-detail-command-state"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    paymentTermLabel,
    SUPPLIER_PAYMENT_TERM_OPTIONS,
} from "@/lib/business-options"
import { compareDecimal } from "@/lib/fixed-decimal"
import {
    FormalCommandKeyLedger,
    classifyFormalCommandError,
} from "@/lib/formal-command"

type EditPayload = Omit<SubmitPurchaseChangeInput, "idempotencyKey"> & {
    paymentTermCode: string
    lines: NonNullable<SubmitPurchaseChangeInput["lines"]>
}

function decimalWithin(
    value: string,
    minimum: "positive" | "zero",
    maximum?: string,
) {
    try {
        const lower = compareDecimal(value.trim(), "0", 6)
        return (
            (minimum === "positive" ? lower > 0 : lower >= 0) &&
            (maximum == null || compareDecimal(value.trim(), maximum, 6) <= 0)
        )
    } catch {
        return false
    }
}

const lineSchema = z
    .object({
        lineKey: z.string().min(1),
        lineType: z.enum(["ITEM_SERVICE", "LOGISTICS_FEE"]),
        quantity: z.string(),
        unitCostGross: z.string(),
        inputTaxRate: z
            .string()
            .refine(
                (value) => decimalWithin(value, "zero", "1"),
                "税率须为 0 至 1 的小数",
            ),
        grossAmount: z.string(),
    })
    .superRefine((line, context) => {
        if (line.lineType === "ITEM_SERVICE") {
            if (!decimalWithin(line.quantity, "positive"))
                context.addIssue({
                    code: "custom",
                    path: ["quantity"],
                    message: "数量须大于 0，最多六位小数",
                })
            if (!decimalWithin(line.unitCostGross, "zero"))
                context.addIssue({
                    code: "custom",
                    path: ["unitCostGross"],
                    message: "单价须为非负金额，最多六位小数",
                })
        } else if (!decimalWithin(line.grossAmount, "zero")) {
            context.addIssue({
                code: "custom",
                path: ["grossAmount"],
                message: "物流费用须为非负金额",
            })
        }
    })

const schema = z.object({
    paymentTermCode: z.string().trim().min(1, "请选择付款条件"),
    lines: z.array(lineSchema).min(1, "采购明细不能为空"),
})

function draftFormValues(draft: PurchaseChangeDraft) {
    return {
        paymentTermCode: draft.payment_term_code,
        lines: draft.lines.map((line, index) => ({
            lineKey: draft.line_keys[index],
            lineType: line.line_type,
            quantity: line.quantity ?? "",
            unitCostGross: line.unit_cost_gross ?? "",
            inputTaxRate: line.input_tax_rate ?? "0",
            grossAmount: line.gross_amount ?? "",
        })),
    }
}

/** 采购变更的原单编辑；完整目标与来源由服务器草稿恢复。 */
export function PurchaseChangeOrderEditDialog({
    open,
    changeOrderId,
    purchaseOrderId,
    approval,
    onOpenChange,
    onResult,
}: {
    open: boolean
    changeOrderId: string
    purchaseOrderId: string
    approval?: DocumentApprovalView
    onOpenChange: (open: boolean) => void
    onResult?: (result: PurchaseOrderDetailResult) => void
}) {
    const query = usePurchaseChangeDraftQuery(changeOrderId, open)
    const [busy, setBusy] = React.useState(false)
    const busyRef = React.useRef(false)
    const changeBusy = React.useCallback((next: boolean) => {
        busyRef.current = next
        setBusy(next)
    }, [])
    const id = `purchase-change-edit-${toAutomationIdSegment(changeOrderId)}`
    const draft = query.data
    const complete = Boolean(
        draft &&
        draft.lines.length > 0 &&
        draft.line_keys?.length === draft.lines.length &&
        draft.line_keys.every((key) => key.trim().length > 0) &&
        new Set(draft.line_keys).size === draft.lines.length,
    )

    return (
        <Dialog
            open={open}
            onOpenChange={(next) => {
                if (!busyRef.current) onOpenChange(next)
            }}
        >
            <DialogContent
                closeButtonId={`${id}-close`}
                showCloseButton={!busy}
                className="max-h-[85dvh] overflow-y-auto sm:max-w-3xl"
            >
                <DialogHeader>
                    <DialogTitle>修改采购变更单</DialogTitle>
                    <DialogDescription>
                        保留原变更单与销售关联，修改目标内容后重新提交审批。
                    </DialogDescription>
                </DialogHeader>
                {query.isPending ? (
                    <p role="status" className="text-sm text-muted-foreground">
                        正在读取原变更内容…
                    </p>
                ) : (query.isError && !draft) || !complete ? (
                    <Alert variant="destructive">
                        <AlertTitle>改单明细暂无法完整读取</AlertTitle>
                        <AlertDescription>
                            {query.isError
                                ? getErrorMessage(query.error)
                                : "请重新读取原单内容后再提交。"}
                            <Button
                                id={`${id}-retry-load`}
                                type="button"
                                variant="outline"
                                className="mt-3"
                                onClick={() => void query.refetch()}
                            >
                                重新读取
                            </Button>
                        </AlertDescription>
                    </Alert>
                ) : draft ? (
                    <PurchaseChangeEditForm
                        key={changeOrderId}
                        draft={draft}
                        id={id}
                        purchaseOrderId={purchaseOrderId}
                        changeOrderId={changeOrderId}
                        approval={approval}
                        onBusyChange={changeBusy}
                        onClose={() => onOpenChange(false)}
                        onResult={onResult}
                    />
                ) : null}
            </DialogContent>
        </Dialog>
    )
}

function PurchaseChangeEditForm({
    draft,
    id,
    purchaseOrderId,
    changeOrderId,
    approval,
    onBusyChange,
    onClose,
    onResult,
}: {
    draft: PurchaseChangeDraft
    id: string
    purchaseOrderId: string
    changeOrderId: string
    approval?: DocumentApprovalView
    onBusyChange: (busy: boolean) => void
    onClose: () => void
    onResult?: (result: PurchaseOrderDetailResult) => void
}) {
    const mutation = useSubmitPurchaseChangeMutation()
    const submissionState = usePurchaseChangeSubmissionQuery(changeOrderId)
    const [unknown, setUnknown] = React.useState(false)
    const [error, setError] = React.useState<string | null>(null)
    const inFlight = React.useRef(false)
    const ledger = React.useMemo(() => new FormalCommandKeyLedger(), [])
    const slot = `edit-purchase-change:${changeOrderId}`
    const defaults = React.useMemo(() => draftFormValues(draft), [draft])
    const appliedDefaults = React.useRef(defaults)
    const paymentOptions = React.useMemo(
        () =>
            SUPPLIER_PAYMENT_TERM_OPTIONS.some(
                (option) => option.value === draft.payment_term_code,
            )
                ? SUPPLIER_PAYMENT_TERM_OPTIONS
                : [
                      {
                          value: draft.payment_term_code,
                          label:
                              paymentTermLabel(draft.payment_term_code) ===
                              draft.payment_term_code
                                  ? "原付款条件"
                                  : paymentTermLabel(draft.payment_term_code),
                      },
                      ...SUPPLIER_PAYMENT_TERM_OPTIONS,
                  ],
        [draft.payment_term_code],
    )

    const submit = async (payload?: EditPayload) => {
        if (inFlight.current) return
        const wasUnknown = unknown
        const command =
            ledger.peek<EditPayload>(slot) ??
            (payload
                ? ledger.acquire(
                      slot,
                      `purchase-change:${changeOrderId}:edit`,
                      payload,
                  )
                : undefined)
        if (!command) return
        inFlight.current = true
        onBusyChange(true)
        setError(null)
        let remainsUnknown = wasUnknown
        try {
            const response = await mutation.mutateAsync({
                ...command.payload,
                idempotencyKey: command.idempotencyKey,
            })
            remainsUnknown =
                response.status === "unknown" ||
                (wasUnknown && response.status !== "succeeded")
            ledger.settle(slot, remainsUnknown ? "unknown" : response.status)
            setUnknown(remainsUnknown)
            if (response.status === "succeeded") {
                onResult?.({
                    status: "succeeded",
                    title: "改单已提交审批",
                    description: `原变更单已进入「${response.data.statusLabel}」。当前采购版本继续有效。`,
                })
                onClose()
            } else {
                setError(response.message)
                onResult?.({
                    status: remainsUnknown ? "unknown" : "blocked",
                    title: remainsUnknown ? "处理结果待确认" : "改单未提交",
                    description: response.message,
                })
            }
        } catch (cause) {
            const status = classifyFormalCommandError(cause)
            remainsUnknown = wasUnknown || status === "unknown"
            ledger.settle(slot, remainsUnknown ? "unknown" : status)
            setUnknown(remainsUnknown)
            setError(getErrorMessage(cause))
        } finally {
            inFlight.current = false
            onBusyChange(remainsUnknown)
        }
    }
    const resolve = async () => {
        const command = ledger.peek<EditPayload>(slot)
        if (!command || inFlight.current) return
        inFlight.current = true
        onBusyChange(true)
        setError(null)
        try {
            const result = await submissionState.query.refetch()
            if (result.isError) throw result.error
            const current = result.data
            if (
                current?.id === changeOrderId &&
                current.purchase_order_id === purchaseOrderId &&
                current.version > command.payload.expectedLockVersion &&
                current.current_submission_id &&
                (current.status === "IN_APPROVAL" ||
                    current.status === "EFFECTIVE")
            ) {
                await submissionState.confirm()
                ledger.settle(slot, "succeeded")
                setUnknown(false)
                onResult?.({
                    status: "succeeded",
                    title: "原变更单状态已确认",
                    description:
                        "原变更单已进入审批或已生效，请查阅最新提交内容。",
                })
                onClose()
            } else
                setError(
                    "原变更单尚未确认进入审批，请保留本次内容并重试原操作。",
                )
        } catch (cause) {
            setError(getErrorMessage(cause, "暂时无法查询当前原单，请重试。"))
        } finally {
            inFlight.current = false
        }
    }
    const form = useAppForm({
        defaultValues: defaults,
        validators: { onChange: schema },
        onSubmit: async ({ value }) => {
            if (unknown || mutation.isPending) return
            const edits = new Map(
                value.lines.map((line) => [line.lineKey, line]),
            )
            const lines = draft.lines.map((line, index) => {
                const edit = edits.get(draft.line_keys[index])!
                return line.line_type === "LOGISTICS_FEE"
                    ? {
                          ...line,
                          gross_amount: edit.grossAmount.trim(),
                          input_tax_rate: edit.inputTaxRate.trim(),
                      }
                    : {
                          ...line,
                          quantity: edit.quantity.trim(),
                          allocated_quantity: edit.quantity.trim(),
                          unit_cost_gross: edit.unitCostGross.trim(),
                          input_tax_rate: edit.inputTaxRate.trim(),
                      }
            })
            await submit({
                purchaseChangeOrderId: changeOrderId,
                purchaseOrderId,
                expectedLockVersion: draft.version,
                paymentTermCode: value.paymentTermCode,
                lines,
            })
        },
    })
    React.useEffect(() => {
        onBusyChange(
            mutation.isPending || submissionState.query.isFetching || unknown,
        )
    }, [
        mutation.isPending,
        submissionState.query.isFetching,
        onBusyChange,
        unknown,
    ])
    React.useEffect(() => {
        if (
            !mutation.isPending &&
            !unknown &&
            !inFlight.current &&
            appliedDefaults.current !== defaults
        ) {
            appliedDefaults.current = defaults
            form.reset(defaults)
        }
    }, [defaults, form, mutation.isPending, unknown])
    const disabled = mutation.isPending || unknown

    return (
        <form.AppForm>
            <form
                id={`${id}-form`}
                className="space-y-5"
                onSubmit={(event) => {
                    event.preventDefault()
                    event.stopPropagation()
                    void form.handleSubmit()
                }}
            >
                <p className="text-sm text-muted-foreground">
                    变更原因：{draft.reason || "未填写"}
                </p>
                <form.AppField name="paymentTermCode">
                    {(field) => (
                        <field.SelectField
                            id={`${id}-payment-term`}
                            label="付款条件"
                            options={paymentOptions}
                            required
                            allowClear={false}
                            disabled={disabled}
                        />
                    )}
                </form.AppField>
                <div className="space-y-4">
                    {defaults.lines.map((line, index) => {
                        const original = draft.lines[index]
                        const prefix = `${id}-line-${toAutomationIdSegment(line.lineKey)}`
                        return (
                            <section
                                key={line.lineKey}
                                className="space-y-3 rounded-lg border border-border p-4"
                            >
                                <h3 className="text-sm font-semibold">
                                    {line.lineType === "LOGISTICS_FEE"
                                        ? "物流费用"
                                        : original.product_name ||
                                          "采购商品/服务"}
                                </h3>
                                {original.specification ? (
                                    <p className="text-xs text-muted-foreground">
                                        {original.specification}
                                    </p>
                                ) : null}
                                <FieldGroup className="grid gap-4 sm:grid-cols-3">
                                    {line.lineType === "ITEM_SERVICE" ? (
                                        <>
                                            <form.AppField
                                                name={`lines[${index}].quantity`}
                                            >
                                                {(field) => (
                                                    <field.TextField
                                                        id={`${prefix}-quantity`}
                                                        label="采购数量"
                                                        inputMode="decimal"
                                                        required
                                                        disabled={disabled}
                                                    />
                                                )}
                                            </form.AppField>
                                            <form.AppField
                                                name={`lines[${index}].unitCostGross`}
                                            >
                                                {(field) => (
                                                    <field.TextField
                                                        id={`${prefix}-unit-cost`}
                                                        label="含税采购单价"
                                                        inputMode="decimal"
                                                        required
                                                        disabled={disabled}
                                                    />
                                                )}
                                            </form.AppField>
                                        </>
                                    ) : (
                                        <form.AppField
                                            name={`lines[${index}].grossAmount`}
                                        >
                                            {(field) => (
                                                <field.TextField
                                                    id={`${prefix}-logistics-gross`}
                                                    label="含税物流费用"
                                                    inputMode="decimal"
                                                    required
                                                    disabled={disabled}
                                                />
                                            )}
                                        </form.AppField>
                                    )}
                                    <form.AppField
                                        name={`lines[${index}].inputTaxRate`}
                                    >
                                        {(field) => (
                                            <field.TextField
                                                id={`${prefix}-tax-rate`}
                                                label="进项税率"
                                                description="例如 0.13 表示 13%"
                                                inputMode="decimal"
                                                required
                                                disabled={disabled}
                                            />
                                        )}
                                    </form.AppField>
                                </FieldGroup>
                            </section>
                        )
                    })}
                </div>
                <SubmissionRouteConfirmation
                    definition={approval?.definition}
                />
                {error ? (
                    <p role="alert" className="text-sm text-destructive">
                        {error}
                    </p>
                ) : null}
                {unknown ? (
                    <Alert variant="warning">
                        <AlertTitle>提交结果待确认</AlertTitle>
                        <AlertDescription>
                            当前变更内容已保留。请查询原单状态或使用本次操作重试，确认前不能修改或关闭。
                        </AlertDescription>
                    </Alert>
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
                    {unknown ? (
                        <>
                            <LoadingButton
                                id={`${id}-resolve-submit`}
                                type="button"
                                variant="outline"
                                loading={submissionState.query.isFetching}
                                disabled={
                                    mutation.isPending ||
                                    submissionState.query.isFetching
                                }
                                onClick={() => void resolve()}
                            >
                                查询原单状态
                            </LoadingButton>
                            <LoadingButton
                                id={`${id}-retry-submit`}
                                type="button"
                                disabled={
                                    mutation.isPending ||
                                    submissionState.query.isFetching
                                }
                                loading={mutation.isPending}
                                onClick={() => void submit()}
                            >
                                使用本次操作重试
                            </LoadingButton>
                        </>
                    ) : (
                        <form.SubmitButton
                            id={`${id}-submit`}
                            label="确认修改并提交审批"
                            pendingLabel="正在提交…"
                            disabled={disabled}
                        />
                    )}
                </DialogFooter>
            </form>
        </form.AppForm>
    )
}
