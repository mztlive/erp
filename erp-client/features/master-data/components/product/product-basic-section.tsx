"use client"

import * as React from "react"

import {
    BrandCombobox,
    CategoryCombobox,
    OptionCombobox,
} from "@/components/business"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Textarea } from "@/components/ui/textarea"
import { ProductSectionFrame } from "@/features/master-data/components/product/product-section-frame"
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
}: ProductBasicSectionProps) {
    const prefix = idPrefix ?? "master-data-product-basic"
    return (
        <ProductSectionFrame
            id="product-section-basic"
            title={masterDataCopy.fieldIdentitySection}
            description={
                isCreate
                    ? masterDataCopy.productCreateDesc
                    : masterDataCopy.productEditDesc
            }
            disabled={!canRevise}
        >
            <div className="grid gap-4 sm:grid-cols-2">
                <div className="space-y-1.5">
                    <Label htmlFor={`${prefix}-product-no`}>商品编号</Label>
                    <Input
                        id={`${prefix}-product-no`}
                        value={fields.productNo}
                        disabled={!isCreate}
                        onChange={(event) =>
                            setFields((previous) => ({
                                ...previous,
                                productNo: event.target.value,
                            }))
                        }
                        placeholder="请输入全局唯一商品编号"
                    />
                    {!isCreate ? (
                        <p className="text-xs text-muted-foreground">
                            商品编号创建后不可修改。
                        </p>
                    ) : null}
                </div>
                <div className="space-y-1.5">
                    <Label htmlFor={`${prefix}-name`}>名称</Label>
                    <Input
                        id={`${prefix}-name`}
                        value={name}
                        onChange={(e) => setName(e.target.value)}
                        placeholder="商品名称（SPU）"
                    />
                </div>
                <div className="space-y-1.5 sm:col-span-2">
                    <Label htmlFor={`${prefix}-description`}>商品描述</Label>
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
                <div className="space-y-1.5">
                    <Label>商品类型</Label>
                    {isCreate ? (
                        <OptionCombobox
                            id={`${prefix}-kind-combobox`}
                            value={fields.productKind || null}
                            onValueChange={(value) =>
                                setFields((previous) => ({
                                    ...previous,
                                    productKind: (value ?? "") as ProductKind,
                                }))
                            }
                            options={PRODUCT_KIND_VALUES.map((kind) => ({
                                value: kind,
                                label: PRODUCT_KIND_LABELS[kind],
                            }))}
                            allowClear={false}
                            placeholder="请选择商品类型"
                            className="w-full"
                        />
                    ) : (
                        <p className="flex h-9 items-center text-sm font-medium">
                            {fields.productKind
                                ? PRODUCT_KIND_LABELS[fields.productKind]
                                : "—"}
                        </p>
                    )}
                    <p className="text-xs text-muted-foreground">
                        决定商品业务作用；创建后不可变，也不随分类变化。
                    </p>
                </div>
                <div className="space-y-1.5">
                    <Label>{masterDataCopy.fBaseUnit}</Label>
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
                        options={(unitOptions ?? []).map((unit) => ({
                            value: unit.id,
                            label: `${unit.label} · ${unit.code}`,
                        }))}
                        allowClear={false}
                        placeholder="请选择基础单位"
                        className="w-full"
                    />
                </div>
                <div className="space-y-1.5">
                    <Label>{masterDataCopy.fCategory}</Label>
                    <CategoryCombobox
                        id={`${prefix}-category-combobox`}
                        categories={categoryOptions}
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
                <div className="space-y-1.5">
                    <Label>{masterDataCopy.fBrand}</Label>
                    <BrandCombobox
                        id={`${prefix}-brand-combobox`}
                        brands={brandOptions}
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
            </div>
        </ProductSectionFrame>
    )
}

export { ProductBasicSection }
