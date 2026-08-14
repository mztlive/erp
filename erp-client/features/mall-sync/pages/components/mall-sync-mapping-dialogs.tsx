"use client"

import { OptionCombobox } from "@/components/business"
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
import { SOURCE_FIX_REASON_OPTIONS } from "@/features/mall-sync/types"
import type {
    MallSyncReleaseFormApi,
    MallSyncSourceFixFormApi,
} from "@/features/mall-sync/pages/hooks/use-mall-sync-page"

type MallSyncSourceFixDialogProps = {
    open: boolean
    onOpenChange: (open: boolean) => void
    form: MallSyncSourceFixFormApi
}

export function MallSyncSourceFixDialog({
    open,
    onOpenChange,
    form,
}: MallSyncSourceFixDialogProps) {
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent>
                <DialogHeader>
                    <DialogTitle>请求来源修复</DialogTitle>
                    <DialogDescription>
                        只向当前映射追加说明和证据要求；任务保持待处理，不创建新的协同任务。
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="space-y-3"
                    onSubmit={(e) => {
                        e.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <form.AppField
                        name="reasonCode"
                        children={(field) => (
                            <div className="space-y-1.5">
                                <Label>原因</Label>
                                <OptionCombobox
                                    value={field.state.value}
                                    onValueChange={(v) => {
                                        if (v)
                                            field.handleChange(
                                                v as
                                                    | "SOURCE_FIELD_MISSING"
                                                    | "SOURCE_FIELD_CONFLICT"
                                                    | "SOURCE_EVIDENCE_REQUIRED"
                                                    | "OTHER",
                                            )
                                    }}
                                    options={SOURCE_FIX_REASON_OPTIONS.map(
                                        (o) => ({
                                            value: o.value,
                                            label: o.label,
                                        }),
                                    )}
                                    allowClear={false}
                                />
                            </div>
                        )}
                    />
                    <form.AppField
                        name="note"
                        children={(field) => (
                            <field.TextareaField label="修复说明" />
                        )}
                    />
                    <form.AppField
                        name="requestedEvidence"
                        children={(field) => (
                            <field.TextareaField
                                label="需要补充的来源证据"
                                placeholder="多项可用逗号或换行分隔"
                            />
                        )}
                    />
                    <DialogFooter>
                        <DialogClose
                            render={
                                <Button type="button" variant="outline" />
                            }
                        >
                            取消
                        </DialogClose>
                        <form.AppForm>
                            <form.SubmitButton label="记录修复要求" />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}

type MallSyncReleaseDialogProps = {
    open: boolean
    onOpenChange: (open: boolean) => void
    form: MallSyncReleaseFormApi
}

export function MallSyncReleaseDialog({
    open,
    onOpenChange,
    form,
}: MallSyncReleaseDialogProps) {
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent>
                <DialogHeader>
                    <DialogTitle>退回团队</DialogTitle>
                    <DialogDescription>
                        清除当前个人责任，原映射任务保持待处理，不改变映射状态。
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="space-y-3"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <form.AppField
                        name="reason"
                        children={(field) => (
                            <field.TextareaField label="退回原因" />
                        )}
                    />
                    <DialogFooter>
                        <DialogClose
                            render={
                                <Button type="button" variant="outline" />
                            }
                        >
                            取消
                        </DialogClose>
                        <form.AppForm>
                            <form.SubmitButton label="确认退回团队" />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
