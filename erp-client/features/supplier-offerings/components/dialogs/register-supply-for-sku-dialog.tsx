"use client"

import * as React from "react"
import { useStore } from "@tanstack/react-form"
import { PackageIcon } from "lucide-react"
import { Checkbox } from "@/components/ui/checkbox"
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
import { DeliveryTemplateField } from "../delivery-templates/delivery-template-field"
import { SupplierTaxRateField } from "../supplier-tax-rate-field"
import { SupplySkuField } from "../supply-sku-field"
import { SupplyAmountField } from "../supply-amount-field"

const prefix = "supplier-offerings-dialog-register"

/** 左侧说明、右侧字段的表单分区。 */
function FormSection({
    id,
    title,
    description,
    children,
}: {
    id: string
    title: string
    description?: string
    children: React.ReactNode
}) {
    return (
        <section
            aria-labelledby={`${prefix}-${id}-heading`}
            className="grid min-w-0 gap-4 border-t py-5 first:border-t-0 first:pt-1 md:grid-cols-[168px_minmax(0,1fr)] md:gap-8"
        >
            <div className="space-y-1">
                <h3
                    id={`${prefix}-${id}-heading`}
                    className="text-sm font-semibold"
                >
                    {title}
                </h3>
                {description ? (
                    <p className="text-xs leading-5 text-muted-foreground">
                        {description}
                    </p>
                ) : null}
            </div>
            <div className="min-w-0">{children}</div>
        </section>
    )
}

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
                        product_capabilities: [],
                        valid_from: value.validFrom,
                        valid_to:
                            value.validityMode === "dated"
                                ? value.validTo
                                : null,
                    },
                    delivery_template_id: value.deliveryTemplateId || null,
                    availability_status: value.availabilityStatus,
                    available_quantity:
                        value.quantityMode === "provided"
                            ? value.availableQuantity.trim()
                            : null,
                    change_reason: value.changeReason.trim(),
                    idempotency_key: idempotencyKey("create-supplier-offering"),
                })
                toast.add({
                    title: "供给已添加",
                    description: "已保存供应商、报价、配送模板与可供情况。",
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
                    className="flex max-h-[92dvh] w-[calc(100vw-2rem)] flex-col gap-0 overflow-hidden p-0 sm:max-w-[920px]"
                >
                    <DialogHeader className="shrink-0 px-5 pt-5 pb-4 pr-14 sm:px-6 sm:pr-16">
                        <DialogTitle className="text-xl">添加供给</DialogTitle>
                        <DialogDescription>
                            为这个商品规格登记一家供应商：报价、配送范围和当前可供情况。
                        </DialogDescription>
                    </DialogHeader>
                    {skuName && (
                        <div className="mx-5 mb-2 flex shrink-0 items-center gap-3 rounded-lg border bg-muted/40 px-3 py-2.5 sm:mx-6">
                            {imageUrl ? (
                                // eslint-disable-next-line @next/next/no-img-element -- 复用商品编辑页已解析的对象存储或本地预览地址。
                                <img
                                    src={imageUrl}
                                    alt=""
                                    className="size-10 shrink-0 rounded-md object-cover"
                                />
                            ) : (
                                <PackageIcon
                                    className="size-8 shrink-0 text-muted-foreground"
                                    aria-hidden="true"
                                />
                            )}
                            <div className="min-w-0">
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
                            className="mx-5 mb-2 w-auto shrink-0 sm:mx-6"
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
                        <div className="min-h-0 flex-1 overflow-x-hidden overflow-y-auto overscroll-contain px-5 pt-3 pb-2 sm:px-6">
                            <fieldset
                                disabled={mutation.isPending}
                                className="min-w-0 [&_[data-slot=field]]:gap-2"
                            >
                                {!fixedSku && (
                                    <FormSection id="sku" title="商品规格">
                                        <form.AppField name="skuId">
                                            {() => (
                                                <SupplySkuField
                                                    id={`${prefix}-sku`}
                                                    options={skuOptions}
                                                    onItemChange={
                                                        setSelectedSku
                                                    }
                                                />
                                            )}
                                        </form.AppField>
                                    </FormSection>
                                )}
                                <FormSection
                                    id="supplier"
                                    title="供应商"
                                    description="下单时按订货编码向供应商订货。"
                                >
                                    <div className="grid items-start gap-4 sm:grid-cols-2">
                                        <form.AppField
                                            name="supplierId"
                                            listeners={{
                                                onChange: () => {
                                                    form.setFieldValue(
                                                        "inputTaxPercentage",
                                                        "",
                                                    )
                                                    form.setFieldValue(
                                                        "deliveryTemplateId",
                                                        "",
                                                    )
                                                },
                                            }}
                                        >
                                            {(field) => {
                                                const invalid =
                                                    field.state.meta
                                                        .isTouched &&
                                                    !field.state.meta.isValid
                                                return (
                                                    <Field
                                                        className="min-w-0 sm:col-span-2"
                                                        data-invalid={
                                                            invalid || undefined
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
                                                                        "") !==
                                                                    field.state
                                                                        .value
                                                                )
                                                                    field.handleChange(
                                                                        value ??
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
                                                                invalid
                                                            }
                                                            aria-describedby={
                                                                invalid
                                                                    ? `${prefix}-supplier-error`
                                                                    : undefined
                                                            }
                                                            placeholder="搜索已启用供应商"
                                                            className="w-full"
                                                        />
                                                        {invalid && (
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
                                                )
                                            }}
                                        </form.AppField>
                                        <form.AppField name="supplierSkuCode">
                                            {(field) => (
                                                <field.TextField
                                                    id={`${prefix}-supplier-sku-code`}
                                                    label="供应商订货编码"
                                                    required
                                                    placeholder="供应商的货号"
                                                />
                                            )}
                                        </form.AppField>
                                        <form.AppField name="supplierProductCode">
                                            {(field) => (
                                                <field.TextField
                                                    id={`${prefix}-supplier-product-code`}
                                                    label="供应商商品编码"
                                                    placeholder="选填"
                                                />
                                            )}
                                        </form.AppField>
                                    </div>
                                </FormSection>
                                <FormSection
                                    id="prices"
                                    title="报价"
                                    description="均为含税价，不含税价按税率自动换算。"
                                >
                                    <div className="grid items-start gap-4 sm:grid-cols-2">
                                        <form.AppField
                                            name="dropshipPrice"
                                            listeners={{
                                                onChange: ({ value }) => {
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
                                                        readOnly={samePrice}
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
                                                                field.state
                                                                    .value
                                                            }
                                                            onCheckedChange={(
                                                                checked,
                                                            ) => {
                                                                field.handleChange(
                                                                    checked,
                                                                )
                                                                if (checked)
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
                                </FormSection>
                                <FormSection
                                    id="delivery"
                                    title="配送"
                                    description="按收货地区判定能否配送、集采是否另付运费。"
                                >
                                    <form.Subscribe
                                        selector={(state) =>
                                            state.values.supplierId
                                        }
                                    >
                                        {(supplierId) => (
                                            <form.AppField name="deliveryTemplateId">
                                                {(field) => (
                                                    <DeliveryTemplateField
                                                        id={`${prefix}-delivery-template`}
                                                        supplierId={supplierId}
                                                        value={
                                                            field.state.value
                                                        }
                                                        onChange={
                                                            field.handleChange
                                                        }
                                                        disabled={
                                                            mutation.isPending
                                                        }
                                                    />
                                                )}
                                            </form.AppField>
                                        )}
                                    </form.Subscribe>
                                </FormSection>
                                <FormSection
                                    id="availability"
                                    title="有效期与可供"
                                    description="可供状态和数量决定这条供给现在能否用于销售。"
                                >
                                    <div className="grid items-start gap-4 sm:grid-cols-2">
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
                                        <form.AppField
                                            name="validTo"
                                            listeners={{
                                                onChange: ({ value }) =>
                                                    form.setFieldValue(
                                                        "validityMode",
                                                        value
                                                            ? "dated"
                                                            : "ongoing",
                                                    ),
                                            }}
                                        >
                                            {(field) => (
                                                <field.DateField
                                                    id={`${prefix}-valid-to`}
                                                    label="失效日期"
                                                    description="不填为长期有效"
                                                    inputClassName="h-9"
                                                />
                                            )}
                                        </form.AppField>
                                        <form.AppField name="availabilityStatus">
                                            {(field) => (
                                                <Field className="min-w-0">
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
                                                            field.state.value
                                                        }
                                                        allowClear={false}
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
                                        <form.AppField
                                            name="availableQuantity"
                                            listeners={{
                                                onChange: ({ value }) =>
                                                    form.setFieldValue(
                                                        "quantityMode",
                                                        value.trim()
                                                            ? "provided"
                                                            : "unknown",
                                                    ),
                                            }}
                                        >
                                            {(field) => (
                                                <field.TextField
                                                    id={`${prefix}-available-quantity`}
                                                    label={`可供数量${unit ? `（${unit}）` : ""}`}
                                                    inputMode="decimal"
                                                    placeholder="选填"
                                                    description="不填表示供应商未提供数量"
                                                />
                                            )}
                                        </form.AppField>
                                    </div>
                                    <form.Subscribe
                                        selector={(state) =>
                                            [
                                                state.values.availabilityStatus,
                                                state.values.availableQuantity,
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
                                                    className="mt-3 text-sm text-warning-soft-foreground"
                                                >
                                                    {status !== "AVAILABLE"
                                                        ? "当前状态不支持销售；恢复可供后还需满足价格和有效期等销售条件。"
                                                        : "当前数量为 0，这条供给暂不能用于销售。"}
                                                </p>
                                            ) : null
                                        }
                                    </form.Subscribe>
                                </FormSection>
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
