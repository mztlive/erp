"use client"

import * as React from "react"
import { LockIcon } from "lucide-react"

import {
    BrandCombobox,
    CategoryCombobox,
    DetailWorkspaceSection,
    OptionCombobox,
} from "@/components/business"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { RequiredMark } from "@/components/ui/required-mark"
import { Textarea } from "@/components/ui/textarea"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import type { SetProductFields } from "@/features/master-data/components/product/product-editor-sections"
import type { ProductFields, ProductKind } from "@/features/master-data/types"
import {
    PRODUCT_KIND_LABELS,
    PRODUCT_KIND_VALUES,
} from "@/features/master-data/types"

type UnitOption = {
    id: string
    code: string
    label: string
}

type ProductBasicSectionProps = {
    idPrefix?: string
    isCreate: boolean
    canRevise: boolean
    name: string
    setName: (next: string) => void
    fields: ProductFields
    setFields: SetProductFields
    unitOptions: readonly UnitOption[] | undefined
    categoryOptions: React.ComponentProps<typeof CategoryCombobox>["categories"]
    brandOptions: React.ComponentProps<typeof BrandCombobox>["brands"]
    categoryLoading: boolean
    brandLoading: boolean
    canSelectUnit: boolean
    canSelectCategory: boolean
    onRefreshUnits: () => Promise<unknown>
    onRefreshCategories: () => Promise<unknown>
    canSelectBrand: boolean
    onRefreshBrands: () => Promise<unknown>
}

function ProductBasicSection({
    idPrefix,
    isCreate,
    canRevise,
    name,
    setName,
    fields,
    setFields,
    unitOptions,
    categoryOptions,
    brandOptions,
    categoryLoading,
    brandLoading,
    canSelectUnit,
    canSelectCategory,
    onRefreshUnits,
    onRefreshCategories,
    canSelectBrand,
    onRefreshBrands,
}: ProductBasicSectionProps) {
    const prefix = idPrefix ?? "master-data-product-basic"
    const kindLabel = fields.productKind
        ? PRODUCT_KIND_LABELS[fields.productKind]
        : "未设置"

    return (
        <DetailWorkspaceSection
            id="product-section-basic"
            title="基础信息"
            headingId={`${prefix}-heading`}
            width="form"
        >
            <fieldset disabled={!canRevise} className="min-w-0">
                <legend className="sr-only">基础信息</legend>
                <div
                    data-slot="detail-form-grid"
                    className="grid gap-x-5 gap-y-4 sm:grid-cols-12"
                >
                    <div className="space-y-1.5 sm:col-span-8">
                        <Label htmlFor={`${prefix}-name`}>
                            商品名称
                            <RequiredMark />
                        </Label>
                        <Input
                            id={`${prefix}-name`}
                            value={name}
                            onChange={(e) => setName(e.target.value)}
                            placeholder="请输入商品名称"
                        />
                    </div>
                    <div className="space-y-1.5 sm:col-span-4">
                        <Label htmlFor={`${prefix}-kind-combobox`}>
                            商品类型
                            {isCreate ? <RequiredMark /> : null}
                        </Label>
                        {isCreate ? (
                            <>
                                <OptionCombobox
                                    id={`${prefix}-kind-combobox`}
                                    value={fields.productKind || null}
                                    disabled={!canRevise}
                                    onValueChange={(value) =>
                                        setFields((previous) => ({
                                            ...previous,
                                            productKind: (value ??
                                                "") as ProductKind,
                                        }))
                                    }
                                    options={PRODUCT_KIND_VALUES.map(
                                        (kind) => ({
                                            value: kind,
                                            label: PRODUCT_KIND_LABELS[kind],
                                        }),
                                    )}
                                    allowClear={false}
                                    placeholder="请选择商品类型"
                                    className="w-full"
                                />
                                <p className="text-xs text-muted-foreground">
                                    创建后不可修改，也不随分类变化。
                                </p>
                            </>
                        ) : (
                            <div
                                id={`${prefix}-kind-combobox`}
                                title="创建后不可修改，也不随分类变化。"
                                className="flex h-control items-center justify-between gap-2 rounded-md bg-muted/45 px-2.5 text-body-compact text-foreground/85"
                            >
                                <span className="truncate font-medium">
                                    {kindLabel}
                                </span>
                                <span className="inline-flex shrink-0 items-center gap-1 text-xs text-muted-foreground/80">
                                    <LockIcon
                                        className="size-3"
                                        aria-hidden="true"
                                    />
                                    创建后不可修改
                                </span>
                            </div>
                        )}
                    </div>

                    {canSelectUnit ? (
                        <div className="space-y-1.5 sm:col-span-4">
                            <Label htmlFor={`${prefix}-unit-combobox`}>
                                {masterDataCopy.fBaseUnit}
                                <RequiredMark />
                            </Label>
                            <OptionCombobox
                                id={`${prefix}-unit-combobox`}
                                value={fields.baseUnitId || null}
                                onValueChange={(id) => {
                                    const unit = unitOptions?.find(
                                        (item) => item.id === id,
                                    )
                                    setFields((prev) => ({
                                        ...prev,
                                        baseUnitId: unit?.id ?? "",
                                        baseUnitCode: unit?.code ?? "",
                                        baseUnit: unit?.label ?? "",
                                        skus: prev.skus.map((sku) => ({
                                            ...sku,
                                            baseUnit: unit?.label ?? "",
                                        })),
                                    }))
                                }}
                                onRefresh={onRefreshUnits}
                                options={(unitOptions ?? []).map((unit) => ({
                                    value: unit.id,
                                    label: `${unit.label} · ${unit.code}`,
                                }))}
                                allowClear={false}
                                placeholder="请选择基础单位"
                                className="w-full"
                            />
                        </div>
                    ) : fields.baseUnit ? (
                        <div className="space-y-1.5 sm:col-span-4">
                            <span className="block text-xs font-medium text-foreground/70">
                                {masterDataCopy.fBaseUnit}
                            </span>
                            <div className="flex h-control items-center rounded-md bg-muted/40 px-2.5 text-body-compact text-foreground/90">
                                {fields.baseUnit}
                            </div>
                        </div>
                    ) : null}

                    {canSelectCategory ? (
                        <div className="space-y-1.5 sm:col-span-4">
                            <Label htmlFor={`${prefix}-category-combobox`}>
                                {masterDataCopy.fCategory}
                                <RequiredMark />
                            </Label>
                            <CategoryCombobox
                                id={`${prefix}-category-combobox`}
                                categories={categoryOptions}
                                onRefresh={onRefreshCategories}
                                value={fields.categoryId || undefined}
                                onValueChange={(id) => {
                                    const hit = categoryOptions.find(
                                        (c) => c.categoryId === id,
                                    )
                                    setFields((prev) => ({
                                        ...prev,
                                        categoryId: id ?? "",
                                        category: hit?.categoryName ?? "",
                                    }))
                                }}
                                loading={categoryLoading}
                                placeholder="请选择分类"
                                emptyLabel="暂无可用分类，请先在商品分类中维护"
                                className="w-full"
                            />
                        </div>
                    ) : fields.category ? (
                        <div className="space-y-1.5 sm:col-span-4">
                            <span className="block text-xs font-medium text-foreground/70">
                                {masterDataCopy.fCategory}
                            </span>
                            <div className="flex h-control items-center rounded-md bg-muted/40 px-2.5 text-body-compact text-foreground/90">
                                {fields.category}
                            </div>
                        </div>
                    ) : null}

                    {canSelectBrand ? (
                        <div className="space-y-1.5 sm:col-span-4">
                            <Label htmlFor={`${prefix}-brand-combobox`}>
                                {masterDataCopy.fBrand}
                                <RequiredMark />
                            </Label>
                            <BrandCombobox
                                id={`${prefix}-brand-combobox`}
                                brands={brandOptions}
                                onRefresh={onRefreshBrands}
                                value={fields.brandId || undefined}
                                onValueChange={(id) => {
                                    const hit = brandOptions.find(
                                        (b) => b.brandId === id,
                                    )
                                    setFields((prev) => ({
                                        ...prev,
                                        brandId: id ?? "",
                                        brand: hit?.brandName ?? "",
                                    }))
                                }}
                                loading={brandLoading}
                                placeholder="请选择品牌"
                                emptyLabel="暂无可用品牌，请先在品牌中维护"
                                className="w-full"
                            />
                        </div>
                    ) : fields.brand ? (
                        <div className="space-y-1.5 sm:col-span-4">
                            <span className="block text-xs font-medium text-foreground/70">
                                {masterDataCopy.fBrand}
                            </span>
                            <div className="flex h-control items-center rounded-md bg-muted/40 px-2.5 text-body-compact text-foreground/90">
                                {fields.brand}
                            </div>
                        </div>
                    ) : null}

                    <div className="space-y-1.5 sm:col-span-12">
                        <Label htmlFor={`${prefix}-description`}>
                            商品描述
                        </Label>
                        <Textarea
                            id={`${prefix}-description`}
                            value={fields.description ?? ""}
                            onChange={(event) =>
                                setFields((previous) => ({
                                    ...previous,
                                    description: event.target.value,
                                }))
                            }
                            placeholder="公司审核后的商品描述"
                            rows={3}
                        />
                    </div>
                </div>
            </fieldset>
        </DetailWorkspaceSection>
    )
}

export { ProductBasicSection }
