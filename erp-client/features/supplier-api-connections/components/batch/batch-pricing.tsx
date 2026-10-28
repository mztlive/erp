"use client"

import * as React from "react"
import { MoneyValue } from "@/components/business"
import {
    TableToolbar,
    TableToolbarScope,
} from "@/components/business/table-toolbar"
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
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import type { BatchView } from "../../api/batch"
import {
    batchManualImpact,
    draftPricing,
} from "../../hooks/use-batch-workspace"
import type {
    PricingMode,
    useBatchWorkspace,
} from "../../hooks/use-batch-workspace"
import { batchPriceSample } from "../../lib/batch-price-preview"

const METHOD_OPTIONS = [
    { value: "UNSET", label: "不设置" },
    { value: "PERCENTAGE", label: "按比例加价" },
    { value: "ADD", label: "增加固定金额" },
    { value: "FIXED", label: "指定金额" },
]
const MODES: Array<{ value: PricingMode; label: string }> = [
    { value: "DROPSHIP_ONLY", label: "仅代发" },
    { value: "SAME", label: "代发与集采同价" },
    { value: "SEPARATE", label: "分别设置" },
]

export function BatchPricingPanel({
    workspace,
    view,
}: {
    workspace: ReturnType<typeof useBatchWorkspace>
    view: BatchView
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
        <div className="min-w-0 space-y-7">
            <div
                role="group"
                aria-label="公司售价设置方式"
                className="grid max-w-3xl grid-cols-1 gap-1 rounded-lg border p-1 sm:grid-cols-3"
            >
                {MODES.map((mode) => (
                    <Button
                        key={mode.value}
                        id={`supplier-catalog-batch-mode-${mode.value.toLowerCase()}`}
                        type="button"
                        variant={
                            values.mode === mode.value ? "secondary" : "ghost"
                        }
                        aria-pressed={values.mode === mode.value}
                        onClick={() => requestMode(mode.value)}
                    >
                        {mode.label}
                    </Button>
                ))}
            </div>
            <section
                className="max-w-4xl space-y-4"
                aria-labelledby="supplier-catalog-batch-pricing-title"
            >
                <h2
                    id="supplier-catalog-batch-pricing-title"
                    className="text-base font-semibold"
                >
                    {values.mode === "SAME"
                        ? "批量售价"
                        : "公司销售参考价（含税）"}
                </h2>
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
                        name === "dropship" ? "公司代发价" : "公司集采价"
                    return (
                        <div
                            key={name}
                            className="flex flex-wrap items-start gap-3"
                        >
                            {values.mode === "SEPARATE" ? (
                                <span className="w-24 pt-2 text-sm font-medium">
                                    {title}
                                </span>
                            ) : null}
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
                                        label={`${title}计算方法`}
                                        hideLabel
                                        options={METHOD_OPTIONS}
                                        allowClear={false}
                                        className="w-44"
                                    />
                                )}
                            </form.AppField>
                            {method !== "UNSET" ? (
                                <div className="flex min-w-0 flex-1 basis-64 items-start gap-3">
                                    <span className="pt-2 text-sm text-muted-foreground">
                                        {method === "FIXED"
                                            ? "统一售价"
                                            : "接口采购报价 +"}
                                    </span>
                                    <form.AppField
                                        name={
                                            name === "dropship"
                                                ? "dropshipValue"
                                                : "bulkValue"
                                        }
                                    >
                                        {(field) => (
                                            <Field className="w-40">
                                                <FieldLabel
                                                    className="sr-only"
                                                    htmlFor={`supplier-catalog-batch-${name}-value`}
                                                >
                                                    {method === "PERCENTAGE"
                                                        ? "加价比例"
                                                        : "金额"}
                                                </FieldLabel>
                                                <Input
                                                    id={`supplier-catalog-batch-${name}-value`}
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
                                                    placeholder="请输入"
                                                />
                                                <FieldError
                                                    errors={
                                                        field.state.meta.errors
                                                    }
                                                />
                                            </Field>
                                        )}
                                    </form.AppField>
                                    <span className="pt-2 text-sm text-muted-foreground">
                                        {method === "PERCENTAGE" ? "%" : "元"}
                                    </span>
                                </div>
                            ) : (
                                <p className="pt-2 text-sm text-muted-foreground">
                                    保留本行已有草稿值
                                </p>
                            )}
                        </div>
                    )
                })}
                {values.mode !== "DROPSHIP_ONLY" ? (
                    <div className="flex flex-wrap items-start gap-4 pt-2">
                        <span className="pt-2 text-sm font-medium">
                            公司集采起订量
                        </span>
                        <form.AppField name="minimumQuantity">
                            {(field) => (
                                <Field className="w-44">
                                    <FieldLabel
                                        className="sr-only"
                                        htmlFor="supplier-catalog-batch-minimum-quantity"
                                    >
                                        公司集采起订量
                                    </FieldLabel>
                                    <Input
                                        id="supplier-catalog-batch-minimum-quantity"
                                        inputMode="decimal"
                                        value={field.state.value}
                                        onChange={(event) =>
                                            field.handleChange(
                                                event.target.value,
                                            )
                                        }
                                        onBlur={field.handleBlur}
                                        placeholder="请输入"
                                    />
                                    <FieldError
                                        errors={field.state.meta.errors}
                                    />
                                </Field>
                            )}
                        </form.AppField>
                        <p className="pt-2 text-sm text-muted-foreground">
                            按各 SKU 的公司基础单位
                        </p>
                    </div>
                ) : hasBulkValues ? (
                    <p className="text-sm text-muted-foreground">
                        {values.dropshipBulkAction === "KEEP"
                            ? "保留本批已填写的公司集采价与门槛；本次只计算代发价。"
                            : "保存时清空本批可替换的公司集采价与门槛，人工例外按更多设置保护。"}
                    </p>
                ) : null}
            </section>
            <section
                className="min-w-0 border-t pt-5"
                aria-labelledby="supplier-catalog-batch-sample-title"
            >
                <h2
                    id="supplier-catalog-batch-sample-title"
                    className="text-base font-semibold"
                >
                    售价预览（含税）
                </h2>
                <TableToolbarScope>
                    <TableToolbar>
                        <span>显示前两项，完整结果在核对页查看</span>
                    </TableToolbar>
                    <Table>
                        <TableHeader>
                            <TableRow>
                                <TableHead>商品与规格</TableHead>
                                <TableHead className="text-right">
                                    接口采购报价
                                </TableHead>
                                <TableHead className="text-right">
                                    公司代发价
                                </TableHead>
                                <TableHead className="text-right">
                                    公司集采价
                                </TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {view.rows
                                .filter((row) => row.status !== "EXCLUDED")
                                .slice(0, 2)
                                .map((row) => {
                                    const sample = batchPriceSample(row, values)
                                    return (
                                        <TableRow key={row.source.id}>
                                            <TableCell>
                                                <p className="font-medium">
                                                    {row.source.name}
                                                </p>
                                                <p className="text-xs text-muted-foreground">
                                                    {row.source.specification}
                                                </p>
                                                {sample.error ? (
                                                    <p className="text-xs text-warning">
                                                        {sample.error}
                                                    </p>
                                                ) : null}
                                            </TableCell>
                                            <TableCell className="text-right">
                                                <MoneyValue
                                                    value={
                                                        row.source
                                                            .purchase_price
                                                    }
                                                />
                                            </TableCell>
                                            <TableCell className="text-right">
                                                <MoneyValue
                                                    value={sample.dropship}
                                                />
                                                {sample.preserved ? (
                                                    <p className="text-xs text-muted-foreground">
                                                        保留原价
                                                    </p>
                                                ) : null}
                                            </TableCell>
                                            <TableCell className="text-right">
                                                <MoneyValue
                                                    value={sample.bulk}
                                                />
                                                {sample.preserved ? (
                                                    <p className="text-xs text-muted-foreground">
                                                        保留原价
                                                    </p>
                                                ) : null}
                                            </TableCell>
                                        </TableRow>
                                    )
                                })}
                        </TableBody>
                    </Table>
                </TableToolbarScope>
            </section>
            <details className="border-t pt-4">
                <summary
                    id="supplier-catalog-batch-more-settings"
                    className="cursor-pointer text-sm font-medium"
                >
                    更多设置{" "}
                    <span className="ml-3 font-normal text-muted-foreground">
                        {values.overwrite ? "覆盖本批未入库项" : "仅填空值"} ·{" "}
                        {values.preserveManual
                            ? "保留人工调整"
                            : `允许覆盖 ${manualCount} 项人工调整`}{" "}
                        · 已有 SKU 原价不变
                    </span>
                </summary>
                <div className="flex flex-wrap gap-5 pt-4">
                    <form.AppField name="overwrite">
                        {(field) => (
                            <label
                                htmlFor="supplier-catalog-batch-overwrite"
                                className="flex items-center gap-2 text-sm"
                            >
                                <Checkbox
                                    id="supplier-catalog-batch-overwrite"
                                    checked={field.state.value}
                                    onCheckedChange={(checked) => {
                                        field.handleChange(checked === true)
                                        if (!checked)
                                            form.setFieldValue(
                                                "preserveManual",
                                                true,
                                            )
                                    }}
                                />
                                覆盖本批未入库项
                            </label>
                        )}
                    </form.AppField>
                    <form.AppField name="preserveManual">
                        {(field) => (
                            <label
                                htmlFor="supplier-catalog-batch-preserve-manual"
                                className="flex items-center gap-2 text-sm"
                            >
                                <Checkbox
                                    id="supplier-catalog-batch-preserve-manual"
                                    checked={field.state.value}
                                    disabled={!values.overwrite}
                                    onCheckedChange={(checked) =>
                                        field.handleChange(checked === true)
                                    }
                                />
                                保留单项人工调整与明确清空
                            </label>
                        )}
                    </form.AppField>
                </div>
                {values.overwrite && !values.preserveManual ? (
                    <p role="status" className="pt-3 text-sm text-warning">
                        应用后将覆盖本批 {manualCount}{" "}
                        个人工调整项，请核对影响范围。
                    </p>
                ) : null}
            </details>
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
