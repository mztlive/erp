"use client"

import * as React from "react"

import {
    BrandCombobox,
    CategoryCombobox,
    DocumentSection,
    OptionCombobox,
    RevisionTimeline,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { DatePicker } from "@/components/ui/date-picker"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Textarea } from "@/components/ui/textarea"
import { masterDataCopy } from "@/features/master-data/copy"
import { formatEffectiveRange } from "@/features/master-data/filter"
import { MediaListEditor } from "@/features/master-data/product-editor-media"
import type {
    MasterDataCenterView,
    ProductFields,
    ProductKind,
} from "@/features/master-data/types"
import {
    PRODUCT_KIND_LABELS,
    PRODUCT_KIND_VALUES,
} from "@/features/master-data/types"
import { cn } from "@/lib/utils"
import { formatDateTime } from "@/lib/datetime"

type SetProductFields = (next: React.SetStateAction<ProductFields>) => void

type UnitOption = {
    id: string
    code: string
    label: string
}

type ProductBasicSectionProps = {
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
    return (
        <fieldset
            id="product-section-basic"
            className={cn(
                "scroll-mt-[var(--product-section-scroll-margin)] space-y-3 border-b border-border/70 p-5 last:border-b-0",
            )}
            disabled={!canRevise}
        >
            <legend className="sr-only">
                {masterDataCopy.fieldIdentitySection}
            </legend>
            <div className="text-base font-semibold">
                {masterDataCopy.fieldIdentitySection}
            </div>
            <p className="text-xs text-muted-foreground">
                {masterDataCopy.productEditDesc}
            </p>
            <div className="grid gap-3 sm:grid-cols-2">
                <div className="space-y-1.5 sm:col-span-2">
                    <Label htmlFor="product-no">商品编号</Label>
                    <Input
                        id="product-no"
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
                <div className="space-y-1.5 sm:col-span-2">
                    <Label htmlFor="product-name">名称</Label>
                    <Input
                        id="product-name"
                        value={name}
                        onChange={(e) => setName(e.target.value)}
                        placeholder="商品名称（SPU）"
                    />
                </div>
                <div className="space-y-1.5 sm:col-span-2">
                    <Label htmlFor="product-description">商品描述</Label>
                    <Textarea
                        id="product-description"
                        value={fields.description ?? ""}
                        onChange={(event) =>
                            setFields((previous) => ({
                                ...previous,
                                description: event.target.value,
                            }))
                        }
                        placeholder="公司审核后的商品描述"
                    />
                </div>
                <div className="space-y-1.5 sm:col-span-2">
                    <Label>商品类型</Label>
                    {isCreate ? (
                        <OptionCombobox
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
                        <div className="flex h-9 items-center rounded-md border border-border bg-muted/40 px-3 text-sm">
                            {fields.productKind
                                ? PRODUCT_KIND_LABELS[fields.productKind]
                                : "—"}
                        </div>
                    )}
                    <p className="text-xs text-muted-foreground">
                        决定商品业务作用；创建后不可变，也不随分类变化。
                    </p>
                </div>
                <div className="space-y-1.5">
                    <Label>{masterDataCopy.fBaseUnit}</Label>
                    <OptionCombobox
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
        </fieldset>
    )
}

type ProductMediaSectionProps = {
    canRevise: boolean
    fields: ProductFields
    setFields: SetProductFields
    rememberPendingFiles: (files: File[]) => void
}

function ProductMediaSection({
    canRevise,
    fields,
    setFields,
    rememberPendingFiles,
}: ProductMediaSectionProps) {
    return (
        <fieldset
            id="product-section-media"
            className={cn(
                "scroll-mt-[var(--product-section-scroll-margin)] space-y-5 border-b border-border/70 p-5 last:border-b-0",
            )}
            disabled={!canRevise}
        >
            <legend className="sr-only">
                {masterDataCopy.fieldMediaSection}
            </legend>
            <div className="text-base font-semibold">
                {masterDataCopy.fieldMediaSection}
            </div>
            <p className="text-xs text-muted-foreground">
                {masterDataCopy.productSpuMediaHint}
            </p>
            <section className="space-y-3">
                <MediaListEditor
                    label={masterDataCopy.fCarouselImages}
                    hint="建议上传 3–5 张，支持排序；首张作为商品首图"
                    value={fields.carouselImages}
                    previewUrls={fields.carouselPreviewUrls}
                    onFilesSelected={rememberPendingFiles}
                    onChange={(next) =>
                        setFields((prev) => {
                            const retained = new Set(next)
                            return {
                                ...prev,
                                carouselImages: next,
                                carouselPreviewUrls: Object.fromEntries(
                                    Object.entries(
                                        prev.carouselPreviewUrls,
                                    ).filter(([name]) => retained.has(name)),
                                ),
                                carouselFileAssetIds: Object.fromEntries(
                                    Object.entries(
                                        prev.carouselFileAssetIds,
                                    ).filter(([name]) => retained.has(name)),
                                ),
                            }
                        })
                    }
                    onPreviewUrlsChange={(next) =>
                        setFields((prev) => ({
                            ...prev,
                            carouselPreviewUrls: next,
                        }))
                    }
                />
            </section>
            <div className="border-t border-border" />
            <section className="space-y-3">
                <MediaListEditor
                    label={masterDataCopy.fDetailImages}
                    hint="支持批量上传与顺序调整，保存后详情图随商品版本一起保留"
                    value={fields.detailImages}
                    previewUrls={fields.detailPreviewUrls}
                    onFilesSelected={rememberPendingFiles}
                    mode="detail"
                    onChange={(next) =>
                        setFields((prev) => {
                            const retained = new Set(next)
                            return {
                                ...prev,
                                detailImages: next,
                                detailPreviewUrls: Object.fromEntries(
                                    Object.entries(
                                        prev.detailPreviewUrls,
                                    ).filter(([name]) => retained.has(name)),
                                ),
                                detailFileAssetIds: Object.fromEntries(
                                    Object.entries(
                                        prev.detailFileAssetIds,
                                    ).filter(([name]) => retained.has(name)),
                                ),
                            }
                        })
                    }
                    onPreviewUrlsChange={(next) =>
                        setFields((prev) => ({
                            ...prev,
                            detailPreviewUrls: next,
                        }))
                    }
                />
            </section>
        </fieldset>
    )
}

type ProductEffectiveSectionProps = {
    isCreate: boolean
    canRevise: boolean
    effectiveFrom: string
    effectiveTo: string
    changeReason: string
    setEffectiveFrom: (next: string) => void
    setEffectiveTo: (next: string) => void
    setChangeReason: (next: string) => void
}

function ProductEffectiveSection({
    isCreate,
    canRevise,
    effectiveFrom,
    effectiveTo,
    changeReason,
    setEffectiveFrom,
    setEffectiveTo,
    setChangeReason,
}: ProductEffectiveSectionProps) {
    return (
        <fieldset
            id="product-section-effective"
            className={cn(
                "scroll-mt-[var(--product-section-scroll-margin)] space-y-3 border-b border-border/70 p-5 last:border-b-0",
            )}
            disabled={!canRevise}
        >
            <legend className="sr-only">生效与原因</legend>
            <div className="text-base font-semibold">生效与原因</div>
            <div className="grid gap-3 sm:grid-cols-2">
                <div className="space-y-1.5">
                    <Label htmlFor="ef-from">
                        {masterDataCopy.fieldEffectiveFrom}
                    </Label>
                    <DatePicker
                        value={effectiveFrom || undefined}
                        onValueChange={(next) => setEffectiveFrom(next ?? "")}
                        className="w-full"
                    />
                </div>
                <div className="space-y-1.5">
                    <Label htmlFor="ef-to">
                        {masterDataCopy.fieldEffectiveTo}
                    </Label>
                    <DatePicker
                        value={effectiveTo || undefined}
                        onValueChange={(next) => setEffectiveTo(next ?? "")}
                        className="w-full"
                    />
                </div>
                <div className="space-y-1.5 sm:col-span-2">
                    <Label htmlFor="reason">
                        {masterDataCopy.fieldChangeReason}
                    </Label>
                    <Textarea
                        id="reason"
                        value={changeReason}
                        onChange={(e) => setChangeReason(e.target.value)}
                        rows={2}
                        placeholder={
                            isCreate
                                ? "新建原因"
                                : "说明本次修改内容，保存后形成新版本"
                        }
                    />
                </div>
            </div>
        </fieldset>
    )
}

type ProductHistorySectionProps = {
    data: MasterDataCenterView | null | undefined
}

function ProductHistorySection({ data }: ProductHistorySectionProps) {
    return data ? (
        <section
            id="product-section-history"
            aria-label="历史与引用"
            className={cn(
                "scroll-mt-[var(--product-section-scroll-margin)] px-5",
            )}
        >
            <DocumentSection
                title={masterDataCopy.centerVersions}
                description={masterDataCopy.centerVersionsDesc}
            >
                <RevisionTimeline
                    revisions={data.revisionTimeline.map((rev) => ({
                        id: rev.id,
                        version: rev.revisionNo,
                        source: "erp-change" as const,
                        actor: rev.actor,
                        effectiveAt: {
                            dateTime: rev.effectiveFrom,
                            label: formatEffectiveRange(
                                rev.effectiveFrom,
                                rev.effectiveTo,
                            ),
                        },
                        reason: (
                            <div className="space-y-1">
                                <div>
                                    {masterDataCopy.centerHistoryName}：
                                    <strong>{rev.nameSnapshot}</strong>
                                </div>
                                <div>{rev.changeReason}</div>
                                <div className="flex flex-wrap gap-2">
                                    <Badge variant="outline">
                                        {rev.timingLabel}
                                    </Badge>
                                    <Badge variant="secondary">
                                        {rev.lifecycleAtRevision === "ENABLED"
                                            ? "启用"
                                            : "停用"}
                                    </Badge>
                                </div>
                                {rev.productSnapshot ? (
                                    <details className="mt-2 rounded-lg border bg-muted/30 p-2 text-xs">
                                        <summary className="cursor-pointer font-medium">
                                            查看本版本的完整 SKU 与价格明细
                                        </summary>
                                        <div className="mt-2 space-y-2">
                                            <div>
                                                单位{" "}
                                                {rev.productSnapshot.baseUnit}（
                                                {
                                                    rev.productSnapshot
                                                        .baseUnitCode
                                                }
                                                ） · 分类{" "}
                                                {rev.productSnapshot.category} ·
                                                品牌 {rev.productSnapshot.brand}
                                            </div>
                                            {rev.productSnapshot.skus.map(
                                                (sku) => (
                                                    <div
                                                        key={`${rev.id}:${sku.skuNo}`}
                                                        className="rounded border bg-card p-2"
                                                    >
                                                        <div className="font-medium">
                                                            {sku.skuNo} ·{" "}
                                                            {sku.specLabel}
                                                        </div>
                                                        <div className="mt-1 text-muted-foreground">
                                                            销售价{" "}
                                                            {sku.salePrice ??
                                                                "—"}{" "}
                                                            · 市场价{" "}
                                                            {sku.marketPrice ??
                                                                "—"}
                                                        </div>
                                                    </div>
                                                ),
                                            )}
                                            <p className="text-muted-foreground">
                                                供应商、订货编码、成本、税费和起订量按供给关系独立维护，不写入商品版本。
                                            </p>
                                        </div>
                                    </details>
                                ) : null}
                            </div>
                        ),
                        isCurrent: rev.isCurrent,
                    }))}
                />
            </DocumentSection>

            <DocumentSection
                title={masterDataCopy.centerRelations}
                description={masterDataCopy.centerRelationsDesc}
            >
                <p className="text-sm">
                    {masterDataCopy.centerUsageCount(
                        data.usageSummary.historicalReferenceCount,
                    )}
                    {data.usageSummary.note}
                </p>
                <ul className="mt-3 space-y-2">
                    {data.selectorEligibility.map((s) => (
                        <li
                            key={s.context}
                            className="flex flex-wrap items-center gap-2 rounded-md bg-muted/40 px-2 py-1.5 text-sm"
                        >
                            <span>{s.contextLabel}</span>
                            <Badge
                                variant={s.eligible ? "success" : "destructive"}
                            >
                                {s.eligible
                                    ? masterDataCopy.eligible
                                    : masterDataCopy.ineligible}
                            </Badge>
                            {s.reason ? (
                                <span className="text-xs text-muted-foreground">
                                    {s.reason}
                                </span>
                            ) : null}
                        </li>
                    ))}
                </ul>
            </DocumentSection>

            <DocumentSection
                title={masterDataCopy.centerAudit}
                description={masterDataCopy.centerAuditDesc}
            >
                {data.auditEvents.length === 0 ? (
                    <p className="text-sm text-muted-foreground">
                        {masterDataCopy.centerNoAudit}
                    </p>
                ) : (
                    <ul className="space-y-2 text-sm">
                        {data.auditEvents.map((ev) => (
                            <li
                                key={ev.id}
                                className="rounded-md border border-border px-3 py-2"
                            >
                                <div className="flex flex-wrap gap-2">
                                    <span className="num text-xs text-muted-foreground">
                                        {formatDateTime(
                                            ev.at,
                                            "full",
                                            "passthrough",
                                        )}
                                    </span>
                                    <span>{ev.actor}</span>
                                    <Badge variant="outline">{ev.action}</Badge>
                                </div>
                                <div className="mt-1 text-muted-foreground">
                                    {ev.detail}
                                </div>
                            </li>
                        ))}
                    </ul>
                )}
            </DocumentSection>
        </section>
    ) : null
}

export {
    ProductBasicSection,
    ProductEffectiveSection,
    ProductHistorySection,
    ProductMediaSection,
}
export type { SetProductFields }
