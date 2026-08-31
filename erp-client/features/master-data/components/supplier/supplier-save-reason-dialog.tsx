"use client"

import { SaveIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogClose,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Label } from "@/components/ui/label"
import { Textarea } from "@/components/ui/textarea"
import { masterDataCopy } from "@/features/master-data/lib/copy"

export function SupplierSaveReasonDialog({
    open,
    onOpenChange,
    isCreate,
    reason,
    onReasonChange,
    reasonError,
    pending,
    onConfirm,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    isCreate: boolean
    reason: string
    onReasonChange: (reason: string) => void
    reasonError: string | null
    pending: boolean
    onConfirm: () => void
}) {
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                closeButtonId="master-data-supplier-save-reason-close"
                className="sm:max-w-md"
            >
                <DialogHeader>
                    <DialogTitle>
                        {isCreate ? "确认创建" : "确认保存"}
                    </DialogTitle>
                    <DialogDescription>
                        {isCreate
                            ? "创建后生成供应商档案；请填写创建说明。"
                            : "保存将生成新版本；变更原因必填。"}
                    </DialogDescription>
                </DialogHeader>
                <div className="space-y-1.5">
                    <Label htmlFor="supplier-save-reason">
                        {masterDataCopy.fieldChangeReason}
                        <span className="text-destructive">*</span>
                    </Label>
                    <Textarea
                        id="supplier-save-reason"
                        value={reason}
                        onChange={(event) => onReasonChange(event.target.value)}
                        rows={3}
                        placeholder={
                            isCreate
                                ? "新建原因"
                                : "说明本次修改内容，保存后形成新版本"
                        }
                    />
                    {reasonError ? (
                        <p className="text-xs text-destructive" role="alert">
                            {reasonError}
                        </p>
                    ) : null}
                </div>
                <DialogFooter>
                    <DialogClose
                        render={
                            <Button
                                id="master-data-supplier-supplier-save-reason-dialog-button-1"
                                type="button"
                                variant="outline"
                                disabled={pending}
                            />
                        }
                    >
                        取消
                    </DialogClose>
                    <Button
                        id="master-data-supplier-supplier-save-reason-dialog-button-2"
                        type="button"
                        disabled={pending}
                        onClick={onConfirm}
                    >
                        <SaveIcon data-icon="inline-start" aria-hidden />
                        {pending
                            ? "提交中…"
                            : isCreate
                              ? masterDataCopy.createSubmit
                              : masterDataCopy.reviseSubmit}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
