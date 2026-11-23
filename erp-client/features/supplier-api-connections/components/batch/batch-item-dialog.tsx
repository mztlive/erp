"use client"

import * as React from "react"
import { useStore } from "@tanstack/react-form"
import {
    AlertCircleIcon,
    ChevronDownIcon,
    PackageIcon,
    TriangleAlertIcon,
} from "lucide-react"
import { z } from "zod"
import { BusinessStatusBadge, MoneyValue } from "@/components/business"
import { useAppForm } from "@/components/form"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
} from "@/components/ui/collapsible"
import {
    Dialog,
    DialogBody,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import {
    Field,
    FieldDescription,
    FieldError,
    FieldLabel,
} from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { LoadingButton } from "@/components/ui/loading-button"
import { DeliveryTemplateField } from "@/features/supplier-offerings/components/delivery-templates/delivery-template-field"
import { SupplierTaxRateField } from "@/features/supplier-offerings/components/supplier-tax-rate-field"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { compareDecimal, parseDecimal } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import type { BatchFieldEdit } from "../../api/batch"
import type { CatalogItem } from "../../api/catalog"
import { catalogUnitLabel } from "../../api/catalog"
import { CatalogSourceImage } from "../catalog/catalog-source-image"
import { SourcePurchaseTerms } from "../catalog/source-purchase-terms"

export type BatchItemDraft = {
    dropship: string
    bulk: string
    minimumQuantity: string
    marketPrice: string
    name: string
    description: string
    categoryId: string
    unitId: string
    inputTaxPercentage: string
    validFrom: string
    validTo: string
    deliveryTemplateId: string
}

export type BatchItemEdits = Record<keyof BatchItemDraft, BatchFieldEdit>

type BatchItemDialogProps = {
    open: boolean
    item: CatalogItem
    draft: BatchItemDraft
    supplierId: string
    resolution: "CREATE_PRODUCT" | "CREATE_SKU" | "EXISTING" | null
    categoryOptions: readonly { value: string; label: string }[]
    unitOptions: readonly { value: string; label: string }[]
    bulkLinked?: boolean
    dropshipOnly?: boolean
    manuallyAdjusted?: boolean
    readOnly?: boolean
    onSave: (edits: BatchItemEdits) => Promise<void>
    onRestore: () => Promise<void>
    onOpenChange: (open: boolean) => void
}

const priceLabels = {
    dropship: "代发价（含税）",
    bulk: "集采价（含税）",
    minimumQuantity: "公司集采起订量",
    marketPrice: "市场价（含税）",
} as const

const MORE_DETAIL_FIELDS = [
    "description",
    "inputTaxPercentage",
    "validFrom",
    "validTo",
    "deliveryTemplateId",
] as const

function draftSchema(existing: boolean, bulkLinked: boolean) {
    return z
        .object({
            dropship: z.string(),
            bulk: z.string(),
            minimumQuantity: z.string(),
            marketPrice: z.string(),
            name: z.string(),
            description: z.string(),
            categoryId: z.string(),
            unitId: z.string(),
            inputTaxPercentage: z.string(),
            validFrom: z.string(),
            validTo: z.string(),
            deliveryTemplateId: z.string(),
            explicitClears: z.array(z.string()),
            bulkDetached: z.boolean(),
        })
        .superRefine((value, context) => {
            if (!existing) {
                for (const name of Object.keys(priceLabels) as Array<
                    keyof typeof priceLabels
                >) {
                    if (!value[name].trim()) continue
                    const quantity = name === "minimumQuantity"
                    try {
                        parseDecimal(value[name], {
                            maxScale: quantity ? 6 : 2,
                        })
                        if (
                            quantity &&
                            compareDecimal(value[name], "0", 6) <= 0
                        )
                            throw new Error()
                    } catch {
                        context.addIssue({
                            code: "custom",
                            path: [name],
                            message: quantity
                                ? "请填写大于 0、最多 6 位小数的起订量"
                                : "请填写非负、最多 2 位小数的金额",
                        })
                    }
                }
                const effectiveBulk =
                    bulkLinked && !value.bulkDetached
                        ? value.dropship
                        : value.bulk
                if (
                    Boolean(effectiveBulk.trim()) !==
                    Boolean(value.minimumQuantity.trim())
                ) {
                    for (const name of ["bulk", "minimumQuantity"]) {
                        context.addIssue({
                            code: "custom",
                            path: [name],
                            message: "公司集采价与起订量须同时填写或同时留空",
                        })
                    }
                }
                if (
                    !value.name.trim() ||
                    Array.from(value.name.trim()).length > 128
                ) {
                    context.addIssue({
                        code: "custom",
                        path: ["name"],
                        message: "请填写 1 至 128 个字符的公司展示名称",
                    })
                }
                if (Array.from(value.description.trim()).length > 512) {
                    context.addIssue({
                        code: "custom",
                        path: ["description"],
                        message:
                            "公司介绍最多 512 个字符，可单独整理或明确清空",
                    })
                }
            }
            if (value.inputTaxPercentage.trim()) {
                try {
                    parseDecimal(value.inputTaxPercentage, { maxScale: 4 })
                    if (compareDecimal(value.inputTaxPercentage, "100", 4) > 0)
                        throw new Error()
                } catch {
                    context.addIssue({
                        code: "custom",
                        path: ["inputTaxPercentage"],
                        message: "请填写 0–100、最多 4 位小数的税率",
                    })
                }
            }
            if (
                value.validFrom &&
                value.validTo &&
                value.validTo <= value.validFrom
            ) {
                context.addIssue({
                    code: "custom",
                    path: ["validTo"],
                    message: "供给失效日期必须晚于生效日期",
                })
            }
        })
}

/** 单项调整仅提交字段变更；空值、人工设置和明确清空保持独立。 */
export function BatchItemDialog(props: BatchItemDialogProps) {
    if (!props.open) return null
    return <BatchItemEditor key={props.item.id} {...props} />
}

function BatchItemEditor({
    item,
    draft,
    supplierId,
    resolution,
    categoryOptions,
    unitOptions,
    bulkLinked = false,
    dropshipOnly = false,
    manuallyAdjusted = false,
    readOnly = false,
    onSave,
    onRestore,
    onOpenChange,
}: BatchItemDialogProps) {
    const [saveError, setSaveError] = React.useState<string | null>(null)
    const [restoring, setRestoring] = React.useState(false)
    const [confirmDiscard, setConfirmDiscard] = React.useState(false)
    const [moreOpen, setMoreOpen] = React.useState(false)
    const existing = resolution === "EXISTING"
    const productPreserved = existing || resolution === "CREATE_SKU"
    const sourceUnitKnown = Boolean(item.suggested_unit || item.unit)
    const prefix = `supplier-catalog-batch-item-${toAutomationIdSegment(item.id)}`
    const moreFieldIds: Record<(typeof MORE_DETAIL_FIELDS)[number], string> = {
        description: `${prefix}-description`,
        inputTaxPercentage: `${prefix}-tax`,
        validFrom: `${prefix}-validFrom`,
        validTo: `${prefix}-validTo`,
        deliveryTemplateId: `${prefix}-delivery`,
    }
    const form = useAppForm({
        defaultValues: {
            ...draft,
            explicitClears: [] as string[],
            bulkDetached: false,
        },
        validators: { onSubmit: draftSchema(existing, bulkLinked) },
        onSubmit: async ({ value }) => {
            setSaveError(null)
            const edits = Object.fromEntries(
                (Object.keys(draft) as Array<keyof BatchItemDraft>).map(
                    (name) => {
                        const changed =
                            form.getFieldMeta(name)?.isDirty ||
                            (name === "bulk" && value.bulkDetached) ||
                            value.explicitClears.includes(name)
                        const protectedField =
                            (existing &&
                                [
                                    "dropship",
                                    "bulk",
                                    "minimumQuantity",
                                    "marketPrice",
                                    "name",
                                    "description",
                                    "categoryId",
                                    "unitId",
                                ].includes(name)) ||
                            (productPreserved &&
                                ["name", "description", "categoryId"].includes(
                                    name,
                                )) ||
                            (sourceUnitKnown && name === "unitId")
                        const edit: BatchFieldEdit =
                            !changed || protectedField
                                ? { action: "KEEP" }
                                : value[name].trim()
                                  ? { action: "SET", value: value[name].trim() }
                                  : { action: "CLEAR" }
                        return [name, edit]
                    },
                ),
            ) as BatchItemEdits
            try {
                await onSave(edits)
                form.reset(value)
                onOpenChange(false)
            } catch (error) {
                setSaveError(
                    error instanceof Error
                        ? error.message
                        : "保存失败，请重试；填写内容仍保留。",
                )
            }
        },
    })
    const values = useStore(form.store, (state) => state.values)
    const fieldMeta = useStore(form.store, (state) => state.fieldMeta)
    const submitting = useStore(form.store, (state) => state.isSubmitting)
    const dirty = useStore(form.store, (state) => !state.isDefaultValue)
    const moreErrorFields = MORE_DETAIL_FIELDS.filter(
        (name) => (fieldMeta[name]?.errors?.length ?? 0) > 0,
    )
    const busy = submitting || restoring
    const requestClose = () => {
        if (busy) return
        if (dirty) setConfirmDiscard(true)
        else onOpenChange(false)
    }
    const clear = (name: keyof BatchItemDraft) => {
        form.setFieldValue(name, "")
        if (name === "bulk") form.setFieldValue("bulkDetached", true)
        form.setFieldValue("explicitClears", (current) =>
            Array.from(new Set([...current, name])),
        )
    }
    const restore = async () => {
        setRestoring(true)
        setSaveError(null)
        try {
            await onRestore()
            form.reset()
            onOpenChange(false)
        } catch (error) {
            setSaveError(
                error instanceof Error ? error.message : "恢复失败，请重试。",
            )
        } finally {
            setRestoring(false)
        }
    }
    return (
        <Dialog
            open
            onOpenChange={(open) => {
                if (!open) requestClose()
            }}
        >
            <DialogContent
                id={`${prefix}-dialog`}
                size="xl"
                scroll="body"
                closeButtonId={`${prefix}-close`}
                showCloseButton={!busy}
            >
                <DialogHeader>
                    <DialogTitle>调整入库商品</DialogTitle>
                    <DialogDescription>
                        {item.name} · {item.specification}
                    </DialogDescription>
                </DialogHeader>
                <form
                    id={`${prefix}-form`}
                    className="flex min-h-0 flex-1 flex-col overflow-hidden"
                    onSubmit={async (event) => {
                        event.preventDefault()
                        event.stopPropagation()
                        await form.handleSubmit()
                        const firstMoreError = MORE_DETAIL_FIELDS.find(
                            (name) =>
                                (form.getFieldMeta(name)?.errors?.length ?? 0) >
                                0,
                        )
                        if (firstMoreError) {
                            setMoreOpen(true)
                            window.requestAnimationFrame(() => {
                                const target = document.getElementById(
                                    moreFieldIds[firstMoreError],
                                )
                                target?.scrollIntoView({
                                    behavior: "smooth",
                                    block: "center",
                                })
                                if (target instanceof HTMLElement) {
                                    target.focus({ preventScroll: true })
                                }
                            })
                        }
                    }}
                >
                    <DialogBody>
                        <fieldset
                            disabled={busy || readOnly}
                            className="space-y-5"
                        >
                            <div className="flex flex-wrap items-center gap-3 text-body-compact">
                                <span className="flex size-16 shrink-0 items-center justify-center overflow-hidden rounded-lg border bg-muted">
                                    {item.images?.[0] ? (
                                        <CatalogSourceImage
                                            image={item.images[0]}
                                            alt=""
                                            width={64}
                                            height={64}
                                            compact
                                            className="size-full object-cover"
                                        />
                                    ) : (
                                        <PackageIcon
                                            className="size-6 text-muted-foreground"
                                            aria-hidden="true"
                                        />
                                    )}
                                </span>
                                <div className="min-w-0 flex-1 space-y-1">
                                    <p className="font-medium">
                                        {item.name} · {item.specification}
                                    </p>
                                    <div className="flex flex-wrap items-center gap-2">
                                        <span className="num text-muted-foreground">
                                            来源 SKU：{item.spec_id}
                                        </span>
                                        <BusinessStatusBadge
                                            tone={
                                                !resolution
                                                    ? "warning"
                                                    : existing
                                                      ? "neutral"
                                                      : "success"
                                            }
                                            label={
                                                !resolution
                                                    ? "待自动核对"
                                                    : existing
                                                      ? "关联已有 SKU · 保留原资料"
                                                      : resolution ===
                                                          "CREATE_SKU"
                                                        ? "已有商品新增规格"
                                                        : "新建 SKU"
                                            }
                                        />
                                        {manuallyAdjusted ? (
                                            <BusinessStatusBadge
                                                tone="warning"
                                                label="已单独调整"
                                            />
                                        ) : null}
                                    </div>
                                </div>
                            </div>
                            <dl className="grid grid-cols-1 gap-4 rounded-lg bg-muted/40 px-4 py-3 sm:grid-cols-3">
                                <div>
                                    <dt className="text-xs text-muted-foreground">
                                        接口采购报价
                                    </dt>
                                    <dd className="num mt-1 text-value-compact font-semibold tracking-tight">
                                        {item.purchase_price !== null ? (
                                            <MoneyValue
                                                value={item.purchase_price}
                                            />
                                        ) : (
                                            "未提供"
                                        )}
                                    </dd>
                                </div>
                                <div>
                                    <dt className="text-xs text-muted-foreground">
                                        来源规格
                                    </dt>
                                    <dd className="mt-1">
                                        {item.specification || "未标注"}
                                    </dd>
                                </div>
                                <div>
                                    <dt className="text-xs text-muted-foreground">
                                        来源单位
                                    </dt>
                                    <dd className="mt-1">
                                        {catalogUnitLabel(item)}
                                    </dd>
                                </div>
                            </dl>
                            <SourcePurchaseTerms
                                terms={item.source_purchase_terms}
                                unit={item.unit}
                            />
                            <section
                                className="space-y-3"
                                aria-labelledby={`${prefix}-prices-title`}
                            >
                                <h3
                                    id={`${prefix}-prices-title`}
                                    className="font-semibold"
                                >
                                    公司销售参考价（含税）
                                </h3>
                                {existing ? (
                                    <p className="text-xs text-muted-foreground">
                                        保留已有 SKU
                                        的原价、上架状态与展示资料。
                                    </p>
                                ) : null}
                                <div className="grid gap-4 sm:grid-cols-2">
                                    {(
                                        Object.keys(priceLabels) as Array<
                                            keyof typeof priceLabels
                                        >
                                    ).map((name) => (
                                        <form.AppField key={name} name={name}>
                                            {(field) => {
                                                const explicitlyCleared =
                                                    values.explicitClears.includes(
                                                        name,
                                                    ) ||
                                                    (field.state.meta.isDirty &&
                                                        !field.state.value.trim())
                                                const linkedBulk =
                                                    bulkLinked &&
                                                    name === "bulk" &&
                                                    !values.bulkDetached &&
                                                    !explicitlyCleared
                                                const displayedValue =
                                                    linkedBulk
                                                        ? values.dropship
                                                        : field.state.value
                                                return (
                                                    <Field>
                                                        <div className="flex items-center justify-between gap-2">
                                                            <FieldLabel
                                                                htmlFor={`${prefix}-${name}`}
                                                            >
                                                                {
                                                                    priceLabels[
                                                                        name
                                                                    ]
                                                                }
                                                            </FieldLabel>
                                                            {!existing ? (
                                                                <Button
                                                                    id={`${prefix}-${name}-clear`}
                                                                    type="button"
                                                                    variant="ghost"
                                                                    size="xs"
                                                                    onClick={() =>
                                                                        clear(
                                                                            name,
                                                                        )
                                                                    }
                                                                >
                                                                    清空
                                                                </Button>
                                                            ) : null}
                                                        </div>
                                                        <Input
                                                            id={`${prefix}-${name}`}
                                                            inputMode="decimal"
                                                            disabled={existing}
                                                            value={
                                                                displayedValue
                                                            }
                                                            onBlur={
                                                                field.handleBlur
                                                            }
                                                            onChange={(
                                                                event,
                                                            ) => {
                                                                field.handleChange(
                                                                    event.target
                                                                        .value,
                                                                )
                                                                if (
                                                                    name ===
                                                                    "bulk"
                                                                )
                                                                    form.setFieldValue(
                                                                        "bulkDetached",
                                                                        true,
                                                                    )
                                                                form.setFieldValue(
                                                                    "explicitClears",
                                                                    (current) =>
                                                                        current.filter(
                                                                            (
                                                                                fieldName,
                                                                            ) =>
                                                                                fieldName !==
                                                                                name,
                                                                        ),
                                                                )
                                                            }}
                                                            aria-invalid={
                                                                field.state.meta
                                                                    .errors
                                                                    .length > 0
                                                            }
                                                            aria-describedby={`${prefix}-${name}-description`}
                                                        />
                                                        <FieldDescription
                                                            id={`${prefix}-${name}-description`}
                                                        >
                                                            {existing
                                                                ? "保留原价"
                                                                : explicitlyCleared
                                                                  ? "明确清空，批量补值默认保留此选择。"
                                                                  : linkedBulk
                                                                    ? "与本行最终代发价联动；单独修改将解除联动。"
                                                                    : field
                                                                            .state
                                                                            .meta
                                                                            .isDirty
                                                                      ? "已单独调整；批量重新计算默认保留。"
                                                                      : name ===
                                                                          "minimumQuantity"
                                                                        ? `按本 SKU 的公司基础单位填写（${catalogUnitLabel(item)}）。`
                                                                        : "留空不等于清空；可恢复批量规则。"}
                                                        </FieldDescription>
                                                        <FieldError
                                                            errors={
                                                                field.state.meta
                                                                    .errors
                                                            }
                                                        />
                                                    </Field>
                                                )
                                            }}
                                        </form.AppField>
                                    ))}
                                </div>
                                <div className="flex flex-wrap items-center gap-3">
                                    {!existing && !readOnly ? (
                                        <LoadingButton
                                            id={`${prefix}-restore-rules`}
                                            type="button"
                                            variant="outline"
                                            loading={restoring}
                                            disabled={submitting}
                                            onClick={() => {
                                                void restore()
                                            }}
                                        >
                                            恢复批量规则
                                        </LoadingButton>
                                    ) : null}
                                    {dropshipOnly ? (
                                        <p className="text-xs text-warning-soft-foreground">
                                            本供给仅支持代发；公司集采参考价不改变供应商采购条件。
                                        </p>
                                    ) : null}
                                </div>
                            </section>
                            <section
                                className="space-y-3 border-t border-grid pt-4"
                                aria-labelledby={`${prefix}-details-title`}
                            >
                                <h3
                                    id={`${prefix}-details-title`}
                                    className="font-semibold"
                                >
                                    建档资料
                                </h3>
                                <div className="grid gap-4 sm:grid-cols-3">
                                    <form.AppField name="name">
                                        {(field) => (
                                            <field.TextField
                                                id={`${prefix}-name`}
                                                label="公司展示名称"
                                                disabled={productPreserved}
                                            />
                                        )}
                                    </form.AppField>
                                    <form.AppField name="unitId">
                                        {(field) =>
                                            sourceUnitKnown || existing ? (
                                                <Field>
                                                    <FieldLabel
                                                        htmlFor={`${prefix}-unitId`}
                                                    >
                                                        公司计量单位
                                                    </FieldLabel>
                                                    <Input
                                                        id={`${prefix}-unitId`}
                                                        readOnly
                                                        value={
                                                            sourceUnitKnown
                                                                ? catalogUnitLabel(
                                                                      item,
                                                                  )
                                                                : (unitOptions.find(
                                                                      (
                                                                          option,
                                                                      ) =>
                                                                          option.value ===
                                                                          field
                                                                              .state
                                                                              .value,
                                                                  )?.label ??
                                                                  "保留原单位")
                                                        }
                                                    />
                                                </Field>
                                            ) : (
                                                <field.SelectField
                                                    id={`${prefix}-unitId`}
                                                    label="公司计量单位"
                                                    options={unitOptions}
                                                />
                                            )
                                        }
                                    </form.AppField>
                                    <form.AppField name="categoryId">
                                        {(field) => (
                                            <field.SelectField
                                                id={`${prefix}-categoryId`}
                                                label="公司分类"
                                                options={categoryOptions}
                                                disabled={productPreserved}
                                            />
                                        )}
                                    </form.AppField>
                                </div>
                                <Collapsible
                                    open={moreOpen}
                                    onOpenChange={setMoreOpen}
                                    className="rounded-lg border border-grid"
                                >
                                    <CollapsibleTrigger
                                        id={`${prefix}-more-details`}
                                        type="button"
                                        className="flex w-full cursor-pointer items-center justify-between gap-2 px-4 py-3 text-left text-body-compact font-medium hover:bg-muted/40"
                                    >
                                        <span className="flex flex-wrap items-center gap-2">
                                            <span>更多资料与供给条件</span>
                                            {moreErrorFields.length > 0 ? (
                                                <BusinessStatusBadge
                                                    className="num"
                                                    tone="destructive"
                                                    label={`${moreErrorFields.length} 项待修正`}
                                                />
                                            ) : null}
                                        </span>
                                        <ChevronDownIcon
                                            aria-hidden="true"
                                            className={cn(
                                                "size-4 shrink-0 text-muted-foreground transition-transform",
                                                moreOpen && "rotate-180",
                                            )}
                                        />
                                    </CollapsibleTrigger>
                                    <CollapsibleContent className="border-t border-grid px-4 py-4">
                                        <div className="space-y-4">
                                            <form.AppField name="description">
                                                {(field) => (
                                                    <field.TextareaField
                                                        id={`${prefix}-description`}
                                                        label="公司商品介绍"
                                                        disabled={
                                                            productPreserved
                                                        }
                                                    />
                                                )}
                                            </form.AppField>
                                            <div className="grid gap-4 sm:grid-cols-3">
                                                <form.AppField name="inputTaxPercentage">
                                                    {(field) => (
                                                        <SupplierTaxRateField
                                                            id={`${prefix}-tax`}
                                                            supplierId={
                                                                supplierId
                                                            }
                                                            value={
                                                                field.state
                                                                    .value
                                                            }
                                                            onChange={
                                                                field.handleChange
                                                            }
                                                            onBlur={
                                                                field.handleBlur
                                                            }
                                                            errors={
                                                                field.state.meta
                                                                    .errors
                                                            }
                                                        />
                                                    )}
                                                </form.AppField>
                                                <form.AppField name="validFrom">
                                                    {(field) => (
                                                        <field.DateField
                                                            id={`${prefix}-validFrom`}
                                                            label="供给生效日期"
                                                        />
                                                    )}
                                                </form.AppField>
                                                <form.AppField name="validTo">
                                                    {(field) => (
                                                        <field.DateField
                                                            id={`${prefix}-validTo`}
                                                            label="供给失效日期"
                                                            description="留空表示不设失效日期。"
                                                        />
                                                    )}
                                                </form.AppField>
                                            </div>
                                            <form.AppField name="deliveryTemplateId">
                                                {(field) => (
                                                    <DeliveryTemplateField
                                                        id={`${prefix}-delivery`}
                                                        supplierId={supplierId}
                                                        value={
                                                            field.state.value
                                                        }
                                                        onChange={
                                                            field.handleChange
                                                        }
                                                    />
                                                )}
                                            </form.AppField>
                                        </div>
                                    </CollapsibleContent>
                                </Collapsible>
                            </section>
                        </fieldset>
                    </DialogBody>
                    {saveError ? (
                        <div className="shrink-0 border-t border-border/80 px-overlay-padding py-3">
                            <Alert variant="destructive">
                                <AlertCircleIcon aria-hidden="true" />
                                <AlertDescription>{saveError}</AlertDescription>
                            </Alert>
                        </div>
                    ) : null}
                    {confirmDiscard ? (
                        <div className="shrink-0 border-t border-border/80 px-overlay-padding py-3">
                            <Alert variant="warning">
                                <TriangleAlertIcon aria-hidden="true" />
                                <AlertDescription className="flex flex-wrap items-center justify-between gap-2">
                                    <span>尚有未保存调整，确认放弃？</span>
                                    <div className="flex gap-2">
                                        <Button
                                            id={`${prefix}-keep-editing`}
                                            type="button"
                                            variant="outline"
                                            size="sm"
                                            onClick={() =>
                                                setConfirmDiscard(false)
                                            }
                                        >
                                            继续调整
                                        </Button>
                                        <Button
                                            id={`${prefix}-discard`}
                                            type="button"
                                            size="sm"
                                            onClick={() => {
                                                form.reset()
                                                onOpenChange(false)
                                            }}
                                        >
                                            放弃调整
                                        </Button>
                                    </div>
                                </AlertDescription>
                            </Alert>
                        </div>
                    ) : null}
                    <DialogFooter className="items-center sm:justify-between">
                        <p className="text-xs text-muted-foreground">
                            保存后须重新核对，旧预览不可提交。
                        </p>
                        <div className="flex justify-end gap-2">
                            <Button
                                id={`${prefix}-cancel`}
                                type="button"
                                variant="outline"
                                disabled={busy}
                                onClick={requestClose}
                            >
                                取消
                            </Button>
                            {!readOnly ? (
                                <LoadingButton
                                    id={`${prefix}-save`}
                                    type="submit"
                                    loading={submitting}
                                    disabled={restoring}
                                >
                                    保存调整
                                </LoadingButton>
                            ) : null}
                        </div>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
