"use client"

import * as React from "react"
import { SaveIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
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
import { RequiredMark } from "@/components/ui/required-mark"
import { Textarea } from "@/components/ui/textarea"
import { masterDataCopy } from "@/features/master-data/lib/copy"

export function SupplierSaveReasonDialog({
    open,
    onOpenChange,
    isCreate,
    reason,
    onReasonChange,
    reasonError,
    changes,
    feedback,
    pending,
    onConfirm,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    isCreate: boolean
    reason: string
    onReasonChange: (reason: string) => void
    reasonError: string | null
    changes: readonly string[]
    feedback?: React.ReactNode
    pending: boolean
    onConfirm: () => void
}) {
    return (
        <Dialog
            open={open}
            onOpenChange={(next) => {
                if (!pending) onOpenChange(next)
            }}
        >
            <DialogContent
                closeButtonId="master-data-supplier-save-reason-close"
                showCloseButton={!pending}
                className="max-h-[90dvh] overflow-y-auto sm:max-w-lg"
            >
                <DialogHeader>
                    <DialogTitle>
                        {isCreate ? "创建供应商" : "保存供应商资料"}
                    </DialogTitle>
                    <DialogDescription>
                        {isCreate
                            ? "核对新供应商资料并填写创建说明。"
                            : "核对本次修改内容，历史版本将保留。"}
                    </DialogDescription>
                </DialogHeader>
                <form
                    id="master-data-supplier-save-reason-form"
                    className="space-y-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        if (!pending) onConfirm()
                    }}
                >
                    <section
                        className="rounded-lg bg-muted/40 p-3.5"
                        aria-label="本次修改摘要"
                    >
                        <h3 className="text-body-compact font-medium">
                            {isCreate ? "新建供应商资料" : "本次修改"}
                        </h3>
                        <ul className="mt-1.5 max-h-32 space-y-1 overflow-y-auto break-words text-body-compact text-muted-foreground">
                            {(changes.length > 0
                                ? changes
                                : [
                                      isCreate
                                          ? "基本信息、商务合作、合同资质与开票信息"
                                          : "供应商资料内容未改变，将按本次原因生成新版本",
                                  ]
                            ).map((change) => (
                                <li key={change}>{change}</li>
                            ))}
                        </ul>
                    </section>
                    {feedback}
                    <div className="space-y-1.5">
                        <Label htmlFor="supplier-save-reason">
                            {isCreate
                                ? "创建说明"
                                : masterDataCopy.fieldChangeReason}
                            <RequiredMark />
                        </Label>
                        <Textarea
                            id="supplier-save-reason"
                            aria-required="true"
                            value={reason}
                            disabled={pending}
                            onChange={(event) =>
                                onReasonChange(event.target.value)
                            }
                            rows={3}
                            placeholder={
                                isCreate ? "新建原因" : "例如：更新结算信息"
                            }
                        />
                        {reasonError ? (
                            <p
                                className="text-xs text-destructive"
                                role="alert"
                            >
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
                        <LoadingButton
                            id="master-data-supplier-supplier-save-reason-dialog-button-2"
                            type="submit"
                            loading={pending}
                            disabled={pending}
                        >
                            <SaveIcon data-icon="inline-start" aria-hidden />
                            {pending
                                ? "提交中…"
                                : isCreate
                                  ? masterDataCopy.createSubmit
                                  : masterDataCopy.reviseSubmit}
                        </LoadingButton>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
