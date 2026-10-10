"use client"

import * as React from "react"
import { useStore } from "@tanstack/react-form"
import { Checkbox } from "@/components/ui/checkbox"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import { ChevronDownIcon, PackageIcon } from "lucide-react"
import { DiscardConfirmDialog, OptionCombobox } from "@/components/business"
import type { ProductComboboxItem } from "@/components/business/entity-comboboxes"
import { useAppForm } from "@/components/form"
import { toFieldErrors } from "@/components/form/utils"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import {
    Dialog,
    DialogClose,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import { toast } from "@/components/ui/toast"
import { SupplierSearchCombobox } from "@/features/entity-selectors"
import { useCreateSupplierOfferingMutation } from "@/features/supplier-offerings/hooks/queries"
import {
    errorMessage,
    idempotencyKey,
    rateFromPercentage,
    splitValues,
} from "@/features/supplier-offerings/lib/offering-forms"
import {
    registerSupplyDefaults,
    registerSupplySchema,
} from "@/features/supplier-offerings/lib/register-supply-form"
import type {
    AvailabilityStatus,
    FixedSku,
} from "@/features/supplier-offerings/types"
import { AVAILABILITY_STATUS_LABELS } from "@/features/supplier-offerings/types"
import { SupplierTaxRateField } from "../supplier-tax-rate-field"
import { SupplySkuField } from "../supply-sku-field"
import { SupplyAmountField } from "../supply-amount-field"
import { SupplyRegionField } from "../supply-region-field"

const prefix = "supplier-offerings-dialog-register"
const supplementaryFields = [
    "supplierProductCode",
    "dropshipExpress",
    "freightAmount",
    "serviceFeeAmount",
    "changeReason",
] as const

export function RegisterSupplyForSkuDialog({
    open,
    onOpenChange,
    fixedSku,
    skuOptions,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    fixedSku?: FixedSku
    skuOptions?: readonly FixedSku[]
}) {
    const mutation = useCreateSupplierOfferingMutation()
    const [supplementaryOpen, setSupplementaryOpen] = React.useState(false)
    const [discardOpen, setDiscardOpen] = React.useState(false)
    const [selectedSku, setSelectedSku] = React.useState<ProductComboboxItem>()
    const [submitError, setSubmitError] = React.useState<string | null>(null)
    const [defaults] = React.useState(() =>
        registerSupplyDefaults(fixedSku?.skuId),
    )
    const formElement = React.useRef<HTMLFormElement>(null)
    const form = useAppForm({
        defaultValues: defaults,
        validators: {
            onSubmit: registerSupplySchema,
            onChange: registerSupplySchema,
        },
        onSubmitInvalid: () => {
            if (
                supplementaryFields.some(
                    (key) => form.state.fieldMeta[key]?.errors.length,
                )
            ) {
                setSupplementaryOpen(true)
            }
            requestAnimationFrame(() => {
                const input = formElement.current?.querySelector<HTMLElement>(
                    "[aria-invalid=true]",
                )
                input?.focus()
                input?.scrollIntoView({ block: "nearest" })
            })
        },
        onSubmit: async ({ value }) => {
            setSubmitError(null)
            try {
                await mutation.mutateAsync({
                    sku_id: fixedSku?.skuId ?? value.skuId,
                    supplier_id: value.supplierId,
                    supplier_product_code:
                        value.supplierProductCode.trim() || null,
                    supplier_sku_code: value.supplierSkuCode.trim(),
                    source_type: "MANUAL",
                    terms: {
                        dropship_supply_price_gross: value.dropshipPrice.trim(),
                        bulk_supply_price_gross: value.bulkPrice.trim(),
                        input_tax_rate: rateFromPercentage(
                            value.inputTaxPercentage,
                        ),
                        bulk_minimum_order_quantity:
                            value.minimumQuantity.trim(),
                        supply_region: splitValues(value.supplyRegionText),
                        product_capabilities: [],
                        valid_from: value.validFrom,
                        valid_to:
                            value.validityMode === "dated"
                                ? value.validTo
                                : null,
                        dropship_express: value.dropshipExpress.trim() || null,
                        freight_amount: value.freightAmount.trim() || null,
                        service_fee_amount:
                            value.serviceFeeAmount.trim() || null,
                    },
                    availability_status: value.availabilityStatus,
                    available_quantity: value.availableQuantity.trim() || null,
                    change_reason: value.changeReason.trim(),
                    idempotency_key: idempotencyKey("create-supplier-offering"),
                })
                toast.add({
                    title: "供给已添加",
                    description: "已保存供应商、供货价格与供货条件。",
                    type: "success",
                    timeout: 4000,
                })
                onOpenChange(false)
            } catch (error) {
                setSubmitError(errorMessage(error, "供给登记失败，请稍后重试"))
            }
        },
    })

    React.useEffect(() => {
        if (!open) return
        const beforeUnload = (event: BeforeUnloadEvent) => {
            if (form.state.isDirty || form.state.isSubmitting)
                event.preventDefault()
        }
        window.addEventListener("beforeunload", beforeUnload)
        return () => window.removeEventListener("beforeunload", beforeUnload)
    }, [form, open])

    const selectedSkuId = useStore(form.store, (state) => state.values.skuId)
    const samePrice = useStore(form.store, (state) => state.values.samePrice)
    const quantityMode = useStore(
        form.store,
        (state) => state.values.quantityMode,
    )
    const activeSku =
        fixedSku ?? skuOptions?.find((sku) => sku.skuId === selectedSkuId)
    const unit = activeSku?.baseUnit || selectedSku?.baseUnit
    const skuName =
        activeSku?.productName ||
        activeSku?.skuName ||
        selectedSku?.name ||
        skuOptions?.[0]?.productName
    const skuCode = activeSku?.skuCode || selectedSku?.sku
    const skuDetails = [
        activeSku?.specification || selectedSku?.description,
        skuCode ? `SKU：${skuCode}` : undefined,
        unit ? `单位：${unit}` : undefined,
    ]
    const imageUrl =
        activeSku?.mainImagePreviewUrl ||
        activeSku?.carouselPreviewUrls?.[activeSku.carouselImages?.[0] ?? ""]
    const validateQuantity = ({ value }: { value: string }) =>
        form.getFieldValue("quantityMode") === "provided" && !value.trim()
            ? "请填写可供数量，或选择数量未提供"
            : undefined
    const requestClose = (next: boolean) => {
        if (mutation.isPending || form.state.isSubmitting) return
        if (!next && form.state.isDirty) setDiscardOpen(true)
        else onOpenChange(next)
    }

    return (
        <>
            <Dialog open={open} onOpenChange={requestClose}>
                <DialogContent
                    closeButtonId={`${prefix}-close`}
                    className="flex max-h-[92dvh] w-[calc(100vw-2rem)] flex-col gap-0 overflow-hidden p-0 sm:max-w-[1120px]"
                >
                    <DialogHeader className="shrink-0 px-5 pt-5 pb-4 pr-14 sm:px-6 sm:pr-16">
                        <DialogTitle className="text-xl">添加供给</DialogTitle>
                        <DialogDescription>
                            登记供应商、含税报价与供货条件。
                        </DialogDescription>
                    </DialogHeader>
                    {skuName && (
                        <div className="mx-5 mb-4 flex shrink-0 items-center gap-3 rounded-lg bg-muted/50 px-3 py-2.5 sm:mx-6">
                            {imageUrl ? (
                                // eslint-disable-next-line @next/next/no-img-element -- 复用商品编辑页已解析的对象存储或本地预览地址。
                                <img
                                    src={imageUrl}
                                    alt=""
                                    className="size-8 shrink-0 rounded-md object-cover"
                                />
                            ) : (
                                <PackageIcon
                                    className="size-7 shrink-0 text-muted-foreground"
                                    aria-hidden="true"
                                />
                            )}
                            <div className="flex min-w-0 flex-wrap items-baseline gap-x-4 gap-y-1">
                                <div className="font-medium break-words">
                                    {skuName}
                                </div>
                                <div className="text-xs text-muted-foreground break-words">
                                    {skuDetails.filter(Boolean).join(" · ") ||
                                        "请选择需要添加供给的商品规格"}
                                </div>
                            </div>
                        </div>
                    )}
                    {submitError && (
                        <Alert
                            variant="destructive"
                            className="mx-5 mb-3 w-auto shrink-0 sm:mx-6"
                        >
                            <AlertTitle>保存失败</AlertTitle>
                            <AlertDescription>{submitError}</AlertDescription>
                        </Alert>
                    )}
                    <form
                        ref={formElement}
                        noValidate
                        className="flex min-h-0 flex-1 flex-col overflow-hidden"
                        onSubmit={(event) => {
                            event.preventDefault()
                            void form.handleSubmit()
                        }}
                    >
                        <div className="min-h-0 flex-1 overflow-x-hidden overflow-y-auto overscroll-contain px-5 pb-4 sm:px-6">
                            <fieldset
                                disabled={mutation.isPending}
                                className="min-w-0 space-y-4 [&_[data-slot=field]]:gap-2"
                            >
                                {!fixedSku && (
                                    <form.AppField name="skuId">
                                        {() => (
                                            <SupplySkuField
                                                id={`${prefix}-sku`}
                                                options={skuOptions}
                                                onItemChange={setSelectedSku}
                                            />
                                        )}
                                    </form.AppField>
                                )}
                                <div className="grid min-w-0 gap-y-6 border-t pt-5 lg:grid-cols-[1.2fr_1fr] lg:gap-x-7">
                                    <section
                                        aria-labelledby={`${prefix}-supplier-heading`}
                                        className="min-w-0 space-y-5"
                                    >
                                        <h3
                                            id={`${prefix}-supplier-heading`}
                                            className="text-base font-semibold"
                                        >
                                            供应商与报价
                                        </h3>
                                        <div className="grid items-start gap-4">
                                            <form.AppField name="supplierId">
                                                {(field) => (
                                                    <Field
                                                        className="min-w-0"
                                                        data-invalid={
                                                            (field.state.meta
                                                                .isTouched &&
                                                                !field.state
                                                                    .meta
                                                                    .isValid) ||
                                                            undefined
                                                        }
                                                    >
                                                        <FieldLabel
                                                            htmlFor={`${prefix}-supplier`}
                                                        >
                                                            供应商
                                                            <span className="text-destructive">
                                                                *
                                                            </span>
                                                        </FieldLabel>
                                                        <SupplierSearchCombobox
                                                            id={`${prefix}-supplier`}
                                                            value={
                                                                field.state
                                                                    .value ||
                                                                undefined
                                                            }
                                                            onValueChange={(
                                                                value,
                                                            ) => {
                                                                if (
                                                                    (value ??
                                                                        "") ===
                                                                    field.state
                                                                        .value
                                                                )
                                                                    return
                                                                field.handleChange(
                                                                    value ?? "",
                                                                )
                                                                form.setFieldValue(
                                                                    "inputTaxPercentage",
                                                                    "",
                                                                )
                                                            }}
                                                            onBlur={
                                                                field.handleBlur
                                                            }
                                                            aria-label="供应商"
                                                            required
                                                            allowClear={false}
                                                            aria-invalid={
                                                                field.state.meta
                                                                    .isTouched &&
                                                                !field.state
                                                                    .meta
                                                                    .isValid
                                                            }
                                                            aria-describedby={
                                                                field.state.meta
                                                                    .errors
                                                                    .length
                                                                    ? `${prefix}-supplier-error`
                                                                    : undefined
                                                            }
                                                            placeholder="搜索已启用供应商"
                                                            className="w-full"
                                                        />
                                                        {field.state.meta
                                                            .isTouched && (
                                                            <FieldError
                                                                id={`${prefix}-supplier-error`}
                                                                errors={toFieldErrors(
                                                                    field.state
                                                                        .meta
                                                                        .errors,
                                                                )}
                                                            />
                                                        )}
                                                    </Field>
                                                )}
                                            </form.AppField>
                                            <form.AppField name="supplierSkuCode">
                                                {(field) => (
                                                    <field.TextField
                                                        id={`${prefix}-supplier-sku-code`}
                                                        label="供应商订货编码"
                                                        required
                                                        placeholder="填写供应商货号"
                                                        description="下单使用的货号"
                                                    />
                                                )}
                                            </form.AppField>
                                        </div>
                                        <section
                                            aria-labelledby={`${prefix}-prices-heading`}
                                            className="space-y-3"
                                        >
                                            <h3
                                                id={`${prefix}-prices-heading`}
                                                className="sr-only"
                                            >
                                                含税报价
                                            </h3>
                                            <div className="grid items-start gap-x-5 gap-y-5 sm:grid-cols-2">
                                                <form.AppField
                                                    name="dropshipPrice"
                                                    listeners={{
                                                        onChange: ({
                                                            value,
                                                        }) => {
                                                            if (
                                                                form.getFieldValue(
                                                                    "samePrice",
                                                                )
                                                            )
                                                                form.setFieldValue(
                                                                    "bulkPrice",
                                                                    value,
                                                                )
                                                        },
                                                    }}
                                                >
                                                    {() => (
                                                        <SupplyAmountField
                                                            id={`${prefix}-dropship-price`}
                                                            label="一件代发价"
                                                            placeholder="请输入价格"
                                                            unit={unit}
                                                            required
                                                        />
                                                    )}
                                                </form.AppField>
                                                <div className="min-w-0 space-y-2">
                                                    <form.AppField name="bulkPrice">
                                                        {() => (
                                                            <SupplyAmountField
                                                                id={`${prefix}-bulk-price`}
                                                                label="集采价"
                                                                unit={unit}
                                                                required
                                                                readOnly={
                                                                    samePrice
                                                                }
                                                                placeholder="请输入价格"
                                                            />
                                                        )}
                                                    </form.AppField>
                                                    <form.AppField name="samePrice">
                                                        {(field) => (
                                                            <label
                                                                htmlFor={`${prefix}-copy-price`}
                                                                className="flex w-fit items-center gap-2 text-xs text-muted-foreground"
                                                            >
                                                                <Checkbox
                                                                    className="rounded-sm border-border bg-background"
                                                                    nativeButton
                                                                    render={
                                                                        <button
                                                                            type="button"
                                                                            aria-label="与代发价相同"
                                                                        />
                                                                    }
                                                                    id={`${prefix}-copy-price`}
                                                                    checked={
                                                                        field
                                                                            .state
                                                                            .value
                                                                    }
                                                                    onCheckedChange={(
                                                                        checked,
                                                                    ) => {
                                                                        field.handleChange(
                                                                            checked,
                                                                        )
                                                                        if (
                                                                            checked
                                                                        )
                                                                            form.setFieldValue(
                                                                                "bulkPrice",
                                                                                form.getFieldValue(
                                                                                    "dropshipPrice",
                                                                                ),
                                                                            )
                                                                    }}
                                                                />
                                                                与代发价相同
                                                            </label>
                                                        )}
                                                    </form.AppField>
                                                </div>
                                                <form.Subscribe
                                                    selector={(state) =>
                                                        state.values.supplierId
                                                    }
                                                >
                                                    {(supplierId) => (
                                                        <form.AppField name="inputTaxPercentage">
                                                            {(field) => (
                                                                <SupplierTaxRateField
                                                                    id={`${prefix}-input-tax-percentage`}
                                                                    supplierId={
                                                                        supplierId
                                                                    }
                                                                    prefill
                                                                    value={
                                                                        field
                                                                            .state
                                                                            .value
                                                                    }
                                                                    onChange={
                                                                        field.handleChange
                                                                    }
                                                                    onBlur={
                                                                        field.handleBlur
                                                                    }
                                                                    errors={
                                                                        field
                                                                            .state
                                                                            .meta
                                                                            .isTouched
                                                                            ? field
                                                                                  .state
                                                                                  .meta
                                                                                  .errors
                                                                            : []
                                                                    }
                                                                />
                                                            )}
                                                        </form.AppField>
                                                    )}
                                                </form.Subscribe>
                                                <form.AppField name="minimumQuantity">
                                                    {(field) => (
                                                        <field.TextField
                                                            id={`${prefix}-minimum-quantity`}
                                                            label={`集采起订量${unit ? `（${unit}）` : ""}`}
                                                            required
                                                            inputMode="decimal"
                                                        />
                                                    )}
                                                </form.AppField>
                                            </div>
                                        </section>
                                    </section>
                                    <div className="min-w-0 space-y-6 border-t pt-5 lg:border-t-0 lg:border-l lg:pt-0 lg:pl-7">
                                        <section
                                            aria-labelledby={`${prefix}-terms-heading`}
                                            className="min-w-0 space-y-5"
                                        >
                                            <h3
                                                id={`${prefix}-terms-heading`}
                                                className="text-base font-semibold"
                                            >
                                                供货条件
                                            </h3>
                                            <form.AppField name="supplyRegionText">
                                                {() => (
                                                    <SupplyRegionField
                                                        id={`${prefix}-supply-region`}
                                                    />
                                                )}
                                            </form.AppField>
                                            <div className="grid items-start gap-x-5 gap-y-5 sm:grid-cols-2">
                                                <form.AppField name="validFrom">
                                                    {(field) => (
                                                        <field.DateField
                                                            id={`${prefix}-valid-from`}
                                                            label="生效日期"
                                                            required
                                                            clearable={false}
                                                            inputClassName="h-9"
                                                        />
                                                    )}
                                                </form.AppField>
                                                <form.AppField name="validityMode">
                                                    {(field) => (
                                                        <Field className="min-w-0">
                                                            <FieldLabel
                                                                htmlFor={`${prefix}-validity-mode`}
                                                            >
                                                                有效期
                                                            </FieldLabel>
                                                            <OptionCombobox
                                                                id={`${prefix}-validity-mode`}
                                                                aria-label="有效期"
                                                                value={
                                                                    field.state
                                                                        .value
                                                                }
                                                                allowClear={
                                                                    false
                                                                }
                                                                options={[
                                                                    {
                                                                        value: "ongoing",
                                                                        label: "长期有效",
                                                                    },
                                                                    {
                                                                        value: "dated",
                                                                        label: "指定失效日期",
                                                                    },
                                                                ]}
                                                                onValueChange={(
                                                                    value,
                                                                ) => {
                                                                    field.handleChange(
                                                                        value ===
                                                                            "dated"
                                                                            ? "dated"
                                                                            : "ongoing",
                                                                    )
                                                                    if (
                                                                        value !==
                                                                        "dated"
                                                                    )
                                                                        form.setFieldValue(
                                                                            "validTo",
                                                                            "",
                                                                        )
                                                                }}
                                                            />
                                                        </Field>
                                                    )}
                                                </form.AppField>
                                                <form.Subscribe
                                                    selector={(state) =>
                                                        state.values
                                                            .validityMode
                                                    }
                                                >
                                                    {(mode) =>
                                                        mode === "dated" && (
                                                            <div className="sm:col-start-2">
                                                                <form.AppField name="validTo">
                                                                    {(
                                                                        field,
                                                                    ) => (
                                                                        <field.DateField
                                                                            id={`${prefix}-valid-to`}
                                                                            label="失效日期"
                                                                            required
                                                                            clearable={
                                                                                false
                                                                            }
                                                                            inputClassName="h-9"
                                                                        />
                                                                    )}
                                                                </form.AppField>
                                                            </div>
                                                        )
                                                    }
                                                </form.Subscribe>
                                                <form.AppField name="availabilityStatus">
                                                    {(field) => (
                                                        <Field className="min-w-0 sm:col-span-2">
                                                            <FieldLabel
                                                                htmlFor={`${prefix}-availability-status`}
                                                            >
                                                                当前可供状态
                                                                <span className="text-destructive">
                                                                    *
                                                                </span>
                                                            </FieldLabel>
                                                            <OptionCombobox
                                                                id={`${prefix}-availability-status`}
                                                                aria-label="当前可供状态"
                                                                value={
                                                                    field.state
                                                                        .value
                                                                }
                                                                allowClear={
                                                                    false
                                                                }
                                                                onValueChange={(
                                                                    value,
                                                                ) =>
                                                                    field.handleChange(
                                                                        (value ??
                                                                            "AVAILABLE") as AvailabilityStatus,
                                                                    )
                                                                }
                                                                options={Object.entries(
                                                                    AVAILABILITY_STATUS_LABELS,
                                                                ).map(
                                                                    ([
                                                                        value,
                                                                        label,
                                                                    ]) => ({
                                                                        value,
                                                                        label,
                                                                    }),
                                                                )}
                                                            />
                                                        </Field>
                                                    )}
                                                </form.AppField>
                                                <div className="min-w-0 space-y-2 sm:col-span-2">
                                                    <form.AppField name="quantityMode">
                                                        {(field) => (
                                                            <Field>
                                                                <FieldLabel
                                                                    id={`${prefix}-quantity-label`}
                                                                >
                                                                    当前可供数量
                                                                </FieldLabel>
                                                                <RadioGroup
                                                                    id={`${prefix}-quantity-mode`}
                                                                    aria-labelledby={`${prefix}-quantity-label`}
                                                                    value={
                                                                        field
                                                                            .state
                                                                            .value
                                                                    }
                                                                    className="flex min-h-9 flex-wrap items-center gap-4"
                                                                    onValueChange={(
                                                                        value,
                                                                    ) => {
                                                                        field.handleChange(
                                                                            value ===
                                                                                "provided"
                                                                                ? "provided"
                                                                                : "unknown",
                                                                        )
                                                                        if (
                                                                            value !==
                                                                            "provided"
                                                                        )
                                                                            form.resetField(
                                                                                "availableQuantity",
                                                                            )
                                                                    }}
                                                                >
                                                                    <label
                                                                        htmlFor={`${prefix}-quantity-unknown`}
                                                                        className="flex items-center gap-2 text-xs"
                                                                    >
                                                                        <RadioGroupItem
                                                                            className="border-border bg-background"
                                                                            nativeButton
                                                                            render={
                                                                                <button
                                                                                    type="button"
                                                                                    aria-label="数量未提供"
                                                                                />
                                                                            }
                                                                            id={`${prefix}-quantity-unknown`}
                                                                            value="unknown"
                                                                        />
                                                                        数量未提供
                                                                    </label>
                                                                    <label
                                                                        htmlFor={`${prefix}-quantity-provided`}
                                                                        className="flex items-center gap-2 text-xs"
                                                                    >
                                                                        <RadioGroupItem
                                                                            className="border-border bg-background"
                                                                            nativeButton
                                                                            render={
                                                                                <button
                                                                                    type="button"
                                                                                    aria-label="填写数量"
                                                                                />
                                                                            }
                                                                            id={`${prefix}-quantity-provided`}
                                                                            value="provided"
                                                                        />
                                                                        填写数量
                                                                    </label>
                                                                </RadioGroup>
                                                            </Field>
                                                        )}
                                                    </form.AppField>
                                                    {quantityMode ===
                                                        "provided" && (
                                                        <form.AppField
                                                            name="availableQuantity"
                                                            validators={{
                                                                onChangeListenTo:
                                                                    [
                                                                        "quantityMode",
                                                                    ],
                                                                onChange:
                                                                    validateQuantity,
                                                                onBlur: validateQuantity,
                                                                onSubmit:
                                                                    validateQuantity,
                                                            }}
                                                        >
                                                            {(field) => (
                                                                <field.TextField
                                                                    id={`${prefix}-available-quantity`}
                                                                    label={`可供数量${unit ? `（${unit}）` : ""}`}
                                                                    required
                                                                    inputMode="decimal"
                                                                    placeholder="请输入可供数量"
                                                                    description="填写 0 时，这条供给暂不能用于销售。"
                                                                />
                                                            )}
                                                        </form.AppField>
                                                    )}
                                                </div>
                                            </div>
                                            <form.Subscribe
                                                selector={(state) =>
                                                    [
                                                        state.values
                                                            .availabilityStatus,
                                                        state.values
                                                            .availableQuantity,
                                                    ] as const
                                                }
                                            >
                                                {([status, quantity]) =>
                                                    status !== "AVAILABLE" ||
                                                    /^0+(?:\.0+)?$/.test(
                                                        quantity.trim(),
                                                    ) ? (
                                                        <p
                                                            role="status"
                                                            className="text-sm text-warning-soft-foreground"
                                                        >
                                                            {status !==
                                                            "AVAILABLE"
                                                                ? "当前状态不支持销售；恢复可供后还需满足价格和有效期等销售条件。"
                                                                : "当前数量为 0，这条供给暂不能用于销售。"}
                                                        </p>
                                                    ) : null
                                                }
                                            </form.Subscribe>
                                        </section>
                                        <details
                                            className="group border-t pt-3"
                                            open={supplementaryOpen}
                                            onToggle={(event) =>
                                                setSupplementaryOpen(
                                                    event.currentTarget.open,
                                                )
                                            }
                                        >
                                            <summary
                                                id="supplier-offerings-register-logistics-toggle"
                                                className="flex cursor-pointer list-none flex-wrap items-center gap-x-2 gap-y-1 rounded-sm text-sm font-medium outline-none focus-visible:ring-2 focus-visible:ring-ring [&::-webkit-details-marker]:hidden"
                                            >
                                                <ChevronDownIcon className="size-4 shrink-0 -rotate-90 transition-transform group-open:rotate-0" />
                                                补充信息
                                                <span className="font-normal text-muted-foreground">
                                                    （选填）
                                                </span>
                                                <form.Subscribe
                                                    selector={(state) =>
                                                        supplementaryFields.filter(
                                                            (key) =>
                                                                state.values[
                                                                    key
                                                                ] &&
                                                                state.values[
                                                                    key
                                                                ] !==
                                                                    defaults[
                                                                        key
                                                                    ],
                                                        ).length
                                                    }
                                                >
                                                    {(count) =>
                                                        count > 0 && (
                                                            <span className="ml-auto text-xs font-normal text-muted-foreground">
                                                                已填 {count} 项
                                                            </span>
                                                        )
                                                    }
                                                </form.Subscribe>
                                            </summary>
                                            <div className="mt-4 grid items-start gap-x-5 gap-y-5 sm:grid-cols-2">
                                                <form.AppField name="supplierProductCode">
                                                    {(field) => (
                                                        <field.TextField
                                                            id={`${prefix}-supplier-product-code`}
                                                            label="供应商商品编码"
                                                        />
                                                    )}
                                                </form.AppField>
                                                <form.AppField name="dropshipExpress">
                                                    {(field) => (
                                                        <field.TextField
                                                            id={`${prefix}-dropship-express`}
                                                            label="一件代发快递说明"
                                                        />
                                                    )}
                                                </form.AppField>
                                                <form.AppField name="freightAmount">
                                                    {() => (
                                                        <SupplyAmountField
                                                            id={`${prefix}-freight-amount`}
                                                            label="运费"
                                                        />
                                                    )}
                                                </form.AppField>
                                                <form.AppField name="serviceFeeAmount">
                                                    {() => (
                                                        <SupplyAmountField
                                                            id={`${prefix}-service-fee-amount`}
                                                            label="服务费"
                                                        />
                                                    )}
                                                </form.AppField>
                                                <div className="sm:col-span-2">
                                                    <form.AppField name="changeReason">
                                                        {(field) => (
                                                            <field.TextField
                                                                id={`${prefix}-change-reason`}
                                                                label="登记说明"
                                                                required
                                                            />
                                                        )}
                                                    </form.AppField>
                                                </div>
                                            </div>
                                        </details>
                                    </div>
                                </div>
                            </fieldset>
                        </div>
                        <DialogFooter className="shrink-0 flex-row items-center justify-end border-t px-5 py-3 sm:px-6">
                            <p className="mr-auto text-xs text-muted-foreground">
                                带 <span className="text-destructive">*</span>{" "}
                                为必填项
                            </p>
                            <DialogClose
                                render={
                                    <Button
                                        id={`${prefix}-cancel`}
                                        type="button"
                                        variant="outline"
                                        disabled={mutation.isPending}
                                    />
                                }
                            >
                                取消
                            </DialogClose>
                            <form.Subscribe
                                selector={(state) => state.isSubmitting}
                            >
                                {(isSubmitting) => (
                                    <LoadingButton
                                        id={`${prefix}-submit`}
                                        loading={
                                            mutation.isPending || isSubmitting
                                        }
                                        type="submit"
                                        disabled={
                                            mutation.isPending || isSubmitting
                                        }
                                    >
                                        {mutation.isPending || isSubmitting
                                            ? "正在保存…"
                                            : "保存供给"}
                                    </LoadingButton>
                                )}
                            </form.Subscribe>
                        </DialogFooter>
                    </form>
                </DialogContent>
            </Dialog>
            <DiscardConfirmDialog
                idPrefix={`${prefix}-discard`}
                open={discardOpen}
                onOpenChange={setDiscardOpen}
                onConfirm={() => {
                    setDiscardOpen(false)
                    onOpenChange(false)
                }}
            />
        </>
    )
}
