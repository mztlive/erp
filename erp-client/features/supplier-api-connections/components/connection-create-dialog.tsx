"use client"

import * as React from "react"
import { z } from "zod"

import { OptionCombobox } from "@/components/business"
import type { ResultState } from "@/components/business/feedback"
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
import { Label } from "@/components/ui/label"
import { SupplierSearchCombobox } from "@/features/entity-selectors"
import { useCreateConnectionMutation } from "@/features/supplier-api-connections/hooks/queries"
import {
    newIdempotencyKey,
    outcomeToResult,
} from "@/features/supplier-api-connections/lib/operations"

const createSchema = z.object({
    connectionCode: z.string().trim().min(3, "请填写连接代码"),
    supplierId: z.string().trim().min(1, "请选择供应商"),
    supplierName: z.string().trim().min(2, "请选择供应商"),
    environment: z.enum(["DEVELOPMENT", "STAGING", "PRODUCTION"]),
})

export function ConnectionCreateDialog({
    open,
    onOpenChange,
    onOpen,
    onResult,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    onOpen: (connectionId: string) => void
    onResult: (
        result: (ResultState & { actions?: React.ReactNode }) | null,
    ) => void
}) {
    const createMutation = useCreateConnectionMutation()

    const form = useAppForm({
        defaultValues: {
            connectionCode: "",
            supplierId: "",
            supplierName: "",
            environment: "PRODUCTION" as
                | "DEVELOPMENT"
                | "STAGING"
                | "PRODUCTION",
        },
        validators: { onChange: createSchema },
        onSubmit: async ({ value }) => {
            const outcome = await createMutation.mutateAsync({
                connectionCode: value.connectionCode,
                supplierId: value.supplierId,
                supplierName: value.supplierName,
                environment: value.environment,
                idempotencyKey: newIdempotencyKey("create"),
            })
            const mapped = outcomeToResult(outcome)
            if (outcome.status === "succeeded" && outcome.connectionId) {
                onOpenChange(false)
                form.reset()
                onResult(
                    mapped
                        ? {
                              ...mapped,
                              actions: (
                                  <Button
                                      type="button"
                                      size="sm"
                                      onClick={() =>
                                          onOpen(outcome.connectionId!)
                                      }
                                  >
                                      打开连接详情
                                  </Button>
                              ),
                          }
                        : mapped,
                )
            } else {
                onResult(mapped)
            }
        },
    })

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent>
                <DialogHeader>
                    <DialogTitle>新建连接身份</DialogTitle>
                    <DialogDescription>
                        连接代码全局唯一，不可与环境组合复用。创建成功后可在结果中打开连接详情完成配置。
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="flex flex-col gap-3"
                    onSubmit={(e) => {
                        e.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <form.AppField
                        name="connectionCode"
                        children={(field) => (
                            <field.TextField
                                label="连接代码"
                                placeholder="CONN-XXX-PROD"
                            />
                        )}
                    />
                    <form.AppField
                        name="supplierId"
                        children={(field) => {
                            const isInvalid =
                                field.state.meta.isTouched &&
                                !field.state.meta.isValid
                            const errors = toFieldErrors(
                                field.state.meta.errors,
                            )
                            return (
                                <Field data-invalid={isInvalid || undefined}>
                                    <FieldLabel htmlFor="create-supplierId">
                                        供应商
                                    </FieldLabel>
                                    <SupplierSearchCombobox
                                        value={field.state.value || undefined}
                                        onValueChange={(id) => {
                                            field.handleChange(id ?? "")
                                        }}
                                        onItemChange={(supplier) => {
                                            form.setFieldValue(
                                                "supplierName",
                                                supplier?.supplierName ?? "",
                                            )
                                        }}
                                        placeholder="搜索供应商名称或编码"
                                    />
                                    {isInvalid ? (
                                        <FieldError errors={errors} />
                                    ) : null}
                                </Field>
                            )
                        }}
                    />
                    <form.AppField
                        name="environment"
                        children={(field) => (
                            <div className="space-y-1.5">
                                <Label>环境</Label>
                                <OptionCombobox
                                    value={field.state.value}
                                    onValueChange={(v) => {
                                        if (v)
                                            field.handleChange(
                                                v as typeof field.state.value,
                                            )
                                    }}
                                    options={[
                                        {
                                            value: "PRODUCTION",
                                            label: "生产",
                                        },
                                        { value: "STAGING", label: "测试" },
                                        {
                                            value: "DEVELOPMENT",
                                            label: "开发",
                                        },
                                    ]}
                                    allowClear={false}
                                />
                                {field.state.value === "PRODUCTION" ? (
                                    <p
                                        className="text-xs text-muted-foreground"
                                        role="status"
                                    >
                                        正在创建生产环境连接身份
                                    </p>
                                ) : null}
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
                                label="创建"
                                disabled={createMutation.isPending}
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
