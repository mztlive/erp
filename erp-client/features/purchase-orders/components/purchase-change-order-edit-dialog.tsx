"use client"

import * as React from "react"
import { XIcon } from "lucide-react"
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
import { SubmissionRouteConfirmation } from "@/features/approval-workflow/components/submission-route-confirmation"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"
import type { PurchaseChangeDraft } from "../api/purchase-change-draft"
import type { SubmitPurchaseChangeInput } from "../api/purchase-order-commands"
import {
    usePurchaseDeliveryOptionsQuery,
    useSubmitPurchaseChangeMutation,
    usePurchaseOrderCenterQuery,
} from "../hooks/queries"
import {
    usePurchaseChangeDraftQuery,
    usePurchaseChangeSubmissionQuery,
} from "../hooks/use-purchase-change-draft"
import type { PurchaseOrderDetailResult } from "../hooks/use-purchase-order-detail-command-state"
import { purchaseChangeDraftSchema } from "../lib/purchase-change-draft-schema"
import {
    purchaseChangeDraftValues,
    purchaseChangeTargetLines,
    type PurchaseChangeDeliveryOptions,
} from "../lib/purchase-change-draft"
import {
    PurchaseChangeBlockers,
    PurchaseChangeDraftFields,
    PurchaseChangeTotalSummary,
} from "./purchase-change-draft-fields"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    FormalCommandKeyLedger,
    classifyFormalCommandError,
} from "@/lib/formal-command"

type EditPayload = Omit<SubmitPurchaseChangeInput, "idempotencyKey">

/** 采购改单：修改原变更单的目标内容后提交审批；生效前采购单按当前版本执行。 */
export function PurchaseChangeOrderEditDialog({
    open,
    changeOrderId,
    purchaseOrderId,
    approval,
    paidAmount,
    onOpenChange,
    onResult,
}: {
    open: boolean
    changeOrderId: string
    purchaseOrderId: string
    approval?: DocumentApprovalView
    /** 已付款金额；改后总额不得低于此金额。 */
    paidAmount?: string
    onOpenChange: (open: boolean) => void
    onResult?: (result: PurchaseOrderDetailResult) => void
}) {
    const query = usePurchaseChangeDraftQuery(changeOrderId, open)
    const optionsQuery = usePurchaseDeliveryOptionsQuery(purchaseOrderId, open)
    const options = React.useMemo<PurchaseChangeDeliveryOptions>(
        () =>
            new Map(
                (optionsQuery.data ?? []).map((option) => [
                    option.salesOrderLineId,
                    option,
                ]),
            ),
        [optionsQuery.data],
    )
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
    const loadError = query.isError
        ? query.error
        : optionsQuery.isError
          ? optionsQuery.error
          : null

    return (
        <Dialog
            open={open}
            onOpenChange={(next) => {
                if (!busyRef.current) onOpenChange(next)
            }}
        >
            <DialogContent
                className="max-h-[90dvh] w-[calc(100vw_-_2rem)] overflow-y-auto sm:max-w-5xl"
                showCloseButton={false}
                closeButtonId={`${id}-close`}
            >
                <DialogHeader className="sticky -top-6 z-10 -mx-6 -mt-6 border-b bg-popover px-6 py-5 pr-16">
                    <Button
                        id={`${id}-close`}
                        type="button"
                        variant="ghost"
                        size="icon-sm"
                        className="absolute top-4 right-4"
                        aria-label="关闭改单"
                        disabled={busy}
                        onClick={() => onOpenChange(false)}
                    >
                        <XIcon />
                    </Button>
                    <DialogTitle>采购改单</DialogTitle>
                    <DialogDescription>
                        修改后填写变更原因并提交审批。审批通过前仍按当前采购版本执行，已有入库、发货和付款记录保留。
                    </DialogDescription>
                </DialogHeader>
                {query.isPending || optionsQuery.isPending ? (
                    <p role="status" className="text-sm text-muted-foreground">
                        正在读取原单…
                    </p>
                ) : loadError || !complete || !draft ? (
                    <div className="space-y-3">
                        <p role="alert" className="text-sm text-destructive">
                            {loadError
                                ? getErrorMessage(loadError)
                                : "改单明细暂无法完整读取，请重新读取后再提交。"}
                        </p>
                        <Button
                            id={`${id}-retry-load`}
                            type="button"
                            variant="outline"
                            onClick={() => {
                                void query.refetch()
                                void optionsQuery.refetch()
                            }}
                        >
                            重新读取
                        </Button>
                    </div>
                ) : (
                    <PurchaseChangeEditForm
                        key={changeOrderId}
                        draft={draft}
                        options={options}
                        id={id}
                        purchaseOrderId={purchaseOrderId}
                        changeOrderId={changeOrderId}
                        approval={approval}
                        paidAmount={paidAmount}
                        onBusyChange={changeBusy}
                        onClose={() => onOpenChange(false)}
                        onResult={onResult}
                    />
                )}
            </DialogContent>
        </Dialog>
    )
}

function PurchaseChangeEditForm({
    draft,
    options,
    id,
    purchaseOrderId,
    changeOrderId,
    approval,
    paidAmount,
    onBusyChange,
    onClose,
    onResult,
}: {
    draft: PurchaseChangeDraft
    options: PurchaseChangeDeliveryOptions
    id: string
    purchaseOrderId: string
    changeOrderId: string
    approval?: DocumentApprovalView
    paidAmount?: string
    onBusyChange: (busy: boolean) => void
    onClose: () => void
    onResult?: (result: PurchaseOrderDetailResult) => void
}) {
    const mutation = useSubmitPurchaseChangeMutation()
    const submissionState = usePurchaseChangeSubmissionQuery(changeOrderId)
    const centerQuery = usePurchaseOrderCenterQuery(purchaseOrderId)
    const occurredExpenseIds = React.useMemo(
        () =>
            new Set(
                (centerQuery.data?.currentContent.lines ?? []).flatMap(
                    (line) =>
                        line.expense && line.expenseOccurrence
                            ? [line.expense.expenseLineId]
                            : [],
                ),
            ),
        [centerQuery.data],
    )
    const [unknown, setUnknown] = React.useState(false)
    const [error, setError] = React.useState<string | null>(null)
    const inFlight = React.useRef(false)
    const ledger = React.useMemo(() => new FormalCommandKeyLedger(), [])
    const slot = `edit-purchase-change:${changeOrderId}`
    const schema = React.useMemo(
        () => purchaseChangeDraftSchema(options),
        [options],
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
                    description:
                        "审批通过后生成新采购版本；生效前采购单仍按当前版本执行。",
                })
                onClose()
            } else setError(response.message)
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
                    title: "改单已提交审批",
                    description: "原变更单已进入审批，请查阅最新状态。",
                })
                onClose()
            } else
                setError("改单尚未确认进入审批，请保留本次内容并重试原操作。")
        } catch (cause) {
            setError(getErrorMessage(cause, "暂时无法查询改单状态，请重试。"))
        } finally {
            inFlight.current = false
        }
    }

    const form = useAppForm({
        defaultValues: purchaseChangeDraftValues(draft, options),
        validators: { onChange: schema },
        onSubmit: async ({ value }) => {
            if (unknown || mutation.isPending) return
            await submit({
                purchaseChangeOrderId: changeOrderId,
                purchaseOrderId,
                expectedLockVersion: draft.version,
                paymentTermCode: value.paymentTermCode,
                reason: value.reason.trim(),
                lines: purchaseChangeTargetLines(value),
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
    const disabled = mutation.isPending || unknown

    return (
        <form
            id={`${id}-form`}
            className="space-y-6"
            onSubmit={(event) => {
                event.preventDefault()
                event.stopPropagation()
                void form.handleSubmit()
            }}
        >
            <PurchaseChangeDraftFields
                form={form}
                id={id}
                draft={draft}
                options={options}
                disabled={disabled}
                occurredExpenseIds={occurredExpenseIds}
            />
            <SubmissionRouteConfirmation definition={approval?.definition} />
            {error ? (
                <p role="alert" className="text-sm text-destructive">
                    {error}
                </p>
            ) : null}
            {unknown ? (
                <Alert variant="warning">
                    <AlertTitle>提交结果待确认</AlertTitle>
                    <AlertDescription>
                        当前改单内容已保留。请查询改单状态或使用本次操作重试，确认前不能修改或关闭。
                    </AlertDescription>
                </Alert>
            ) : null}
            <DialogFooter className="sticky -bottom-6 z-10 -mx-6 flex-col border-t bg-popover px-6 py-4 sm:items-center sm:justify-between">
                <PurchaseChangeTotalSummary
                    form={form}
                    baseAmount={draft.base_gross_amount ?? ""}
                    paidAmount={paidAmount}
                />
                <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:gap-3">
                    {unknown ? null : (
                        <PurchaseChangeBlockers form={form} id={id} />
                    )}
                    <div className="flex flex-col-reverse gap-2 sm:flex-row">
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
                                    查询改单状态
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
                            <form.AppForm>
                                <form.SubmitButton
                                    id={`${id}-submit`}
                                    label="提交改单"
                                    pendingLabel="正在提交…"
                                    disabled={disabled}
                                />
                            </form.AppForm>
                        )}
                    </div>
                </div>
            </DialogFooter>
        </form>
    )
}
