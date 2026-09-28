"use client"

import { OptionCombobox } from "@/components/business"
import type { ProductComboboxItem } from "@/components/business/entity-comboboxes"
import { useFieldContext } from "@/components/form/form-context"
import { toFieldErrors } from "@/components/form/utils"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import { CompanySkuSearchCombobox } from "@/features/entity-selectors"
import type { FixedSku } from "@/features/supplier-offerings/types"

/** 商品入口仅选择当前商品的规格；供给中心入口支持搜索全部授权 SKU。 */
export function SupplySkuField({
    id,
    options,
    onItemChange,
}: {
    id: string
    options?: readonly FixedSku[]
    onItemChange: (item: ProductComboboxItem | undefined) => void
}) {
    const field = useFieldContext<string>()
    const invalid = field.state.meta.isTouched && !field.state.meta.isValid
    const common = {
        id,
        value: field.state.value || undefined,
        allowClear: false,
        required: true,
        "aria-invalid": invalid,
        "aria-describedby": invalid ? `${id}-error` : undefined,
        onValueChange: (value: string | null | undefined) =>
            field.handleChange(value ?? ""),
        onBlur: field.handleBlur,
    }
    return (
        <Field className="min-w-0" data-invalid={invalid || undefined}>
            <FieldLabel htmlFor={id}>
                商品规格<span className="text-destructive">*</span>
            </FieldLabel>
            {options ? (
                <OptionCombobox
                    {...common}
                    aria-label="商品规格"
                    placeholder="请选择当前商品的规格"
                    options={options.map((sku) => ({
                        value: sku.skuId,
                        label: `${sku.skuName} · ${sku.specification} · ${sku.skuCode}`,
                    }))}
                />
            ) : (
                <CompanySkuSearchCombobox
                    {...common}
                    label="商品规格"
                    placeholder="搜索商品名称或 SKU 编号"
                    onItemChange={onItemChange}
                    className="w-full"
                />
            )}
            {invalid && (
                <FieldError
                    id={`${id}-error`}
                    errors={toFieldErrors(field.state.meta.errors)}
                />
            )}
        </Field>
    )
}
