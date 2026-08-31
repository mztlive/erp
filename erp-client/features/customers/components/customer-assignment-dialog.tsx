"use client"

import * as React from "react"
import { useSelector } from "@tanstack/react-form"

import { OwnerCombobox } from "@/components/business"
import { toFieldErrors, useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { useApplyCustomerAssignmentMutation } from "@/features/customers/hooks/queries"
import {
    customerAssignmentDefaults,
    customerAssignmentSchema,
} from "@/features/customers/lib/customer-assignment-form"
import type { CustomerAssignmentView } from "@/features/customers/types"
import { useOwnerOptionsQuery } from "@/hooks/use-options"
import { getErrorMessage } from "@/lib/api/errors"

/** 从统一 API 错误中提取服务端业务消息。 */
const mutationMessage = (error: unknown): string =>
    getErrorMessage(error, "归属调整失败，请重试。")

/** 建立/换任责任归属，或结束一条当前协作归属。 */
export function CustomerAssignmentDialog({
    customerId,
    open,
    target,
    onOpenChange,
}: {
    customerId: string
    open: boolean
    target?: CustomerAssignmentView
    onOpenChange: (open: boolean) => void
}) {
    const mutation = useApplyCustomerAssignmentMutation()
    const resetMutation = mutation.reset
    const owners = useOwnerOptionsQuery()
    const ending = target != null

    const form = useAppForm({
        defaultValues: customerAssignmentDefaults(target),
        validators: { onChange: customerAssignmentSchema(target) },
        onSubmit: async ({ value }) => {
            if (target) {
                await mutation.mutateAsync({
                    customerId,
                    action: "end",
                    effectiveTo: value.effectiveTo,
                    assignmentId: target.id,
                    version: target.version,
                    changeReason: value.reason,
                })
            } else {
                await mutation.mutateAsync({
                    customerId,
                    action: "assign",
                    userId: value.userId,
                    role: value.role,
                    effectiveFrom: value.effectiveFrom,
                    effectiveTo: value.effectiveTo || undefined,
                    changeReason: value.reason,
                })
            }
            onOpenChange(false)
        },
    })
    const dirty = useSelector(form.store, (state) => state.isDirty)

    React.useEffect(() => {
        if (!open) return
        form.reset(customerAssignmentDefaults(target))
        resetMutation()
    }, [form, open, resetMutation, target])

    React.useEffect(() => {
        if (!open || !dirty) return
        const onBeforeUnload = (event: BeforeUnloadEvent) => {
            event.preventDefault()
            event.returnValue = "当前归属调整尚未提交，刷新后将丢失。"
        }
        window.addEventListener("beforeunload", onBeforeUnload)
        return () => window.removeEventListener("beforeunload", onBeforeUnload)
    }, [dirty, open])

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent closeButtonId="customers-assignment-dialog-close">
                <DialogHeader>
                    <DialogTitle>
                        {ending ? "结束协作归属" : "调整客户归属"}
                    </DialogTitle>
                    <DialogDescription>
                        {ending
                            ? "结束日期当日起不再计入协作范围，历史责任关系保留。"
                            : "换任负责人会结束重叠的旧负责人归属；新增协作不会改变负责人。"}
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="space-y-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    {ending ? (
                        <div className="rounded-lg bg-muted px-3 py-2 text-sm">
                            {target.userName} · 协作销售 ·{" "}
                            {target.effectiveFrom} 起
                        </div>
                    ) : (
                        <>
                            <form.AppField
                                name="role"
                                children={(field) => (
                                    <div className="space-y-2">
                                        <Label htmlFor="customer-assignment-role">
                                            责任角色
                                        </Label>
                                        <NativeSelect
                                            id="customer-assignment-role"
                                            className="w-full"
                                            value={field.state.value}
                                            onBlur={field.handleBlur}
                                            onChange={(event) =>
                                                field.handleChange(
                                                    event.target
                                                        .value as typeof field.state.value,
                                                )
                                            }
                                        >
                                            <NativeSelectOption value="OWNER">
                                                负责销售
                                            </NativeSelectOption>
                                            <NativeSelectOption value="COLLABORATOR">
                                                协作销售
                                            </NativeSelectOption>
                                        </NativeSelect>
                                    </div>
                                )}
                            />
                            <form.AppField
                                name="userId"
                                children={(field) => {
                                    const invalid =
                                        field.state.meta.isTouched &&
                                        !field.state.meta.isValid
                                    return (
                                        <Field
                                            data-invalid={invalid || undefined}
                                        >
                                            <FieldLabel>销售人员</FieldLabel>
                                            <OwnerCombobox
                                                id="customers-assignment-dialog-owner"
                                                owners={owners.data ?? []}
                                                value={
                                                    field.state.value ||
                                                    undefined
                                                }
                                                onValueChange={(value) =>
                                                    field.handleChange(
                                                        value ?? "",
                                                    )
                                                }
                                            />
                                            {invalid ? (
                                                <FieldError
                                                    errors={toFieldErrors(
                                                        field.state.meta.errors,
                                                    )}
                                                />
                                            ) : null}
                                        </Field>
                                    )
                                }}
                            />
                            <div className="grid gap-3 sm:grid-cols-2">
                                <form.AppField
                                    name="effectiveFrom"
                                    children={(field) => (
                                        <div className="space-y-2">
                                            <Label htmlFor="customer-assignment-from">
                                                生效日期
                                            </Label>
                                            <Input
                                                id="customer-assignment-from"
                                                type="date"
                                                value={field.state.value}
                                                onBlur={field.handleBlur}
                                                onChange={(event) =>
                                                    field.handleChange(
                                                        event.target.value,
                                                    )
                                                }
                                            />
                                        </div>
                                    )}
                                />
                                <form.AppField
                                    name="effectiveTo"
                                    children={(field) => {
                                        const invalid =
                                            field.state.meta.isTouched &&
                                            !field.state.meta.isValid
                                        return (
                                            <Field
                                                data-invalid={
                                                    invalid || undefined
                                                }
                                            >
                                                <FieldLabel htmlFor="customer-assignment-to">
                                                    结束日期（可选）
                                                </FieldLabel>
                                                <Input
                                                    id="customer-assignment-to"
                                                    type="date"
                                                    value={field.state.value}
                                                    onBlur={field.handleBlur}
                                                    onChange={(event) =>
                                                        field.handleChange(
                                                            event.target.value,
                                                        )
                                                    }
                                                />
                                                {invalid ? (
                                                    <FieldError
                                                        errors={toFieldErrors(
                                                            field.state.meta
                                                                .errors,
                                                        )}
                                                    />
                                                ) : null}
                                            </Field>
                                        )
                                    }}
                                />
                            </div>
                        </>
                    )}
                    {ending ? (
                        <form.AppField
                            name="effectiveTo"
                            children={(field) => {
                                const invalid =
                                    field.state.meta.isTouched &&
                                    !field.state.meta.isValid
                                return (
                                    <Field data-invalid={invalid || undefined}>
                                        <FieldLabel htmlFor="customer-assignment-end-date">
                                            结束日期
                                        </FieldLabel>
                                        <Input
                                            id="customer-assignment-end-date"
                                            type="date"
                                            value={field.state.value}
                                            onBlur={field.handleBlur}
                                            onChange={(event) =>
                                                field.handleChange(
                                                    event.target.value,
                                                )
                                            }
                                        />
                                        {invalid ? (
                                            <FieldError
                                                errors={toFieldErrors(
                                                    field.state.meta.errors,
                                                )}
                                            />
                                        ) : null}
                                    </Field>
                                )
                            }}
                        />
                    ) : null}
                    <form.AppField
                        name="reason"
                        children={(field) => (
                            <field.TextareaField
                                id="customers-assignment-dialog-reason"
                                label="调整原因"
                                required
                                disabled={mutation.isPending}
                                placeholder="说明换任、协作或结束原因"
                            />
                        )}
                    />
                    {mutation.isError ? (
                        <p className="text-sm text-destructive" role="alert">
                            {mutationMessage(mutation.error)}
                        </p>
                    ) : null}
                    <DialogFooter>
                        <Button
                            id="customers-assignment-dialog-cancel"
                            type="button"
                            variant="outline"
                            onClick={() => onOpenChange(false)}
                            disabled={mutation.isPending}
                        >
                            取消
                        </Button>
                        <form.AppForm>
                            <form.SubmitButton
                                id="customers-assignment-dialog-submit"
                                label={ending ? "确认结束" : "确认调整"}
                                disabled={mutation.isPending}
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
