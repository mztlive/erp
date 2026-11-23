"use client"

import { PackageSearchIcon } from "lucide-react"

import { toFieldErrors } from "@/components/form"
import { Button } from "@/components/ui/button"
import { Field, FieldError } from "@/components/ui/field"
import { SkuThumbnail } from "@/features/sales-orders/components/sku-thumbnail"
import { VoucherCategorySearchCombobox } from "@/features/sales-orders/components/voucher-category-search-combobox"
import type { SalesOrderCreateFormApi } from "@/features/sales-orders/lib/sales-order-create-form-types"
import type { CreateSalesOrderFormValues } from "@/features/sales-orders/lib/sales-order-create-model"
import type { SalesOrderNature } from "@/features/sales-orders/types"
import { toAutomationIdSegment } from "@/lib/automation-id"

export type SalesOrderCreateLineSkuEditorProps = {
    form: SalesOrderCreateFormApi
    values: CreateSalesOrderFormValues
    nature: SalesOrderNature
    rowIndex: number
    onPickSku?: (rowIndex: number) => void
    imageAssetId?: string
}

/** 明细行“销售项目”列：卡券选类目，实物/服务选公司商品池 SKU。 */
export function SalesOrderCreateLineSkuEditor({
    form,
    values,
    nature,
    rowIndex,
    onPickSku,
    imageAssetId,
}: SalesOrderCreateLineSkuEditorProps) {
    const nameFieldName = `lineItems[${rowIndex}].name` as const
    const lineKey = values.lineItems[rowIndex]?.rowKey
    if (!lineKey) return null

    return nature === "card_voucher" ? (
        <div className="min-w-52 max-w-sm">
            <form.AppField name={nameFieldName}>{() => null}</form.AppField>
            <form.AppField name={`lineItems[${rowIndex}].sku`}>
                {(field) => {
                    const isInvalid =
                        field.state.meta.isTouched && !field.state.meta.isValid
                    const errors = toFieldErrors(field.state.meta.errors)
                    return (
                        <Field
                            className="gap-0"
                            data-invalid={isInvalid || undefined}
                        >
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
        <div className="min-w-48 max-w-xs">
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
                        <Field
                            className="gap-0"
                            data-invalid={isInvalid || undefined}
                        >
                            <Button
                                id={`sales-orders-create-line-${toAutomationIdSegment(lineKey)}-pick-sku`}
                                type="button"
                                variant="ghost"
                                // 商品名与规格可换行，避免标准按钮的固定高度覆盖内容高度。
                                size={null}
                                className="group/sku -mx-1.5 -my-1 h-auto min-h-control w-[calc(100%+0.75rem)] justify-start gap-2.5 rounded-md px-1.5 py-1 text-left font-normal hover:bg-muted/60"
                                aria-label={
                                    selectedLabel
                                        ? `更换销售项目 ${selectedLabel}`
                                        : "选择商品"
                                }
                                onClick={() => onPickSku?.(rowIndex)}
                            >
                                {line?.sku ? (
                                    <SkuThumbnail
                                        assetId={imageAssetId}
                                        label={selectedLabel}
                                        className="size-9 rounded-md"
                                    />
                                ) : (
                                    <PackageSearchIcon
                                        data-icon="inline-start"
                                        className="text-muted-foreground"
                                        aria-hidden="true"
                                    />
                                )}
                                <span className="min-w-0 flex-1 whitespace-normal break-words">
                                    <span
                                        className={
                                            selectedLabel
                                                ? "block font-medium text-foreground"
                                                : "block text-muted-foreground"
                                        }
                                    >
                                        {selectedLabel || "选择商品"}
                                    </span>
                                    {line?.specification ? (
                                        <span className="mt-0.5 block text-xs text-muted-foreground">
                                            {line.specification}
                                        </span>
                                    ) : null}
                                </span>
                                {selectedLabel ? (
                                    <span className="ml-auto shrink-0 text-xs text-muted-foreground/75 transition-colors group-hover/sku:text-primary group-focus-visible/sku:text-primary">
                                        更换
                                    </span>
                                ) : null}
                            </Button>
                            {isInvalid ? <FieldError errors={errors} /> : null}
                        </Field>
                    )
                }}
            </form.AppField>
        </div>
    )
}
