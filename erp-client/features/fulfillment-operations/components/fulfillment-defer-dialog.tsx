"use client"

import { z } from "zod"

import { OptionCombobox } from "@/components/business"
import { useAppForm } from "@/components/form"
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
import type { DeferReasonCode } from "./types"
import { DEFER_REASON_LABEL } from "./types"

const deferSchema = z.object({
    reasonCode: z.enum([
        "WAITING_SUPPLIER",
        "WAITING_WAREHOUSE",
        "WAITING_PAYMENT",
        "NEED_CLARIFICATION",
        "OTHER",
    ]),
    reasonNote: z.string(),
})

export type DeferSubmitValue = {
    reasonCode: DeferReasonCode
    reasonNote: string
}

export function FulfillmentDeferDialog({
    open,
    onOpenChange,
    pending,
    onSubmit,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    pending: boolean
    onSubmit: (value: DeferSubmitValue) => Promise<void>
}) {
    const form = useAppForm({
        defaultValues: {
            reasonCode: "WAITING_WAREHOUSE" as DeferReasonCode,
            reasonNote: "",
        },
        validators: { onChange: deferSchema },
        onSubmit: async ({ value }) => {
            await onSubmit({
                reasonCode: value.reasonCode as DeferReasonCode,
                reasonNote: value.reasonNote,
            })
        },
    })

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent>
                <DialogHeader>
                    <DialogTitle>先跳过这一条</DialogTitle>
                    <DialogDescription>
                        选一个原因。跳过之后这条还会留在你的列表里，等条件好了再回来处理。
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
                            <div className="space-y-2">
                                <Label htmlFor="defer-reason">为什么跳过</Label>
                                <OptionCombobox
                                    id="defer-reason"
                                    value={field.state.value}
                                    onValueChange={(v) =>
                                        field.handleChange(
                                            (v ??
                                                field.state
                                                    .value) as DeferReasonCode,
                                        )
                                    }
                                    options={(
                                        Object.keys(
                                            DEFER_REASON_LABEL,
                                        ) as DeferReasonCode[]
                                    ).map((code) => ({
                                        value: code,
                                        label: DEFER_REASON_LABEL[code],
                                    }))}
                                    allowClear={false}
                                    aria-label="为什么跳过"
                                    placeholder="选一个原因"
                                />
                            </div>
                        )}
                    />
                    <form.AppField
                        name="reasonNote"
                        children={(field) => (
                            <div className="space-y-2">
                                <Label htmlFor="defer-note">
                                    补充说明（可选）
                                </Label>
                                <Textarea
                                    id="defer-note"
                                    value={field.state.value ?? ""}
                                    onChange={(e) =>
                                        field.handleChange(e.target.value)
                                    }
                                    rows={3}
                                />
                            </div>
                        )}
                    />
                    <DialogFooter>
                        <DialogClose
                            render={<Button type="button" variant="outline" />}
                        >
                            取消
                        </DialogClose>
                        <form.AppForm>
                            <Button type="submit" disabled={pending}>
                                确认跳过
                            </Button>
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
