"use client"

import * as React from "react"
import { SaveIcon } from "lucide-react"

import { detailWorkspaceStyles } from "@/components/business"
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
import { toAutomationIdSegment } from "@/lib/automation-id"

export const QUICK_CHANGE_REASONS = [
    "更新联系方式",
    "补录信用代码",
    "调整付款条件",
    "维护收货地址",
    "更新银行账户",
] as const

/**
 * 客户资料保存原因确认弹窗：
 * 当用户从顶部/悬浮保存条直接保存且尚未填写底部变更说明时弹出，避免长页面来回滚动。
 */
export function CustomerSaveReasonDialog({
    open,
    onOpenChange,
    changeCount,
    changedLabels = [],
    reason,
    onReasonChange,
    reasonError,
    pending,
    onConfirm,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    changeCount: number
    changedLabels?: readonly string[]
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
                        项内容
                        {changedLabels.length > 0
                            ? `（涉及：${changedLabels.join("、")}）`
                            : ""}
                        ，仅对后续新业务生效，不影响已创建的合同与销售单。
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="space-y-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        if (!pending) onConfirm()
                    }}
                >
                    <div className="space-y-2">
                        <Label htmlFor="customers-form-save-reason-input">
                            修改原因
                            <RequiredMark />
                        </Label>
                        <Textarea
                            ref={textareaRef}
                            id="customers-form-save-reason-input"
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
                        <div className="flex flex-wrap gap-1.5">
                            {QUICK_CHANGE_REASONS.map((preset) => (
                                <Button
                                    key={preset}
                                    id={`customers-form-save-reason-preset-${toAutomationIdSegment(preset)}`}
                                    type="button"
                                    size="xs"
                                    variant="ghost"
                                    className={
                                        detailWorkspaceStyles.subtleButton
                                    }
                                    disabled={pending}
                                    onClick={() => onReasonChange(preset)}
                                >
                                    {preset}
                                </Button>
                            ))}
                        </div>
                        {reasonError ? (
                            <p
                                id="customers-form-save-reason-error"
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
