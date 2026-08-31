"use client"

import { SlashIcon } from "lucide-react"

import { OptionCombobox } from "@/components/business"
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
    onCancel: () => void
}

export function AdjustmentDialog({
    open,
    meta,
    form,
    onCancel,
}: AdjustmentDialogProps) {
    return (
        <Dialog
            open={open}
            onOpenChange={(nextOpen) => {
                if (!nextOpen) onCancel()
            }}
        >
            <DialogContent
                closeButtonId="inventory-adjustment-dialog-close"
                className="sm:max-w-lg"
            >
                <DialogHeader>
                    <DialogTitle>发起库存调整</DialogTitle>
                    <DialogDescription>
                        从当前余额上下文创建调整单草稿。提交后进入审批，不会立即改库存。
                    </DialogDescription>
                </DialogHeader>

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
                                <div>
                                    数据版本{" "}
                                    <span className="num text-foreground">
                                        已按最新核对
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
                            <div className="grid gap-1.5">
                                <Label htmlFor="inventory-adjustment-dialog-reason-type">
                                    原因类型
                                    <span className="text-destructive">*</span>
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
                                                field.state.value || undefined
                                            }
                                            onValueChange={(next) =>
                                                field.handleChange(next ?? "")
                                            }
                                            className="w-full"
                                        />
                                        {field.state.meta.errors[0] ? (
                                            <p
                                                className="text-xs text-destructive"
                                                role="alert"
                                            >
                                                {String(
                                                    field.state.meta.errors[0],
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

                            <div className="rounded-lg border bg-card p-3 text-xs text-muted-foreground space-y-1">
                                <div className="font-medium text-foreground">
                                    提交约束
                                </div>
                                <ul className="list-disc pl-4 space-y-0.5">
                                    <li>不会直接修改账面或可用数量</li>
                                    <li>经办与审批岗位分离，提交后进入审批</li>
                                    <li>
                                        按当前数据版本提交；若已被他人修改，将提示冲突并保留你的输入。
                                    </li>
                                </ul>
                            </div>

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
                                    />
                                </form.AppForm>
                            </DialogFooter>
                        </form>
                    </div>
                ) : null}
            </DialogContent>
        </Dialog>
    )
}
