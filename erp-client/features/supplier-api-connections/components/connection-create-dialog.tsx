"use client"

import * as React from "react"
import { z } from "zod"

import { OptionCombobox } from "@/components/business/option-combobox"
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
import { SupplierSearchCombobox } from "@/features/entity-selectors"
import {
    useConnectorTypesQuery,
    useCreateConnectionMutation,
} from "@/features/supplier-api-connections/hooks/queries"
import {
    newIdempotencyKey,
    outcomeToResult,
} from "@/features/supplier-api-connections/lib/operations"

const createSchema = z.object({
    connectionCode: z.string().trim().min(3, "请填写连接代码"),
    supplierId: z.string().trim().min(1, "请选择供应商"),
    supplierName: z.string().trim().min(2, "请选择供应商"),
    connectorType: z.string().min(1, "请选择接口类型"),
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
    const connectorTypes = useConnectorTypesQuery(open)

    const form = useAppForm({
        defaultValues: {
            connectionCode: "",
            supplierId: "",
            supplierName: "",
            connectorType: "",
        },
        validators: { onChange: createSchema },
        onSubmit: async ({ value }) => {
            const outcome = await createMutation.mutateAsync({
                connectionCode: value.connectionCode,
                supplierId: value.supplierId,
                supplierName: value.supplierName,
                connectorType: value.connectorType,
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
                                      id="supplier-api-connections-create-success-open"
                                      type="button"
                                      size="sm"
                                      onClick={() =>
                                          onOpen(outcome.connectionId!)
                                      }
                                  >
                                      打开 API 设置
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
            <DialogContent closeButtonId="supplier-api-connections-create-close">
                <DialogHeader>
                    <DialogTitle>新建连接</DialogTitle>
                    <DialogDescription>
                        选择供应商和接口类型，填写唯一的连接代码。接口类型创建后不可修改。
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
                                id="supplier-api-connections-create-connection-code"
                                label="连接代码"
                                required
                                placeholder="CONN-XXX"
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
                                    <FieldLabel htmlFor="supplier-api-connections-create-supplier">
                                        供应商
                                        <span className="text-destructive">
                                            *
                                        </span>
                                    </FieldLabel>
                                    <SupplierSearchCombobox
                                        id="supplier-api-connections-create-supplier"
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
                        name="connectorType"
                        children={(field) => (
                            <Field>
                                <FieldLabel htmlFor="supplier-api-connections-create-connector-type">
                                    接口类型
                                    <span className="text-destructive">*</span>
                                </FieldLabel>
                                <OptionCombobox
                                    id="supplier-api-connections-create-connector-type"
                                    value={field.state.value || null}
                                    onValueChange={(value) =>
                                        field.handleChange(value ?? "")
                                    }
                                    options={(connectorTypes.data ?? []).map(
                                        (option) => ({
                                            value: option.code,
                                            label: option.label,
                                        }),
                                    )}
                                    placeholder={
                                        connectorTypes.isPending
                                            ? "正在加载接口类型…"
                                            : "请选择接口类型"
                                    }
                                    disabled={
                                        connectorTypes.isPending ||
                                        connectorTypes.isError
                                    }
                                />
                                {field.state.meta.isTouched &&
                                !field.state.meta.isValid ? (
                                    <FieldError
                                        errors={toFieldErrors(
                                            field.state.meta.errors,
                                        )}
                                    />
                                ) : null}
                                {(() => {
                                    const selected = (
                                        connectorTypes.data ?? []
                                    ).find(
                                        (option) =>
                                            option.code === field.state.value,
                                    )
                                    if (!selected) return null
                                    return (
                                        <p className="text-xs text-muted-foreground">
                                            创建后即可同步商品并选品。自动下单默认关闭，可在
                                            API 设置中开启。
                                        </p>
                                    )
                                })()}
                            </Field>
                        )}
                    />
                    {connectorTypes.isError ? (
                        <div role="alert" className="text-sm text-destructive">
                            接口类型加载失败，请重试。
                            <Button
                                id="supplier-api-connections-create-types-retry"
                                type="button"
                                variant="link"
                                onClick={() => void connectorTypes.refetch()}
                            >
                                重试
                            </Button>
                        </div>
                    ) : null}
                    <DialogFooter>
                        <Button
                            id="supplier-api-connections-create-cancel"
                            type="button"
                            variant="ghost"
                            disabled={createMutation.isPending}
                            onClick={() => onOpenChange(false)}
                        >
                            取消
                        </Button>
                        <form.AppForm>
                            <form.SubmitButton
                                id="supplier-api-connections-create-submit"
                                label="创建"
                                loading={createMutation.isPending}
                                disabled={
                                    createMutation.isPending ||
                                    !connectorTypes.data?.length ||
                                    connectorTypes.isError
                                }
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
