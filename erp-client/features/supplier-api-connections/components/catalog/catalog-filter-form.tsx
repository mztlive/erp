"use client"

import { useRef, useState } from "react"
import { createPortal } from "react-dom"
import { useStore } from "@tanstack/react-form"
import { z } from "zod"
import {
    ListFilterRangeField,
    ListFilterSection,
    ListFilterSections,
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
    MultiOptionCombobox,
    OptionCombobox,
} from "@/components/business"
import { useTableAppliedChipsHost } from "@/components/business/table-toolbar"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { NativeCheckbox } from "@/components/ui/checkbox"
import { Field, FieldError } from "@/components/ui/field"
import { compareDecimal, parseDecimal } from "@/lib/fixed-decimal"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { useSearchHotkey } from "@/lib/use-search-hotkey"
import { cn } from "@/lib/utils"
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

const advancedFields = [
    "include_descendants",
    "only_unadopted",
    "price_min",
    "price_max",
    "availability",
    "selection_status",
] as const

const catalogMultiFilterClassName = [
    "max-w-full min-w-0 [&>button]:border-transparent [&>button]:bg-muted/65 [&>button:hover]:bg-muted",
    "[&>button:focus-visible]:border-primary/60 [&>button:focus-visible]:bg-card",
    "data-[has-value=true]:[&>button]:border-primary/25 data-[has-value=true]:[&>button]:bg-sidebar-accent/70",
    "data-[has-value=true]:[&>button:hover]:bg-sidebar-accent data-[has-value=true]:[&>button]:text-sidebar-accent-foreground",
].join(" ")

function fieldErrorText(
    errors: ReadonlyArray<{ message?: string } | undefined>,
) {
    return errors
        .map((issue) => issue?.message)
        .filter(Boolean)
        .join("；")
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
    resultStatus,
    canReadCost,
    onApply,
}: {
    value: CatalogFilters
    options?: CatalogFilterOptions
    loading: boolean
    resultStatus: string | null
    canReadCost: boolean
    onApply: (filters: CatalogFilters) => void
}) {
    const searchInputRef = useRef<HTMLInputElement>(null)
    const [moreOpen, setMoreOpen] = useState(false)
    const chipsHost = useTableAppliedChipsHost()
    useSearchHotkey(searchInputRef)
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
        onSubmit: ({ value: values }) => {
            setMoreOpen(false)
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
            })
        },
    })
    const hasPendingChanges = useStore(
        form.store,
        (state) => !state.isDefaultValue,
    )
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
        ...(canReadCost && (value.price_min || value.price_max)
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
        ...(value.include_descendants && hierarchySupported
            ? [
                  {
                      key: "descendants",
                      label: "包含子分类",
                      remove: () =>
                          onApply({ ...value, include_descendants: false }),
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
    const chips = applied.length ? (
        <div className="flex min-w-0 flex-wrap items-center gap-1.5 text-xs">
            <span className="text-muted-foreground">已选条件：</span>
            {applied.map((item) => (
                <Button
                    key={item.key}
                    id={`supplier-catalog-batch-filter-remove-${toAutomationIdSegment(item.key)}`}
                    variant="secondary"
                    size="xs"
                    type="button"
                    className="max-w-full border-border/65 bg-muted/65 font-normal text-foreground/85 shadow-none"
                    disabled={loading}
                    onClick={item.remove}
                    aria-label={`移除${item.label}筛选`}
                >
                    <span className="truncate">{item.label}</span>
                    <span aria-hidden="true">×</span>
                </Button>
            ))}
        </div>
    ) : null

    return (
        <div id="supplier-catalog-filters" className="min-w-0">
            <ListWorkspaceFilterBar
                idPrefix="supplier-catalog-filter"
                formAriaLabel="供应商品库查询"
                queryButtonId="supplier-catalog-filter-apply"
                morePresentation="popover"
                moreOpen={moreOpen}
                moreCount={
                    applied.filter(
                        (item) =>
                            item.key !== "keyword" &&
                            !item.key.startsWith("category-") &&
                            !item.key.startsWith("brand-"),
                    ).length
                }
                onToggleMore={() => {
                    if (moreOpen) {
                        for (const name of advancedFields) form.resetField(name)
                    }
                    setMoreOpen(!moreOpen)
                }}
                onResetMore={() => {
                    for (const name of advancedFields)
                        form.setFieldValue(name, emptyValues[name])
                }}
                onSubmit={() => {
                    if (!loading) void form.handleSubmit()
                }}
                resultStatus={resultStatus}
                hasPendingChanges={hasPendingChanges}
                search={
                    <form.AppField name="keyword">
                        {(field) => (
                            <ListSearchField
                                id="supplier-catalog-keyword"
                                aria-label="商品搜索"
                                searchInputRef={searchInputRef}
                                value={field.state.value}
                                onChange={field.handleChange}
                                placeholder="名称、规格或供应商订货编码"
                            />
                        )}
                    </form.AppField>
                }
                primaryFilters={(["category_ids", "brand_ids"] as const).map(
                    (name) => (
                        <form.AppField key={name} name={name}>
                            {(field) => {
                                const label =
                                    name === "category_ids"
                                        ? "供应商分类"
                                        : "品牌"
                                const choices =
                                    name === "category_ids"
                                        ? options?.categories
                                        : options?.brands
                                return (
                                    <MultiOptionCombobox
                                        id={`supplier-catalog-${name}`}
                                        aria-label={label}
                                        className={cn(
                                            catalogMultiFilterClassName,
                                            name === "category_ids"
                                                ? "w-44"
                                                : "w-32",
                                        )}
                                        value={field.state.value}
                                        onValueChange={field.handleChange}
                                        options={(choices ?? []).map(
                                            (option) => ({
                                                value: option.id,
                                                label: option.name,
                                            }),
                                        )}
                                        filterLabel={label}
                                        emptyLabel={`暂无${label}`}
                                        loading={loading && !options}
                                    />
                                )
                            }}
                        </form.AppField>
                    ),
                )}
                extraPrimary={
                    <Button
                        id="supplier-catalog-filter-reset"
                        type="button"
                        variant="ghost"
                        size="sm"
                        className="font-normal text-muted-foreground"
                        disabled={loading}
                        onClick={() => {
                            form.reset(emptyValues)
                            setMoreOpen(false)
                            onApply({})
                        }}
                    >
                        重置
                    </Button>
                }
                morePanel={
                    <ListFilterSections>
                        <ListFilterSection legend="商品">
                            <form.AppField name="include_descendants">
                                {(field) => (
                                    <Field>
                                        <label
                                            htmlFor="supplier-catalog-batch-include-descendants"
                                            className="flex min-h-control items-center gap-2 text-body-compact"
                                        >
                                            <NativeCheckbox
                                                id="supplier-catalog-batch-include-descendants"
                                                checked={
                                                    hierarchySupported &&
                                                    field.state.value
                                                }
                                                onCheckedChange={
                                                    field.handleChange
                                                }
                                                disabled={!hierarchySupported}
                                                aria-describedby={
                                                    !hierarchySupported
                                                        ? "supplier-catalog-batch-hierarchy-reason"
                                                        : undefined
                                                }
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
                            {canReadCost ? (
                                <form.AppField name="price_min">
                                    {(min) => (
                                        <form.AppField name="price_max">
                                            {(max) => (
                                                <ListFilterRangeField
                                                    label="接口采购报价"
                                                    minId="supplier-catalog-price_min"
                                                    maxId="supplier-catalog-price_max"
                                                    errorId="supplier-catalog-price-range-error"
                                                    min={min.state.value}
                                                    max={max.state.value}
                                                    minAriaLabel="最低采购报价（含边界）"
                                                    maxAriaLabel="最高采购报价（含边界）"
                                                    unit="元"
                                                    error={
                                                        fieldErrorText([
                                                            ...min.state.meta
                                                                .errors,
                                                            ...max.state.meta
                                                                .errors,
                                                        ]) || null
                                                    }
                                                    onMinChange={
                                                        min.handleChange
                                                    }
                                                    onMaxChange={
                                                        max.handleChange
                                                    }
                                                />
                                            )}
                                        </form.AppField>
                                    )}
                                </form.AppField>
                            ) : null}
                        </ListFilterSection>
                        <ListFilterSection legend="状态">
                            {(
                                ["availability", "selection_status"] as const
                            ).map((name) => {
                                const choices =
                                    name === "availability"
                                        ? [
                                              {
                                                  value: "AVAILABLE",
                                                  label: "可供",
                                              },
                                              {
                                                  value: "UNAVAILABLE",
                                                  label: "暂不可供",
                                              },
                                              {
                                                  value: "UNKNOWN",
                                                  label: "待核验",
                                              },
                                              {
                                                  value: "WITHDRAWN",
                                                  label: "已下架",
                                              },
                                          ]
                                        : [
                                              {
                                                  value: "UNSELECTED",
                                                  label: "未入库",
                                              },
                                              {
                                                  value: "SELECTED",
                                                  label: "已入库",
                                              },
                                          ]
                                const label =
                                    name === "availability"
                                        ? "可供状态"
                                        : "选品状态"
                                const id = `supplier-catalog-${name}`
                                return (
                                    <form.AppField key={name} name={name}>
                                        {(field) => {
                                            const errors =
                                                field.state.meta.errors
                                            const invalid = errors.length > 0
                                            return (
                                                <div className="min-w-0">
                                                    <ListWorkspaceFilterField
                                                        label={label}
                                                        htmlFor={id}
                                                    >
                                                        <OptionCombobox
                                                            id={id}
                                                            value={
                                                                field.state
                                                                    .value ||
                                                                undefined
                                                            }
                                                            onValueChange={(
                                                                next,
                                                            ) =>
                                                                field.handleChange(
                                                                    next ?? "",
                                                                )
                                                            }
                                                            options={choices}
                                                            placeholder="全部"
                                                            searchPlaceholder={`搜索${label}`}
                                                            emptyLabel={`暂无${label}`}
                                                            aria-invalid={
                                                                invalid ||
                                                                undefined
                                                            }
                                                            aria-describedby={
                                                                invalid
                                                                    ? `${id}-error`
                                                                    : undefined
                                                            }
                                                        />
                                                    </ListWorkspaceFilterField>
                                                    <FieldError
                                                        id={`${id}-error`}
                                                        errors={errors}
                                                    />
                                                </div>
                                            )
                                        }}
                                    </form.AppField>
                                )
                            })}
                            <form.AppField name="only_unadopted">
                                {(field) => (
                                    <Field>
                                        <label
                                            className="flex min-h-control items-center gap-2 text-body-compact"
                                            htmlFor="supplier-catalog-batch-only-unadopted"
                                        >
                                            <NativeCheckbox
                                                id="supplier-catalog-batch-only-unadopted"
                                                checked={field.state.value}
                                                onCheckedChange={
                                                    field.handleChange
                                                }
                                            />
                                            仅处理未入库项
                                        </label>
                                    </Field>
                                )}
                            </form.AppField>
                        </ListFilterSection>
                    </ListFilterSections>
                }
            />
            {chips && chipsHost ? createPortal(chips, chipsHost) : chips}
        </div>
    )
}
