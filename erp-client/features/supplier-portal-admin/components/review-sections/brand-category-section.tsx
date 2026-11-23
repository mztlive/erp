"use client"

import { PlusIcon, RefreshCwIcon } from "lucide-react"
import { detailWorkspaceStyles } from "@/components/business/detail-workspace"
import { withForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import type {
    NewProductInput,
    PortalCategoryMappingSuggestion,
    PortalDictionary,
} from "@/features/supplier-portal/types"
import { PortalCategoryHierarchyConfirmation } from "../category-hierarchy-confirmation"
import { PortalCategoryMappingEvidence } from "../category-suggestion"
import {
    dictionaryOptions,
    fieldHighlightClass,
    reviewFormBaseOptions,
    type CreateDictionaryRequest,
} from "./types"

export const BrandCategorySection = withForm({
    ...reviewFormBaseOptions,
    props: {
        rawProduct: {} as NewProductInput,
        brands: [] as PortalDictionary[],
        categories: [] as PortalDictionary[],
        categorySuggestion: null as PortalCategoryMappingSuggestion | null,
        canCreateBrand: false,
        canCreateCategory: false,
        disabled: false,
        highlightedFieldId: null as string | null,
        onClearHighlight: () => {},
        onRefreshBrands: async () => {},
        onRefreshCategories: async () => {},
        onReloadDictionaries: async () => {},
        onCreateDictionary: (_req: CreateDictionaryRequest) => {},
    },
    render: function BrandCategorySectionContent({
        form,
        rawProduct,
        brands,
        categories,
        categorySuggestion,
        canCreateBrand,
        canCreateCategory,
        disabled,
        highlightedFieldId,
        onClearHighlight,
        onRefreshBrands,
        onRefreshCategories,
        onReloadDictionaries,
        onCreateDictionary,
    }) {
        const matchingCategories = categories.filter(
            (item) =>
                !item.product_kind ||
                item.product_kind === rawProduct.product_kind,
        )

        return (
            <div className="max-w-4xl space-y-4">
                <div className="grid gap-4 md:grid-cols-2">
                    <div
                        className={fieldHighlightClass(
                            highlightedFieldId,
                            "supplier-portal-review-brand",
                        )}
                    >
                        <form.AppField name="brandId">
                            {(field) => (
                                <field.SelectField
                                    onValueChange={(id) => {
                                        onClearHighlight()
                                        form.setFieldValue(
                                            "brandVersion",
                                            brands.find((row) => row.id === id)
                                                ?.version ?? 0,
                                        )
                                    }}
                                    id="supplier-portal-review-brand"
                                    label={`提报品牌：${rawProduct.brand.raw_name}`}
                                    required
                                    options={dictionaryOptions(brands)}
                                    onRefresh={onRefreshBrands}
                                    disabled={
                                        disabled ||
                                        !!rawProduct.brand.selected_id
                                    }
                                    allowClear={!rawProduct.brand.selected_id}
                                />
                            )}
                        </form.AppField>
                    </div>
                    <div
                        className={fieldHighlightClass(
                            highlightedFieldId,
                            "supplier-portal-review-category",
                        )}
                    >
                        <form.AppField name="categoryId">
                            {(field) => (
                                <field.SelectField
                                    onRefresh={onRefreshCategories}
                                    onValueChange={(id) => {
                                        onClearHighlight()
                                        form.setFieldValue(
                                            "categoryVersion",
                                            categories.find(
                                                (row) => row.id === id,
                                            )?.version ?? 0,
                                        )
                                        form.setFieldValue(
                                            "categoryHierarchy",
                                            [],
                                        )
                                    }}
                                    id="supplier-portal-review-category"
                                    label={`提报分类：${rawProduct.category.raw_name}`}
                                    required
                                    options={dictionaryOptions(
                                        matchingCategories,
                                    )}
                                    disabled={
                                        disabled ||
                                        !!rawProduct.category.selected_id
                                    }
                                    allowClear={
                                        !rawProduct.category.selected_id
                                    }
                                />
                            )}
                        </form.AppField>
                    </div>
                </div>

                <div
                    className={fieldHighlightClass(
                        highlightedFieldId,
                        "supplier-portal-review-category-path-confirm",
                    )}
                >
                    <form.Subscribe
                        selector={(state) => ({
                            id: state.values.categoryId,
                            version: state.values.categoryVersion,
                            hierarchy: state.values.categoryHierarchy,
                        })}
                    >
                        {(selection) => (
                            <PortalCategoryHierarchyConfirmation
                                category={categories.find(
                                    (item) => item.id === selection.id,
                                )}
                                expectedVersion={selection.version}
                                value={selection.hierarchy}
                                disabled={disabled}
                                onConfirm={(hierarchy) => {
                                    onClearHighlight()
                                    form.setFieldValue(
                                        "categoryHierarchy",
                                        hierarchy,
                                    )
                                }}
                            />
                        )}
                    </form.Subscribe>
                </div>

                <p className="text-xs text-muted-foreground">
                    未匹配的品牌、分类或单位需由有权限的同事新增；完成后重新核对候选。可转交当前申请，供应商提报资料保留。
                </p>

                <div className="flex flex-wrap gap-2">
                    <Button
                        id="supplier-portal-review-dictionaries-reload"
                        type="button"
                        size="sm"
                        variant="ghost"
                        className={detailWorkspaceStyles.subtleButton}
                        disabled={disabled}
                        onClick={() => void onReloadDictionaries()}
                    >
                        <RefreshCwIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        刷新品牌、分类与单位选项
                    </Button>
                    {canCreateBrand && !rawProduct.brand.selected_id && (
                        <Button
                            id="supplier-portal-review-create-brand"
                            type="button"
                            size="sm"
                            variant="ghost"
                            className={detailWorkspaceStyles.subtleButton}
                            disabled={disabled}
                            onClick={() =>
                                onCreateDictionary({
                                    kind: "brand",
                                    rawName: rawProduct.brand.raw_name,
                                })
                            }
                        >
                            <PlusIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            在当前审核补建品牌
                        </Button>
                    )}
                    {canCreateCategory && !rawProduct.category.selected_id && (
                        <Button
                            id="supplier-portal-review-create-category"
                            type="button"
                            size="sm"
                            variant="ghost"
                            className={detailWorkspaceStyles.subtleButton}
                            disabled={disabled}
                            onClick={() =>
                                onCreateDictionary({
                                    kind: "category",
                                    rawName: rawProduct.category.raw_name,
                                })
                            }
                        >
                            <PlusIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            在当前审核补建分类
                        </Button>
                    )}
                </div>

                <PortalCategoryMappingEvidence
                    suggestion={categorySuggestion}
                />
            </div>
        )
    },
})
