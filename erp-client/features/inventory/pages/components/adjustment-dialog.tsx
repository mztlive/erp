"use client"

import { SlashIcon } from "lucide-react"

import { OptionCombobox, QuantityValue } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { DateTimeLocalPicker } from "@/components/ui/date-picker"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Label } from "@/components/ui/label"
import { AdjustmentApprovalArea } from "@/features/inventory/components/adjustment-approval-area"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { REASON_TYPE_OPTIONS } from "@/features/inventory/types"
import type { AdjustmentReasonType } from "@/features/inventory/types"
import type {
    AdjustmentFormApi,
    AdjustmentMeta,
} from "../hooks/use-adjustment-workflow"

interface AdjustmentDialogProps {
    open: boolean
    meta: AdjustmentMeta | null
    form: AdjustmentFormApi
    pending: boolean
    unresolved: boolean
    feedback?: React.ReactNode
    onCancel: () => void
}

export function AdjustmentDialog({
    open,
    meta,
    form,
    onCancel,
    pending,
    unresolved,
    feedback,
}: AdjustmentDialogProps) {
    return (
        <Dialog
            open={open}
            onOpenChange={(nextOpen) => {
                if (!nextOpen && !pending && !unresolved) onCancel()
            }}
        >
            <DialogContent
                closeButtonId="inventory-adjustment-dialog-close"
                showCloseButton={!pending && !unresolved}
                className="max-h-[90dvh] overflow-y-auto sm:max-w-lg"
            >
                <DialogHeader>
                    <DialogTitle>发起库存调整</DialogTitle>
                    <DialogDescription>审批通过后调整库存。</DialogDescription>
                </DialogHeader>

                {feedback}
                {meta ? (
                    <div className="space-y-4">
                        <div className="rounded-xl border bg-muted/40 p-3 text-sm">
                            <div className="font-medium">
                                {meta.warehouseName}
                                <span className="num ml-2 text-muted-foreground">
                                    {meta.skuCode}
                                </span>
                            </div>
                            <div>{meta.skuName}</div>
                            <div className="mt-2 grid grid-cols-2 gap-2 text-xs text-muted-foreground">
                                <div>
                                    账面现存{" "}
                                    <span className="num text-foreground">
                                        {meta.onHand} {meta.baseUnit}
                                    </span>
                                </div>
                                <div>
                                    可用{" "}
                                    <span className="num text-foreground">
                                        {meta.available} {meta.baseUnit}
                                    </span>
                                </div>
                                <div>
                                    草稿号{" "}
                                    <span className="num text-foreground">
                                        {meta.adjustmentNo}
                                    </span>
                                </div>
                            </div>
                        </div>

                        <Alert>
                            <SlashIcon className="size-4" aria-hidden />
                            <AlertTitle>岗位分离</AlertTitle>
                            <AlertDescription className="text-xs">
                                {meta.segregationNote}
                            </AlertDescription>
                        </Alert>

                        <form
                            className="space-y-3"
                            onSubmit={(e) => {
                                e.preventDefault()
                                void form.handleSubmit()
                            }}
                        >
                            <fieldset
                                disabled={pending || unresolved}
                                className="space-y-3"
                            >
                                <div className="grid gap-1.5">
                                    <Label htmlFor="inventory-adjustment-dialog-reason-type">
                                        原因类型
                                        <span className="text-destructive">
                                            *
                                        </span>
                                    </Label>
                                    <form.AppField
                                        name="reasonType"
                                        children={(field) => (
                                            <OptionCombobox
                                                id="inventory-adjustment-dialog-reason-type"
                                                value={field.state.value}
                                                onValueChange={(v) => {
                                                    field.handleChange(
                                                        (v ??
                                                            field.state
                                                                .value) as AdjustmentReasonType,
                                                    )
                                                }}
                                                options={REASON_TYPE_OPTIONS.map(
                                                    (opt) => ({
                                                        value: opt.value,
                                                        label: `${opt.label}（${
                                                            opt.direction ===
                                                            "increase"
                                                                ? "增加"
                                                                : "减少"
                                                        }）`,
                                                    }),
                                                )}
                                                className="w-full"
                                                allowClear={false}
                                                aria-label="原因类型"
                                                placeholder="原因类型"
                                            />
                                        )}
                                    />
                                </div>

                                <form.AppField
                                    name="quantity"
                                    children={(field) => (
                                        <field.TextField
                                            id="inventory-adjustment-dialog-quantity"
                                            label={`调整数量（${meta.baseUnit}，正数）`}
                                            required
                                        />
                                    )}
                                />

                                <form.AppField
                                    name="occurredAt"
                                    children={(field) => (
                                        <div className="space-y-1.5">
                                            <Label htmlFor="inventory-adjustment-dialog-occurred-at">
                                                业务发生时间
                                                <span className="text-destructive">
                                                    *
                                                </span>
                                            </Label>
                                            <DateTimeLocalPicker
                                                id="inventory-adjustment-dialog-occurred-at"
                                                value={
                                                    field.state.value ||
                                                    undefined
                                                }
                                                onValueChange={(next) =>
                                                    field.handleChange(
                                                        next ?? "",
                                                    )
                                                }
                                                className="w-full"
                                            />
                                            {field.state.meta.errors[0] ? (
                                                <p
                                                    className="text-xs text-destructive"
                                                    role="alert"
                                                >
                                                    {String(
                                                        field.state.meta
                                                            .errors[0],
                                                    )}
                                                </p>
                                            ) : null}
                                        </div>
                                    )}
                                />

                                <form.AppField
                                    name="note"
                                    children={(field) => (
                                        <field.TextareaField
                                            id="inventory-adjustment-dialog-note"
                                            label="原因说明"
                                            required
                                        />
                                    )}
                                />

                                <AdjustmentApprovalArea
                                    id={`inventory-adjustment-dialog-approval-bar-${toAutomationIdSegment(meta.stockAdjustmentId)}`}
                                    phase="draft"
                                    approval={meta.approval}
                                    documentId={meta.stockAdjustmentId}
                                />

                                <form.Subscribe
                                    selector={(state) => state.values}
                                >
                                    {(value) => (
                                        <p className="text-sm font-medium">
                                            本次
                                            {REASON_TYPE_OPTIONS.find(
                                                (reason) =>
                                                    reason.value ===
                                                    value.reasonType,
                                            )?.direction === "increase"
                                                ? "增加"
                                                : "减少"}{" "}
                                            <QuantityValue
                                                value={value.quantity || "0"}
                                                unit={meta.baseUnit}
                                            />
                                        </p>
                                    )}
                                </form.Subscribe>
                                <DialogFooter className="gap-2 sm:justify-between">
                                    <Button
                                        id="inventory-adjustment-dialog-cancel"
                                        type="button"
                                        variant="outline"
                                        onClick={onCancel}
                                    >
                                        取消
                                    </Button>
                                    <form.AppForm>
                                        <form.SubmitButton
                                            id="inventory-adjustment-dialog-submit"
                                            label="提交审批"
                                            disabled={
                                                !meta.approval?.submitCommand ||
                                                !meta.approval.allowedActions.includes(
                                                    "SUBMIT",
                                                )
                                            }
                                        />
                                    </form.AppForm>
                                </DialogFooter>
                            </fieldset>
                        </form>
                    </div>
                ) : null}
            </DialogContent>
        </Dialog>
    )
}
