"use client"

import * as React from "react"
import { createPortal } from "react-dom"
import { z } from "zod"
import { useAppForm, withForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { DatePicker } from "@/components/ui/date-picker"
import { Label } from "@/components/ui/label"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { demandFormDefaults } from "../lib/demand-form"
import {
    inquiryDate,
    inquiryDateInput,
    inquiryDateSeconds,
} from "../lib/format"

type DescriptionValues = { deadline_at: number; note: string }

export const InquiryDemandHeader = withForm({
    defaultValues: demandFormDefaults,
    props: {
        actionsContainer: null as HTMLElement | null,
        disabled: false,
    },
    render: function DemandHeader({ form, actionsContainer, disabled }) {
        const [open, setOpen] = React.useState(false)
        const trigger = React.useRef<HTMLButtonElement>(null)
        return (
            <>
                <form.Subscribe
                    selector={(state) =>
                        [
                            state.values.deadline_at,
                            state.values.draft.note,
                        ] as const
                    }
                >
                    {([deadline, note]) => (
                        <div className="flex flex-wrap gap-x-6 gap-y-2 text-sm text-muted-foreground">
                            <span>询价截止：{inquiryDate(deadline)}</span>
                            <p className="flex min-w-0 gap-2">
                                <span className="shrink-0">需求说明</span>
                                <span className="min-w-0 whitespace-pre-wrap wrap-anywhere">
                                    {note || "未填写"}
                                </span>
                            </p>
                        </div>
                    )}
                </form.Subscribe>
                {actionsContainer &&
                    createPortal(
                        <Button
                            id="sales-inquiry-description-edit"
                            ref={trigger}
                            type="button"
                            variant="outline"
                            size="sm"
                            disabled={disabled}
                            onClick={() => setOpen(true)}
                        >
                            修改需求说明
                        </Button>,
                        actionsContainer,
                    )}
                <Dialog open={open} onOpenChange={setOpen}>
                    <DialogContent
                        closeButtonId="sales-inquiry-description-close"
                        finalFocus={trigger}
                    >
                        <DialogHeader>
                            <DialogTitle>修改需求说明</DialogTitle>
                            <DialogDescription>
                                同时修改询价截止日期和需求说明，应用后随“保存需求”一起保存。
                            </DialogDescription>
                        </DialogHeader>
                        {open && (
                            <DescriptionForm
                                initial={{
                                    deadline_at: form.state.values.deadline_at,
                                    note: form.state.values.draft.note,
                                }}
                                disabled={disabled}
                                onCancel={() => setOpen(false)}
                                onApply={(value) => {
                                    form.setFieldValue(
                                        "deadline_at",
                                        value.deadline_at,
                                    )
                                    form.setFieldValue("draft.note", value.note)
                                    setOpen(false)
                                }}
                            />
                        )}
                    </DialogContent>
                </Dialog>
            </>
        )
    },
})

function DescriptionForm({
    initial,
    disabled,
    onApply,
    onCancel,
}: {
    initial: DescriptionValues
    disabled: boolean
    onApply: (value: DescriptionValues) => void
    onCancel: () => void
}) {
    const form = useAppForm({
        defaultValues: initial,
        validators: {
            onChange: z.object({
                deadline_at: z.number().int().nonnegative(),
                note: z.string().max(4000, "需求说明最多填写 4000 字"),
            }),
        },
        onSubmit: ({ value }) => onApply(value),
    })
    return (
        <form
            className="space-y-5"
            onSubmit={(event) => {
                event.preventDefault()
                event.stopPropagation()
                if (!disabled) void form.handleSubmit()
            }}
        >
            <fieldset disabled={disabled} className="space-y-5">
                <form.Field name="deadline_at">
                    {(field) => (
                        <div className="space-y-2">
                            <Label htmlFor="sales-inquiry-description-deadline">
                                询价截止日期
                            </Label>
                            <DatePicker
                                id="sales-inquiry-description-deadline"
                                size="default"
                                value={inquiryDateInput(field.state.value)}
                                onValueChange={(value) =>
                                    field.handleChange(
                                        inquiryDateSeconds(value ?? ""),
                                    )
                                }
                            />
                        </div>
                    )}
                </form.Field>
                <form.AppField name="note">
                    {(field) => (
                        <field.TextareaField
                            id="sales-inquiry-description-note"
                            label="需求说明（选填）"
                            rows={4}
                            maxLength={4000}
                            placeholder="客户交付要求及需要采购确认的事项"
                        />
                    )}
                </form.AppField>
            </fieldset>
            <DialogFooter>
                <Button
                    id="sales-inquiry-description-cancel"
                    type="button"
                    variant="outline"
                    onClick={onCancel}
                >
                    取消
                </Button>
                <Button
                    id="sales-inquiry-description-apply"
                    type="submit"
                    disabled={disabled}
                >
                    应用修改
                </Button>
            </DialogFooter>
        </form>
    )
}
