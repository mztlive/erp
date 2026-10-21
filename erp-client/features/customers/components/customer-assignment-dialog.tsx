"use client"

import type { CustomerAssignmentView } from "@/features/customers/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { RequiredMark } from "@/components/ui/required-mark"

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
import { useApplyCustomerAssignmentMutation } from "@/features/customers/hooks/queries"
import {
    customerAssignmentDefaults,
    customerAssignmentSchema,
} from "@/features/customers/lib/customer-assignment-form"
import { useOwnerOptionsQuery } from "@/hooks/use-options"
import { getErrorMessage } from "@/lib/api/errors"

/** 从统一 API 错误中提取服务端业务消息。 */
const mutationMessage = (error: unknown): string =>
    getErrorMessage(error, "归属调整失败，请重试。")

/** 增加或结束客户负责销售关系。 */
export function CustomerAssignmentDialog({
    customerId,
    assignments,
    open,
    onOpenChange,
}: {
    customerId: string
    assignments: readonly CustomerAssignmentView[]
    open: boolean
    onOpenChange: (open: boolean) => void
}) {
    const [target, setTarget] = React.useState<
        CustomerAssignmentView | undefined
    >()
    const mutation = useApplyCustomerAssignmentMutation()
    const resetMutation = mutation.reset
    const owners = useOwnerOptionsQuery()

    const form = useAppForm({
        defaultValues: customerAssignmentDefaults(),
        validators: { onChange: customerAssignmentSchema(target) },
        onSubmit: async ({ value }) => {
            await mutation.mutateAsync({
                customerId,
                action: target ? "end" : "assign",
                userId: target ? undefined : value.userId,
                role: target ? undefined : "OWNER",
                effectiveFrom: target ? undefined : value.effectiveFrom,
                assignmentId: target?.id,
                version: target?.version,
                effectiveTo: value.effectiveTo || undefined,
                changeReason: value.reason,
            })
            onOpenChange(false)
        },
    })
    const dirty = useSelector(form.store, (state) => state.isDirty)

    React.useEffect(() => {
        if (!open) return
        setTarget(undefined)
        form.reset(customerAssignmentDefaults())
        resetMutation()
    }, [form, open, resetMutation])

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
                        {target
                            ? `结束${target.userName}的负责关系`
                            : "管理负责销售"}
                    </DialogTitle>
                    <DialogDescription>
                        增加销售不影响其他负责人员。结束负责关系后保留已有业务责任，客户至少保留一名有效负责销售。
                    </DialogDescription>
                </DialogHeader>
                {!target ? (
                    <div className="space-y-2">
                        {assignments
                            .filter(
                                (item) =>
                                    item.role === "OWNER" && item.isCurrent,
                            )
                            .map((item) => (
                                <div
                                    key={item.id}
                                    className="flex items-center justify-between gap-2 text-sm"
                                >
                                    <span>
                                        {item.userName} · {item.effectiveFrom}{" "}
                                        起
                                    </span>
                                    <Button
                                        id={`customers-assignment-end-${toAutomationIdSegment(item.id)}`}
                                        type="button"
                                        variant="outline"
                                        size="sm"
                                        disabled={mutation.isPending}
                                        onClick={() => {
                                            setTarget(item)
                                            form.reset(
                                                customerAssignmentDefaults(
                                                    item,
                                                ),
                                            )
                                            resetMutation()
                                        }}
                                    >
                                        结束负责
                                    </Button>
                                </div>
                            ))}
                    </div>
                ) : (
                    <Button
                        id="customers-assignment-back-add"
                        type="button"
                        variant="outline"
                        disabled={mutation.isPending}
                        onClick={() => {
                            setTarget(undefined)
                            form.reset(customerAssignmentDefaults())
                            resetMutation()
                        }}
                    >
                        返回人员名单
                    </Button>
                )}
                <form
                    className="space-y-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    {!target ? (
                        <form.AppField
                            name="userId"
                            children={(field) => {
                                const invalid =
                                    field.state.meta.isTouched &&
                                    !field.state.meta.isValid
                                return (
                                    <Field data-invalid={invalid || undefined}>
                                        <FieldLabel>
                                            销售人员
                                            <RequiredMark />
                                        </FieldLabel>
                                        <OwnerCombobox
                                            id="customers-assignment-dialog-owner"
                                            owners={owners.data ?? []}
                                            onRefresh={() =>
                                                owners.refetch({
                                                    throwOnError: true,
                                                })
                                            }
                                            value={
                                                field.state.value || undefined
                                            }
                                            onValueChange={(value) =>
                                                field.handleChange(value ?? "")
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
                    <div className="grid gap-3 sm:grid-cols-2">
                        {!target ? (
                            <form.AppField
                                name="effectiveFrom"
                                children={(field) => (
                                    <div className="space-y-2">
                                        <Label htmlFor="customer-assignment-from">
                                            生效日期
                                            <RequiredMark />
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
                        ) : null}
                        <form.AppField
                            name="effectiveTo"
                            children={(field) => {
                                const invalid =
                                    field.state.meta.isTouched &&
                                    !field.state.meta.isValid
                                return (
                                    <Field data-invalid={invalid || undefined}>
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
                                                    field.state.meta.errors,
                                                )}
                                            />
                                        ) : null}
                                    </Field>
                                )
                            }}
                        />
                    </div>
                    <form.AppField
                        name="reason"
                        children={(field) => (
                            <field.TextareaField
                                id="customers-assignment-dialog-reason"
                                label="调整原因"
                                required
                                disabled={mutation.isPending}
                                placeholder="说明新增或结束负责的原因"
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
                                label={target ? "确认结束负责" : "添加负责销售"}
                                disabled={mutation.isPending}
                                loading={mutation.isPending}
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
