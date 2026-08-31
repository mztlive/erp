"use client"

import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { InputGroup, InputGroupInput } from "@/components/ui/input-group"
import { Label } from "@/components/ui/label"
import { Spinner } from "@/components/ui/spinner"
import { Textarea } from "@/components/ui/textarea"
import type { ReverseTarget } from "@/features/supplier-payables/types"

export interface ReverseDialogProps {
    target: ReverseTarget
    reason: string
    onReasonChange: (value: string) => void
    redInvoiceNo: string
    onRedInvoiceNoChange: (value: string) => void
    submitting: boolean
    onCancel: () => void
    onSubmit: () => void
}

export function ReverseDialog({
    target,
    reason,
    onReasonChange,
    redInvoiceNo,
    onRedInvoiceNoChange,
    submitting,
    onCancel,
    onSubmit,
}: ReverseDialogProps) {
    return (
        <Dialog
            open
            onOpenChange={() => {
                onCancel()
            }}
        >
            <DialogContent closeButtonId="supplier-payables-reverse-dialog-close">
                <DialogHeader>
                    <DialogTitle>
                        {target.kind === "payment" ? "付款冲正" : "进项红票"}
                    </DialogTitle>
                    <DialogDescription>
                        原单 {target.no} 将保留；请填写业务原因。
                    </DialogDescription>
                </DialogHeader>
                <div className="space-y-3">
                    <div className="space-y-1">
                        <Label htmlFor="supplier-payables-reverse-dialog-reason">
                            原因
                        </Label>
                        <Textarea
                            id="supplier-payables-reverse-dialog-reason"
                            value={reason}
                            onChange={(e) => onReasonChange(e.target.value)}
                            placeholder="至少 2 个字"
                        />
                    </div>
                    {target.kind === "invoice" ? (
                        <div className="space-y-1">
                            <Label htmlFor="supplier-payables-reverse-dialog-red-invoice-no">
                                红票号码
                            </Label>
                            <InputGroup>
                                <InputGroupInput
                                    id="supplier-payables-reverse-dialog-red-invoice-no"
                                    value={redInvoiceNo}
                                    onChange={(e) =>
                                        onRedInvoiceNoChange(e.target.value)
                                    }
                                />
                            </InputGroup>
                            {!redInvoiceNo.trim() ? (
                                <p
                                    className="text-xs text-destructive"
                                    role="alert"
                                >
                                    红票号码必填；红票将作为独立记录登记。
                                </p>
                            ) : null}
                        </div>
                    ) : null}
                </div>
                <DialogFooter>
                    <Button
                        id="supplier-payables-reverse-dialog-cancel"
                        type="button"
                        variant="outline"
                        disabled={submitting}
                        onClick={onCancel}
                    >
                        取消
                    </Button>
                    <Button
                        id="supplier-payables-reverse-dialog-confirm"
                        type="button"
                        disabled={
                            reason.trim().length < 2 ||
                            (target.kind === "invoice" &&
                                !redInvoiceNo.trim()) ||
                            submitting
                        }
                        onClick={onSubmit}
                    >
                        {submitting ? (
                            <Spinner
                                className="size-4 animate-spin"
                                aria-hidden="true"
                            />
                        ) : null}
                        {submitting ? "提交中…" : "确认追加反向记录"}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
