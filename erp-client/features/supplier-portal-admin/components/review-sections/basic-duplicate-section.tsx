"use client"

import { SearchIcon, TriangleAlertIcon } from "lucide-react"
import {
    DetailWorkspaceFact,
    DetailWorkspaceFacts,
    detailWorkspaceStyles,
} from "@/components/business/detail-workspace"
import { withForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { StatusBadge } from "@/components/ui/status-badge"
import { kindLabels } from "@/features/supplier-portal/lib/presentation"
import type {
    NewProductInput,
    PortalApplication,
} from "@/features/supplier-portal/types"
import type { WorkItemDto } from "@/features/work-items/types"
import type { PortalDuplicate } from "../../api"
import {
    PRODUCT_KIND_LABELS,
    fieldHighlightClass,
    reviewFormBaseOptions,
    type ExistingOfferingCandidate,
    type ReviewSectionId,
} from "./types"

export const BasicDuplicateSection = withForm({
    ...reviewFormBaseOptions,
    props: {
        application: {} as PortalApplication,
        task: {} as WorkItemDto,
        rawProduct: null as NewProductInput | null,
        search: "",
        onSearchChange: (_value: string) => {},
        onSearchSubmit: () => {},
        duplicates: [] as PortalDuplicate[],
        duplicatesFetching: false,
        onRefreshDuplicates: async () => {},
        existingCandidates: [] as ExistingOfferingCandidate[],
        disabled: false,
        highlightedFieldId: null as string | null,
        onClearHighlight: () => {},
        onJumpToSection: (_sectionId: ReviewSectionId) => {},
    },
    render: function BasicDuplicateSectionContent({
        form,
        application,
        task,
        rawProduct,
        search,
        onSearchChange,
        onSearchSubmit,
        duplicates,
        duplicatesFetching,
        onRefreshDuplicates,
        existingCandidates,
        disabled,
        highlightedFieldId,
        onClearHighlight,
        onJumpToSection,
    }) {
        const productKindLabel = rawProduct
            ? (PRODUCT_KIND_LABELS[rawProduct.product_kind] ??
              rawProduct.product_kind)
            : null

        return (
            <div className="max-w-4xl space-y-5">
                <DetailWorkspaceFacts
                    columns={3}
                    ariaLabel="供应商提报概览"
                    className="border-b border-border/60 pb-4"
                >
                    <DetailWorkspaceFact
                        label="申请类型"
                        value={kindLabels[application.kind]}
                    />
                    <DetailWorkspaceFact
                        label="申请单号"
                        value={application.application_no ?? "—"}
                        numeric
                    />
                    <DetailWorkspaceFact
                        label="供应商"
                        value={application.supplier_name ?? "—"}
                    />
                    <DetailWorkspaceFact
                        label="当前处理人"
                        value={task.owner_user?.display_name ?? "待核对"}
                    />
                    {rawProduct ? (
                        <>
                            <DetailWorkspaceFact
                                label="提报商品名称"
                                value={rawProduct.name}
                            />
                            <DetailWorkspaceFact
                                label="商品类型与规格数"
                                value={
                                    <span>
                                        {productKindLabel} ·{" "}
                                        <span className="num">
                                            {rawProduct.skus.length}
                                        </span>{" "}
                                        个规格
                                    </span>
                                }
                            />
                            <DetailWorkspaceFact
                                label="提报品牌原文"
                                value={rawProduct.brand.raw_name}
                            />
                            <DetailWorkspaceFact
                                label="提报分类原文"
                                value={rawProduct.category.raw_name}
                                span={2}
                            />
                        </>
                    ) : null}
                </DetailWorkspaceFacts>

                {rawProduct ? (
                    <div className="space-y-4">
                        <div className="flex flex-wrap items-center justify-between gap-2">
                            <h3 className="text-body-compact font-semibold text-foreground">
                                商品查重与建档决定
                            </h3>
                            <StatusBadge
                                tone={
                                    duplicates.length > 0 ? "warning" : "info"
                                }
                                appearance="quiet"
                                label={
                                    duplicatesFetching
                                        ? "正在查重…"
                                        : duplicates.length > 0
                                          ? `命中 ${duplicates.length} 个相似商品候选`
                                          : "未检索到相似商品候选"
                                }
                            />
                        </div>

                        {existingCandidates.length > 0 ? (
                            <div className="flex flex-wrap items-center justify-between gap-2 border-l-2 border-warning-border bg-warning-soft/60 px-3.5 py-2.5 text-xs text-warning-soft-foreground">
                                <span className="flex min-w-0 items-center gap-1.5">
                                    <TriangleAlertIcon
                                        aria-hidden="true"
                                        className="size-3.5 shrink-0"
                                    />
                                    <span>
                                        检测到{" "}
                                        <span className="num font-semibold">
                                            {existingCandidates.length}
                                        </span>{" "}
                                        项相同订货编码的既有供给，复用对应商品并在「SKU
                                        单位与规格映射」确认修订。
                                    </span>
                                </span>
                                <Button
                                    id="supplier-portal-review-basic-jump-existing"
                                    type="button"
                                    size="sm"
                                    variant="ghost"
                                    className="h-7 px-2 text-xs text-warning-soft-foreground hover:bg-warning-soft"
                                    onClick={() =>
                                        onJumpToSection("sku-mapping")
                                    }
                                >
                                    前往规格核对
                                </Button>
                            </div>
                        ) : null}

                        <div className="flex flex-wrap gap-2">
                            <Input
                                id="supplier-portal-review-duplicate-query"
                                aria-label="重复匹配候选搜索"
                                placeholder="按商品名称或关键字检索已有公司商品"
                                className="min-w-0 flex-1"
                                value={search}
                                disabled={disabled}
                                onChange={(event) =>
                                    onSearchChange(event.target.value)
                                }
                                onKeyDown={(event) => {
                                    if (event.key === "Enter") {
                                        event.preventDefault()
                                        onSearchSubmit()
                                    }
                                }}
                            />
                            <Button
                                id="supplier-portal-review-duplicate-search"
                                type="button"
                                size="sm"
                                variant="ghost"
                                className={detailWorkspaceStyles.subtleButton}
                                disabled={disabled}
                                onClick={onSearchSubmit}
                            >
                                <SearchIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                查找重复候选
                            </Button>
                        </div>

                        <div
                            className={fieldHighlightClass(
                                highlightedFieldId,
                                "supplier-portal-review-product-target",
                            )}
                        >
                            <form.AppField name="productTarget">
                                {(field) => (
                                    <field.SelectField
                                        onRefresh={onRefreshDuplicates}
                                        onValueChange={onClearHighlight}
                                        id="supplier-portal-review-product-target"
                                        label="本次商品建档决定"
                                        required
                                        allowClear={false}
                                        options={[
                                            {
                                                value: "unconfirmed",
                                                label: "请明确核对结果",
                                                disabled: true,
                                            },
                                            {
                                                value: "new",
                                                label: "新建公司商品",
                                            },
                                            ...duplicates.map((item) => ({
                                                value: item.product_id,
                                                label: `复用商品：${item.name}`,
                                            })),
                                        ]}
                                        disabled={disabled}
                                    />
                                )}
                            </form.AppField>
                        </div>

                        <form.Subscribe
                            selector={(state) => state.values.productTarget}
                        >
                            {(productTarget) => {
                                const selectedCandidate = duplicates.find(
                                    (item) => item.product_id === productTarget,
                                )
                                if (!selectedCandidate) return null
                                return (
                                    <div className="space-y-1 border-l-2 border-info-border bg-info-soft/45 px-3.5 py-2.5 text-xs text-info-soft-foreground">
                                        <p className="font-medium">
                                            已选复用商品：
                                            {selectedCandidate.name}
                                        </p>
                                        <p>
                                            当前包含{" "}
                                            <span className="num font-medium">
                                                {selectedCandidate.skus.length}
                                            </span>{" "}
                                            个已有公司规格，可在第 3 步「SKU
                                            单位与规格映射」逐项选择复用或新增规格。
                                        </p>
                                    </div>
                                )
                            }}
                        </form.Subscribe>
                    </div>
                ) : null}
            </div>
        )
    },
})
