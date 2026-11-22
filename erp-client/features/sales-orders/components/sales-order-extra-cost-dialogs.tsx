"use client"

import * as React from "react"
import { ArrowRightIcon } from "lucide-react"

import { detailWorkspaceStyles } from "@/components/business"
import { useAppForm } from "@/components/form"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Field, FieldLabel } from "@/components/ui/field"
import { toast } from "@/components/ui/toast"
import { SupplierSearchCombobox } from "@/features/entity-selectors/components/supplier-search-combobox"
import type {
    ExtraCostCategory,
    SalesExtraCost,
} from "@/features/sales-orders/api/sales-order-extra-costs"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import {
    useReduceSalesExtraCostMutation,
    useRegisterSalesExtraCostMutation,
} from "@/features/sales-orders/hooks/queries"
import {
    COMMON_TAX_RATE_PERCENTS,
    EXTRA_COST_CATEGORY_LABEL,
    EXTRA_COST_WHOLE_ORDER,
    REGISTER_CATEGORY_OPTIONS,
    newExtraCostRequestKey,
    reduceDefaults,
    reduceExtraCostSchema,
    registerDefaults,
    registerExtraCostSchema,
    samePercentValue,
    taxRateFromPercent,
    todayDate,
} from "@/features/sales-orders/lib/extra-cost"
import { getErrorMessage } from "@/lib/api/errors"
import { cn } from "@/lib/utils"
import { ExtraCostAmountBreakdown } from "./extra-cost-amount-breakdown"
import {
    InquiryFeeRegisterSection,
    useInquiryFeeRegisterState,
} from "./inquiry-fee-register-section"

type RegisterExtraCostMode = "manual" | "inquiry"

/** 财务按凭证登记附加费用。 */
export function RegisterExtraCostDialog({
    order,
    open,
    onOpenChange,
}: {
    order: SalesOrderDetailView
    open: boolean
    onOpenChange: (open: boolean) => void
}) {
    const mutation = useRegisterSalesExtraCostMutation(order.id)
    const [mode, setMode] = React.useState<RegisterExtraCostMode>("manual")
    const hasChosenModeRef = React.useRef(false)
    const [requestKey, setRequestKey] = React.useState(() =>
        newExtraCostRequestKey("register"),
    )
    const [failure, setFailure] = React.useState<string | null>(null)
    const lineOptions = React.useMemo(
        () => [
            { value: EXTRA_COST_WHOLE_ORDER, label: "整张销售单" },
            ...order.lineItems
                .filter((line) => line.quantity !== "0")
                .map((line) => ({
                    value: line.id,
                    label: line.sku ? `${line.name}（${line.sku}）` : line.name,
                })),
        ],
        [order.lineItems],
    )

    const inquiry = useInquiryFeeRegisterState({
        salesOrderId: order.id,
        open,
        onRegistered: (input) => {
            onOpenChange(false)
            toast.add({
                title: "询价附加费用已登记",
                description: `${EXTRA_COST_CATEGORY_LABEL[input.category]} · ${input.description}，已扣减对应询价额度并计入本单盈亏。`,
                type: "success",
            })
        },
    })

    const form = useAppForm({
        defaultValues: registerDefaults(),
        validators: { onChange: registerExtraCostSchema },
        onSubmit: async ({ value }) => {
            setFailure(null)
            const category = value.category as ExtraCostCategory
            const description = value.description.trim()
            try {
                await mutation.mutateAsync({
                    category,
                    description,
                    grossAmount: value.grossAmount.trim(),
                    inputTaxRate: taxRateFromPercent(value.taxRatePercent),
                    occurredOn: value.occurredOn,
                    supplierId: value.supplierId || undefined,
                    salesOrderLineId:
                        value.salesOrderLineId === EXTRA_COST_WHOLE_ORDER
                            ? undefined
                            : value.salesOrderLineId || undefined,
                    evidence: value.evidence as File,
                    idempotencyKey: requestKey,
                })
                onOpenChange(false)
                toast.add({
                    title: "附加费用已登记",
                    description: `${EXTRA_COST_CATEGORY_LABEL[category]} · ${description}，已计入本单实际经营盈亏。`,
                    type: "success",
                })
            } catch (error) {
                setFailure(getErrorMessage(error, "登记失败，请稍后重试。"))
            }
        },
    })

    const wasOpen = React.useRef(false)
    React.useEffect(() => {
        if (open && !wasOpen.current) {
            form.reset(registerDefaults())
            setRequestKey(newExtraCostRequestKey("register"))
            setFailure(null)
            setMode("manual")
            hasChosenModeRef.current = false
        }
        wasOpen.current = open
    }, [form, open])

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

    const selectMode = (next: RegisterExtraCostMode) => {
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
                className="flex max-h-[min(90svh,48rem)] flex-col gap-0 overflow-hidden p-0 sm:max-w-2xl"
                closeButtonId="sales-orders-extra-cost-register-close"
                showCloseButton={!busy}
            >
                <DialogHeader className="shrink-0 gap-2 border-b border-grid px-6 py-4 text-left">
                    <div className="flex flex-wrap items-center gap-2 pr-8">
                        <DialogTitle>登记附加费用</DialogTitle>
                        <Badge variant="secondary" className="num font-normal">
                            {order.documentNumber}
                        </Badge>
                        {order.customerName ? (
                            <span className="max-w-60 truncate text-xs text-muted-foreground">
                                {order.customerName}
                            </span>
                        ) : null}
                    </div>
                    <DialogDescription>
                        登记已在 ERP
                        外支付或无需付款的费用，直接计入本单实际经营盈亏；需通过
                        ERP 向供应商付款的请由采购「追加费用采购」。
                    </DialogDescription>
                    {showModeSwitcher ? (
                        <div className="pt-1">
                            <div
                                role="tablist"
                                aria-label="附加费用登记方式"
                                className={detailWorkspaceStyles.subTabs}
                            >
                                <button
                                    id="sales-orders-extra-cost-mode-manual"
                                    type="button"
                                    role="tab"
                                    aria-selected={mode === "manual"}
                                    data-active={mode === "manual"}
                                    disabled={busy}
                                    className={detailWorkspaceStyles.subTab}
                                    onClick={() => selectMode("manual")}
                                >
                                    直接登记
                                </button>
                                <button
                                    id="sales-orders-extra-cost-mode-inquiry"
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
                                        <span className="num rounded-sm bg-sidebar-accent px-1.5 py-0.5 text-tiny leading-none font-medium text-sidebar-accent-foreground">
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
                    <InquiryFeeRegisterSection
                        state={inquiry}
                        salesOrderNo={order.documentNumber}
                        lineOptions={lineOptions}
                        onCancel={() => handleOpenChange(false)}
                        onSwitchManual={() => selectMode("manual")}
                    />
                ) : (
                    <form
                        className="flex min-h-0 flex-1 flex-col"
                        onSubmit={(event) => {
                            event.preventDefault()
                            void form.handleSubmit()
                        }}
                    >
                        <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-6 py-5">
                            {inquiry.pending || inquiry.availableCount > 0 ? (
                                <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-info-border/70 bg-info-soft/45 px-3.5 py-2 text-xs text-info-soft-foreground">
                                    <span>
                                        {inquiry.pending
                                            ? "本单有一笔询价费用登记结果待确认，建议先核对原结果。"
                                            : `本单有 ${inquiry.availableCount} 笔已确认询价费用待登记；直接登记不会自动关联原询价额度。`}
                                    </span>
                                    <Button
                                        id="sales-orders-extra-cost-switch-inquiry"
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

                            <div className="grid grid-cols-1 gap-x-4 gap-y-3.5 sm:grid-cols-12">
                                <form.AppField
                                    name="category"
                                    children={(field) => (
                                        <field.SelectField
                                            id="sales-orders-extra-cost-register-category"
                                            label="费用类别"
                                            required
                                            allowClear={false}
                                            options={REGISTER_CATEGORY_OPTIONS}
                                            placeholder="选择类别"
                                            className="sm:col-span-4"
                                        />
                                    )}
                                />
                                <form.AppField
                                    name="description"
                                    children={(field) => (
                                        <field.TextField
                                            id="sales-orders-extra-cost-register-description"
                                            label="费用说明"
                                            required
                                            placeholder="例如：第三方加急快递、临时仓储费"
                                            className="sm:col-span-8"
                                        />
                                    )}
                                />
                            </div>

                            <div className="space-y-2.5">
                                <div className="grid grid-cols-1 items-start gap-x-4 gap-y-3.5 sm:grid-cols-12">
                                    <form.AppField
                                        name="grossAmount"
                                        children={(field) => (
                                            <field.TextField
                                                money
                                                id="sales-orders-extra-cost-register-gross"
                                                label="含税金额（元）"
                                                required
                                                inputMode="decimal"
                                                inputClassName="num"
                                                placeholder="0.00"
                                                className="sm:col-span-5"
                                            />
                                        )}
                                    />
                                    <form.AppField
                                        name="taxRatePercent"
                                        children={(field) => (
                                            <field.TextField
                                                id="sales-orders-extra-cost-register-tax-rate"
                                                label="进项税率（%）"
                                                required
                                                inputMode="decimal"
                                                inputClassName="num"
                                                placeholder="0"
                                                className="sm:col-span-3"
                                            />
                                        )}
                                    />
                                    <form.AppField
                                        name="occurredOn"
                                        children={(field) => (
                                            <field.DateField
                                                id="sales-orders-extra-cost-register-occurred-on"
                                                label="发生日期"
                                                required
                                                clearable={false}
                                                disabledDates={{
                                                    after: new Date(
                                                        `${todayDate()}T00:00:00`,
                                                    ),
                                                }}
                                                className="sm:col-span-4"
                                            />
                                        )}
                                    />
                                </div>

                                <form.Subscribe
                                    selector={(state) =>
                                        [
                                            state.values.grossAmount,
                                            state.values.taxRatePercent,
                                        ] as const
                                    }
                                >
                                    {([grossAmount, taxRatePercent]) => (
                                        <div className="space-y-2.5">
                                            <div className="flex flex-wrap items-center justify-between gap-2">
                                                <div className="flex flex-wrap items-center gap-1.5">
                                                    <span className="text-xs text-muted-foreground">
                                                        常用税率
                                                    </span>
                                                    {COMMON_TAX_RATE_PERCENTS.map(
                                                        (rate) => {
                                                            const active =
                                                                samePercentValue(
                                                                    taxRatePercent,
                                                                    rate,
                                                                )
                                                            return (
                                                                <button
                                                                    key={rate}
                                                                    id={`sales-orders-extra-cost-register-tax-preset-${rate}`}
                                                                    type="button"
                                                                    aria-pressed={
                                                                        active
                                                                    }
                                                                    disabled={
                                                                        mutation.isPending
                                                                    }
                                                                    onClick={() =>
                                                                        form.setFieldValue(
                                                                            "taxRatePercent",
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
                                                                    {rate}%
                                                                </button>
                                                            )
                                                        },
                                                    )}
                                                </div>
                                                <span className="text-xs text-muted-foreground">
                                                    无票或免税填 0%
                                                </span>
                                            </div>
                                            <ExtraCostAmountBreakdown
                                                grossAmount={grossAmount}
                                                taxRatePercent={taxRatePercent}
                                            />
                                        </div>
                                    )}
                                </form.Subscribe>
                            </div>

                            <form.AppField
                                name="evidence"
                                children={(field) => (
                                    <field.EvidenceUploadField
                                        id="sales-orders-extra-cost-register-evidence"
                                        label="费用凭证"
                                        required
                                        density="compact"
                                        uploadLabel="上传账单、运单或支付凭证"
                                        description="支持 PDF、JPG、PNG 或 WebP，不超过 5 MB。"
                                    />
                                )}
                            />

                            <div className="space-y-3 border-t border-border/60 pt-4">
                                <div className="flex flex-wrap items-baseline gap-2">
                                    <span className="text-xs font-medium text-foreground/80">
                                        归属与收款方
                                    </span>
                                    <span className="text-xs text-muted-foreground">
                                        选填，默认归属整张销售单
                                    </span>
                                </div>
                                <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
                                    <form.AppField
                                        name="salesOrderLineId"
                                        children={(field) => (
                                            <field.SelectField
                                                id="sales-orders-extra-cost-register-line"
                                                label="归属明细"
                                                allowClear={false}
                                                options={lineOptions}
                                            />
                                        )}
                                    />
                                    <form.AppField
                                        name="supplierId"
                                        children={(field) => (
                                            <Field>
                                                <FieldLabel htmlFor="sales-orders-extra-cost-register-supplier">
                                                    收款供应商
                                                </FieldLabel>
                                                <SupplierSearchCombobox
                                                    id="sales-orders-extra-cost-register-supplier"
                                                    value={
                                                        field.state.value ||
                                                        undefined
                                                    }
                                                    onValueChange={(id) =>
                                                        field.handleChange(
                                                            id ?? "",
                                                        )
                                                    }
                                                    placeholder="选填，收款方已建档时选择"
                                                />
                                            </Field>
                                        )}
                                    />
                                </div>
                            </div>

                            {failure ? (
                                <Alert variant="destructive">
                                    <AlertDescription>
                                        {failure}
                                    </AlertDescription>
                                </Alert>
                            ) : null}
                        </div>

                        <DialogFooter className="shrink-0 border-t border-grid bg-card px-6 py-3.5 sm:items-center sm:justify-between">
                            <p className="text-xs text-muted-foreground">
                                登记后直接计入本单实际经营盈亏，不生成应付。
                            </p>
                            <div className="flex items-center justify-end gap-2">
                                <Button
                                    id="sales-orders-extra-cost-register-cancel"
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
                                        id="sales-orders-extra-cost-register-submit"
                                        size="sm"
                                        showError={false}
                                        label="登记费用"
                                        pendingLabel="登记中…"
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

/** 冲减一笔财务登记的附加费用。 */
export function ReduceExtraCostDialog({
    order,
    cost,
    onOpenChange,
}: {
    order: SalesOrderDetailView
    cost: SalesExtraCost | null
    onOpenChange: (open: boolean) => void
}) {
    const mutation = useReduceSalesExtraCostMutation(order.id)
    const remaining = cost?.remainingGrossAmount ?? "0.00"
    const [requestKey, setRequestKey] = React.useState(() =>
        newExtraCostRequestKey("reduce"),
    )
    const [failure, setFailure] = React.useState<string | null>(null)
    const schema = React.useMemo(
        () => reduceExtraCostSchema(remaining),
        [remaining],
    )
    const form = useAppForm({
        defaultValues: reduceDefaults(),
        validators: { onChange: schema },
        onSubmit: async ({ value }) => {
            if (!cost) return
            setFailure(null)
            try {
                await mutation.mutateAsync({
                    costEntryId: cost.id,
                    value: {
                        description: value.description.trim(),
                        grossAmount: value.grossAmount.trim(),
                        occurredOn: value.occurredOn,
                        evidence: value.evidence as File,
                        idempotencyKey: requestKey,
                    },
                })
                onOpenChange(false)
            } catch (error) {
                setFailure(getErrorMessage(error, "冲减失败，请稍后重试。"))
            }
        },
    })

    const openId = cost?.id ?? null
    const lastOpenId = React.useRef<string | null>(null)
    React.useEffect(() => {
        if (openId && lastOpenId.current !== openId) {
            form.reset(reduceDefaults())
            setRequestKey(newExtraCostRequestKey("reduce"))
            setFailure(null)
        }
        lastOpenId.current = openId
    }, [form, openId])

    return (
        <Dialog open={Boolean(cost)} onOpenChange={onOpenChange}>
            <DialogContent
                className="sm:max-w-lg"
                closeButtonId="sales-orders-extra-cost-reduce-close"
            >
                <DialogHeader>
                    <DialogTitle>冲减附加费用</DialogTitle>
                    <DialogDescription>
                        {cost
                            ? `「${cost.description}」剩余 ${remaining} 元可冲减。冲减不修改原费用，追加一笔冲减记录。`
                            : null}
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="flex flex-col gap-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <form.AppField
                        name="description"
                        children={(field) => (
                            <field.TextField
                                id="sales-orders-extra-cost-reduce-description"
                                label="冲减原因"
                                required
                                placeholder="例如：快递公司退回部分运费"
                            />
                        )}
                    />
                    <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
                        <form.AppField
                            name="grossAmount"
                            children={(field) => (
                                <field.TextField
                                    money
                                    id="sales-orders-extra-cost-reduce-gross"
                                    label="冲减金额（元）"
                                    required
                                    inputMode="decimal"
                                    placeholder={remaining}
                                />
                            )}
                        />
                        <form.AppField
                            name="occurredOn"
                            children={(field) => (
                                <field.DateField
                                    id="sales-orders-extra-cost-reduce-occurred-on"
                                    label="冲减日期"
                                    required
                                    clearable={false}
                                    disabledDates={{
                                        after: new Date(
                                            `${todayDate()}T00:00:00`,
                                        ),
                                    }}
                                />
                            )}
                        />
                    </div>
                    <form.AppField
                        name="evidence"
                        children={(field) => (
                            <field.EvidenceUploadField
                                id="sales-orders-extra-cost-reduce-evidence"
                                label="冲减凭证"
                                required
                                uploadLabel="上传退款或更正凭证"
                            />
                        )}
                    />
                    {failure ? (
                        <p
                            role="alert"
                            className="text-body-compact text-destructive"
                        >
                            {failure}
                        </p>
                    ) : null}
                    <DialogFooter>
                        <Button
                            id="sales-orders-extra-cost-reduce-cancel"
                            type="button"
                            variant="ghost"
                            disabled={mutation.isPending}
                            onClick={() => onOpenChange(false)}
                        >
                            取消
                        </Button>
                        <form.AppForm>
                            <form.SubmitButton
                                id="sales-orders-extra-cost-reduce-submit"
                                label="确认冲减"
                                pendingLabel="冲减中…"
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
