"use client"

import { DatePicker } from "@/components/ui/date-picker"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { DeliveryTemplateField } from "@/features/supplier-offerings/components/delivery-templates/delivery-template-field"
import type { useBatchWorkspace } from "../../hooks/use-batch-workspace"
import { BatchCategoryMappings } from "./batch-category-mappings"
import { BatchClearButton } from "./batch-clear-button"

const DATES = [
    {
        field: "validFrom",
        wire: "valid_from",
        id: "supplier-catalog-batch-valid-from",
        label: "生效日期",
    },
    {
        field: "validTo",
        wire: "valid_to",
        id: "supplier-catalog-batch-valid-to",
        label: "失效日期",
    },
] as const

export function BatchDetailsPanel({
    workspace,
    supplierId,
    categories,
}: {
    workspace: ReturnType<typeof useBatchWorkspace>
    supplierId: string
    categories: Array<{ value: string; label: string }>
}) {
    const { form, values } = workspace
    return (
        <div className="min-w-0 space-y-6">
            <BatchCategoryMappings
                workspace={workspace}
                categories={categories}
            />
            <section
                className="space-y-4 rounded-lg border p-4"
                aria-labelledby="supplier-catalog-batch-terms-title"
            >
                <h2
                    id="supplier-catalog-batch-terms-title"
                    className="text-base font-semibold"
                >
                    供给条件
                </h2>
                <div className="grid gap-5 border-t pt-4 lg:grid-cols-2">
                    <div className="space-y-4">
                        <form.AppField name="inputTaxPercentage">
                            {(field) => (
                                <Field>
                                    <div className="flex items-center justify-between gap-2">
                                        <FieldLabel htmlFor="supplier-catalog-batch-input-tax">
                                            进项税率（%）
                                        </FieldLabel>
                                        <BatchClearButton
                                            workspace={workspace}
                                            field="input_tax_rate"
                                        />
                                    </div>
                                    <Input
                                        id="supplier-catalog-batch-input-tax"
                                        inputMode="decimal"
                                        placeholder="请输入"
                                        value={field.state.value}
                                        onChange={(event) => {
                                            field.handleChange(
                                                event.target.value,
                                            )
                                            workspace.setDefaultClear(
                                                "input_tax_rate",
                                                false,
                                            )
                                        }}
                                        onBlur={field.handleBlur}
                                        aria-invalid={
                                            field.state.meta.errors.length > 0
                                        }
                                        aria-describedby={
                                            field.state.meta.errors.length > 0
                                                ? "supplier-catalog-batch-input-tax-error"
                                                : undefined
                                        }
                                    />
                                    <FieldError
                                        id="supplier-catalog-batch-input-tax-error"
                                        errors={field.state.meta.errors}
                                    />
                                </Field>
                            )}
                        </form.AppField>
                        {DATES.map((date) => (
                            <form.AppField key={date.field} name={date.field}>
                                {(field) => (
                                    <Field>
                                        <div className="flex items-center justify-between gap-2">
                                            <FieldLabel htmlFor={date.id}>
                                                {date.label}
                                            </FieldLabel>
                                            <BatchClearButton
                                                workspace={workspace}
                                                field={date.wire}
                                            />
                                        </div>
                                        <DatePicker
                                            id={date.id}
                                            value={
                                                field.state.value || undefined
                                            }
                                            onValueChange={(next) => {
                                                field.handleChange(next ?? "")
                                                field.handleBlur()
                                                workspace.setDefaultClear(
                                                    date.wire,
                                                    false,
                                                )
                                            }}
                                            clearable={false}
                                            placeholder={
                                                date.field === "validTo" &&
                                                values.clearFields.includes(
                                                    date.wire,
                                                )
                                                    ? "长期有效"
                                                    : "选择日期"
                                            }
                                            aria-invalid={
                                                field.state.meta.errors.length >
                                                0
                                            }
                                            aria-describedby={
                                                field.state.meta.errors.length >
                                                0
                                                    ? `${date.id}-error`
                                                    : undefined
                                            }
                                        />
                                        <FieldError
                                            id={`${date.id}-error`}
                                            errors={field.state.meta.errors}
                                        />
                                    </Field>
                                )}
                            </form.AppField>
                        ))}
                    </div>
                    <form.AppField name="deliveryTemplateId">
                        {(field) => (
                            <DeliveryTemplateField
                                id="supplier-catalog-batch-delivery-template"
                                supplierId={supplierId}
                                value={field.state.value}
                                labelAction={
                                    <BatchClearButton
                                        workspace={workspace}
                                        field="delivery_template_id"
                                        label="恢复默认"
                                    />
                                }
                                onChange={(next) => {
                                    field.handleChange(next)
                                    workspace.setDefaultClear(
                                        "delivery_template_id",
                                        next === "",
                                    )
                                }}
                            />
                        )}
                    </form.AppField>
                </div>
            </section>
        </div>
    )
}
