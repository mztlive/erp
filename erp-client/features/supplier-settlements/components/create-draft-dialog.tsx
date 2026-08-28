"use client"

import { z } from "zod"

import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { DatePicker } from "@/components/ui/date-picker"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Label } from "@/components/ui/label"
import { SupplierSearchCombobox } from "@/features/entity-selectors"
import { useCreateDraftMutation } from "@/features/supplier-settlements/hooks/queries"
import { newKey } from "@/features/supplier-settlements/lib/operations"
import type { FormalOutcome } from "@/features/supplier-settlements/types"

const createSchema = z.object({
    supplierId: z.string().min(1, "请选择供应商"),
    periodStart: z.string().min(1, "请选择期间起"),
    periodEnd: z.string().min(1, "请选择期间止"),
})

export function CreateDraftDialog({
    open,
    onOpenChange,
    onCreated,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    onCreated: (outcome: FormalOutcome) => void
}) {
    const createMutation = useCreateDraftMutation()

    const form = useAppForm({
        defaultValues: {
            supplierId: "",
            periodStart: "",
            periodEnd: "",
        },
        validators: { onChange: createSchema },
        onSubmit: async ({ value }) => {
            const outcome = await createMutation.mutateAsync({
                supplierId: value.supplierId,
                periodStart: value.periodStart,
                periodEnd: value.periodEnd,
                requestId: newKey("req"),
                idempotencyKey: newKey("create"),
            })
            onCreated(outcome)
            if (outcome.status === "succeeded" && outcome.statementId) {
                onOpenChange(false)
                form.reset()
            }
        },
    })

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent>
                <DialogHeader>
                    <DialogTitle>新建结算草稿</DialogTitle>
                    <DialogDescription>
                        选择供应商与结算期间，创建后进入待对账。
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
                        name="supplierId"
                        children={(field) => (
                            <div className="space-y-1.5">
                                <Label htmlFor="supplierId">供应商<span className="text-destructive">*</span></Label>
                                <SupplierSearchCombobox
                                    value={field.state.value || undefined}
                                    onValueChange={(id) =>
                                        field.handleChange(id ?? "")
                                    }
                                    placeholder="请选择供应商"
                                />
                            </div>
                        )}
                    />
                    <form.AppField
                        name="periodStart"
                        children={(field) => (
                            <div className="space-y-1.5">
                                <Label htmlFor="periodStart">期间起<span className="text-destructive">*</span></Label>
                                <DatePicker
                                    className="w-full"
                                    value={field.state.value || undefined}
                                    onValueChange={(next) =>
                                        field.handleChange(next ?? "")
                                    }
                                />
                            </div>
                        )}
                    />
                    <form.AppField
                        name="periodEnd"
                        children={(field) => (
                            <div className="space-y-1.5">
                                <Label htmlFor="periodEnd">期间止<span className="text-destructive">*</span></Label>
                                <DatePicker
                                    className="w-full"
                                    value={field.state.value || undefined}
                                    onValueChange={(next) =>
                                        field.handleChange(next ?? "")
                                    }
                                />
                            </div>
                        )}
                    />
                    <DialogFooter>
                        <Button
                            type="button"
                            variant="ghost"
                            disabled={createMutation.isPending}
                            onClick={() => onOpenChange(false)}
                        >
                            取消
                        </Button>
                        <form.AppForm>
                            <form.SubmitButton
                                label="确认创建草稿"
                                disabled={createMutation.isPending}
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
