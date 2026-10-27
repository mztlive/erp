"use client"

import {
    AdoptionSalesPrices,
    SALES_PRICE_LABELS,
} from "./adoption-sales-prices"
import { catalogUnitLabel } from "../../api/catalog"
import { AUTO_UNIT } from "../../hooks/use-adoption-batch"

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
import { DialogFooter } from "@/components/ui/dialog"
import { DatePicker } from "@/components/ui/date-picker"
import {
    Field,
    FieldDescription,
    FieldError,
    FieldLabel,
} from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { LoadingButton } from "@/components/ui/loading-button"
import { RequiredMark } from "@/components/ui/required-mark"
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
            className="space-y-3 border-t border-grid pt-4"
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
    onCancel,
}: {
    connectionId: string
    supplierId: string
    rows: AdoptionPreview["rows"]
    preview: AdoptionPreview
    onComplete: (rows: AdoptionResultRow[]) => void
    onPendingChange: (pending: boolean) => void
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
    const [submitCount, setSubmitCount] = React.useState(0)

    React.useEffect(() => {
        onPendingChange(mutation.isPending)
    }, [mutation.isPending, onPendingChange])

    React.useEffect(() => {
        return () => onPendingChange(false)
    }, [onPendingChange])

    React.useEffect(() => {
        detailRef.current?.scrollTo({ top: 0 })
    }, [index])

    const item = rows[index]?.item
    const targetData = editor.target?.data
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
    const nextIssueIndex = rowStates.findIndex(
        (state) => state === "INCOMPLETE" || state === "FAILED",
    )
    const fieldOptions = {
        categoryId: (editor.categories.data?.rows ?? []).map((row) => ({
            value: row.stableId,
            label: row.name,
        })),
        unitId: [
            ...(item.suggested_unit || item.unit
                ? [
                      {
                          value: AUTO_UNIT,
                          label: `${item.suggested_unit || item.unit}（自动匹配，缺少时创建）`,
                      },
                  ]
                : []),
            ...(editor.units.data ?? []).map((row) => ({
                value: row.id,
                label: row.label,
            })),
        ],
        productKind: PRODUCT_KIND_VALUES.filter(
            (kind) => kind !== "VOUCHER",
        ).map((kind) => ({
            value: kind,
            label: PRODUCT_KIND_LABELS[kind],
        })),
    } as const
    const fieldLabels = {
        categoryId: "公司分类",
        unitId: "公司计量单位",
        productKind: "商品类型",
    } as const
    const issueLabels: Record<string, string> = {
        ...SALES_PRICE_LABELS,
        companyName: "公司展示名称",
        companyDescription: "公司商品介绍",
        categoryId: "公司分类",
        unitId: "公司计量单位",
        productKind: "商品类型",
        specification: "公司规格",
        dropshipPrice: "一件代发采购价（含税）",
        bulkPrice: "集采采购价（含税）",
        minimumQuantity: "集采起订数量",
        inputTaxPercentage: "税率",
        validFrom: "生效日期",
        validTo: "失效日期",
        deliveryTemplateId: "配送模板",
    }
    const targetIds: Record<string, string | undefined> = {
        ...Object.fromEntries(
            Object.keys(SALES_PRICE_LABELS).map((name) => [
                name,
                `${prefix}-${name}`,
            ]),
        ),
        companyName: `${prefix}-company-name`,
        companyDescription: `${prefix}-company-description`,
        categoryId: `${prefix}-categoryId`,
        unitId: `${prefix}-unitId`,
        productKind: `${prefix}-productKind`,
        specification: `${prefix}-specification`,
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
                setSubmitCount((count) => count + 1)
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
                    <div className="space-y-4 px-5 py-4 sm:px-6">
                        <header className="flex flex-col gap-4 sm:flex-row sm:items-start">
                            <ProductThumb item={item} />
                            <div className="min-w-0 flex-1 space-y-1.5">
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
                                <dl className="grid grid-cols-2 gap-x-4 gap-y-1 text-sm sm:grid-cols-4">
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
                                            商品单位
                                        </dt>
                                        <dd className="mt-0.5 break-words">
                                            {catalogUnitLabel(item)}
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
                                                unavailableReason={
                                                    item.purchase_price == null
                                                        ? "未提供"
                                                        : undefined
                                                }
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
                                            将分类、单位、商品类型和共同采购条件应用到其他商品。
                                        </p>
                                    </div>
                                ) : null}
                            </div>
                        </header>

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

                        <EditorSection title="自动匹配结果">
                            <p
                                id={`${prefix}-match-result`}
                                className="text-sm"
                                role="status"
                            >
                                {editor.target?.isFetching
                                    ? "正在匹配公司 SKU…"
                                    : editor.target?.isError
                                      ? editor.target.error.message
                                      : targetData
                                        ? targetData.kind === "EXISTING"
                                            ? `将关联已有 SKU：${targetData.sku_no}`
                                            : targetData.kind === "CREATE_SKU"
                                              ? "将在已有公司商品下新增规格"
                                              : "未匹配到已有 SKU，将新建公司商品及规格"
                                        : "请选择公司计量单位，系统将自动匹配已有 SKU。"}
                            </p>
                            {item.barcode ? (
                                <p className="text-sm text-muted-foreground">
                                    供应商条码：{item.barcode}
                                </p>
                            ) : null}
                            {targetData ? (
                                <p className="text-xs text-muted-foreground">
                                    {targetData.reason}
                                </p>
                            ) : null}
                            <Button
                                id={`${prefix}-refresh-match`}
                                type="button"
                                variant="outline"
                                disabled={
                                    !active.unitId || editor.target?.isFetching
                                }
                                onClick={() => void editor.target?.refetch()}
                            >
                                重新匹配
                            </Button>
                        </EditorSection>

                        <EditorSection title="商品建档">
                            <div className="grid gap-4 sm:grid-cols-2">
                                {(
                                    [
                                        "categoryId",
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
                                                            field.state.value ||
                                                            undefined
                                                        }
                                                        options={
                                                            fieldOptions[name]
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
                                                                "categoryId"
                                                            )
                                                                editor.markCategoryEdited(
                                                                    item.id,
                                                                )
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
                                                                    next ?? "",
                                                                )
                                                        }}
                                                        placeholder={`请选择${label}`}
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
                            {editor.categories.isError ||
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
                                            void editor.units.refetch()
                                        }}
                                    >
                                        重试
                                    </Button>
                                </div>
                            ) : null}
                        </EditorSection>
                        <EditorSection title="公司销售参考价">
                            {targetData?.kind === "EXISTING" ? (
                                <p className="text-sm text-muted-foreground">
                                    关联已有
                                    SKU，保留其销售参考价；需要调整时请打开商品资料。
                                </p>
                            ) : (
                                <AdoptionSalesPrices
                                    form={form}
                                    index={index}
                                    prefix={prefix}
                                    item={item}
                                />
                            )}
                        </EditorSection>
                        <EditorSection title="采购供给">
                            <div className="grid gap-4 sm:grid-cols-2">
                                <form.AppField
                                    name={`rows[${index}].dropshipPrice`}
                                >
                                    {(field) => (
                                        <Field>
                                            <FieldLabel
                                                htmlFor={`${prefix}-dropshipPrice`}
                                            >
                                                一件代发采购价（含税）
                                                <RequiredMark />
                                            </FieldLabel>
                                            <Input
                                                id={`${prefix}-dropshipPrice`}
                                                readOnly
                                                placeholder="供应商暂未提供报价"
                                                inputMode="decimal"
                                                value={field.state.value}
                                                onChange={(event) =>
                                                    field.handleChange(
                                                        event.target.value,
                                                    )
                                                }
                                                onBlur={field.handleBlur}
                                                aria-invalid={
                                                    field.state.meta.errors
                                                        .length > 0
                                                }
                                            />
                                            <FieldDescription>
                                                供应商提供的含税采购价，由接口同步，不可修改。
                                            </FieldDescription>
                                            <FieldError
                                                errors={field.state.meta.errors}
                                            />
                                        </Field>
                                    )}
                                </form.AppField>
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
                                <form.AppField
                                    name={`rows[${index}].validFrom`}
                                >
                                    {(field) => (
                                        <Field>
                                            <FieldLabel
                                                htmlFor={`${prefix}-validFrom`}
                                            >
                                                生效日期
                                                <RequiredMark />
                                            </FieldLabel>
                                            <DatePicker
                                                id={`${prefix}-validFrom`}
                                                size="default"
                                                value={
                                                    field.state.value ||
                                                    undefined
                                                }
                                                onValueChange={(value) => {
                                                    field.handleChange(
                                                        value ?? "",
                                                    )
                                                    field.handleBlur()
                                                }}
                                                aria-invalid={
                                                    field.state.meta.errors
                                                        .length > 0
                                                }
                                            />
                                            <FieldError
                                                errors={field.state.meta.errors}
                                            />
                                        </Field>
                                    )}
                                </form.AppField>
                            </div>
                            <details
                                key={`${item.id}-${submitCount}`}
                                open={attempted}
                                className="group rounded-lg border bg-muted/20"
                            >
                                <summary
                                    id={`${prefix}-supply-options-toggle`}
                                    className="flex cursor-pointer list-none items-center gap-2 px-3 py-2 text-sm font-medium [&::-webkit-details-marker]:hidden"
                                >
                                    <ChevronRightIcon
                                        className="size-4 text-muted-foreground transition-transform group-open:rotate-90"
                                        aria-hidden="true"
                                    />
                                    更多供给条件
                                </summary>
                                <div className="space-y-3 border-t p-3">
                                    <div className="grid gap-3 sm:grid-cols-2">
                                        {(
                                            [
                                                "bulkPrice",
                                                "minimumQuantity",
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
                                                                    bulkPrice:
                                                                        "集采采购价（含税）",
                                                                    minimumQuantity:
                                                                        "集采起订数量",
                                                                    validTo:
                                                                        "失效日期",
                                                                }[name]
                                                            }
                                                        </FieldLabel>
                                                        {name === "validTo" ? (
                                                            <DatePicker
                                                                id={`${prefix}-${name}`}
                                                                size="default"
                                                                value={
                                                                    field.state
                                                                        .value ||
                                                                    undefined
                                                                }
                                                                onValueChange={(
                                                                    value,
                                                                ) => {
                                                                    field.handleChange(
                                                                        value ??
                                                                            "",
                                                                    )
                                                                    field.handleBlur()
                                                                }}
                                                                aria-invalid={
                                                                    field.state
                                                                        .meta
                                                                        .errors
                                                                        .length >
                                                                    0
                                                                }
                                                            />
                                                        ) : (
                                                            <Input
                                                                id={`${prefix}-${name}`}
                                                                readOnly
                                                                placeholder="接口未提供"
                                                                inputMode="decimal"
                                                                value={
                                                                    field.state
                                                                        .value
                                                                }
                                                                onChange={(
                                                                    event,
                                                                ) =>
                                                                    field.handleChange(
                                                                        event
                                                                            .target
                                                                            .value,
                                                                    )
                                                                }
                                                                aria-invalid={
                                                                    field.state
                                                                        .meta
                                                                        .errors
                                                                        .length >
                                                                    0
                                                                }
                                                            />
                                                        )}
                                                        <FieldError
                                                            errors={
                                                                field.state.meta
                                                                    .errors
                                                            }
                                                        />
                                                    </Field>
                                                )}
                                            </form.AppField>
                                        ))}
                                    </div>
                                    <p className="text-xs text-muted-foreground">
                                        接口未提供集采报价和起订量，本供给仅支持一件代发采购。
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
                                </div>
                            </details>
                        </EditorSection>

                        {targetData?.kind !== "EXISTING" &&
                        (Array.from(item.name.trim()).length > 128 ||
                            Array.from(item.description?.trim() ?? "").length >
                                512) ? (
                            <EditorSection title="整理供应商资料">
                                <AdoptionProductText
                                    key={`${item.id}-${submitCount}`}
                                    form={form}
                                    item={item}
                                    index={index}
                                    prefix={prefix}
                                    attempted={attempted}
                                />
                            </EditorSection>
                        ) : null}
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
                        disabled={
                            readyCount === 0 && !editor.hasUnresolvedRequest
                        }
                    >
                        {editor.hasUnresolvedRequest
                            ? "重试上次入库"
                            : `确认入库（${readyCount} 项）`}
                    </LoadingButton>
                </div>
            </DialogFooter>
        </form>
    )
}
