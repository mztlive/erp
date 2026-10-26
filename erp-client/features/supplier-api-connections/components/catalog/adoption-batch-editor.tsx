"use client"

import * as React from "react"
import Link from "next/link"
import { ChevronRightIcon, LayersIcon, PackageIcon } from "lucide-react"
import {
    MoneyValue,
    OptionCombobox,
    ValidationSummary,
    type ValidationIssue,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { DialogFooter } from "@/components/ui/dialog"
import {
    Field,
    FieldDescription,
    FieldError,
    FieldLabel,
} from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { LoadingButton } from "@/components/ui/loading-button"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import { RequiredMark } from "@/components/ui/required-mark"
import { CompanySkuSearchCombobox } from "@/features/entity-selectors"
import { DeliveryTemplateField } from "@/features/supplier-offerings/components/delivery-templates/delivery-template-field"
import { SupplierTaxRateField } from "@/features/supplier-offerings/components/supplier-tax-rate-field"
import {
    PRODUCT_KIND_LABELS,
    PRODUCT_KIND_VALUES,
} from "@/features/master-data/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import type { AdoptionPreview, AdoptionResultRow } from "../../api/adoption"
import type { CatalogAvailability, CatalogItem } from "../../api/catalog"
import { hasVerifiedCatalogImage } from "../../lib/catalog-media"
import {
    type AdoptionRowState,
    rowSchema,
    useAdoptionBatch,
} from "../../hooks/use-adoption-batch"
import { AdoptionProductText } from "./adoption-product-text"
import { CatalogSourceImage } from "./catalog-source-image"

const availabilityLabels: Record<CatalogAvailability, string> = {
    AVAILABLE: "可供",
    UNAVAILABLE: "暂不可供",
    UNKNOWN: "待核验",
    WITHDRAWN: "已下架",
}

function availabilityVariant(availability: CatalogAvailability) {
    if (availability === "AVAILABLE") return "success" as const
    if (availability === "UNKNOWN") return "warning" as const
    return "secondary" as const
}

const rowStateMeta: Record<
    AdoptionRowState,
    { label: string; dot: string; text: string }
> = {
    SUCCEEDED: {
        label: "已入库",
        dot: "bg-success",
        text: "text-success-soft-foreground",
    },
    FAILED: {
        label: "待重试",
        dot: "bg-destructive",
        text: "text-destructive-soft-foreground",
    },
    READY: {
        label: "已就绪",
        dot: "bg-success",
        text: "text-muted-foreground",
    },
    INCOMPLETE: {
        label: "待补充",
        dot: "bg-warning",
        text: "text-warning-soft-foreground",
    },
}

/** 商品缩略图；归档图片不可用时退回占位图标，避免把“图片尚未同步”塞进小方块。 */
function ProductThumb({
    item,
    size = "sm",
}: {
    item: CatalogItem
    size?: "sm" | "lg"
}) {
    const image = item.images?.[0]
    const boxClass = size === "lg" ? "size-20 sm:size-24" : "size-12 sm:size-14"
    if (!image || !hasVerifiedCatalogImage(image)) {
        return (
            <span
                className={cn(
                    "flex shrink-0 items-center justify-center rounded-lg border bg-muted",
                    boxClass,
                )}
            >
                <PackageIcon
                    className="size-5 text-muted-foreground"
                    aria-hidden="true"
                />
            </span>
        )
    }
    return (
        <span
            className={cn(
                "shrink-0 overflow-hidden rounded-lg border bg-muted",
                boxClass,
            )}
        >
            <CatalogSourceImage
                image={image}
                alt=""
                width={size === "lg" ? 96 : 56}
                height={size === "lg" ? 96 : 56}
                className="size-full object-cover"
            />
        </span>
    )
}

function EditorSection({
    id,
    title,
    description,
    children,
}: {
    id?: string
    title: string
    description?: string
    children: React.ReactNode
}) {
    return (
        <section
            aria-labelledby={id}
            className="space-y-4 border-t border-grid pt-5"
        >
            <div className="space-y-0.5">
                <h3 id={id} className="text-sm font-semibold">
                    {title}
                </h3>
                {description ? (
                    <p className="text-xs text-muted-foreground">
                        {description}
                    </p>
                ) : null}
            </div>
            {children}
        </section>
    )
}

export function AdoptionBatchEditor({
    connectionId,
    supplierId,
    rows,
    preview,
    onComplete,
    onPendingChange,
    onDirtyChange,
    onCancel,
}: {
    connectionId: string
    supplierId: string
    rows: AdoptionPreview["rows"]
    preview: AdoptionPreview
    onComplete: (rows: AdoptionResultRow[]) => void
    onPendingChange: (pending: boolean) => void
    onDirtyChange: (dirty: boolean) => void
    onCancel: () => void
}) {
    const editor = useAdoptionBatch({
        connectionId,
        rows,
        preview,
        onComplete,
    })
    const { form, active, activeIndex: index, mutation, rowStates } = editor
    const detailRef = React.useRef<HTMLDivElement>(null)
    const [attempted, setAttempted] = React.useState(false)

    React.useEffect(() => {
        onPendingChange(mutation.isPending)
    }, [mutation.isPending, onPendingChange])

    React.useEffect(() => {
        onDirtyChange(editor.isDirty)
    }, [editor.isDirty, onDirtyChange])

    React.useEffect(() => {
        return () => {
            onPendingChange(false)
            onDirtyChange(false)
        }
    }, [onPendingChange, onDirtyChange])

    React.useEffect(() => {
        detailRef.current?.scrollTo({ top: 0 })
    }, [index])

    const item = rows[index]?.item
    const targetData = editor.target.data
    if (!active || !item) return null

    const prefix = `supplier-catalog-adoption-${toAutomationIdSegment(item.id)}`
    const succeededCount = rowStates.filter(
        (state) => state === "SUCCEEDED",
    ).length
    const incompleteCount = rowStates.filter(
        (state) => state === "INCOMPLETE",
    ).length
    const failedCount = rowStates.filter((state) => state === "FAILED").length
    const readyCount = rowStates.filter((state) => state === "READY").length
    const remainingCount = rows.length - succeededCount
    const nextIssueIndex = rowStates.findIndex(
        (state) => state === "INCOMPLETE" || state === "FAILED",
    )
    const sourceImageCount =
        (item.images?.length ?? 0) + (item.detail_images?.length ?? 0)
    const fieldOptions = {
        categoryId: (editor.categories.data?.rows ?? []).map((row) => ({
            value: row.stableId,
            label: row.name,
        })),
        brandId: (editor.brands.data?.rows ?? []).map((row) => ({
            value: row.stableId,
            label: row.name,
        })),
        unitId: (editor.units.data ?? []).map((row) => ({
            value: row.id,
            label: row.label,
        })),
        productKind: PRODUCT_KIND_VALUES.filter(
            (kind) => kind !== "VOUCHER",
        ).map((kind) => ({
            value: kind,
            label: PRODUCT_KIND_LABELS[kind],
        })),
    } as const
    const fieldLabels = {
        categoryId: "公司分类",
        brandId: "公司品牌",
        unitId: "公司计量单位",
        productKind: "商品类型",
    } as const
    const issueLabels: Record<string, string> = {
        companyName: "公司展示名称",
        companyDescription: "公司商品介绍",
        categoryId: "公司分类",
        brandId: "公司品牌",
        unitId: "公司计量单位",
        productKind: "商品类型",
        skuId: "公司商品规格",
        specification: "公司规格",
        confirmed: "核对确认",
        dropshipPrice: "一件代发含税供给价",
        bulkPrice: "集采含税供给价",
        minimumQuantity: "集采起订数量",
        inputTaxPercentage: "税率",
        validFrom: "生效日期",
        validTo: "失效日期",
        deliveryTemplateId: "配送模板",
    }
    const targetIds: Record<string, string | undefined> = {
        companyName: `${prefix}-company-name`,
        companyDescription: `${prefix}-company-description`,
        categoryId: `${prefix}-categoryId`,
        brandId: `${prefix}-brandId`,
        unitId:
            active.kind === "EXISTING"
                ? `${prefix}-company-sku`
                : `${prefix}-unitId`,
        productKind: `${prefix}-productKind`,
        skuId: `${prefix}-company-sku`,
        specification: `${prefix}-specification`,
        confirmed: `${prefix}-confirmed`,
        dropshipPrice: `${prefix}-dropshipPrice`,
        bulkPrice: `${prefix}-bulkPrice`,
        minimumQuantity: `${prefix}-minimumQuantity`,
        inputTaxPercentage: `${prefix}-tax-rate`,
        validFrom: `${prefix}-validFrom`,
        validTo: `${prefix}-validTo`,
        deliveryTemplateId: `${prefix}-delivery-template`,
    }
    const validationIssues: ValidationIssue[] = []
    const parsedActive = rowSchema.safeParse(active)
    if (!parsedActive.success) {
        const byField = new Map<string, string>()
        for (const issue of parsedActive.error.issues) {
            const field = String(issue.path[0] ?? "")
            if (!field || byField.has(field)) continue
            byField.set(field, issue.message)
        }
        for (const [field, message] of byField)
            validationIssues.push({
                id: `${active.itemId}-${field}`,
                label: issueLabels[field] ?? "本项资料",
                message,
                targetId: targetIds[field],
            })
    }

    return (
        <form
            id="supplier-catalog-adoption-batch-form"
            noValidate
            onSubmit={(event) => {
                event.preventDefault()
                setAttempted(true)
                detailRef.current?.scrollTo({ top: 0 })
                void form.handleSubmit()
            }}
            className="flex min-h-0 flex-1 flex-col"
        >
            <fieldset
                disabled={mutation.isPending}
                className={cn(
                    "m-0 flex min-h-0 min-w-0 flex-1 flex-col border-0 p-0",
                    rows.length > 1 &&
                        "lg:grid lg:grid-cols-[264px_minmax(0,1fr)]",
                )}
            >
                {rows.length > 1 ? (
                    <nav
                        aria-label="本批次选品商品"
                        className="shrink-0 border-b border-grid lg:min-h-0 lg:border-r lg:border-b-0"
                    >
                        <div className="flex gap-2 overflow-x-auto p-2.5 sm:p-3 lg:h-full lg:flex-col lg:overflow-x-hidden lg:overflow-y-auto">
                            <div className="hidden items-center justify-between gap-2 px-1 pb-1 lg:flex">
                                <span className="text-xs font-medium text-muted-foreground">
                                    本批 {rows.length} 项
                                </span>
                                <span className="text-xs text-muted-foreground">
                                    {incompleteCount + failedCount > 0
                                        ? `待补 ${incompleteCount + failedCount}`
                                        : "全部就绪"}
                                </span>
                            </div>
                            {rows.map(({ item: row }, rowIndex) => {
                                const state = rowStates[rowIndex]
                                const meta = rowStateMeta[state]
                                const isActive = rowIndex === index
                                return (
                                    <button
                                        key={row.id}
                                        id={`supplier-catalog-adoption-row-${toAutomationIdSegment(row.id)}`}
                                        type="button"
                                        className={cn(
                                            "flex w-56 shrink-0 items-center gap-3 rounded-lg border p-2 text-left transition-colors lg:w-full",
                                            isActive
                                                ? "border-primary bg-secondary"
                                                : "border-transparent hover:bg-muted/60",
                                            state === "SUCCEEDED" &&
                                                "opacity-60",
                                        )}
                                        aria-current={
                                            isActive ? "step" : undefined
                                        }
                                        disabled={state === "SUCCEEDED"}
                                        onClick={() =>
                                            editor.setActiveIndex(rowIndex)
                                        }
                                    >
                                        <ProductThumb item={row} />
                                        <span className="min-w-0 flex-1 space-y-0.5">
                                            <span className="line-clamp-2 block text-sm font-medium">
                                                {row.name}
                                            </span>
                                            <span className="block truncate text-xs text-muted-foreground">
                                                {row.specification ||
                                                    "未标注规格"}
                                            </span>
                                            <span
                                                className={cn(
                                                    "flex items-center gap-1.5 text-xs",
                                                    meta.text,
                                                )}
                                            >
                                                <span
                                                    className={cn(
                                                        "size-1.5 rounded-full",
                                                        meta.dot,
                                                    )}
                                                    aria-hidden="true"
                                                />
                                                {meta.label}
                                            </span>
                                        </span>
                                    </button>
                                )
                            })}
                        </div>
                    </nav>
                ) : null}
                <div ref={detailRef} className="min-h-0 flex-1 overflow-y-auto">
                    <div className="space-y-6 px-5 py-4 sm:px-6 sm:py-5">
                        <header className="flex flex-col gap-4 sm:flex-row sm:items-start">
                            <ProductThumb item={item} size="lg" />
                            <div className="min-w-0 flex-1 space-y-2.5">
                                <div className="flex flex-wrap items-center gap-2">
                                    <h2 className="text-base leading-6 font-semibold">
                                        {item.name}
                                    </h2>
                                    <Badge
                                        variant={availabilityVariant(
                                            item.availability,
                                        )}
                                    >
                                        {availabilityLabels[item.availability]}
                                    </Badge>
                                    {item.requires_identity_review ? (
                                        <Badge variant="warning">
                                            规格待核对
                                        </Badge>
                                    ) : null}
                                </div>
                                <dl className="grid grid-cols-2 gap-x-5 gap-y-2 text-sm">
                                    <div className="min-w-0">
                                        <dt className="text-xs text-muted-foreground">
                                            来源规格
                                        </dt>
                                        <dd className="mt-0.5 break-words">
                                            {item.specification || "未标注"}
                                        </dd>
                                    </div>
                                    <div className="min-w-0">
                                        <dt className="text-xs text-muted-foreground">
                                            来源单位
                                        </dt>
                                        <dd className="mt-0.5 break-words">
                                            {item.unit || "供应商未提供"}
                                        </dd>
                                    </div>
                                    <div className="min-w-0">
                                        <dt className="text-xs text-muted-foreground">
                                            接口采购报价
                                        </dt>
                                        <dd className="mt-0.5">
                                            <MoneyValue
                                                value={item.purchase_price}
                                                display="exact"
                                                unavailableReason="未提供"
                                            />
                                        </dd>
                                    </div>
                                    <div className="min-w-0">
                                        <dt className="text-xs text-muted-foreground">
                                            供应商订货编码
                                        </dt>
                                        <dd className="mt-0.5 break-all">
                                            {item.spec_id || "未提供"}
                                        </dd>
                                    </div>
                                </dl>
                                {rows[index]?.missing_fields.length ? (
                                    <p className="text-xs text-warning-soft-foreground">
                                        本项需确认：
                                        {rows[index].missing_fields.join("、")}
                                    </p>
                                ) : null}
                                {rows.length > 1 ? (
                                    <div className="space-y-1">
                                        <Button
                                            id={`${prefix}-apply-shared`}
                                            type="button"
                                            variant="outline"
                                            size="sm"
                                            onClick={editor.applyShared}
                                        >
                                            <LayersIcon
                                                className="size-4"
                                                aria-hidden="true"
                                            />
                                            应用当前设置到本批
                                        </Button>
                                        <p className="text-xs text-muted-foreground">
                                            将分类、品牌、单位、商品类型和共同采购条件应用到其他商品，每项核对状态会重置。
                                        </p>
                                    </div>
                                ) : null}
                            </div>
                        </header>

                        {sourceImageCount ? (
                            <details className="group rounded-lg border bg-muted/20">
                                <summary
                                    id={`${prefix}-source-images-toggle`}
                                    className="flex cursor-pointer list-none flex-wrap items-center justify-between gap-2 px-3 py-2.5 text-sm font-medium [&::-webkit-details-marker]:hidden"
                                >
                                    <span className="flex items-center gap-2">
                                        <ChevronRightIcon
                                            className="size-4 text-muted-foreground transition-transform group-open:rotate-90"
                                            aria-hidden="true"
                                        />
                                        查看商品轮播图和详情图
                                    </span>
                                    <span className="text-xs font-normal text-muted-foreground">
                                        {sourceImageCount} 张
                                    </span>
                                </summary>
                                <div className="space-y-4 border-t p-3">
                                    {[
                                        {
                                            title: "商品轮播图",
                                            images: item.images,
                                            detail: false,
                                        },
                                        {
                                            title: "商品详情图",
                                            images: item.detail_images,
                                            detail: true,
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
                                                <div className="grid grid-cols-3 gap-2 sm:grid-cols-4">
                                                    {group.images.map(
                                                        (image, position) => (
                                                            <CatalogSourceImage
                                                                key={`${image.file_asset_id ?? image.url}-${position}`}
                                                                image={image}
                                                                alt={`${group.title} ${position + 1}`}
                                                                width={240}
                                                                height={240}
                                                                className={cn(
                                                                    "w-full rounded-md border bg-muted/30",
                                                                    group.detail
                                                                        ? "aspect-[4/3] object-contain"
                                                                        : "aspect-square object-cover",
                                                                )}
                                                            />
                                                        ),
                                                    )}
                                                </div>
                                            </section>
                                        ) : null,
                                    )}
                                </div>
                            </details>
                        ) : null}

                        {editor.rowResults.length ? (
                            <div className="rounded-lg border bg-muted/20 p-3 text-sm">
                                <div className="flex flex-wrap items-center justify-between gap-2">
                                    <p className="font-medium">本批处理结果</p>
                                    <span className="text-xs text-muted-foreground">
                                        已入库 {succeededCount} · 未成功{" "}
                                        {failedCount}
                                    </span>
                                </div>
                                <ul className="mt-2 space-y-1.5">
                                    {editor.rowResults.map((result) => {
                                        const resultItem = preview.rows.find(
                                            (row) =>
                                                row.item.id === result.item_id,
                                        )?.item
                                        return (
                                            <li
                                                key={result.item_id}
                                                className="flex flex-wrap items-center justify-between gap-x-3 gap-y-1"
                                            >
                                                <span
                                                    className={cn(
                                                        "min-w-0",
                                                        result.status ===
                                                            "FAILED" &&
                                                            "text-destructive-soft-foreground",
                                                    )}
                                                >
                                                    {resultItem?.name ??
                                                        result.item_id}
                                                    {" · "}
                                                    {result.status ===
                                                    "SUCCEEDED"
                                                        ? "已入库"
                                                        : result.message ||
                                                          "入库失败，请调整后重试"}
                                                </span>
                                                {result.product_id ? (
                                                    <Link
                                                        id={`supplier-catalog-adoption-result-${toAutomationIdSegment(result.item_id)}`}
                                                        href={`/master-data/products/${encodeURIComponent(result.product_id)}`}
                                                        className="shrink-0 text-primary underline"
                                                    >
                                                        打开公司商品
                                                    </Link>
                                                ) : null}
                                            </li>
                                        )
                                    })}
                                </ul>
                                <p className="mt-2 text-xs text-muted-foreground">
                                    已成功的商品不会重复创建。请修正未成功的项目后再次确认。
                                </p>
                            </div>
                        ) : null}

                        {attempted && validationIssues.length ? (
                            <ValidationSummary
                                id={`${prefix}-validation-summary`}
                                compact
                                title={`本项还需补充 ${validationIssues.length} 项`}
                                issues={validationIssues}
                            />
                        ) : null}

                        <EditorSection
                            id={`${prefix}-target-kind-title`}
                            title="入库方式"
                            description="新建公司商品，或关联已有公司 SKU。"
                        >
                            <form.AppField name={`rows[${index}].kind`}>
                                {(field) => (
                                    <Field>
                                        <RadioGroup
                                            id={`${prefix}-target-kind`}
                                            aria-labelledby={`${prefix}-target-kind-title`}
                                            className="grid w-full max-w-lg grid-cols-2 gap-0"
                                            value={field.state.value}
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
                                        >
                                            {[
                                                {
                                                    value: "CREATE",
                                                    label: "创建公司商品及 SKU",
                                                },
                                                {
                                                    value: "EXISTING",
                                                    label: "关联已有公司 SKU",
                                                },
                                            ].map((option) => (
                                                <RadioGroupItem
                                                    key={option.value}
                                                    id={`${prefix}-target-kind-${option.value.toLowerCase()}`}
                                                    value={option.value}
                                                    variant="segment"
                                                    nativeButton
                                                    render={
                                                        <button
                                                            type="button"
                                                            aria-label={
                                                                option.label
                                                            }
                                                        />
                                                    }
                                                >
                                                    {option.label}
                                                </RadioGroupItem>
                                            ))}
                                        </RadioGroup>
                                        <FieldDescription>
                                            来源商品尚未匹配公司 SKU
                                            时默认新建；已匹配时可改为关联。
                                        </FieldDescription>
                                    </Field>
                                )}
                            </form.AppField>
                        </EditorSection>

                        {active.kind === "CREATE" ? (
                            <EditorSection
                                title="公司资料"
                                description="供应商原文保留只读，这里维护公司商品展示内容。"
                            >
                                <AdoptionProductText
                                    key={item.id}
                                    form={form}
                                    item={item}
                                    index={index}
                                    prefix={prefix}
                                />
                            </EditorSection>
                        ) : null}

                        {active.kind === "CREATE" ? (
                            <EditorSection
                                title="商品归属"
                                description="公司商品按这里的分类、品牌和单位建档。"
                            >
                                <div className="grid gap-4 sm:grid-cols-2">
                                    {(
                                        [
                                            "categoryId",
                                            "brandId",
                                            "unitId",
                                            "productKind",
                                        ] as const
                                    ).map((name) => {
                                        const label = fieldLabels[name]
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
                                                            <RequiredMark />
                                                        </FieldLabel>
                                                        <OptionCombobox
                                                            id={`${prefix}-${name}`}
                                                            value={
                                                                field.state
                                                                    .value ||
                                                                undefined
                                                            }
                                                            options={
                                                                fieldOptions[
                                                                    name
                                                                ]
                                                            }
                                                            allowClear={
                                                                name !==
                                                                "productKind"
                                                            }
                                                            onValueChange={(
                                                                next,
                                                            ) => {
                                                                if (
                                                                    name ===
                                                                    "productKind"
                                                                )
                                                                    form.setFieldValue(
                                                                        `rows[${index}].productKind`,
                                                                        PRODUCT_KIND_VALUES.find(
                                                                            (
                                                                                kind,
                                                                            ) =>
                                                                                kind ===
                                                                                next,
                                                                        ) ??
                                                                            "PHYSICAL",
                                                                    )
                                                                else
                                                                    form.setFieldValue(
                                                                        `rows[${index}].${name}`,
                                                                        next ??
                                                                            "",
                                                                    )
                                                                form.setFieldValue(
                                                                    `rows[${index}].confirmed`,
                                                                    false,
                                                                )
                                                            }}
                                                            placeholder={`请选择${label}`}
                                                            aria-invalid={
                                                                field.state.meta
                                                                    .errors
                                                                    .length > 0
                                                            }
                                                        />
                                                        <FieldError
                                                            errors={
                                                                field.state.meta
                                                                    .errors
                                                            }
                                                        />
                                                    </Field>
                                                )}
                                            </form.AppField>
                                        )
                                    })}
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
                                {editor.categories.isError ||
                                editor.brands.isError ||
                                editor.units.isError ? (
                                    <div
                                        className="flex items-center gap-2 text-sm text-destructive"
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
                            </EditorSection>
                        ) : (
                            <EditorSection
                                title="关联公司 SKU"
                                description="搜索并核对要关联的公司商品规格。"
                            >
                                <div className="space-y-2">
                                    <form.AppField
                                        name={`rows[${index}].skuId`}
                                    >
                                        {(field) => (
                                            <Field>
                                                <FieldLabel
                                                    htmlFor={`${prefix}-company-sku`}
                                                >
                                                    公司商品规格
                                                    <RequiredMark />
                                                </FieldLabel>
                                                <CompanySkuSearchCombobox
                                                    id={`${prefix}-company-sku`}
                                                    value={
                                                        field.state.value ||
                                                        undefined
                                                    }
                                                    onValueChange={(next) => {
                                                        field.handleChange(
                                                            next ?? "",
                                                        )
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
                                                    errors={
                                                        field.state.meta.errors
                                                    }
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
                                        <div className="rounded-lg border bg-muted/20 px-3 py-2 text-sm">
                                            <span className="text-muted-foreground">
                                                公司规格：
                                            </span>
                                            {targetData.specification ||
                                                "未标注"}
                                            <span className="text-muted-foreground">
                                                {" "}
                                                · SKU 编号：
                                            </span>
                                            {targetData.sku_no}
                                            <span className="text-muted-foreground">
                                                {" "}
                                                · 单位：
                                            </span>
                                            {editor.units.data?.find(
                                                (unit) =>
                                                    unit.id ===
                                                    targetData.unit_id,
                                            )?.label ?? "请核对公司商品资料"}
                                        </div>
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
                            </EditorSection>
                        )}

                        <EditorSection
                            title="采购供给"
                            description="报价、税率和有效期写入本次供给；供应商未提供的集采条件可以留空。"
                        >
                            <div className="grid gap-4 sm:grid-cols-2">
                                {item.purchase_price != null ? (
                                    <Field>
                                        <FieldLabel>
                                            一件代发含税供给价
                                            <RequiredMark />
                                        </FieldLabel>
                                        <div
                                            id={`${prefix}-dropshipPrice`}
                                            className="flex min-h-9 items-center justify-between gap-2 rounded-lg border bg-muted/30 px-3 py-1.5"
                                        >
                                            <MoneyValue
                                                value={item.purchase_price}
                                                display="exact"
                                            />
                                            <Badge variant="secondary">
                                                来自供应商接口
                                            </Badge>
                                        </div>
                                        <FieldDescription>
                                            使用接口当前报价，不可手工覆盖。
                                        </FieldDescription>
                                    </Field>
                                ) : (
                                    <form.AppField
                                        name={`rows[${index}].dropshipPrice`}
                                    >
                                        {(field) => (
                                            <Field>
                                                <FieldLabel
                                                    htmlFor={`${prefix}-dropshipPrice`}
                                                >
                                                    一件代发含税供给价
                                                    <RequiredMark />
                                                </FieldLabel>
                                                <Input
                                                    id={`${prefix}-dropshipPrice`}
                                                    inputMode="decimal"
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
                                                <FieldError
                                                    errors={
                                                        field.state.meta.errors
                                                    }
                                                />
                                            </Field>
                                        )}
                                    </form.AppField>
                                )}
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
                                {(
                                    [
                                        "bulkPrice",
                                        "minimumQuantity",
                                        "validFrom",
                                        "validTo",
                                    ] as const
                                ).map((name) => {
                                    const label = {
                                        bulkPrice: "集采含税供给价（可不填）",
                                        minimumQuantity:
                                            "集采起订数量（可不填）",
                                        validFrom: "生效日期",
                                        validTo: "失效日期（可不填）",
                                    }[name]
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
                                                        {name ===
                                                        "validFrom" ? (
                                                            <RequiredMark />
                                                        ) : null}
                                                    </FieldLabel>
                                                    <Input
                                                        id={`${prefix}-${name}`}
                                                        type={
                                                            name ===
                                                                "validFrom" ||
                                                            name === "validTo"
                                                                ? "date"
                                                                : "text"
                                                        }
                                                        inputMode={
                                                            name ===
                                                                "validFrom" ||
                                                            name === "validTo"
                                                                ? undefined
                                                                : "decimal"
                                                        }
                                                        value={
                                                            field.state.value
                                                        }
                                                        onChange={(event) =>
                                                            field.handleChange(
                                                                event.target
                                                                    .value,
                                                            )
                                                        }
                                                        aria-invalid={
                                                            field.state.meta
                                                                .errors.length >
                                                            0
                                                        }
                                                    />
                                                    <FieldError
                                                        errors={
                                                            field.state.meta
                                                                .errors
                                                        }
                                                    />
                                                </Field>
                                            )}
                                        </form.AppField>
                                    )
                                })}
                            </div>
                            <p className="text-xs text-muted-foreground">
                                集采价与集采起订数量需同时填写，或同时留空。
                            </p>
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
                        </EditorSection>

                        <section className="rounded-lg border bg-muted/20 p-4">
                            <form.AppField name={`rows[${index}].confirmed`}>
                                {(field) => (
                                    <Field>
                                        <div className="flex items-start gap-3">
                                            <Checkbox
                                                id={`${prefix}-confirmed`}
                                                checked={field.state.value}
                                                onCheckedChange={(next) =>
                                                    field.handleChange(
                                                        next === true,
                                                    )
                                                }
                                                disabled={
                                                    active.kind ===
                                                        "EXISTING" &&
                                                    (!editor.target.data ||
                                                        editor.target
                                                            .isFetching)
                                                }
                                            />
                                            <div className="space-y-1">
                                                <FieldLabel
                                                    htmlFor={`${prefix}-confirmed`}
                                                    className="leading-5"
                                                >
                                                    已核对本项规格、包装数量和单位，与供应商实际供货一致
                                                </FieldLabel>
                                                <p className="text-xs text-muted-foreground">
                                                    每项需单独核对；确认后才会提交本项。
                                                </p>
                                            </div>
                                        </div>
                                        <FieldError
                                            errors={field.state.meta.errors}
                                        />
                                    </Field>
                                )}
                            </form.AppField>
                        </section>
                    </div>
                </div>
            </fieldset>

            {mutation.error ? (
                <div
                    role="alert"
                    className="shrink-0 border-t border-destructive-border bg-destructive-soft px-6 py-2.5 text-sm text-destructive-soft-foreground"
                >
                    {mutation.error.message}
                </div>
            ) : null}

            <DialogFooter className="shrink-0 flex-col items-stretch gap-3 border-t border-grid bg-background px-6 py-4 sm:flex-row sm:items-center sm:justify-between">
                <div
                    className="min-w-0 space-y-0.5 text-sm text-muted-foreground"
                    role="status"
                    aria-live="polite"
                >
                    <p className="font-medium text-foreground">
                        本批 {rows.length} 项
                    </p>
                    <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs">
                        <span>已就绪 {readyCount}</span>
                        {succeededCount > 0 ? (
                            <span className="text-success-soft-foreground">
                                已入库 {succeededCount}
                            </span>
                        ) : null}
                        {incompleteCount > 0 ? (
                            <span className="text-warning-soft-foreground">
                                {incompleteCount} 项待补充
                            </span>
                        ) : null}
                        {failedCount > 0 ? (
                            <span className="text-destructive-soft-foreground">
                                {failedCount} 项待重试
                            </span>
                        ) : null}
                        {nextIssueIndex >= 0 ? (
                            <Button
                                type="button"
                                variant="link"
                                size="xs"
                                className="h-auto px-0"
                                disabled={mutation.isPending}
                                onClick={() =>
                                    editor.setActiveIndex(nextIssueIndex)
                                }
                            >
                                定位下一项
                            </Button>
                        ) : null}
                    </div>
                </div>
                <div className="flex w-full flex-wrap items-center justify-end gap-2 sm:w-auto">
                    <Button
                        id="supplier-catalog-adoption-cancel"
                        type="button"
                        variant="outline"
                        disabled={mutation.isPending}
                        onClick={onCancel}
                    >
                        取消
                    </Button>
                    <LoadingButton
                        id="supplier-catalog-adoption-confirm-batch"
                        type="submit"
                        loading={mutation.isPending}
                        disabled={remainingCount === 0}
                    >
                        确认入库（{remainingCount} 项）
                    </LoadingButton>
                </div>
            </DialogFooter>
        </form>
    )
}
