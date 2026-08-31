"use client"

import { PackageSearchIcon } from "lucide-react"

import { toFieldErrors } from "@/components/form"
import { Button } from "@/components/ui/button"
import { Field, FieldError } from "@/components/ui/field"
import type { CreateSalesOrderFormValues } from "@/features/sales-orders/lib/sales-order-create-model"
import type { SalesOrderCreateFormApi } from "@/features/sales-orders/lib/sales-order-create-form-types"
import type { SalesOrderNature } from "@/features/sales-orders/types"
import { VoucherCategorySearchCombobox } from "@/features/sales-orders/components/voucher-category-search-combobox"
import { toAutomationIdSegment } from "@/lib/automation-id"

export type SalesOrderCreateLineSkuEditorProps = {
    form: SalesOrderCreateFormApi
    values: CreateSalesOrderFormValues
    nature: SalesOrderNature
    rowIndex: number
    onPickSku?: (rowIndex: number) => void
}

/** 明细行"销售项目"列：卡券选类目，实物/服务选公司商品池 SKU。 */
export function SalesOrderCreateLineSkuEditor({
    form,
    values,
    nature,
    rowIndex,
    onPickSku,
}: SalesOrderCreateLineSkuEditorProps) {
    const nameFieldName = `lineItems[${rowIndex}].name` as const
    const lineKey = values.lineItems[rowIndex]?.rowKey
    if (!lineKey) return null

    return nature === "card_voucher" ? (
        <div className="min-w-52">
            <form.AppField name={nameFieldName}>{() => null}</form.AppField>
            <form.AppField name={`lineItems[${rowIndex}].sku`}>
                {(field) => {
                    const isInvalid =
                        field.state.meta.isTouched && !field.state.meta.isValid
                    const errors = toFieldErrors(field.state.meta.errors)
                    return (
                        <Field data-invalid={isInvalid || undefined}>
                            <VoucherCategorySearchCombobox
                                id={`sales-orders-create-line-${toAutomationIdSegment(lineKey)}-category`}
                                value={field.state.value || undefined}
                                onValueChange={(id) => {
                                    // 提交 voucher_category_sku_id 用 SKU 稳定 id
                                    field.handleChange(id ?? "")
                                }}
                                onItemChange={(category) => {
                                    form.setFieldValue(
                                        `lineItems[${rowIndex}].skuRevisionId`,
                                        category?.revisionId ?? "",
                                    )
                                    form.setFieldValue(
                                        nameFieldName,
                                        category?.name ?? "",
                                    )
                                    form.setFieldValue(
                                        `lineItems[${rowIndex}].unit`,
                                        "张",
                                    )
                                }}
                                selectedItem={
                                    values.lineItems[rowIndex]?.sku
                                        ? {
                                              productId:
                                                  values.lineItems[rowIndex]
                                                      .sku,
                                              revisionId:
                                                  values.lineItems[rowIndex]
                                                      .skuRevisionId,
                                              sku: values.lineItems[rowIndex]
                                                  .sku,
                                              name:
                                                  values.lineItems[rowIndex]
                                                      .name ||
                                                  values.lineItems[rowIndex]
                                                      .sku,
                                              baseUnit: "张",
                                          }
                                        : undefined
                                }
                                placeholder="搜索卡券类目"
                                emptyLabel="暂无可用的卡券类目"
                            />
                            {isInvalid ? <FieldError errors={errors} /> : null}
                        </Field>
                    )
                }}
            </form.AppField>
        </div>
    ) : (
        <div className="min-w-52">
            <form.AppField name={nameFieldName}>{() => null}</form.AppField>
            <form.AppField name={`lineItems[${rowIndex}].sku`}>
                {(field) => {
                    const isInvalid =
                        field.state.meta.isTouched && !field.state.meta.isValid
                    const errors = toFieldErrors(field.state.meta.errors)
                    const line = values.lineItems[rowIndex]
                    const selectedLabel =
                        line?.name.trim() || line?.sku.trim() || ""
                    return (
                        <Field data-invalid={isInvalid || undefined}>
                            <Button
                                id={`sales-orders-create-line-${toAutomationIdSegment(lineKey)}-pick-sku`}
                                type="button"
                                variant="outline"
                                className="w-full justify-start"
                                aria-label={
                                    selectedLabel
                                        ? `更换销售项目 ${selectedLabel}`
                                        : "选择商品"
                                }
                                onClick={() => onPickSku?.(rowIndex)}
                            >
                                <PackageSearchIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                <span className="min-w-0 truncate">
                                    {selectedLabel || "选择商品"}
                                </span>
                            </Button>
                            {isInvalid ? <FieldError errors={errors} /> : null}
                        </Field>
                    )
                }}
            </form.AppField>
        </div>
    )
}
