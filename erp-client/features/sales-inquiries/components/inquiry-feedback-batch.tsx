"use client"

import * as React from "react"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Label } from "@/components/ui/label"
import { DatePicker } from "@/components/ui/date-picker"
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
    DialogFooter,
} from "@/components/ui/dialog"

export type FeedbackBatchValues = {
    available: boolean
    due: string
    valid: string
    tax: string
    overwrite: boolean
}
export function InquiryFeedbackBatch({
    count,
    disabled,
    onApply,
}: {
    count: number
    disabled?: boolean
    onApply: (values: FeedbackBatchValues) => void
}) {
    const [open, setOpen] = React.useState(false)
    const form = useAppForm({
        defaultValues: {
            available: false,
            due: "",
            valid: "",
            tax: "",
            overwrite: false,
        },
        onSubmit: ({ value }) => {
            onApply(value)
            setOpen(false)
        },
    })
    return (
        <>
            <Button
                id="procurement-feedback-batch-open"
                type="button"
                variant="outline"
                disabled={disabled || count === 0}
                onClick={() => setOpen(true)}
            >
                批量填写{count > 0 ? `（${count}）` : ""}
            </Button>
            <Dialog open={open} onOpenChange={setOpen}>
                <DialogContent
                    id="procurement-feedback-batch-dialog"
                    closeButtonId="procurement-feedback-batch-close"
                >
                    <DialogHeader>
                        <DialogTitle>批量填写 {count} 件商品</DialogTitle>
                        <DialogDescription>
                            默认仅填写缺失内容。要修改已有结论、交期或税率，请勾选覆盖；价格和销售说明保持当前填写内容。
                        </DialogDescription>
                    </DialogHeader>
                    <form
                        className="space-y-4"
                        onSubmit={(event) => {
                            event.preventDefault()
                            event.stopPropagation()
                            void form.handleSubmit()
                        }}
                    >
                        <form.Field
                            name="available"
                            children={(field) => (
                                <div className="flex items-center gap-2">
                                    <Checkbox
                                        id="procurement-feedback-batch-available"
                                        checked={field.state.value}
                                        onCheckedChange={(value) =>
                                            field.handleChange(Boolean(value))
                                        }
                                    />
                                    <Label htmlFor="procurement-feedback-batch-available">
                                        设为可供，数量等于需求
                                    </Label>
                                </div>
                            )}
                        />
                        <form.Field
                            name="due"
                            children={(field) => (
                                <div className="space-y-2">
                                    <Label htmlFor="procurement-feedback-batch-due">
                                        承诺交付日
                                    </Label>
                                    <DatePicker
                                        id="procurement-feedback-batch-due"
                                        value={field.state.value}
                                        onValueChange={(date) =>
                                            field.handleChange(date ?? "")
                                        }
                                    />
                                </div>
                            )}
                        />
                        <form.Field
                            name="valid"
                            children={(field) => (
                                <div className="space-y-2">
                                    <Label htmlFor="procurement-feedback-batch-valid">
                                        报价有效至
                                    </Label>
                                    <DatePicker
                                        id="procurement-feedback-batch-valid"
                                        value={field.state.value}
                                        onValueChange={(date) =>
                                            field.handleChange(date ?? "")
                                        }
                                    />
                                </div>
                            )}
                        />
                        <form.AppField
                            name="tax"
                            children={(field) => (
                                <field.TextField
                                    id="procurement-feedback-batch-tax"
                                    label="税率（0.13 表示 13%）"
                                    inputMode="decimal"
                                    placeholder="留空则保留"
                                />
                            )}
                        />
                        <form.Field
                            name="overwrite"
                            children={(field) => (
                                <div className="flex items-center gap-2">
                                    <Checkbox
                                        id="procurement-feedback-batch-overwrite"
                                        checked={field.state.value}
                                        onCheckedChange={(value) =>
                                            field.handleChange(Boolean(value))
                                        }
                                    />
                                    <Label htmlFor="procurement-feedback-batch-overwrite">
                                        覆盖已有填写内容
                                    </Label>
                                </div>
                            )}
                        />
                        <DialogFooter>
                            <Button
                                id="procurement-feedback-batch-cancel"
                                type="button"
                                variant="outline"
                                onClick={() => setOpen(false)}
                            >
                                取消
                            </Button>
                            <Button
                                id="procurement-feedback-batch-apply"
                                type="submit"
                                disabled={disabled || count === 0}
                            >
                                应用到所选商品
                            </Button>
                        </DialogFooter>
                    </form>
                </DialogContent>
            </Dialog>
        </>
    )
}
