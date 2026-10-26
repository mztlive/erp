"use client"

import * as React from "react"
import Link from "next/link"
import { OptionCombobox } from "@/components/business"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import {
    Field,
    FieldDescription,
    FieldError,
    FieldLabel,
} from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { LoadingButton } from "@/components/ui/loading-button"
import { CompanySkuSearchCombobox } from "@/features/entity-selectors"
import { DeliveryTemplateField } from "@/features/supplier-offerings/components/delivery-templates/delivery-template-field"
import { SupplierTaxRateField } from "@/features/supplier-offerings/components/supplier-tax-rate-field"
import {
    PRODUCT_KIND_LABELS,
    PRODUCT_KIND_VALUES,
} from "@/features/master-data/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { AdoptionPreview, AdoptionResultRow } from "../../api/adoption"
import { useAdoptionBatch } from "../../hooks/use-adoption-batch"
import { AdoptionProductText } from "./adoption-product-text"
import { CatalogSourceImage } from "./catalog-source-image"

export function AdoptionBatchEditor({
    connectionId,
    supplierId,
    rows,
    preview,
    onComplete,
    onPendingChange,
}: {
    connectionId: string
    supplierId: string
    rows: AdoptionPreview["rows"]
    preview: AdoptionPreview
    onComplete: (rows: AdoptionResultRow[]) => void
    onPendingChange: (pending: boolean) => void
}) {
    const editor = useAdoptionBatch({ connectionId, rows, preview, onComplete })
    const { form, active, activeIndex: index, mutation } = editor
    React.useEffect(
        () => onPendingChange(mutation.isPending),
        [mutation.isPending, onPendingChange],
    )
    const item = rows[index]?.item
    const targetData = editor.target.data
    if (!active || !item) return null
    const prefix = `supplier-catalog-adoption-${toAutomationIdSegment(item.id)}`
    return (
        <form
            id="supplier-catalog-adoption-batch-form"
            onSubmit={(event) => {
                event.preventDefault()
                void form.handleSubmit()
            }}
            className="space-y-4"
        >
            <fieldset
                disabled={mutation.isPending}
                className="grid min-h-0 min-w-0 gap-5 lg:grid-cols-[200px_minmax(0,1fr)]"
            >
                <nav
                    aria-label="本批次选品商品"
                    className="max-h-48 space-y-1 overflow-auto lg:max-h-[62vh]"
                >
                    {rows.map(({ item: row }, rowIndex) => (
                        <Button
                            key={row.id}
                            id={`supplier-catalog-adoption-row-${toAutomationIdSegment(row.id)}`}
                            type="button"
                            variant={rowIndex === index ? "secondary" : "ghost"}
                            className="h-auto w-full justify-start whitespace-normal text-left"
                            aria-current={
                                rowIndex === index ? "step" : undefined
                            }
                            disabled={editor.rowResults.some(
                                (result) =>
                                    result.item_id === row.id &&
                                    result.status === "SUCCEEDED",
                            )}
                            onClick={() => editor.setActiveIndex(rowIndex)}
                        >
                            <span className="line-clamp-2">
                                {row.name} · {row.specification || "未标注规格"}
                            </span>
                        </Button>
                    ))}
                </nav>
                <div className="max-h-[62vh] min-w-0 space-y-5 overflow-auto pr-1">
                    <div className="rounded-lg bg-muted/40 p-3 text-sm">
                        <p className="font-medium">{item.name}</p>
                        <p className="mt-1 text-muted-foreground">
                            来源规格：{item.specification || "未标注"} ·
                            来源单位：{item.unit || "供应商未提供"} · 采购报价：
                            {item.purchase_price ?? "未提供"}
                        </p>
                        <p className="mt-1 text-xs text-muted-foreground">
                            展示资料默认由供应商带入，可整理公司名称与介绍。相同供应商商品的多个规格会归入同一个公司商品，分类、品牌及商品类型须一致。新建商品将保持未上架，入库后可维护销售价格。
                        </p>
                        <p className="mt-1 text-xs text-muted-foreground">
                            轮播图和详情图按供应商顺序带入，SKU
                            主图默认使用第一张轮播图。供应商未提供单位时，请在下方选择公司计量单位。
                        </p>
                        {rows[index]?.missing_fields.length ? (
                            <p className="mt-2 text-xs text-muted-foreground">
                                本项需确认：
                                {rows[index].missing_fields.join("、")}。
                            </p>
                        ) : null}
                        {item.images?.length || item.detail_images?.length ? (
                            <details className="mt-3 text-sm">
                                <summary
                                    id={`${prefix}-source-images-toggle`}
                                    className="cursor-pointer font-medium"
                                >
                                    查看商品轮播图和详情图
                                </summary>
                                <div className="mt-3 space-y-4">
                                    {[
                                        {
                                            title: "商品轮播图",
                                            images: item.images,
                                        },
                                        {
                                            title: "商品详情图",
                                            images: item.detail_images,
                                        },
                                    ].map((group) =>
                                        group.images?.length ? (
                                            <section
                                                key={group.title}
                                                className="space-y-2"
                                            >
                                                <h4 className="text-xs text-muted-foreground">
                                                    {group.title}
                                                </h4>
                                                {group.images.map(
                                                    (image, position) => (
                                                        <CatalogSourceImage
                                                            key={`${image.file_asset_id ?? image.url}-${position}`}
                                                            image={image}
                                                            alt={`${group.title} ${position + 1}`}
                                                            width={800}
                                                            height={800}
                                                            className="h-auto w-full rounded border object-contain"
                                                        />
                                                    ),
                                                )}
                                            </section>
                                        ) : null,
                                    )}
                                </div>
                            </details>
                        ) : null}
                    </div>
                    <form.AppField name={`rows[${index}].kind`}>
                        {(field) => (
                            <Field>
                                <FieldLabel htmlFor={`${prefix}-target-kind`}>
                                    入库方式
                                </FieldLabel>
                                <OptionCombobox
                                    id={`${prefix}-target-kind`}
                                    value={field.state.value}
                                    allowClear={false}
                                    options={[
                                        {
                                            value: "CREATE",
                                            label: "创建公司商品及 SKU",
                                        },
                                        {
                                            value: "EXISTING",
                                            label: "关联已有公司 SKU",
                                        },
                                    ]}
                                    onValueChange={(next) => {
                                        field.handleChange(
                                            next === "EXISTING"
                                                ? "EXISTING"
                                                : "CREATE",
                                        )
                                        form.setFieldValue(
                                            `rows[${index}].confirmed`,
                                            false,
                                        )
                                    }}
                                />
                            </Field>
                        )}
                    </form.AppField>
                    {active.kind === "CREATE" ? (
                        <AdoptionProductText
                            form={form}
                            item={item}
                            index={index}
                            prefix={prefix}
                        />
                    ) : null}
                    {active.kind === "CREATE" ? (
                        <div className="grid gap-3 sm:grid-cols-2">
                            {(
                                [
                                    "categoryId",
                                    "brandId",
                                    "unitId",
                                    "productKind",
                                ] as const
                            ).map((name) => {
                                const label = {
                                    categoryId: "公司分类",
                                    brandId: "公司品牌",
                                    unitId: "公司计量单位",
                                    productKind: "商品类型",
                                }[name]
                                const options =
                                    name === "categoryId"
                                        ? (
                                              editor.categories.data?.rows ?? []
                                          ).map((row) => ({
                                              value: row.stableId,
                                              label: row.name,
                                          }))
                                        : name === "brandId"
                                          ? (
                                                editor.brands.data?.rows ?? []
                                            ).map((row) => ({
                                                value: row.stableId,
                                                label: row.name,
                                            }))
                                          : name === "unitId"
                                            ? (editor.units.data ?? []).map(
                                                  (row) => ({
                                                      value: row.id,
                                                      label: row.label,
                                                  }),
                                              )
                                            : PRODUCT_KIND_VALUES.filter(
                                                  (kind) => kind !== "VOUCHER",
                                              ).map((kind) => ({
                                                  value: kind,
                                                  label: PRODUCT_KIND_LABELS[
                                                      kind
                                                  ],
                                              }))
                                return (
                                    <form.AppField
                                        key={name}
                                        name={`rows[${index}].${name}`}
                                    >
                                        {(field) => (
                                            <Field>
                                                <FieldLabel
                                                    htmlFor={`${prefix}-${name}`}
                                                >
                                                    {label}
                                                </FieldLabel>
                                                <OptionCombobox
                                                    id={`${prefix}-${name}`}
                                                    value={
                                                        field.state.value ||
                                                        undefined
                                                    }
                                                    options={options}
                                                    allowClear={
                                                        name !== "productKind"
                                                    }
                                                    onValueChange={(next) => {
                                                        if (
                                                            name ===
                                                            "productKind"
                                                        )
                                                            form.setFieldValue(
                                                                `rows[${index}].productKind`,
                                                                PRODUCT_KIND_VALUES.find(
                                                                    (kind) =>
                                                                        kind ===
                                                                        next,
                                                                ) ?? "PHYSICAL",
                                                            )
                                                        else
                                                            form.setFieldValue(
                                                                `rows[${index}].${name}`,
                                                                next ?? "",
                                                            )
                                                        form.setFieldValue(
                                                            `rows[${index}].confirmed`,
                                                            false,
                                                        )
                                                    }}
                                                    placeholder={`请选择${label}`}
                                                    aria-invalid={
                                                        field.state.meta.errors
                                                            .length > 0
                                                    }
                                                />
                                                <FieldError
                                                    errors={
                                                        field.state.meta.errors
                                                    }
                                                />
                                            </Field>
                                        )}
                                    </form.AppField>
                                )
                            })}
                            {editor.categories.isError ||
                            editor.brands.isError ||
                            editor.units.isError ? (
                                <div
                                    className="flex items-center gap-2 text-sm text-destructive sm:col-span-2"
                                    role="alert"
                                >
                                    公司字典读取失败，请重试后选择。
                                    <Button
                                        id={`${prefix}-retry-dictionaries`}
                                        type="button"
                                        variant="outline"
                                        size="sm"
                                        onClick={() => {
                                            void editor.categories.refetch()
                                            void editor.brands.refetch()
                                            void editor.units.refetch()
                                        }}
                                    >
                                        重试
                                    </Button>
                                </div>
                            ) : null}
                            <form.AppField
                                name={`rows[${index}].specification`}
                            >
                                {(field) => (
                                    <Field className="sm:col-span-2">
                                        <FieldLabel
                                            htmlFor={`${prefix}-specification`}
                                        >
                                            公司规格
                                        </FieldLabel>
                                        <Input
                                            id={`${prefix}-specification`}
                                            value={field.state.value}
                                            onChange={(event) => {
                                                field.handleChange(
                                                    event.target.value,
                                                )
                                                form.setFieldValue(
                                                    `rows[${index}].confirmed`,
                                                    false,
                                                )
                                            }}
                                        />
                                        <FieldDescription>
                                            须与供应商实际供货内容一致；来源未标注规格时，确认无规格后可留空。
                                        </FieldDescription>
                                    </Field>
                                )}
                            </form.AppField>
                        </div>
                    ) : (
                        <div className="space-y-2">
                            <form.AppField name={`rows[${index}].skuId`}>
                                {(field) => (
                                    <Field>
                                        <FieldLabel
                                            htmlFor={`${prefix}-company-sku`}
                                        >
                                            公司商品规格
                                        </FieldLabel>
                                        <CompanySkuSearchCombobox
                                            id={`${prefix}-company-sku`}
                                            value={
                                                field.state.value || undefined
                                            }
                                            onValueChange={(next) => {
                                                field.handleChange(next ?? "")
                                                form.setFieldValue(
                                                    `rows[${index}].skuVersion`,
                                                    0,
                                                )
                                                form.setFieldValue(
                                                    `rows[${index}].productVersion`,
                                                    0,
                                                )
                                                form.setFieldValue(
                                                    `rows[${index}].confirmed`,
                                                    false,
                                                )
                                            }}
                                            label="公司商品规格"
                                            placeholder="搜索商品名称或 SKU 编号"
                                        />
                                        <FieldError
                                            errors={field.state.meta.errors}
                                        />
                                    </Field>
                                )}
                            </form.AppField>
                            {editor.target.isFetching ? (
                                <p className="text-sm text-muted-foreground">
                                    正在核对公司规格和单位…
                                </p>
                            ) : null}
                            {targetData ? (
                                <p className="text-sm text-muted-foreground">
                                    公司规格：
                                    {targetData.specification || "未标注"} · SKU
                                    编号：{targetData.sku_no} · 单位：
                                    {editor.units.data?.find(
                                        (unit) =>
                                            unit.id === targetData.unit_id,
                                    )?.label ?? "请核对公司商品资料"}
                                </p>
                            ) : null}
                            {editor.target.isError ? (
                                <p
                                    role="alert"
                                    className="text-sm text-destructive"
                                >
                                    {editor.target.error.message}
                                </p>
                            ) : null}
                        </div>
                    )}
                    <section
                        className="space-y-3 border-t pt-4"
                        aria-labelledby={`${prefix}-terms-title`}
                    >
                        <h3
                            id={`${prefix}-terms-title`}
                            className="text-sm font-medium"
                        >
                            采购供给条件
                        </h3>
                        <div className="grid gap-3 sm:grid-cols-2">
                            {(
                                [
                                    "dropshipPrice",
                                    "bulkPrice",
                                    "minimumQuantity",
                                    "validFrom",
                                    "validTo",
                                ] as const
                            ).map((name) => (
                                <form.AppField
                                    key={name}
                                    name={`rows[${index}].${name}`}
                                >
                                    {(field) => (
                                        <Field>
                                            <FieldLabel
                                                htmlFor={`${prefix}-${name}`}
                                            >
                                                {
                                                    {
                                                        dropshipPrice:
                                                            "一件代发含税供给价",
                                                        bulkPrice:
                                                            "集采含税供给价（可不填）",
                                                        minimumQuantity:
                                                            "集采起订数量（可不填）",
                                                        validFrom: "生效日期",
                                                        validTo:
                                                            "失效日期（可不填）",
                                                    }[name]
                                                }
                                            </FieldLabel>
                                            <Input
                                                id={`${prefix}-${name}`}
                                                type={
                                                    name === "validFrom" ||
                                                    name === "validTo"
                                                        ? "date"
                                                        : "text"
                                                }
                                                inputMode={
                                                    name === "validFrom" ||
                                                    name === "validTo"
                                                        ? undefined
                                                        : "decimal"
                                                }
                                                readOnly={
                                                    name === "dropshipPrice" &&
                                                    item.purchase_price != null
                                                }
                                                value={field.state.value}
                                                onChange={(event) =>
                                                    field.handleChange(
                                                        event.target.value,
                                                    )
                                                }
                                                aria-invalid={
                                                    field.state.meta.errors
                                                        .length > 0
                                                }
                                            />
                                            {name === "dropshipPrice" &&
                                            item.purchase_price != null ? (
                                                <FieldDescription>
                                                    使用接口当前报价，不能手工覆盖。
                                                </FieldDescription>
                                            ) : null}
                                            <FieldError
                                                errors={field.state.meta.errors}
                                            />
                                        </Field>
                                    )}
                                </form.AppField>
                            ))}
                            <form.AppField
                                name={`rows[${index}].inputTaxPercentage`}
                            >
                                {(field) => (
                                    <SupplierTaxRateField
                                        id={`${prefix}-tax-rate`}
                                        supplierId={supplierId}
                                        value={field.state.value}
                                        onChange={field.handleChange}
                                        onBlur={field.handleBlur}
                                        errors={field.state.meta.errors}
                                        prefill={false}
                                    />
                                )}
                            </form.AppField>
                        </div>
                        <form.AppField
                            name={`rows[${index}].deliveryTemplateId`}
                        >
                            {(field) => (
                                <DeliveryTemplateField
                                    id={`${prefix}-delivery-template`}
                                    supplierId={supplierId}
                                    value={field.state.value}
                                    onChange={field.handleChange}
                                />
                            )}
                        </form.AppField>
                        {rows.length > 1 ? (
                            <Button
                                id={`${prefix}-apply-shared`}
                                type="button"
                                variant="outline"
                                onClick={editor.applyShared}
                            >
                                将本项分类、品牌、单位及共同采购条件应用到本批次
                            </Button>
                        ) : null}
                        <p className="text-xs text-muted-foreground">
                            供应商没有集采条件时，集采价与集采起订数量均留空。一件代发价和规格保留各商品内容；关联已有商品的单位不被批量设置覆盖。每项规格与单位仍须逐项确认。
                        </p>
                    </section>
                    <form.AppField name={`rows[${index}].confirmed`}>
                        {(field) => (
                            <Field>
                                <div className="flex items-start gap-2">
                                    <Checkbox
                                        id={`${prefix}-confirmed`}
                                        checked={field.state.value}
                                        onCheckedChange={(next) =>
                                            field.handleChange(next === true)
                                        }
                                        disabled={
                                            active.kind === "EXISTING" &&
                                            (!editor.target.data ||
                                                editor.target.isFetching)
                                        }
                                    />
                                    <FieldLabel
                                        htmlFor={`${prefix}-confirmed`}
                                        className="leading-5"
                                    >
                                        已核对本项规格、包装数量和单位，与供应商实际供货一致
                                    </FieldLabel>
                                </div>
                                <FieldError errors={field.state.meta.errors} />
                            </Field>
                        )}
                    </form.AppField>
                </div>
            </fieldset>
            {mutation.error ? (
                <p role="alert" className="text-sm text-destructive">
                    {mutation.error.message}
                </p>
            ) : null}
            {editor.rowResults.length ? (
                <div
                    className="space-y-2 rounded-lg border p-3 text-sm"
                    aria-live="polite"
                >
                    {editor.rowResults.map((result) => (
                        <p
                            key={result.item_id}
                            className={
                                result.status === "FAILED"
                                    ? "text-destructive"
                                    : "text-muted-foreground"
                            }
                        >
                            {
                                rows.find(
                                    (row) => row.item.id === result.item_id,
                                )?.item.name
                            }
                            ：
                            {result.status === "SUCCEEDED"
                                ? "已入库"
                                : result.message || "入库失败，请调整后重试"}
                            {result.product_id ? (
                                <Link
                                    id={`supplier-catalog-adoption-result-${toAutomationIdSegment(result.item_id)}`}
                                    href={`/master-data/products/${encodeURIComponent(result.product_id)}`}
                                    className="ml-2 text-primary underline"
                                >
                                    打开公司商品
                                </Link>
                            ) : null}
                        </p>
                    ))}
                    <p className="text-xs text-muted-foreground">
                        已成功的商品不会重复创建。请修正未成功的项目后再次确认。
                    </p>
                </div>
            ) : null}
            <div className="flex flex-wrap items-center justify-between gap-3 border-t pt-4">
                <span className="text-sm text-muted-foreground">
                    本批 {rows.length} 项；将创建或关联公司商品并建立供给。
                </span>
                <LoadingButton
                    id="supplier-catalog-adoption-confirm-batch"
                    type="submit"
                    loading={mutation.isPending}
                >
                    确认本批入库
                </LoadingButton>
            </div>
        </form>
    )
}
