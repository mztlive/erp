"use client"

import { toFieldErrors } from "@/components/form"
import { Field, FieldError } from "@/components/ui/field"
import type { CreateSalesOrderFormValues } from "@/features/sales-orders/lib/sales-order-create-model"
import type { SalesOrderCreateFormApi } from "@/features/sales-orders/lib/sales-order-create-form-types"
import type { SalesOrderNature } from "@/features/sales-orders/types"
import {
    SellableSkuSearchCombobox,
    VoucherCategorySearchCombobox,
} from "@/features/entity-selectors"

export type SalesOrderCreateLineSkuEditorProps = {
    form: SalesOrderCreateFormApi
    values: CreateSalesOrderFormValues
    nature: SalesOrderNature
    rowIndex: number
}

/** 明细行"销售项目"列：卡券选类目，实物/服务选公司商品池 SKU。 */
export function SalesOrderCreateLineSkuEditor({
    form,
    values,
    nature,
    rowIndex,
}: SalesOrderCreateLineSkuEditorProps) {
    const nameFieldName = `lineItems[${rowIndex}].name` as const

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
        <div className="min-w-48">
            <form.AppField name={nameFieldName}>{() => null}</form.AppField>
            <form.AppField name={`lineItems[${rowIndex}].sku`}>
                {(field) => {
                    const isInvalid =
                        field.state.meta.isTouched && !field.state.meta.isValid
                    const errors = toFieldErrors(field.state.meta.errors)
                    return (
                        <Field data-invalid={isInvalid || undefined}>
                            <SellableSkuSearchCombobox
                                value={field.state.value || undefined}
                                onValueChange={(id) => {
                                    // 公司商品池稳定身份 = sku_id
                                    field.handleChange(id ?? "")
                                }}
                                onItemChange={(product) => {
                                    form.setFieldValue(
                                        `lineItems[${rowIndex}].skuRevisionId`,
                                        product?.revisionId ?? "",
                                    )
                                    form.setFieldValue(
                                        nameFieldName,
                                        product?.name ?? "",
                                    )
                                    form.setFieldValue(
                                        `lineItems[${rowIndex}].unit`,
                                        product?.baseUnit ?? "",
                                    )
                                }}
                                excludeProductKind="VOUCHER"
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
                                              baseUnit:
                                                  values.lineItems[rowIndex]
                                                      .unit,
                                          }
                                        : undefined
                                }
                                placeholder="搜索 SKU 或商品名称"
                                emptyLabel="暂无可用的实物/服务 SKU（已排除卡券）"
                            />
                            {isInvalid ? <FieldError errors={errors} /> : null}
                        </Field>
                    )
                }}
            </form.AppField>
        </div>
    )
}
