"use client"

import * as React from "react"
import { ArrowRightIcon, PlusIcon, Trash2Icon } from "lucide-react"
import { z } from "zod"

import { MoneyValue, detailWorkspaceStyles } from "@/components/business"
import { toFieldErrors, useAppForm } from "@/components/form"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogBody,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import { RequiredMark } from "@/components/ui/required-mark"
import { SupplierSearchCombobox } from "@/features/entity-selectors/components/supplier-search-combobox"
import { useCreateExpensePurchaseMutation } from "@/features/purchase-orders/hooks/queries"
import {
    PURCHASE_EXPENSE_CATEGORY_LABEL,
    type PurchaseExpenseCategory,
} from "@/features/purchase-orders/types"
import { getErrorMessage } from "@/lib/api/errors"
import { SUPPLIER_PAYMENT_TERM_OPTIONS } from "@/lib/business-options"
import {
    compareDecimal,
    divideFixed,
    splitFeeGrossByPercentRate,
    sumFixed,
} from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import {
    InquiryExpenseSourceSection,
    useInquiryExpenseSourceState,
} from "./inquiry-expense-source-section"

const CATEGORY_OPTIONS = (
    Object.keys(PURCHASE_EXPENSE_CATEGORY_LABEL) as PurchaseExpenseCategory[]
).map((value) => ({ value, label: PURCHASE_EXPENSE_CATEGORY_LABEL[value] }))

const COMMON_TAX_RATE_PERCENTS = ["0", "1", "3", "6", "9", "13"] as const

// SelectField 将空字符串视为未选择，整单归属使用独立的表单值。
const WHOLE_ORDER = "whole-order"
const LINE_LIMIT = 50

const amountPattern = /^(?:0|[1-9]\d*)(?:\.\d{1,2})?$/
const percentPattern = /^(?:0|[1-9]\d?)(?:\.\d{1,2})?$/

const lineSchema = z.object({
    rowId: z.string(),
    category: z.string().min(1, "请选择费用类别"),
    description: z
        .string()
        .trim()
        .min(1, "请填写费用说明")
        .max(200, "费用说明不能超过 200 字"),
    grossAmount: z
        .string()
        .trim()
        .regex(amountPattern, "金额最多保留两位小数")
        .refine((value) => {
            try {
                return compareDecimal(value, "0", 2) > 0
            } catch {
                return false
            }
        }, "金额必须大于 0"),
    taxRatePercent: z
        .string()
        .trim()
        .regex(percentPattern, "税率填写 0–99 的百分数，最多两位小数"),
    salesOrderLineId: z.string(),
})

const schema = z.object({
    supplierId: z.string().min(1, "请选择收款供应商"),
    paymentTermCode: z.string().min(1, "请选择付款条件"),
    lines: z.array(lineSchema).min(1, "至少填写一行费用").max(LINE_LIMIT),
})

type FormValues = z.input<typeof schema>
type LineValues = FormValues["lines"][number]
type ExpensePurchaseMode = "manual" | "inquiry"

function newLine(): LineValues {
    return {
        rowId: crypto.randomUUID(),
        category: "",
        description: "",
        grossAmount: "",
        taxRatePercent: "0",
        salesOrderLineId: WHOLE_ORDER,
    }
}

/** 运费另计跟进时预填的收款供应商与物流费用行。 */
export type ExpensePurchasePrefill = Readonly<{
    supplierId: string
    salesOrderLineId?: string
    description?: string
    category?: PurchaseExpenseCategory
}>

function defaults(prefill?: ExpensePurchasePrefill): FormValues {
    if (!prefill) {
        return { supplierId: "", paymentTermCode: "", lines: [newLine()] }
    }
    return {
        supplierId: prefill.supplierId,
        paymentTermCode: "",
        lines: [
            {
                ...newLine(),
                category: prefill.category ?? "logistics",
                description: prefill.description ?? "",
                salesOrderLineId: prefill.salesOrderLineId ?? WHOLE_ORDER,
            },
        ],
    }
}

function percentToRate(percent: string) {
    return divideFixed(percent.trim() || "0", "100", {
        numeratorMaxScale: 6,
        denominatorMaxScale: 0,
        outputScale: 6,
    })
}

function samePercentValue(left: string, right: string): boolean {
    const a = left.trim()
    const b = right.trim()
    if (!a || !b) return false
    try {
        return compareDecimal(a, b, 2) === 0
    } catch {
        return false
    }
}

function lineBreakdown(line: LineValues) {
    const gross = line.grossAmount.trim()
    const taxRate = line.taxRatePercent.trim()
    if (!amountPattern.test(gross) || !percentPattern.test(taxRate)) {
        return null
    }
    try {
        if (compareDecimal(gross, "0", 2) <= 0) return null
        return splitFeeGrossByPercentRate(gross, taxRate)
    } catch {
        return null
    }
}

function summarizeLines(lines: readonly LineValues[]) {
    const parts = lines.map(lineBreakdown)
    const valid = parts.filter(
        (item): item is NonNullable<typeof item> => item !== null,
    )
    if (valid.length === 0) return null
    try {
        return {
            gross: sumFixed(
                valid.map((item) => item.gross),
                { maxScale: 2, outputScale: 2 },
            ),
            tax: sumFixed(
                valid.map((item) => item.tax),
                { maxScale: 2, outputScale: 2 },
            ),
            net: sumFixed(
                valid.map((item) => item.net),
                { maxScale: 2, outputScale: 2 },
            ),
        }
    } catch {
        return null
    }
}

export type ExpensePurchaseCreated = {
    purchaseOrderId: string
    purchaseNo: string
}

/**
 * 从销售单追加费用采购单：选收款供应商与付款条件，填写费用行后创建并提交审批。
 * 不经供给分配，不计入销售明细覆盖；费用发生后在采购单上确认发生。
 */
export function ExpensePurchaseCreateDialog({
    salesOrderId,
    salesLines,
    open,
    onOpenChange,
    onCreated,
    prefill,
    sourceLabel,
    title = "追加费用采购",
    onChangeSource,
}: {
    salesOrderId: string
    sourceLabel?: string
    title?: string
    onChangeSource?: () => void
    salesLines: ReadonlyArray<{ id: string; name: string }>
    open: boolean
    onOpenChange: (open: boolean) => void
    onCreated?: (created: ExpensePurchaseCreated) => void
    /** 预填收款供应商与物流费用行；缺省时为空白表单。 */
    prefill?: ExpensePurchasePrefill
}) {
    const mutation = useCreateExpensePurchaseMutation(salesOrderId)
    const [mode, setMode] = React.useState<ExpensePurchaseMode>("manual")
    const hasChosenModeRef = React.useRef(false)
    const [requestKey, setRequestKey] = React.useState(() =>
        crypto.randomUUID(),
    )
    const [failure, setFailure] = React.useState<string | null>(null)
    const lineOptions = React.useMemo(
        () => [
            { value: WHOLE_ORDER, label: "整张销售单" },
            ...salesLines.map((line) => ({ value: line.id, label: line.name })),
        ],
        [salesLines],
    )

    const inquiry = useInquiryExpenseSourceState({
        salesOrderId,
        open,
        onCreated: (created) => {
            onOpenChange(false)
            onCreated?.(created)
        },
    })

    const form = useAppForm({
        defaultValues: defaults(),
        validators: { onChange: schema },
        onSubmit: async ({ value }) => {
            setFailure(null)
            try {
                const created = await mutation.mutateAsync({
                    supplierId: value.supplierId,
                    paymentTermCode: value.paymentTermCode,
                    lines: value.lines.map((line) => ({
                        category: line.category as PurchaseExpenseCategory,
                        description: line.description.trim(),
                        grossAmount: line.grossAmount.trim(),
                        inputTaxRate: percentToRate(line.taxRatePercent),
                        salesOrderLineId:
                            line.salesOrderLineId === WHOLE_ORDER
                                ? undefined
                                : line.salesOrderLineId || undefined,
                    })),
                    idempotencyKey: `expense-purchase:${requestKey}`,
                })
                onOpenChange(false)
                onCreated?.({
                    purchaseOrderId: created.purchase_order_id,
                    purchaseNo: created.purchase_no,
                })
            } catch (error) {
                setFailure(getErrorMessage(error, "创建失败，请稍后重试。"))
            }
        },
    })

    const wasOpen = React.useRef(false)
    React.useEffect(() => {
        if (open && !wasOpen.current) {
            form.reset(defaults(prefill))
            setRequestKey(crypto.randomUUID())
            setFailure(null)
            setMode("manual")
            // 运费跟进等显式预填入口直接停留在手工表单，不自动跳走。
            hasChosenModeRef.current = Boolean(prefill)
        }
        wasOpen.current = open
    }, [form, open, prefill])

    React.useEffect(() => {
        if (!open || hasChosenModeRef.current) return
        if (inquiry.pending) {
            hasChosenModeRef.current = true
            setMode("inquiry")
            return
        }
        if (
            inquiry.query.isSuccess &&
            inquiry.availableCount > 0 &&
            !form.state.isDirty
        ) {
            hasChosenModeRef.current = true
            setMode("inquiry")
        }
    }, [
        form,
        inquiry.availableCount,
        inquiry.pending,
        inquiry.query.isSuccess,
        open,
    ])

    const busy = mutation.isPending || inquiry.isSubmitting
    const showModeSwitcher =
        inquiry.allowed &&
        (Boolean(inquiry.pending) ||
            inquiry.candidates.length > 0 ||
            inquiry.query.isError)

    const selectMode = (next: ExpensePurchaseMode) => {
        hasChosenModeRef.current = true
        setMode(next)
    }

    const handleOpenChange = (next: boolean) => {
        if (!next && busy) return
        onOpenChange(next)
    }

    return (
        <Dialog open={open} onOpenChange={handleOpenChange}>
            <DialogContent
                size="xl"
                scroll="body"
                closeButtonId="sales-orders-expense-purchase-close"
                showCloseButton={!busy}
            >
                <DialogHeader className="gap-2 border-grid text-left">
                    <div className="flex flex-wrap items-center gap-2">
                        <DialogTitle>{title}</DialogTitle>
                        {sourceLabel ? (
                            <Badge
                                variant="secondary"
                                className="num max-w-80 truncate font-normal"
                            >
                                {sourceLabel}
                            </Badge>
                        ) : null}
                        {onChangeSource ? (
                            <Button
                                id="procurement-expense-change-source"
                                type="button"
                                variant="ghost"
                                size="xs"
                                className="h-6 px-2 text-xs font-normal text-primary shadow-none hover:bg-sidebar-accent/70 hover:text-primary"
                                disabled={busy}
                                onClick={onChangeSource}
                            >
                                重新选择
                            </Button>
                        ) : null}
                    </div>
                    <DialogDescription>
                        为本单发生、由我方承担的第三方费用（物流、包装、上门安装等）创建费用采购单，提交后进入采购审批；客户另付费请改单加明细。
                    </DialogDescription>
                    {showModeSwitcher ? (
                        <div className="pt-1">
                            <div
                                role="tablist"
                                aria-label="费用采购创建方式"
                                className={detailWorkspaceStyles.subTabs}
                            >
                                <button
                                    id="sales-orders-expense-purchase-mode-manual"
                                    type="button"
                                    role="tab"
                                    aria-selected={mode === "manual"}
                                    data-active={mode === "manual"}
                                    disabled={busy}
                                    className={detailWorkspaceStyles.subTab}
                                    onClick={() => selectMode("manual")}
                                >
                                    直接录入
                                </button>
                                <button
                                    id="sales-orders-expense-purchase-mode-inquiry"
                                    type="button"
                                    role="tab"
                                    aria-selected={mode === "inquiry"}
                                    data-active={mode === "inquiry"}
                                    disabled={busy}
                                    className={detailWorkspaceStyles.subTab}
                                    onClick={() => selectMode("inquiry")}
                                >
                                    <span>关联询价费用</span>
                                    {inquiry.pending ? (
                                        <span
                                            className="size-1.5 rounded-full bg-warning"
                                            aria-label="有待确认结果"
                                        />
                                    ) : inquiry.availableCount > 0 ? (
                                        <span className="num rounded-sm bg-sidebar-accent px-1.5 py-0.5 text-tiny font-medium text-sidebar-accent-foreground">
                                            {inquiry.availableCount}
                                        </span>
                                    ) : inquiry.candidates.length > 0 ? (
                                        <span className="num text-xs text-muted-foreground">
                                            {inquiry.candidates.length}
                                        </span>
                                    ) : null}
                                </button>
                            </div>
                        </div>
                    ) : null}
                </DialogHeader>

                {mode === "inquiry" && showModeSwitcher ? (
                    <InquiryExpenseSourceSection
                        state={inquiry}
                        salesOrderId={salesOrderId}
                        sourceLabel={sourceLabel}
                        onCancel={() => handleOpenChange(false)}
                        onSwitchManual={() => selectMode("manual")}
                    />
                ) : (
                    <form
                        className="flex min-h-0 flex-1 flex-col overflow-hidden"
                        onSubmit={(event) => {
                            event.preventDefault()
                            void form.handleSubmit()
                        }}
                    >
                        <DialogBody className="space-y-5">
                            {inquiry.pending || inquiry.availableCount > 0 ? (
                                <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-info-border/70 bg-info-soft/45 px-3.5 py-2 text-xs text-info-soft-foreground">
                                    <span>
                                        {inquiry.pending
                                            ? "本单有一笔询价费用采购结果待确认，建议先核对原结果。"
                                            : `本单有 ${inquiry.availableCount} 笔已确认询价费用可转采购；直接录入不会自动扣减原询价额度。`}
                                    </span>
                                    <Button
                                        id="sales-orders-expense-purchase-switch-inquiry"
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        className="h-6 px-2 text-xs font-medium text-info-soft-foreground hover:bg-info-soft hover:text-info-soft-foreground"
                                        disabled={busy}
                                        onClick={() => selectMode("inquiry")}
                                    >
                                        {inquiry.pending
                                            ? "去核对"
                                            : "去关联询价费用"}
                                        <ArrowRightIcon
                                            aria-hidden="true"
                                            data-icon="inline-end"
                                        />
                                    </Button>
                                </div>
                            ) : null}

                            <div className="grid grid-cols-1 items-start gap-4 sm:grid-cols-2">
                                <form.AppField
                                    name="supplierId"
                                    children={(field) => (
                                        <Field
                                            data-invalid={
                                                field.state.meta.errors.length >
                                                    0 || undefined
                                            }
                                        >
                                            <FieldLabel htmlFor="sales-orders-expense-purchase-supplier">
                                                收款供应商
                                                <RequiredMark />
                                            </FieldLabel>
                                            <SupplierSearchCombobox
                                                id="sales-orders-expense-purchase-supplier"
                                                value={
                                                    field.state.value ||
                                                    undefined
                                                }
                                                onValueChange={(id) => {
                                                    field.handleChange(id ?? "")
                                                    field.handleBlur()
                                                }}
                                                placeholder="选择已建档的收款方"
                                            />
                                            <FieldError
                                                errors={toFieldErrors(
                                                    field.state.meta.errors,
                                                )}
                                            />
                                        </Field>
                                    )}
                                />
                                <form.AppField
                                    name="paymentTermCode"
                                    children={(field) => (
                                        <field.SelectField
                                            id="sales-orders-expense-purchase-payment-term"
                                            label="付款条件"
                                            required
                                            allowClear={false}
                                            options={
                                                SUPPLIER_PAYMENT_TERM_OPTIONS
                                            }
                                            placeholder="选择付款条件"
                                        />
                                    )}
                                />
                            </div>

                            <form.AppField name="lines" mode="array">
                                {(array) => (
                                    <section className="space-y-3 border-t border-border/60 pt-4">
                                        <div className="flex flex-wrap items-center justify-between gap-2">
                                            <div className="flex items-center gap-2">
                                                <h3 className="font-semibold text-foreground">
                                                    费用明细
                                                </h3>
                                                <Badge
                                                    variant="secondary"
                                                    className="num font-normal"
                                                >
                                                    {array.state.value.length}{" "}
                                                    行
                                                </Badge>
                                            </div>
                                            <Button
                                                id="sales-orders-expense-purchase-add-line"
                                                type="button"
                                                variant="ghost"
                                                size="sm"
                                                className={
                                                    detailWorkspaceStyles.subtleButton
                                                }
                                                disabled={
                                                    array.state.value.length >=
                                                        LINE_LIMIT ||
                                                    mutation.isPending
                                                }
                                                onClick={() =>
                                                    array.pushValue(newLine())
                                                }
                                            >
                                                <PlusIcon
                                                    aria-hidden="true"
                                                    data-icon="inline-start"
                                                />
                                                添加一行
                                            </Button>
                                        </div>

                                        <div className="space-y-3">
                                            {array.state.value.map(
                                                (line, index) => {
                                                    const prefix = `sales-orders-expense-purchase-line-${line.rowId}`
                                                    const breakdown =
                                                        lineBreakdown(line)
                                                    return (
                                                        <div
                                                            key={line.rowId}
                                                            className="space-y-3 rounded-lg border border-border/80 bg-card p-3.5"
                                                        >
                                                            <div className="flex flex-wrap items-center justify-between gap-2">
                                                                <div className="flex flex-wrap items-center gap-2">
                                                                    <span className="num text-xs font-medium text-muted-foreground">
                                                                        第{" "}
                                                                        {index +
                                                                            1}{" "}
                                                                        行
                                                                    </span>
                                                                    {breakdown ? (
                                                                        <span className="num text-xs text-muted-foreground">
                                                                            不含税{" "}
                                                                            <MoneyValue
                                                                                value={
                                                                                    breakdown.net
                                                                                }
                                                                                display="exact"
                                                                            />{" "}
                                                                            ·
                                                                            税额{" "}
                                                                            <MoneyValue
                                                                                value={
                                                                                    breakdown.tax
                                                                                }
                                                                                display="exact"
                                                                            />
                                                                        </span>
                                                                    ) : null}
                                                                </div>
                                                                <Button
                                                                    id={`${prefix}-remove`}
                                                                    type="button"
                                                                    variant="ghost"
                                                                    size="xs"
                                                                    className={
                                                                        detailWorkspaceStyles.quietDangerButton
                                                                    }
                                                                    disabled={
                                                                        array
                                                                            .state
                                                                            .value
                                                                            .length <=
                                                                            1 ||
                                                                        mutation.isPending
                                                                    }
                                                                    onClick={() =>
                                                                        array.removeValue(
                                                                            index,
                                                                        )
                                                                    }
                                                                >
                                                                    <Trash2Icon
                                                                        aria-hidden="true"
                                                                        data-icon="inline-start"
                                                                    />
                                                                    移除
                                                                </Button>
                                                            </div>

                                                            <div className="grid grid-cols-1 gap-x-3 gap-y-3 sm:grid-cols-12">
                                                                <form.AppField
                                                                    name={`lines[${index}].category`}
                                                                >
                                                                    {(
                                                                        field,
                                                                    ) => (
                                                                        <field.SelectField
                                                                            id={`${prefix}-category`}
                                                                            label="费用类别"
                                                                            required
                                                                            allowClear={
                                                                                false
                                                                            }
                                                                            options={
                                                                                CATEGORY_OPTIONS
                                                                            }
                                                                            placeholder="选择类别"
                                                                            className="sm:col-span-4"
                                                                        />
                                                                    )}
                                                                </form.AppField>
                                                                <form.AppField
                                                                    name={`lines[${index}].description`}
                                                                >
                                                                    {(
                                                                        field,
                                                                    ) => (
                                                                        <field.TextField
                                                                            id={`${prefix}-description`}
                                                                            label="费用说明"
                                                                            required
                                                                            placeholder="例如：加急专车配送、定制外包装"
                                                                            className="sm:col-span-8"
                                                                        />
                                                                    )}
                                                                </form.AppField>
                                                            </div>

                                                            <div className="grid grid-cols-1 items-start gap-x-3 gap-y-3 sm:grid-cols-12">
                                                                <form.AppField
                                                                    name={`lines[${index}].grossAmount`}
                                                                >
                                                                    {(
                                                                        field,
                                                                    ) => (
                                                                        <field.TextField
                                                                            money
                                                                            id={`${prefix}-gross`}
                                                                            label="含税金额（元）"
                                                                            required
                                                                            inputMode="decimal"
                                                                            inputClassName="num"
                                                                            placeholder="0.00"
                                                                            className="sm:col-span-4"
                                                                        />
                                                                    )}
                                                                </form.AppField>
                                                                <form.AppField
                                                                    name={`lines[${index}].taxRatePercent`}
                                                                >
                                                                    {(
                                                                        field,
                                                                    ) => (
                                                                        <field.TextField
                                                                            id={`${prefix}-tax-rate`}
                                                                            label="进项税率（%）"
                                                                            required
                                                                            inputMode="decimal"
                                                                            inputClassName="num"
                                                                            placeholder="0"
                                                                            className="sm:col-span-3"
                                                                        />
                                                                    )}
                                                                </form.AppField>
                                                                <form.AppField
                                                                    name={`lines[${index}].salesOrderLineId`}
                                                                >
                                                                    {(
                                                                        field,
                                                                    ) => (
                                                                        <field.SelectField
                                                                            id={`${prefix}-sales-line`}
                                                                            label="归属明细"
                                                                            allowClear={
                                                                                false
                                                                            }
                                                                            options={
                                                                                lineOptions
                                                                            }
                                                                            className="sm:col-span-5"
                                                                        />
                                                                    )}
                                                                </form.AppField>
                                                            </div>

                                                            <div className="flex flex-wrap items-center gap-1.5 pt-0.5">
                                                                <span className="text-xs text-muted-foreground">
                                                                    常用税率
                                                                </span>
                                                                {COMMON_TAX_RATE_PERCENTS.map(
                                                                    (rate) => {
                                                                        const active =
                                                                            samePercentValue(
                                                                                line.taxRatePercent,
                                                                                rate,
                                                                            )
                                                                        return (
                                                                            <button
                                                                                key={
                                                                                    rate
                                                                                }
                                                                                id={`${prefix}-tax-preset-${rate}`}
                                                                                type="button"
                                                                                aria-pressed={
                                                                                    active
                                                                                }
                                                                                disabled={
                                                                                    mutation.isPending
                                                                                }
                                                                                onClick={() =>
                                                                                    form.setFieldValue(
                                                                                        `lines[${index}].taxRatePercent`,
                                                                                        rate,
                                                                                    )
                                                                                }
                                                                                className={cn(
                                                                                    "num inline-flex h-6 cursor-pointer items-center rounded-md px-2 text-xs transition-colors focus-visible:outline-2 focus-visible:outline-primary disabled:pointer-events-none disabled:opacity-50",
                                                                                    active
                                                                                        ? "bg-sidebar-accent font-medium text-sidebar-accent-foreground"
                                                                                        : "bg-muted/65 text-muted-foreground hover:bg-muted hover:text-foreground",
                                                                                )}
                                                                            >
                                                                                {
                                                                                    rate
                                                                                }
                                                                                %
                                                                            </button>
                                                                        )
                                                                    },
                                                                )}
                                                            </div>
                                                        </div>
                                                    )
                                                },
                                            )}
                                        </div>
                                    </section>
                                )}
                            </form.AppField>

                            {failure ? (
                                <Alert variant="destructive">
                                    <AlertDescription>
                                        {failure}
                                    </AlertDescription>
                                </Alert>
                            ) : null}
                        </DialogBody>

                        <DialogFooter className="border-grid bg-card sm:items-center sm:justify-between">
                            <form.Subscribe
                                selector={(state) => state.values.lines}
                            >
                                {(lines) => {
                                    const summary = summarizeLines(lines)
                                    return (
                                        <div className="flex flex-wrap items-baseline gap-x-3 gap-y-0.5 text-left">
                                            <div className="flex items-baseline gap-1.5">
                                                <span className="text-xs text-muted-foreground">
                                                    含税合计
                                                </span>
                                                {summary ? (
                                                    <MoneyValue
                                                        value={summary.gross}
                                                        display="exact"
                                                    />
                                                ) : (
                                                    <span className="num text-muted-foreground">
                                                        —
                                                    </span>
                                                )}
                                            </div>
                                            {summary ? (
                                                <span className="num text-xs text-muted-foreground">
                                                    不含税{" "}
                                                    <MoneyValue
                                                        value={summary.net}
                                                        display="exact"
                                                    />{" "}
                                                    · 税额{" "}
                                                    <MoneyValue
                                                        value={summary.tax}
                                                        display="exact"
                                                    />
                                                </span>
                                            ) : null}
                                        </div>
                                    )
                                }}
                            </form.Subscribe>
                            <div className="flex items-center justify-end gap-2">
                                <Button
                                    id="sales-orders-expense-purchase-cancel"
                                    type="button"
                                    variant="outline"
                                    size="sm"
                                    disabled={mutation.isPending}
                                    onClick={() => handleOpenChange(false)}
                                >
                                    取消
                                </Button>
                                <form.AppForm>
                                    <form.SubmitButton
                                        id="sales-orders-expense-purchase-submit"
                                        size="sm"
                                        showError={false}
                                        label="创建并提交审批"
                                        pendingLabel="提交中…"
                                    />
                                </form.AppForm>
                            </div>
                        </DialogFooter>
                    </form>
                )}
            </DialogContent>
        </Dialog>
    )
}
