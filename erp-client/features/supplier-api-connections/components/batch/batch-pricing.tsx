"use client"

import * as React from "react"
import {
    CalculatorIcon,
    EqualIcon,
    PackageIcon,
    ShieldCheckIcon,
    SlidersHorizontalIcon,
    TruckIcon,
} from "lucide-react"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import type { BatchView } from "../../api/batch"
import {
    batchManualImpact,
    draftPricing,
} from "../../hooks/use-batch-workspace"
import type {
    PricingMode,
    useBatchWorkspace,
} from "../../hooks/use-batch-workspace"
import { BatchPriceTable } from "./batch-price-table"

const METHOD_OPTIONS = [
    { value: "UNSET", label: "暂不设置" },
    { value: "PERCENTAGE", label: "采购价 + 百分比" },
    { value: "ADD", label: "采购价 + 金额" },
    { value: "FIXED", label: "指定售价" },
]
const MODES: Array<{ value: PricingMode; label: string }> = [
    { value: "DROPSHIP_ONLY", label: "只设置代发价" },
    { value: "SAME", label: "代发价和集采价相同" },
    { value: "SEPARATE", label: "代发价和集采价分别设置" },
]

export function BatchPricingPanel({
    workspace,
    view,
    disabled,
}: {
    workspace: ReturnType<typeof useBatchWorkspace>
    view: BatchView
    disabled: boolean
}) {
    const { form, values } = workspace
    const manualCount = batchManualImpact(view, values)
    const [pendingMode, setPendingMode] = React.useState<PricingMode | null>(
        null,
    )
    const hasBulkValues = view.rows.some(
        (row) =>
            row.sales_prices.bulk.value != null ||
            row.sales_prices.minimum_quantity.value != null,
    )
    const changeMode = (mode: PricingMode) => {
        const inactive = values.inactivePricing
        if (
            values.mode === "SEPARATE" ||
            (values.mode === "SAME" && inactive?.mode !== "SEPARATE")
        )
            form.setFieldValue("inactivePricing", draftPricing(values))
        if (mode === "SEPARATE") {
            const previous = inactive?.mode === "SEPARATE" ? inactive : null
            const drop = previous?.dropship
            const bulk = previous?.bulk
            if (drop) {
                form.setFieldValue("dropshipMethod", drop.method)
                form.setFieldValue(
                    "dropshipValue",
                    drop.method === "UNSET" ? "" : drop.value,
                )
            }
            form.setFieldValue(
                "bulkMethod",
                bulk?.method ?? values.dropshipMethod,
            )
            form.setFieldValue(
                "bulkValue",
                bulk
                    ? bulk.method === "UNSET"
                        ? ""
                        : bulk.value
                    : values.dropshipValue,
            )
            if (previous)
                form.setFieldValue(
                    "minimumQuantity",
                    previous.minimum_quantity ?? "",
                )
        }
        form.setFieldValue("mode", mode)
        setPendingMode(null)
    }
    const requestMode = (mode: PricingMode) => {
        if (mode === values.mode) return
        if (
            (mode === "DROPSHIP_ONLY" && hasBulkValues) ||
            (mode === "SAME" &&
                values.mode === "SEPARATE" &&
                (values.dropshipMethod !== values.bulkMethod ||
                    values.dropshipValue !== values.bulkValue))
        )
            setPendingMode(mode)
        else changeMode(mode)
    }
    return (
        <div className="grid min-w-0 items-start gap-6 xl:grid-cols-[minmax(320px,380px)_minmax(0,1fr)]">
            <fieldset
                disabled={disabled}
                className="min-w-0 space-y-5 rounded-xl border bg-muted/20 p-5 disabled:opacity-70"
                aria-labelledby="supplier-catalog-batch-pricing-title"
            >
                <div className="space-y-2">
                    <div className="flex items-center gap-2">
                        <CalculatorIcon
                            className="size-4 text-muted-foreground"
                            aria-hidden
                        />
                        <h2
                            id="supplier-catalog-batch-pricing-title"
                            className="text-base font-semibold"
                        >
                            设置售价
                        </h2>
                        <span className="ml-auto text-xs text-muted-foreground">
                            含税
                        </span>
                    </div>
                </div>
                <div className="space-y-3">
                    <h3
                        id="supplier-catalog-batch-mode-label"
                        className="text-sm font-medium"
                    >
                        定价模式
                    </h3>
                    <RadioGroup
                        id="supplier-catalog-batch-mode"
                        aria-labelledby="supplier-catalog-batch-mode-label"
                        value={values.mode}
                        disabled={disabled}
                        onValueChange={(value) => {
                            const mode = MODES.find(
                                (option) => option.value === value,
                            )
                            if (mode) requestMode(mode.value)
                        }}
                        className="gap-2.5"
                    >
                        {MODES.map((mode) => (
                            <label
                                key={mode.value}
                                htmlFor={`supplier-catalog-batch-mode-${mode.value.toLowerCase()}`}
                                className="flex cursor-pointer items-center gap-2.5 text-sm"
                            >
                                <RadioGroupItem
                                    id={`supplier-catalog-batch-mode-${mode.value.toLowerCase()}`}
                                    value={mode.value}
                                />
                                {mode.label}
                            </label>
                        ))}
                    </RadioGroup>
                </div>
                {(
                    [
                        "dropship",
                        ...(values.mode === "SEPARATE" ? ["bulk"] : []),
                    ] as Array<"dropship" | "bulk">
                ).map((name) => {
                    const method =
                        name === "dropship"
                            ? values.dropshipMethod
                            : values.bulkMethod
                    const title =
                        values.mode === "SAME"
                            ? "代发价与集采价"
                            : name === "dropship"
                              ? "代发售价"
                              : "集采售价"
                    const PriceIcon =
                        values.mode === "SAME"
                            ? EqualIcon
                            : name === "dropship"
                              ? TruckIcon
                              : PackageIcon
                    return (
                        <div key={name} className="space-y-3 border-t pt-4">
                            <h3 className="flex items-center gap-2 text-sm font-semibold">
                                <PriceIcon
                                    className="size-4 text-muted-foreground"
                                    aria-hidden
                                />
                                {title}
                            </h3>
                            <div className="grid min-w-0 grid-cols-[minmax(0,3fr)_minmax(0,2fr)] items-start gap-3">
                                <form.AppField
                                    name={
                                        name === "dropship"
                                            ? "dropshipMethod"
                                            : "bulkMethod"
                                    }
                                >
                                    {(field) => (
                                        <field.SelectField
                                            id={`supplier-catalog-batch-${name}-method`}
                                            label="计算方式"
                                            options={METHOD_OPTIONS}
                                            allowClear={false}
                                            disabled={disabled}
                                            className={
                                                method === "UNSET"
                                                    ? "col-span-2"
                                                    : "min-w-0"
                                            }
                                        />
                                    )}
                                </form.AppField>
                                {method !== "UNSET" ? (
                                    <form.AppField
                                        name={
                                            name === "dropship"
                                                ? "dropshipValue"
                                                : "bulkValue"
                                        }
                                    >
                                        {(field) => (
                                            <Field className="min-w-0">
                                                <FieldLabel
                                                    htmlFor={`supplier-catalog-batch-${name}-value`}
                                                >
                                                    {method === "PERCENTAGE"
                                                        ? "加价比例（%）"
                                                        : method === "ADD"
                                                          ? "加价金额（元）"
                                                          : "售价（元）"}
                                                </FieldLabel>
                                                <Input
                                                    id={`supplier-catalog-batch-${name}-value`}
                                                    className="bg-background"
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
                                                    placeholder={
                                                        method === "PERCENTAGE"
                                                            ? "如 15"
                                                            : "输入金额"
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
                                ) : null}
                            </div>
                            {values.mode !== "DROPSHIP_ONLY" &&
                            (values.mode === "SAME" || name === "bulk") ? (
                                <form.AppField name="minimumQuantity">
                                    {(field) => (
                                        <Field>
                                            <FieldLabel htmlFor="supplier-catalog-batch-minimum-quantity">
                                                集采起订量（基础单位）
                                            </FieldLabel>
                                            <Input
                                                id="supplier-catalog-batch-minimum-quantity"
                                                className="bg-background"
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
                                                placeholder="输入数量"
                                            />
                                            <FieldError
                                                errors={field.state.meta.errors}
                                            />
                                        </Field>
                                    )}
                                </form.AppField>
                            ) : null}
                        </div>
                    )
                })}
                {values.mode === "DROPSHIP_ONLY" && hasBulkValues ? (
                    <p className="text-xs leading-relaxed text-muted-foreground">
                        {values.dropshipBulkAction === "KEEP"
                            ? "保留已有集采价与起订量。"
                            : "保存时清空可替换的集采价与起订量。"}
                    </p>
                ) : null}
                <section
                    className="space-y-3 border-t pt-4"
                    aria-labelledby="supplier-catalog-batch-more-settings"
                >
                    <h3
                        id="supplier-catalog-batch-more-settings"
                        className="flex items-center gap-2 text-sm font-semibold"
                    >
                        <SlidersHorizontalIcon
                            className="size-4 text-muted-foreground"
                            aria-hidden
                        />
                        已有售价处理
                    </h3>
                    <div className="space-y-3">
                        <form.AppField name="overwrite">
                            {(field) => (
                                <RadioGroup
                                    id="supplier-catalog-batch-overwrite"
                                    aria-label="已有售价处理方式"
                                    value={
                                        field.state.value ? "overwrite" : "fill"
                                    }
                                    disabled={disabled}
                                    onValueChange={(value) => {
                                        const overwrite = value === "overwrite"
                                        field.handleChange(overwrite)
                                        if (!overwrite)
                                            form.setFieldValue(
                                                "preserveManual",
                                                true,
                                            )
                                    }}
                                >
                                    <label
                                        htmlFor="supplier-catalog-batch-overwrite-fill"
                                        className="flex items-center gap-2 text-sm"
                                    >
                                        <RadioGroupItem
                                            id="supplier-catalog-batch-overwrite-fill"
                                            value="fill"
                                        />
                                        仅补空价
                                    </label>
                                    <label
                                        htmlFor="supplier-catalog-batch-overwrite-replace"
                                        className="flex items-center gap-2 text-sm"
                                    >
                                        <RadioGroupItem
                                            id="supplier-catalog-batch-overwrite-replace"
                                            value="overwrite"
                                        />
                                        更新本批未入库项
                                    </label>
                                </RadioGroup>
                            )}
                        </form.AppField>
                        {values.overwrite ? (
                            <form.AppField name="preserveManual">
                                {(field) => (
                                    <label
                                        htmlFor="supplier-catalog-batch-preserve-manual"
                                        className="flex items-start gap-2 text-sm"
                                    >
                                        <Checkbox
                                            id="supplier-catalog-batch-preserve-manual"
                                            checked={field.state.value}
                                            onCheckedChange={(checked) =>
                                                field.handleChange(
                                                    checked === true,
                                                )
                                            }
                                        />
                                        保留手动修改与清空
                                    </label>
                                )}
                            </form.AppField>
                        ) : null}
                    </div>
                </section>
                {values.overwrite && !values.preserveManual ? (
                    <p
                        role="status"
                        className="text-xs leading-relaxed text-warning"
                    >
                        保存后将覆盖 {manualCount} 个手动调整项。
                    </p>
                ) : null}
                <p className="flex items-start gap-2 text-xs leading-relaxed text-muted-foreground">
                    <ShieldCheckIcon
                        className="mt-0.5 size-4 shrink-0"
                        aria-hidden
                    />
                    已有 SKU 原价不变
                </p>
            </fieldset>
            <BatchPriceTable view={view} values={values} />
            <Dialog
                open={pendingMode != null}
                onOpenChange={(open) => !open && setPendingMode(null)}
            >
                <DialogContent closeButtonId="supplier-catalog-batch-mode-confirm-close">
                    <DialogHeader>
                        <DialogTitle>确认切换定价方式</DialogTitle>
                        <DialogDescription>
                            {pendingMode === "DROPSHIP_ONLY"
                                ? "本批已有公司集采价与门槛。请选择本次保留还是明确清空；单项人工调整仍按保护设置执行。"
                                : "代发与集采规则不同，切换后将以当前代发规则统一。分别设置的草稿会保留，可再次切回。"}
                        </DialogDescription>
                    </DialogHeader>
                    <DialogFooter>
                        <Button
                            id="supplier-catalog-batch-mode-confirm-cancel"
                            variant="outline"
                            type="button"
                            onClick={() => setPendingMode(null)}
                        >
                            取消
                        </Button>
                        {pendingMode === "DROPSHIP_ONLY" ? (
                            <>
                                <Button
                                    id="supplier-catalog-batch-mode-keep-bulk"
                                    variant="outline"
                                    type="button"
                                    onClick={() => {
                                        form.setFieldValue(
                                            "dropshipBulkAction",
                                            "KEEP",
                                        )
                                        changeMode("DROPSHIP_ONLY")
                                    }}
                                >
                                    保留已有集采值
                                </Button>
                                <Button
                                    id="supplier-catalog-batch-mode-clear-bulk"
                                    type="button"
                                    onClick={() => {
                                        form.setFieldValue(
                                            "dropshipBulkAction",
                                            "CLEAR",
                                        )
                                        changeMode("DROPSHIP_ONLY")
                                    }}
                                >
                                    清空可替换集采值
                                </Button>
                            </>
                        ) : (
                            <Button
                                id="supplier-catalog-batch-mode-unify-confirm"
                                type="button"
                                onClick={() => changeMode("SAME")}
                            >
                                使用代发规则统一
                            </Button>
                        )}
                    </DialogFooter>
                </DialogContent>
            </Dialog>
        </div>
    )
}
