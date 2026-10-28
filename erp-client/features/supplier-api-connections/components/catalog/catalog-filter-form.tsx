"use client"

import { z } from "zod"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { NativeCheckbox } from "@/components/ui/checkbox"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { MultiOptionCombobox } from "@/components/business/multi-option-combobox"
import { OptionCombobox } from "@/components/business"
import { compareDecimal, parseDecimal } from "@/lib/fixed-decimal"
import { toAutomationIdSegment } from "@/lib/automation-id"
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

const emptyValues = {
    keyword: "",
    category_ids: [] as string[],
    brand_ids: [] as string[],
    include_descendants: false,
    only_unadopted: false,
    price_min: "",
    price_max: "",
    availability: "",
    selection_status: "",
}

const schema = z
    .object({
        keyword: z.string(),
        category_ids: z.array(z.string()),
        brand_ids: z.array(z.string()),
        include_descendants: z.boolean(),
        only_unadopted: z.boolean(),
        price_min: z.string(),
        price_max: z.string(),
        availability: z.string(),
        selection_status: z.string(),
    })
    .superRefine((values, ctx) => {
        if (values.only_unadopted && values.selection_status === "SELECTED")
            ctx.addIssue({
                code: "custom",
                path: ["selection_status"],
                message: "仅处理未入库项时，请选择未入库或全部选品状态",
            })
        for (const field of ["price_min", "price_max"] as const) {
            const value = values[field].trim()
            if (value && !isValidAmount(value))
                ctx.addIssue({
                    code: "custom",
                    path: [field],
                    message: "请填写不小于 0、最多四位小数的金额",
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
            ...emptyValues,
            keyword: value.keyword ?? "",
            category_ids:
                value.category_ids ??
                (value.category_id ? [value.category_id] : []),
            brand_ids:
                value.brand_ids ?? (value.brand_id ? [value.brand_id] : []),
            price_min: value.price_min ?? "",
            price_max: value.price_max ?? "",
            availability: value.availability ?? "",
            selection_status: value.selection_status ?? "",
            include_descendants: value.include_descendants ?? false,
            only_unadopted: value.only_unadopted ?? false,
        },
        validators: { onSubmit: schema },
        onSubmit: ({ value: values }) =>
            onApply({
                keyword: values.keyword.trim() || undefined,
                category_ids: values.category_ids.length
                    ? values.category_ids
                    : undefined,
                brand_ids: values.brand_ids.length
                    ? values.brand_ids
                    : undefined,
                include_descendants: Boolean(
                    options?.category_hierarchy_supported &&
                    values.include_descendants,
                ),
                only_unadopted: values.only_unadopted,
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
    const hierarchySupported = options?.category_hierarchy_supported === true
    const applied = [
        ...(value.category_ids ?? []).map((id) => ({
            key: `category-${id}`,
            label:
                options?.categories.find((option) => option.id === id)?.name ??
                "已选分类",
            remove: () =>
                onApply({
                    ...value,
                    category_ids: value.category_ids?.filter(
                        (current) => current !== id,
                    ),
                }),
        })),
        ...(value.brand_ids ?? []).map((id) => ({
            key: `brand-${id}`,
            label:
                options?.brands.find((option) => option.id === id)?.name ??
                "已选品牌",
            remove: () =>
                onApply({
                    ...value,
                    brand_ids: value.brand_ids?.filter(
                        (current) => current !== id,
                    ),
                }),
        })),
        ...(value.keyword
            ? [
                  {
                      key: "keyword",
                      label: `关键词：${value.keyword}`,
                      remove: () => onApply({ ...value, keyword: undefined }),
                  },
              ]
            : []),
        ...(value.price_min || value.price_max
            ? [
                  {
                      key: "price",
                      label: `报价 ${value.price_min ?? "不限"} 至 ${value.price_max ?? "不限"} 元`,
                      remove: () =>
                          onApply({
                              ...value,
                              price_min: undefined,
                              price_max: undefined,
                          }),
                  },
              ]
            : []),
        ...(value.availability
            ? [
                  {
                      key: "availability",
                      label: {
                          AVAILABLE: "可供",
                          UNAVAILABLE: "暂不可供",
                          UNKNOWN: "待核验",
                          WITHDRAWN: "已下架",
                      }[value.availability],
                      remove: () =>
                          onApply({ ...value, availability: undefined }),
                  },
              ]
            : []),
        ...(value.selection_status && value.selection_status !== "ALL"
            ? [
                  {
                      key: "adoption",
                      label:
                          value.selection_status === "SELECTED"
                              ? "已入库"
                              : "未入库",
                      remove: () =>
                          onApply({ ...value, selection_status: undefined }),
                  },
              ]
            : []),
        ...(value.only_unadopted
            ? [
                  {
                      key: "unadopted",
                      label: "仅处理未入库项",
                      remove: () =>
                          onApply({ ...value, only_unadopted: false }),
                  },
              ]
            : []),
    ]
    return (
        <form
            id="supplier-catalog-filters"
            className="grid min-w-0 gap-4 sm:grid-cols-2 xl:grid-cols-4"
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
            {(["category_ids", "brand_ids"] as const).map((name) => (
                <form.AppField key={name} name={name}>
                    {(field) => {
                        const label =
                            name === "category_ids" ? "供应商分类" : "品牌"
                        const choices =
                            name === "category_ids"
                                ? options?.categories
                                : options?.brands
                        return (
                            <Field>
                                <FieldLabel
                                    htmlFor={`supplier-catalog-${name}`}
                                >
                                    {label}
                                </FieldLabel>
                                <MultiOptionCombobox
                                    id={`supplier-catalog-${name}`}
                                    value={field.state.value}
                                    onValueChange={field.handleChange}
                                    options={(choices ?? []).map((option) => ({
                                        value: option.id,
                                        label: option.name,
                                    }))}
                                    placeholder={`全部${label}`}
                                    filterLabel={label}
                                    emptyLabel={`暂无${label}`}
                                    loading={loading && !options}
                                />
                            </Field>
                        )
                    }}
                </form.AppField>
            ))}
            <form.AppField name="include_descendants">
                {(field) => (
                    <Field className="justify-end">
                        <label
                            htmlFor="supplier-catalog-batch-include-descendants"
                            className="flex items-center gap-2 text-sm"
                        >
                            <NativeCheckbox
                                id="supplier-catalog-batch-include-descendants"
                                checked={
                                    hierarchySupported && field.state.value
                                }
                                onCheckedChange={field.handleChange}
                                disabled={!hierarchySupported}
                                aria-describedby="supplier-catalog-batch-hierarchy-reason"
                            />
                            包含子分类
                        </label>
                        {!hierarchySupported ? (
                            <p
                                id="supplier-catalog-batch-hierarchy-reason"
                                className="text-xs text-muted-foreground"
                            >
                                {options?.category_hierarchy_reason ||
                                    "来源未提供可靠分类层级，仅匹配所选分类。"}
                            </p>
                        ) : null}
                    </Field>
                )}
            </form.AppField>
            <Field>
                <FieldLabel htmlFor="supplier-catalog-price_min">
                    接口采购报价（元，含上下边界）
                </FieldLabel>
                <div className="flex min-w-0 items-start gap-2">
                    {(["price_min", "price_max"] as const).map((name) => (
                        <form.AppField key={name} name={name}>
                            {(field) => (
                                <div className="min-w-0 flex-1">
                                    <Input
                                        id={`supplier-catalog-${name}`}
                                        aria-label={
                                            name === "price_min"
                                                ? "最低采购报价"
                                                : "最高采购报价"
                                        }
                                        inputMode="decimal"
                                        disabled={!canReadCost}
                                        value={field.state.value}
                                        onChange={(event) =>
                                            field.handleChange(
                                                event.target.value,
                                            )
                                        }
                                        aria-invalid={
                                            field.state.meta.errors.length > 0
                                        }
                                        placeholder={
                                            canReadCost
                                                ? name === "price_min"
                                                    ? "最低，不限"
                                                    : "最高，不限"
                                                : "无查看权限"
                                        }
                                    />
                                    <FieldError
                                        errors={field.state.meta.errors}
                                    />
                                </div>
                            )}
                        </form.AppField>
                    ))}
                </div>
            </Field>
            {(["availability", "selection_status"] as const).map((name) => {
                const choices =
                    name === "availability"
                        ? [
                              { value: "AVAILABLE", label: "可供" },
                              { value: "UNAVAILABLE", label: "暂不可供" },
                              { value: "UNKNOWN", label: "待核验" },
                              { value: "WITHDRAWN", label: "已下架" },
                          ]
                        : [
                              { value: "UNSELECTED", label: "未入库" },
                              { value: "SELECTED", label: "已入库" },
                          ]
                const label = name === "availability" ? "可供状态" : "选品状态"
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
                                    options={choices}
                                    placeholder={`全部${label}`}
                                    searchPlaceholder={`搜索${label}`}
                                    emptyLabel={`暂无${label}`}
                                />
                                <FieldError errors={field.state.meta.errors} />
                            </Field>
                        )}
                    </form.AppField>
                )
            })}
            <div className="flex flex-wrap items-end gap-2">
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
                    disabled={loading}
                    onClick={() => {
                        form.reset(emptyValues)
                        onApply({})
                    }}
                >
                    重置
                </Button>
                <form.AppField name="only_unadopted">
                    {(field) => (
                        <label
                            className="flex items-center gap-2 py-2 text-xs"
                            htmlFor="supplier-catalog-batch-only-unadopted"
                        >
                            <NativeCheckbox
                                id="supplier-catalog-batch-only-unadopted"
                                checked={field.state.value}
                                onCheckedChange={field.handleChange}
                            />
                            仅处理未入库项
                        </label>
                    )}
                </form.AppField>
            </div>
            {applied.length ? (
                <div className="flex flex-wrap items-center gap-2 text-xs sm:col-span-2 xl:col-span-4">
                    <span className="text-muted-foreground">已选条件：</span>
                    {applied.map((item) => (
                        <Button
                            key={item.key}
                            id={`supplier-catalog-batch-filter-remove-${toAutomationIdSegment(item.key)}`}
                            variant="secondary"
                            size="sm"
                            type="button"
                            disabled={loading}
                            onClick={item.remove}
                        >
                            {item.label}
                            <span aria-hidden="true">×</span>
                            <span className="sr-only">移除此条件</span>
                        </Button>
                    ))}
                </div>
            ) : null}
        </form>
    )
}
