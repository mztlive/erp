"use client"

import { z } from "zod"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { OptionCombobox } from "@/components/business"
import { compareDecimal, parseDecimal } from "@/lib/fixed-decimal"
import type {
    CatalogAvailability,
    CatalogFilterOptions,
    CatalogFilters,
} from "../../api/catalog"

function isValidAmount(value: string) {
    try {
        parseDecimal(value, { maxScale: 4 })
        return true
    } catch {
        return false
    }
}

const schema = z
    .object({
        keyword: z.string(),
        category_id: z.string(),
        brand_id: z.string(),
        price_min: z.string(),
        price_max: z.string(),
        availability: z.string(),
        selection_status: z.string(),
    })
    .superRefine((values, ctx) => {
        for (const field of ["price_min", "price_max"] as const) {
            const value = values[field].trim()
            if (value && !isValidAmount(value))
                ctx.addIssue({
                    code: "custom",
                    path: [field],
                    message: "请填写不小于 0 的金额",
                })
        }
        if (
            isValidAmount(values.price_min) &&
            isValidAmount(values.price_max) &&
            compareDecimal(values.price_min, values.price_max, 4) > 0
        )
            ctx.addIssue({
                code: "custom",
                path: ["price_max"],
                message: "最高报价不得低于最低报价",
            })
    })

export function CatalogFilterForm({
    value,
    options,
    loading,
    canReadCost,
    onApply,
}: {
    value: CatalogFilters
    options?: CatalogFilterOptions
    loading: boolean
    canReadCost: boolean
    onApply: (filters: CatalogFilters) => void
}) {
    const form = useAppForm({
        defaultValues: {
            keyword: value.keyword ?? "",
            category_id: value.category_id ?? "",
            brand_id: value.brand_id ?? "",
            price_min: value.price_min ?? "",
            price_max: value.price_max ?? "",
            availability: value.availability ?? "",
            selection_status: value.selection_status ?? "",
        },
        validators: { onSubmit: schema },
        onSubmit: ({ value: values }) =>
            onApply({
                keyword: values.keyword.trim() || undefined,
                category_id: values.category_id || undefined,
                brand_id: values.brand_id || undefined,
                price_min: canReadCost
                    ? values.price_min.trim() || undefined
                    : undefined,
                price_max: canReadCost
                    ? values.price_max.trim() || undefined
                    : undefined,
                availability:
                    (values.availability as CatalogAvailability) || undefined,
                selection_status:
                    (values.selection_status as CatalogFilters["selection_status"]) ||
                    undefined,
            }),
    })
    return (
        <form
            id="supplier-catalog-filters"
            className="grid min-w-0 gap-3 sm:grid-cols-2 xl:grid-cols-4"
            onSubmit={(event) => {
                event.preventDefault()
                void form.handleSubmit()
            }}
        >
            <form.AppField name="keyword">
                {(field) => (
                    <Field>
                        <FieldLabel htmlFor="supplier-catalog-keyword">
                            商品搜索
                        </FieldLabel>
                        <Input
                            id="supplier-catalog-keyword"
                            value={field.state.value}
                            onChange={(event) =>
                                field.handleChange(event.target.value)
                            }
                            placeholder="名称、规格或供应商订货编码"
                        />
                    </Field>
                )}
            </form.AppField>
            {(
                [
                    "category_id",
                    "brand_id",
                    "availability",
                    "selection_status",
                ] as const
            ).map((name) => {
                const choices = {
                    category_id: options?.categories ?? [],
                    brand_id: options?.brands ?? [],
                    availability: [
                        { id: "AVAILABLE", name: "可供" },
                        { id: "UNAVAILABLE", name: "暂不可供" },
                        { id: "UNKNOWN", name: "待核验" },
                        { id: "WITHDRAWN", name: "已下架" },
                    ],
                    selection_status: [
                        { id: "UNSELECTED", name: "未选品" },
                        { id: "SELECTED", name: "已选品" },
                    ],
                }[name]
                const label = {
                    category_id: "供应商分类",
                    brand_id: "品牌",
                    availability: "可供状态",
                    selection_status: "选品状态",
                }[name]
                return (
                    <form.AppField key={name} name={name}>
                        {(field) => (
                            <Field>
                                <FieldLabel
                                    htmlFor={`supplier-catalog-${name}`}
                                >
                                    {label}
                                </FieldLabel>
                                <OptionCombobox
                                    id={`supplier-catalog-${name}`}
                                    value={field.state.value || undefined}
                                    onValueChange={(next) =>
                                        field.handleChange(next ?? "")
                                    }
                                    options={choices.map((option) => ({
                                        value: option.id,
                                        label: option.name,
                                    }))}
                                    placeholder={`全部${label}`}
                                    searchPlaceholder={`搜索${label}`}
                                    emptyLabel={`暂无${label}`}
                                />
                            </Field>
                        )}
                    </form.AppField>
                )
            })}
            {(["price_min", "price_max"] as const).map((name) => (
                <form.AppField key={name} name={name}>
                    {(field) => (
                        <Field>
                            <FieldLabel htmlFor={`supplier-catalog-${name}`}>
                                {name === "price_min"
                                    ? "最低采购报价"
                                    : "最高采购报价"}
                            </FieldLabel>
                            <Input
                                id={`supplier-catalog-${name}`}
                                inputMode="decimal"
                                disabled={!canReadCost}
                                value={field.state.value}
                                onChange={(event) =>
                                    field.handleChange(event.target.value)
                                }
                                aria-invalid={
                                    field.state.meta.errors.length > 0
                                }
                                placeholder={
                                    canReadCost ? "不限" : "无采购报价查看权限"
                                }
                            />
                            <FieldError errors={field.state.meta.errors} />
                        </Field>
                    )}
                </form.AppField>
            ))}
            <div className="flex items-end gap-2">
                <Button
                    id="supplier-catalog-filter-apply"
                    type="submit"
                    disabled={loading}
                >
                    查询
                </Button>
                <Button
                    id="supplier-catalog-filter-reset"
                    type="button"
                    variant="outline"
                    onClick={() => {
                        form.reset({
                            keyword: "",
                            category_id: "",
                            brand_id: "",
                            price_min: "",
                            price_max: "",
                            availability: "",
                            selection_status: "",
                        })
                        onApply({})
                    }}
                >
                    重置
                </Button>
            </div>
        </form>
    )
}
