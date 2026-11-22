"use client"

import * as React from "react"
import { SaveIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Label } from "@/components/ui/label"
import { LoadingButton } from "@/components/ui/loading-button"
import { RequiredMark } from "@/components/ui/required-mark"
import { Textarea } from "@/components/ui/textarea"

/**
 * 客户资料保存原因确认弹窗：
 * 收口必填「修改原因」到点击保存后的轻量确认层，避免页底修改字段时被页顶必填框静音拦截。
 */
export function CustomerSaveReasonDialog({
    open,
    onOpenChange,
    changeCount,
    reason,
    onReasonChange,
    reasonError,
    pending,
    onConfirm,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    changeCount: number
    reason: string
    onReasonChange: (value: string) => void
    reasonError: string | null
    pending: boolean
    onConfirm: () => void
}) {
    const textareaRef = React.useRef<HTMLTextAreaElement>(null)

    React.useEffect(() => {
        if (!open) return
        const frame = requestAnimationFrame(() => {
            textareaRef.current?.focus()
        })
        return () => cancelAnimationFrame(frame)
    }, [open])

    return (
        <Dialog
            open={open}
            onOpenChange={(next) => {
                if (!pending) onOpenChange(next)
            }}
        >
            <DialogContent
                closeButtonId="customers-form-save-reason-close"
                showCloseButton={!pending}
                className="sm:max-w-md"
            >
                <DialogHeader>
                    <DialogTitle>保存客户资料</DialogTitle>
                    <DialogDescription>
                        本次修改{" "}
                        <span className="num font-medium text-foreground">
                            {changeCount}
                        </span>{" "}
                        项内容，仅对后续新业务生效，不影响已创建的合同与销售单。
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="space-y-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        if (!pending) onConfirm()
                    }}
                >
                    <div className="space-y-1.5">
                        <Label htmlFor="customers-form-change-reason">
                            修改原因
                            <RequiredMark />
                        </Label>
                        <Textarea
                            ref={textareaRef}
                            id="customers-form-change-reason"
                            aria-required="true"
                            aria-invalid={reasonError ? true : undefined}
                            value={reason}
                            disabled={pending}
                            onChange={(event) =>
                                onReasonChange(event.target.value)
                            }
                            rows={3}
                            placeholder="例如：更新默认付款条件或联系信息"
                        />
                        {reasonError ? (
                            <p
                                id="customers-form-change-reason-error"
                                role="alert"
                                className="text-xs text-destructive"
                            >
                                {reasonError}
                            </p>
                        ) : null}
                    </div>
                    <DialogFooter>
                        <Button
                            id="customers-form-save-reason-cancel"
                            type="button"
                            variant="outline"
                            disabled={pending}
                            onClick={() => onOpenChange(false)}
                        >
                            继续编辑
                        </Button>
                        <LoadingButton
                            id="customers-form-save-reason-confirm"
                            type="submit"
                            loading={pending}
                            disabled={pending}
                        >
                            <SaveIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            {pending ? "保存中…" : "确认保存"}
                        </LoadingButton>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
