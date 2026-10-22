"use client"

import * as React from "react"
import { useAppForm } from "@/components/form"
import {
    BusinessFailureState,
    MoneyValue,
    QuantityValue,
    RateValue,
} from "@/components/business"
import { OptionCombobox } from "@/components/business/option-combobox"
import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import { Alert, AlertDescription } from "@/components/ui/alert"
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
    DialogFooter,
} from "@/components/ui/dialog"
import type { SupplierOfferingView } from "@/features/supplier-offerings/types"
import { useSupplierOfferingsForSkusQuery } from "@/features/supplier-offerings/hooks/queries"
import { getErrorMessage } from "@/lib/api/errors"
import { hasPermission } from "@/lib/permissions"
import { multiplyFixed } from "@/lib/fixed-decimal"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { inquiryPath } from "../api"
import { useInquirySkuDetailsQuery } from "../hooks/queries"
import { useInquiryCommand } from "../hooks/use-inquiry-command"
import { inquiryDateInput } from "../lib/format"
import {
    inquiryStorageKey,
    readInquiryLocalValue,
    writeInquiryLocalValue,
} from "../lib/local-draft"
import type { InquiryLine, ProcurementInquiry } from "../types"
import type { InquiryTask } from "./inquiry-feedback-editor"

import { useInternalQuotes } from "../hooks/use-internal-quotes"
import {
    latestSupplierQuotes,
    quoteExclusion,
    type QuoteInput,
    type InternalQuote,
} from "../lib/internal-quotes"

function blankQuote(): QuoteInput {
    return {
        quote_id: crypto.randomUUID(),
        revision_no: 1,
        line_id: "",
        supplier_id: "",
        supplier_offering_id: "",
        internal_note: "",
        terms: {
            unavailable: false,
            freight_gross: "",
            unit_cost_gross: "",
            input_tax_rate: "",
            available_quantity: "",
            expected_delivery_date: "",
            valid_to: "",
        },
    }
}

function normalizedQuote(value: QuoteInput): QuoteInput {
    return {
        quote_id: value.quote_id,
        revision_no: value.revision_no,
        line_id: value.line_id,
        supplier_id: value.supplier_id,
        supplier_offering_id: value.supplier_offering_id,
        internal_note: value.internal_note,
        terms: {
            unavailable: value.terms.unavailable ?? false,
            freight_gross: value.terms.freight_gross ?? "",
            unit_cost_gross: value.terms.unit_cost_gross,
            input_tax_rate: value.terms.input_tax_rate,
            available_quantity: value.terms.available_quantity,
            expected_delivery_date: value.terms.expected_delivery_date,
            valid_to: value.terms.valid_to ?? "",
        },
    }
}

function demandTerms(line?: InquiryLine): Partial<QuoteInput["terms"]> {
    if (!line) return {}
    return {
        available_quantity: line.quantity,
        expected_delivery_date: inquiryDateInput(line.delivery_due_at),
    }
}

export function InquiryInternalQuotes({
    inquiry,
    task,
    userId,
    permissions,
    onSaved,
    lineId,
    onAdopt,
}: {
    inquiry: ProcurementInquiry
    lineId?: string
    onAdopt?: (quote: InternalQuote) => boolean
    task: InquiryTask
    userId: string
    permissions: readonly string[]
    onSaved: () => void
}) {
    const [open, setOpen] = React.useState(false)
    const [showHistory, setShowHistory] = React.useState(false)
    const [initial, setInitial] = React.useState<QuoteInput | null>(null)
    const canRead = hasPermission(
        permissions,
        "procurement_inquiry:internal_quote",
    )
    const quotes = useInternalQuotes(inquiry.id, task.id, canRead)
    const [adoptMessage, setAdoptMessage] = React.useState("")
    const skuDetails = useInquirySkuDetailsQuery(
        canRead ? inquiry.draft.lines.map((line) => line.sku_id) : [],
    )
    const offerings = useSupplierOfferingsForSkusQuery(
        canRead ? inquiry.draft.lines.map((line) => line.sku_id) : [],
    )
    const storageKey = inquiryStorageKey(
        userId,
        `${inquiry.id}:${task.id}:internal-quote${lineId ? `:${lineId}` : ""}`,
    )
    const legacyStorageKey = inquiryStorageKey(
        userId,
        `${inquiry.id}:${task.id}:internal-quote`,
    )
    const allQuotes = (quotes.data ?? []).filter(
        (quote) => !lineId || quote.line_id === lineId,
    )
    const currentQuotes = latestSupplierQuotes(allQuotes)
    const visibleQuotes = showHistory ? allQuotes : currentQuotes
    if (!canRead) return null
    return (
        <section className="space-y-4" aria-label="采购内部报价">
            <div className="flex flex-wrap items-center justify-between gap-3">
                <div>
                    <h2 className="text-lg font-medium">供应商报价</h2>
                    <p className="mt-1 text-xs text-muted-foreground">
                        仅当前责任采购可见。每次保存新增报价版本，不修改已发布的对销售结果。
                    </p>
                </div>
                <Button
                    id="procurement-inquiry-add-internal-quote"
                    disabled={offerings.isPending}
                    type="button"
                    variant="outline"
                    onClick={() => {
                        const legacy =
                            readInquiryLocalValue<QuoteInput>(legacyStorageKey)
                        const saved =
                            readInquiryLocalValue<QuoteInput>(storageKey) ??
                            (legacy && legacy.line_id === lineId
                                ? legacy
                                : null)
                        const line =
                            inquiry.draft.lines.find(
                                (item) => item.line_id === lineId,
                            ) ??
                            (inquiry.draft.lines.length === 1
                                ? inquiry.draft.lines[0]
                                : undefined)
                        const candidates =
                            offerings.data?.filter(
                                (item) =>
                                    item.sku_id === line?.sku_id &&
                                    item.status === "ACTIVE",
                            ) ?? []
                        const offering =
                            candidates.length === 1 ? candidates[0] : undefined
                        const fresh = blankQuote()
                        setInitial(
                            saved
                                ? normalizedQuote(saved)
                                : {
                                      ...fresh,
                                      line_id: line?.line_id ?? "",
                                      supplier_id: offering?.supplier_id ?? "",
                                      supplier_offering_id: offering?.id ?? "",
                                      terms: {
                                          ...fresh.terms,
                                          ...demandTerms(line),
                                          input_tax_rate:
                                              offering?.input_tax_rate ?? "",
                                      },
                                  },
                        )
                        setOpen(true)
                    }}
                >
                    登记供应商报价
                </Button>
            </div>
            {allQuotes.length > currentQuotes.length && (
                <Button
                    id="procurement-inquiry-quote-history"
                    type="button"
                    size="sm"
                    variant="ghost"
                    onClick={() => setShowHistory((value) => !value)}
                >
                    {showHistory
                        ? "收起历史报价"
                        : `查看历史报价（${allQuotes.length - currentQuotes.length}）`}
                </Button>
            )}
            {adoptMessage && (
                <Alert>
                    <AlertDescription>{adoptMessage}</AlertDescription>
                </Alert>
            )}
            {quotes.isError ? (
                <BusinessFailureState
                    id="procurement-inquiry-quotes-retry"
                    title="内部报价加载失败"
                    error={quotes.error}
                    onRetry={() => void quotes.refetch()}
                />
            ) : quotes.isPending ? (
                <p role="status">正在读取内部报价…</p>
            ) : visibleQuotes.length === 0 ? (
                <p className="rounded-lg border border-dashed p-4 text-sm text-muted-foreground">
                    尚未登记内部报价。
                </p>
            ) : (
                <div className="space-y-3">
                    {visibleQuotes.map((quote) => {
                        const offering = offerings.data?.find(
                            (item) => item.id === quote.supplier_offering_id,
                        )
                        const line = inquiry.draft.lines.find(
                            (item) => item.line_id === quote.line_id,
                        )
                        const prefix = `procurement-inquiry-quote-${toAutomationIdSegment(quote.id)}`
                        const isCurrent = currentQuotes.some(
                            (item) => item.id === quote.id,
                        )
                        const excluded = offerings.isError
                            ? "供给资料读取失败"
                            : !offering || offering.status !== "ACTIVE"
                              ? "供给关系当前不可用"
                              : line
                                ? quoteExclusion(
                                      quote,
                                      line,
                                      inquiry.feedback_round + 1,
                                  )
                                : "商品需求缺失"
                        return (
                            <article
                                key={quote.id}
                                className="space-y-3 rounded-lg border p-4"
                            >
                                <div className="space-y-3">
                                    <div>
                                        <h3 className="wrap-anywhere font-medium">
                                            {!lineId &&
                                                `${line?.product_name ?? "商品报价"} · `}
                                            {offering?.supplier_name ??
                                                "已登记供应商"}
                                        </h3>
                                        <p className="mt-1 text-xs text-muted-foreground">
                                            {isCurrent
                                                ? "当前回复"
                                                : "历史回复"}{" "}
                                            · 第 {quote.revision_no} 版 ·{" "}
                                            {quote.terms.valid_to
                                                ? `${quote.terms.valid_to} 前有效`
                                                : "未限定失效日期"}
                                        </p>
                                    </div>
                                    <div className="flex flex-wrap items-center gap-2">
                                        {onAdopt && isCurrent && (
                                            <Button
                                                id={`${prefix}-adopt`}
                                                size="sm"
                                                variant="outline"
                                                disabled={
                                                    Boolean(excluded) ||
                                                    quotes.isFetching
                                                }
                                                title={excluded ?? undefined}
                                                onClick={() => {
                                                    setAdoptMessage(
                                                        onAdopt(quote)
                                                            ? "已带入供给价、税率与有效期，请核对后保存反馈。"
                                                            : "反馈当前不可编辑，请关闭侧栏检查页面提示。",
                                                    )
                                                }}
                                            >
                                                采用此报价
                                            </Button>
                                        )}
                                        <Button
                                            id={`${prefix}-revise`}
                                            disabled={!isCurrent}
                                            type="button"
                                            size="sm"
                                            variant="outline"
                                            onClick={() => {
                                                setInitial({
                                                    ...quote,
                                                    terms: {
                                                        ...quote.terms,
                                                        unavailable: false,
                                                        freight_gross:
                                                            quote.terms
                                                                .freight_gross ??
                                                            "",
                                                        available_quantity:
                                                            quote.terms
                                                                .unavailable
                                                                ? (line?.quantity ??
                                                                  "")
                                                                : quote.terms
                                                                      .available_quantity,
                                                        unit_cost_gross: quote
                                                            .terms.unavailable
                                                            ? ""
                                                            : quote.terms
                                                                  .unit_cost_gross,
                                                        valid_to:
                                                            quote.terms
                                                                .valid_to ?? "",
                                                    },
                                                    revision_no:
                                                        quote.revision_no + 1,
                                                })
                                                setOpen(true)
                                            }}
                                        >
                                            登记新版本
                                        </Button>
                                    </div>
                                </div>
                                {excluded && (
                                    <p className="text-xs text-muted-foreground">
                                        {excluded}
                                    </p>
                                )}
                                {!quote.terms.unavailable && (
                                    <p className="grid gap-2 text-sm">
                                        <span>
                                            运费{" "}
                                            {quote.terms.freight_gross ==
                                            null ? (
                                                "未确认"
                                            ) : (
                                                <MoneyValue
                                                    value={
                                                        quote.terms
                                                            .freight_gross
                                                    }
                                                />
                                            )}
                                        </span>
                                        <span>
                                            承诺交付{" "}
                                            {quote.terms.expected_delivery_date}
                                        </span>
                                        <span>
                                            含税报价{" "}
                                            <MoneyValue
                                                value={
                                                    quote.terms.unit_cost_gross
                                                }
                                            />
                                        </span>
                                        <span>
                                            税率{" "}
                                            <RateValue
                                                value={multiplyFixed(
                                                    quote.terms.input_tax_rate,
                                                    "100",
                                                    {
                                                        leftMaxScale: 6,
                                                        rightMaxScale: 0,
                                                        outputScale: 2,
                                                    },
                                                )}
                                                precision={2}
                                            />
                                        </span>
                                        <span>
                                            可供{" "}
                                            <QuantityValue
                                                value={
                                                    quote.terms
                                                        .available_quantity
                                                }
                                                unit={
                                                    line
                                                        ? (skuDetails.units.get(
                                                              line.unit_code,
                                                          ) ?? "单位未匹配")
                                                        : undefined
                                                }
                                            />
                                        </span>
                                    </p>
                                )}
                                {quote.internal_note && (
                                    <p className="wrap-anywhere whitespace-pre-wrap text-sm text-muted-foreground">
                                        {quote.internal_note}
                                    </p>
                                )}
                            </article>
                        )
                    })}
                </div>
            )}
            {initial && (
                <QuoteEditor
                    key={`${initial.quote_id}:${initial.revision_no}`}
                    initial={initial}
                    inquiry={
                        lineId
                            ? {
                                  ...inquiry,
                                  draft: {
                                      ...inquiry.draft,
                                      lines: inquiry.draft.lines.filter(
                                          (line) => line.line_id === lineId,
                                      ),
                                  },
                              }
                            : inquiry
                    }
                    task={task}
                    userId={userId}
                    storageKey={storageKey}
                    open={open}
                    onOpenChange={setOpen}
                    offerings={offerings.data ?? []}
                    offeringError={offerings.error}
                    units={skuDetails.units}
                    onSaved={() => {
                        if (
                            lineId &&
                            readInquiryLocalValue<QuoteInput>(legacyStorageKey)
                                ?.line_id === lineId
                        )
                            localStorage.removeItem(legacyStorageKey)
                        void quotes.refetch()
                        onSaved()
                    }}
                />
            )}
        </section>
    )
}

function QuoteEditor({
    initial,
    inquiry,
    task,
    userId,
    storageKey,
    open,
    onOpenChange,
    offerings,
    offeringError,
    units,
    onSaved,
}: {
    initial: QuoteInput
    inquiry: ProcurementInquiry
    task: InquiryTask
    userId: string
    storageKey: string
    open: boolean
    onOpenChange: (value: boolean) => void
    offerings: readonly SupplierOfferingView[]
    offeringError: unknown
    units: ReadonlyMap<string, string>
    onSaved: () => void
}) {
    const command = useInquiryCommand(userId, inquiry.id)
    const moreConditions = React.useRef<HTMLDetailsElement>(null)
    const [error, setError] = React.useState<string | null>(null)
    const form = useAppForm({
        defaultValues: normalizedQuote(initial),
        listeners: {
            onChange: ({ formApi }) => {
                try {
                    writeInquiryLocalValue(storageKey, formApi.state.values)
                } catch {
                    setError("浏览器无法保留输入，请先登记报价后离开。")
                }
            },
        },
        onSubmit: async ({ value }) => {
            setError(null)
            try {
                if (!value.line_id || !value.supplier_offering_id)
                    throw new Error("请选择商品和当前可用的供应商供给。")
                if (!value.terms.input_tax_rate)
                    throw new Error("请展开“数量、交期及税率”补充税率。")
                const quote = {
                    quote_id: value.quote_id,
                    revision_no: value.revision_no,
                    line_id: value.line_id,
                    supplier_id: value.supplier_id,
                    supplier_offering_id: value.supplier_offering_id,
                    terms: {
                        ...normalizedQuote(value).terms,
                        valid_to: value.terms.valid_to || null,
                        freight_gross: value.terms.freight_gross || null,
                    },
                    internal_note: value.internal_note,
                }
                await command.run(
                    `${inquiryPath(true, inquiry.id)}/internal-quotes`,
                    {
                        expected_version: inquiry.version,
                        task_id: task.id,
                        task_version: task.version,
                        quote,
                    },
                )
                localStorage.removeItem(storageKey)
                onOpenChange(false)
                onSaved()
            } catch (failure) {
                if (moreConditions.current) moreConditions.current.open = true
                setError(
                    getErrorMessage(
                        failure,
                        "登记内部报价失败，请保留输入并重试。",
                    ),
                )
            }
        },
    })
    const selectLine = (lineId: string) => {
        const line = inquiry.draft.lines.find((item) => item.line_id === lineId)
        const candidates = offerings.filter(
            (item) => item.sku_id === line?.sku_id && item.status === "ACTIVE",
        )
        const offering = candidates.length === 1 ? candidates[0] : undefined
        form.setFieldValue("line_id", lineId)
        form.setFieldValue("supplier_offering_id", offering?.id ?? "")
        form.setFieldValue("supplier_id", offering?.supplier_id ?? "")
        form.setFieldValue("terms", {
            ...blankQuote().terms,
            ...demandTerms(line),
            input_tax_rate: offering?.input_tax_rate ?? "",
        })
    }
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                closeButtonId="procurement-inquiry-quote-editor-close"
                className="flex max-h-[85svh] flex-col gap-0 overflow-hidden p-0 sm:max-w-2xl"
            >
                <DialogHeader className="shrink-0 border-b px-6 py-5 pr-14">
                    <DialogTitle>登记供应商报价</DialogTitle>
                    <DialogDescription>
                        填写每单位商品的含税价格，仅采购可见。
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="flex min-h-0 flex-1 flex-col"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <fieldset
                        disabled={command.isPending}
                        className="min-h-0 min-w-0 flex-1 space-y-5 overflow-y-auto px-6 py-5"
                    >
                        {error && (
                            <Alert variant="destructive">
                                <AlertDescription>{error}</AlertDescription>
                            </Alert>
                        )}
                        {offeringError != null && (
                            <Alert variant="warning">
                                <AlertDescription>
                                    {getErrorMessage(
                                        offeringError,
                                        "供应商读取失败，请关闭后重试。",
                                    )}
                                </AlertDescription>
                            </Alert>
                        )}
                        <form.AppField
                            name="terms.freight_gross"
                            children={(field) => (
                                <field.TextField
                                    id="procurement-inquiry-quote-freight"
                                    label="本行总运费（元）"
                                    placeholder="已含运费填写 0，留空表示未确认"
                                    inputMode="decimal"
                                />
                            )}
                        />
                        <form.Subscribe
                            selector={(state) => state.values.line_id}
                            children={(lineId) => {
                                const line = inquiry.draft.lines.find(
                                    (item) => item.line_id === lineId,
                                )
                                const unit = line
                                    ? (units.get(line.unit_code) ??
                                      "单位未匹配")
                                    : ""
                                return (
                                    <>
                                        {inquiry.draft.lines.length > 1 && (
                                            <div className="space-y-2">
                                                <Label htmlFor="procurement-inquiry-quote-line">
                                                    商品需求
                                                </Label>
                                                <OptionCombobox
                                                    id="procurement-inquiry-quote-line"
                                                    aria-label="商品需求"
                                                    allowClear={false}
                                                    value={lineId}
                                                    disabled={
                                                        initial.revision_no > 1
                                                    }
                                                    options={inquiry.draft.lines.map(
                                                        (item) => ({
                                                            value: item.line_id,
                                                            label: `${item.product_name} · ${item.sku_code}`,
                                                        }),
                                                    )}
                                                    onValueChange={(id) =>
                                                        selectLine(id ?? "")
                                                    }
                                                />
                                            </div>
                                        )}
                                        {line && (
                                            <div className="space-y-3 rounded-lg bg-muted/40 p-4">
                                                <div>
                                                    <p className="font-medium">
                                                        {line.product_name}
                                                    </p>
                                                    <p className="mt-1 text-xs text-muted-foreground">
                                                        {line.specification
                                                            ? `${line.specification} · `
                                                            : ""}
                                                        SKU {line.sku_code}
                                                    </p>
                                                </div>
                                                <div className="flex flex-wrap gap-x-6 gap-y-2 text-sm">
                                                    <span className="text-muted-foreground">
                                                        需求数量{" "}
                                                        <QuantityValue
                                                            value={
                                                                line.quantity
                                                            }
                                                            unit={unit}
                                                        />
                                                    </span>
                                                    <span className="text-muted-foreground">
                                                        要求交付{" "}
                                                        <span className="text-foreground">
                                                            {inquiryDateInput(
                                                                line.delivery_due_at,
                                                            ) || "未设置"}
                                                        </span>
                                                    </span>
                                                </div>
                                                <p className="text-xs text-muted-foreground">
                                                    {line.allocations.length
                                                        ? `配送按询价需求的 ${line.allocations.length} 个地址执行`
                                                        : "交付按询价需求执行"}
                                                </p>
                                            </div>
                                        )}
                                        <div className="grid gap-4 sm:grid-cols-2">
                                            <form.Field
                                                name="supplier_offering_id"
                                                children={(field) => (
                                                    <div className="min-w-0 space-y-2">
                                                        <Label htmlFor="procurement-inquiry-quote-offering">
                                                            供应商
                                                        </Label>
                                                        <OptionCombobox
                                                            id="procurement-inquiry-quote-offering"
                                                            aria-label="供应商"
                                                            allowClear={false}
                                                            value={
                                                                field.state
                                                                    .value
                                                            }
                                                            disabled={
                                                                initial.revision_no >
                                                                1
                                                            }
                                                            options={offerings
                                                                .filter(
                                                                    (item) =>
                                                                        item.sku_id ===
                                                                            line?.sku_id &&
                                                                        item.status ===
                                                                            "ACTIVE",
                                                                )
                                                                .map(
                                                                    (item) => ({
                                                                        value: item.id,
                                                                        label:
                                                                            item.supplier_name ??
                                                                            "供应商名称未匹配",
                                                                    }),
                                                                )}
                                                            onValueChange={(
                                                                id,
                                                            ) => {
                                                                field.handleChange(
                                                                    id ?? "",
                                                                )
                                                                const offering =
                                                                    offerings.find(
                                                                        (
                                                                            item,
                                                                        ) =>
                                                                            item.id ===
                                                                            id,
                                                                    )
                                                                form.setFieldValue(
                                                                    "supplier_id",
                                                                    offering?.supplier_id ??
                                                                        "",
                                                                )
                                                                form.setFieldValue(
                                                                    "terms.input_tax_rate",
                                                                    offering?.input_tax_rate ??
                                                                        "",
                                                                )
                                                            }}
                                                        />
                                                    </div>
                                                )}
                                            />
                                            <form.AppField
                                                name="terms.unit_cost_gross"
                                                children={(field) => (
                                                    <field.TextField
                                                        id="procurement-inquiry-quote-cost"
                                                        label={
                                                            unit
                                                                ? `含税单价（元 / ${unit}）`
                                                                : "含税单价（元）"
                                                        }
                                                        placeholder="填写单价"
                                                        inputMode="decimal"
                                                        required
                                                    />
                                                )}
                                            />
                                        </div>
                                        <details
                                            ref={moreConditions}
                                            className="group rounded-lg border"
                                        >
                                            <summary
                                                id="procurement-inquiry-quote-more"
                                                className="cursor-pointer px-4 py-3 text-sm font-medium"
                                            >
                                                数量、交期及税率{" "}
                                                <span className="ml-2 font-normal text-muted-foreground">
                                                    按需调整
                                                </span>
                                            </summary>
                                            <div className="grid gap-4 border-t p-4 sm:grid-cols-2">
                                                <form.AppField
                                                    name="terms.available_quantity"
                                                    children={(field) => (
                                                        <field.TextField
                                                            id="procurement-inquiry-quote-available"
                                                            label={`可供数量${unit ? `（${unit}）` : ""}`}
                                                            inputMode="decimal"
                                                        />
                                                    )}
                                                />
                                                <form.AppField
                                                    name="terms.expected_delivery_date"
                                                    children={(field) => (
                                                        <field.DateField
                                                            id="procurement-inquiry-quote-delivery"
                                                            label="承诺交付日期"
                                                        />
                                                    )}
                                                />
                                                <form.AppField
                                                    name="terms.input_tax_rate"
                                                    children={(field) => (
                                                        <field.TextField
                                                            id="procurement-inquiry-quote-tax"
                                                            label="进项税率"
                                                            description="例如 0.13 表示 13%"
                                                            inputMode="decimal"
                                                        />
                                                    )}
                                                />
                                                <form.AppField
                                                    name="terms.valid_to"
                                                    children={(field) => (
                                                        <field.DateField
                                                            id="procurement-inquiry-quote-valid-to"
                                                            label="报价失效日期（选填）"
                                                            description="失效日期当天起不可采用"
                                                        />
                                                    )}
                                                />
                                            </div>
                                        </details>
                                    </>
                                )
                            }}
                        />
                        <form.AppField
                            name="internal_note"
                            children={(field) => (
                                <field.TextareaField
                                    id="procurement-inquiry-quote-note"
                                    label="备注（选填）"
                                    placeholder="有需要说明的报价条件时填写"
                                    rows={2}
                                    textareaClassName="min-h-20 resize-y"
                                />
                            )}
                        />
                    </fieldset>
                    <DialogFooter className="shrink-0 border-t px-6 py-4">
                        <Button
                            id="procurement-inquiry-quote-cancel"
                            type="button"
                            variant="outline"
                            onClick={() => onOpenChange(false)}
                        >
                            取消
                        </Button>
                        <Button
                            id="procurement-inquiry-quote-save"
                            type="submit"
                            disabled={
                                command.isPending ||
                                command.unresolved ||
                                !command.ready
                            }
                        >
                            {command.isPending ? "正在保存…" : "保存报价"}
                        </Button>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
