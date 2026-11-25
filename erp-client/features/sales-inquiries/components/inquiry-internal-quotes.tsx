"use client"

import * as React from "react"
import {
    ArrowLeftIcon,
    CheckIcon,
    ChevronDownIcon,
    ChevronLeftIcon,
    ChevronRightIcon,
    ChevronUpIcon,
    CircleAlertIcon,
    PlusIcon,
} from "lucide-react"
import { useStore } from "@tanstack/react-form"
import { toFieldErrors, useAppForm } from "@/components/form"
import {
    BusinessFailureState,
    DetailInlineTable,
    MoneyValue,
    OptionCombobox,
    QuantityValue,
    RateValue,
    detailWorkspaceStyles,
} from "@/components/business"
import { Alert, AlertDescription } from "@/components/ui/alert"
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
import {
    Field,
    FieldDescription,
    FieldError,
    FieldLabel,
} from "@/components/ui/field"
import { StatusBadge } from "@/components/ui/status-badge"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { toast } from "@/components/ui/toast"
import type { SupplierOfferingView } from "@/features/supplier-offerings/types"
import { useSupplierOfferingsForSkusQuery } from "@/features/supplier-offerings/hooks/queries"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    compactFixed,
    compareDecimal,
    multiplyFixed,
    parseDecimal,
    subtractFixed,
} from "@/lib/fixed-decimal"
import { hasPermission } from "@/lib/permissions"
import { cn } from "@/lib/utils"
import { inquiryPath } from "../api"
import { useInquirySkuDetailsQuery } from "../hooks/queries"
import { useInquiryCommand } from "../hooks/use-inquiry-command"
import { useInternalQuotes } from "../hooks/use-internal-quotes"
import { inquiryDateInput } from "../lib/format"
import {
    latestSupplierQuotes,
    quoteExclusion,
    quoteReferencePrice,
    type InternalQuote,
    type QuoteInput,
} from "../lib/internal-quotes"
import {
    inquiryStorageKey,
    readInquiryLocalValue,
    writeInquiryLocalValue,
} from "../lib/local-draft"
import type {
    InquiryFeedbackLine,
    InquiryLine,
    ProcurementInquiry,
} from "../types"
import type { InquiryTask } from "./inquiry-feedback-editor"
import { InquiryTaxRateInput } from "./inquiry-tax-rate-input"

const SYSTEM_REFERENCE_NOTE =
    "系统供给参考价；运费、交期及实际可供数量须由采购核对。"

type DialogViewState =
    | { mode: "compare" }
    | {
          mode: "edit"
          initial: QuoteInput
          title: string
          lockSupplier: boolean
      }

type QuoteRowModel = {
    quote: InternalQuote
    offering?: SupplierOfferingView
    supplierName: string
    excluded: string | null
    refPrice: string | null
    isComparable: boolean
    isLowest: boolean
    note: string
}

type DeltaMeta = {
    kind: "same" | "lower" | "higher"
    label: string
}

function todayBeijingDate(): string {
    return new Date(Date.now() + 8 * 60 * 60 * 1000).toISOString().slice(0, 10)
}

function isValidIsoDate(value: string): boolean {
    if (!/^\d{4}-\d{2}-\d{2}$/.test(value)) return false
    const parsed = new Date(`${value}T00:00:00Z`)
    return (
        Number.isFinite(parsed.getTime()) &&
        parsed.toISOString().slice(0, 10) === value
    )
}

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

function formatTaxPercent(rate: string): string {
    if (!rate) return ""
    try {
        return compactFixed(
            multiplyFixed(rate, "100", {
                leftMaxScale: 6,
                rightMaxScale: 0,
                outputScale: 4,
            }),
        )
    } catch {
        return rate
    }
}

function safeReferencePrice(
    quote: InternalQuote,
    quantity: string,
): string | null {
    if (quote.terms.unavailable) return null
    try {
        return quoteReferencePrice(quote, quantity)
    } catch {
        return null
    }
}

function computePriceDelta(
    refPrice: string,
    currentPrice: string | null | undefined,
): DeltaMeta | null {
    if (!currentPrice) return null
    try {
        const cmp = compareDecimal(refPrice, currentPrice, 6)
        if (cmp === 0) {
            return { kind: "same", label: "与当前填报一致" }
        }
        const diff = compactFixed(
            subtractFixed(
                cmp < 0 ? currentPrice : refPrice,
                cmp < 0 ? refPrice : currentPrice,
                { maxScale: 6, outputScale: 4 },
            ),
        )
        return cmp < 0
            ? { kind: "lower", label: `比当前低 ¥${diff}` }
            : { kind: "higher", label: `比当前高 ¥${diff}` }
    } catch {
        return null
    }
}

function buildComparisonRows(
    quotes: readonly InternalQuote[],
    line: InquiryLine,
    offerings: readonly SupplierOfferingView[],
    offeringsError: boolean,
    round: number,
): QuoteRowModel[] {
    const offeringById = new Map(offerings.map((item) => [item.id, item]))
    const rows = quotes.map((quote) => {
        const offering = offeringById.get(quote.supplier_offering_id)
        const excluded = offeringsError
            ? "供给资料读取失败"
            : !offering || offering.status !== "ACTIVE"
              ? "供给关系当前不可用"
              : quoteExclusion(quote, line, round)
        const refPrice = safeReferencePrice(quote, line.quantity)
        const isComparable =
            !excluded && !quote.terms.unavailable && refPrice != null
        const note =
            quote.system_price_kind &&
            quote.internal_note.trim() === SYSTEM_REFERENCE_NOTE
                ? ""
                : quote.internal_note.trim()
        return {
            quote,
            offering,
            supplierName:
                offering?.supplier_name ??
                quote.supplier_name ??
                "已登记供应商",
            excluded,
            refPrice,
            isComparable,
            isLowest: false,
            note,
        }
    })

    const comparablePrices = rows
        .filter((row) => row.isComparable && row.refPrice != null)
        .map((row) => row.refPrice!)
    const lowestPrice =
        comparablePrices.length >= 2
            ? comparablePrices.reduce((min, cur) =>
                  compareDecimal(cur, min, 4) < 0 ? cur : min,
              )
            : null

    if (lowestPrice != null) {
        for (const row of rows) {
            if (
                row.isComparable &&
                row.refPrice != null &&
                compareDecimal(row.refPrice, lowestPrice, 4) === 0
            ) {
                row.isLowest = true
            }
        }
    }

    rows.sort((left, right) => {
        const rank = (row: QuoteRowModel) => {
            if (!row.quote.system_price_kind && row.isComparable) return 0
            if (row.quote.system_price_kind && row.isComparable) return 1
            if (!row.quote.system_price_kind) return 2
            return 3
        }
        const rankDiff = rank(left) - rank(right)
        if (rankDiff !== 0) return rankDiff
        if (left.refPrice && right.refPrice) {
            const cmp = compareDecimal(left.refPrice, right.refPrice, 4)
            if (cmp !== 0) return cmp
        }
        return right.quote.created_at - left.quote.created_at
    })

    return rows
}

function buildQuoteSummaryHeader(
    line: InquiryLine,
    unit: string,
    feedbackLine?: InquiryFeedbackLine,
) {
    const dueDate = inquiryDateInput(line.delivery_due_at) || "未设置"
    const currentPrice = feedbackLine?.prices[0]?.unit_price_gross
    const currentTax = feedbackLine?.prices[0]?.tax_rate
    const taxLabel = currentTax ? formatTaxPercent(currentTax) : ""

    return (
        <div className="mt-2.5 flex flex-wrap items-center justify-between gap-x-4 gap-y-1.5 rounded-lg bg-surface-sunken/80 px-3.5 py-2 text-xs">
            <div className="flex flex-wrap items-center gap-x-2 gap-y-1 text-muted-foreground">
                <span>
                    需求{" "}
                    <QuantityValue
                        value={line.quantity}
                        unit={unit}
                        className="font-medium text-foreground"
                    />
                </span>
                <span aria-hidden="true" className="text-muted-foreground/40">
                    ·
                </span>
                <span>
                    要求交付{" "}
                    <span className="num font-medium text-foreground">
                        {dueDate}
                    </span>
                </span>
                <span aria-hidden="true" className="text-muted-foreground/40">
                    ·
                </span>
                <span>
                    {line.allocations.length > 0
                        ? `${line.allocations.length} 个配送地址`
                        : "按需求交付"}
                </span>
            </div>
            <div className="flex items-center gap-2">
                {feedbackLine?.conclusion === "unavailable" ? (
                    <StatusBadge tone="neutral" label="主表当前：不供应" />
                ) : currentPrice ? (
                    <span className="text-muted-foreground">
                        主表当前{" "}
                        <MoneyValue
                            value={currentPrice}
                            display="exact"
                            showHint={false}
                        />
                        {taxLabel ? (
                            <span className="num text-muted-foreground">
                                {" "}
                                · 税率 {taxLabel}%
                            </span>
                        ) : null}
                    </span>
                ) : (
                    <span className="text-muted-foreground">
                        主表当前：尚未填写供给价
                    </span>
                )}
            </div>
        </div>
    )
}

/** 供应商报价横向比价与单窗登记/修订工作台弹窗。 */
export function InquiryInternalQuotesDialog({
    open,
    onOpenChange,
    lineId,
    onLineIdChange,
    inquiry,
    task,
    userId,
    permissions,
    feedbackLines = [],
    onSaved,
    onAdopt,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    lineId: string | null
    onLineIdChange: (lineId: string) => void
    inquiry: ProcurementInquiry
    task: InquiryTask
    userId: string
    permissions: readonly string[]
    feedbackLines?: readonly InquiryFeedbackLine[]
    onSaved: () => void
    onAdopt?: (quote: InternalQuote) => boolean
}) {
    const canRead = hasPermission(
        permissions,
        "procurement_inquiry:internal_quote",
    )
    const [view, setView] = React.useState<DialogViewState>({ mode: "compare" })
    const [showHistory, setShowHistory] = React.useState(false)
    const [adoptError, setAdoptError] = React.useState<string | null>(null)
    const [editorPending, setEditorPending] = React.useState(false)
    const [draftCheckTick, setDraftCheckTick] = React.useState(0)
    const lastLineIdRef = React.useRef<string | null>(null)

    React.useEffect(() => {
        if (lineId) lastLineIdRef.current = lineId
        setView({ mode: "compare" })
        setShowHistory(false)
        setAdoptError(null)
    }, [lineId, open])

    const quotes = useInternalQuotes(inquiry.id, task.id, canRead && open)
    const skuIds = React.useMemo(
        () => inquiry.draft.lines.map((item) => item.sku_id),
        [inquiry.draft.lines],
    )
    const skuDetails = useInquirySkuDetailsQuery(canRead && open ? skuIds : [])
    const offerings = useSupplierOfferingsForSkusQuery(
        canRead && open ? skuIds : [],
    )

    if (!canRead) return null

    const lineIndex = inquiry.draft.lines.findIndex(
        (item) => item.line_id === lineId,
    )
    const line = lineIndex >= 0 ? inquiry.draft.lines[lineIndex] : undefined
    const feedbackLine = line
        ? feedbackLines.find((item) => item.line_id === line.line_id)
        : undefined
    const unit = line
        ? (skuDetails.units.get(line.unit_code) ??
          (skuDetails.isPending ? "" : "单位未匹配"))
        : ""
    const skuOfferings = line
        ? (offerings.data ?? []).filter(
              (item) => item.sku_id === line.sku_id && item.status === "ACTIVE",
          )
        : []

    const storageKey = line
        ? inquiryStorageKey(
              userId,
              `${inquiry.id}:${task.id}:internal-quote:${line.line_id}`,
          )
        : ""
    const legacyStorageKey = inquiryStorageKey(
        userId,
        `${inquiry.id}:${task.id}:internal-quote`,
    )
    const savedDraft = (() => {
        void draftCheckTick
        if (!open || !line) return null
        const lineSaved = readInquiryLocalValue<QuoteInput>(storageKey)
        if (lineSaved) return normalizedQuote(lineSaved)
        const legacy = readInquiryLocalValue<QuoteInput>(legacyStorageKey)
        return legacy && legacy.line_id === line.line_id
            ? normalizedQuote(legacy)
            : null
    })()

    const lineQuotes = line
        ? (quotes.data ?? []).filter((quote) => quote.line_id === line.line_id)
        : []
    const currentQuotes = latestSupplierQuotes(lineQuotes)
    const historicalQuotes = lineQuotes
        .filter((quote) => !currentQuotes.some((cur) => cur.id === quote.id))
        .sort(
            (a, b) =>
                b.feedback_round - a.feedback_round ||
                b.revision_no - a.revision_no ||
                b.created_at - a.created_at,
        )

    const comparisonRows = line
        ? buildComparisonRows(
              currentQuotes,
              line,
              offerings.data ?? [],
              offerings.isError,
              inquiry.feedback_round + 1,
          )
        : []
    const historicalRows = line
        ? buildComparisonRows(
              historicalQuotes,
              line,
              offerings.data ?? [],
              offerings.isError,
              inquiry.feedback_round + 1,
          )
        : []

    const lineUnavailableInFeedback = feedbackLine?.conclusion === "unavailable"
    const hasSystemReference = comparisonRows.some(
        (row) => row.quote.system_price_kind != null,
    )

    const openNewQuote = () => {
        if (!line) return
        const offering = skuOfferings.length === 1 ? skuOfferings[0] : undefined
        const fresh = blankQuote()
        setAdoptError(null)
        setView({
            mode: "edit",
            title: "登记供应商报价",
            lockSupplier: false,
            initial: {
                ...fresh,
                line_id: line.line_id,
                supplier_id: offering?.supplier_id ?? "",
                supplier_offering_id: offering?.id ?? "",
                terms: {
                    ...fresh.terms,
                    ...demandTerms(line),
                    input_tax_rate: offering?.input_tax_rate ?? "",
                },
            },
        })
    }

    const openDraftQuote = () => {
        if (!savedDraft) return
        setAdoptError(null)
        setView({
            mode: "edit",
            title:
                savedDraft.revision_no > 1
                    ? `继续填写报价 · 第 ${savedDraft.revision_no} 版`
                    : "继续填写报价草稿",
            lockSupplier: savedDraft.revision_no > 1,
            initial: savedDraft,
        })
    }

    const openReviseQuote = (quote: InternalQuote) => {
        if (!line) return
        const fromSystem = Boolean(quote.system_price_kind)
        setAdoptError(null)
        setView({
            mode: "edit",
            title: fromSystem
                ? "核价并登记正式报价"
                : `更新报价 · 第 ${quote.revision_no + 1} 版`,
            lockSupplier: !fromSystem,
            initial: {
                ...quote,
                quote_id: fromSystem ? crypto.randomUUID() : quote.quote_id,
                revision_no: fromSystem ? 1 : quote.revision_no + 1,
                internal_note: fromSystem ? "" : quote.internal_note,
                terms: {
                    ...quote.terms,
                    unavailable: false,
                    freight_gross: quote.terms.freight_gross ?? "",
                    available_quantity: quote.terms.unavailable
                        ? line.quantity
                        : quote.terms.available_quantity,
                    unit_cost_gross: quote.terms.unavailable
                        ? ""
                        : quote.terms.unit_cost_gross,
                    expected_delivery_date:
                        quote.terms.expected_delivery_date ||
                        inquiryDateInput(line.delivery_due_at),
                },
            },
        })
    }

    const handleAdoptRow = (row: QuoteRowModel) => {
        if (!onAdopt) return
        if (lineUnavailableInFeedback) {
            setAdoptError(
                "主表当前商品行已设为「不供应」，若需采用供应商报价请先在主表调整供给结论。",
            )
            return
        }
        const ok = onAdopt(row.quote)
        if (!ok) {
            setAdoptError("反馈当前不可编辑，请关闭弹窗检查页面提示。")
            return
        }
        toast.add({
            title: "已采用供应商报价",
            description: row.refPrice
                ? `已将「${row.supplierName}」含运费参考单价 ¥${row.refPrice} 与税率填入当前行，请核对后保存处理进度。`
                : `已带入「${row.supplierName}」报价，请核对后保存处理进度。`,
            type: "success",
        })
        onOpenChange(false)
    }

    const discardDraft = () => {
        if (!line) return
        localStorage.removeItem(storageKey)
        if (
            readInquiryLocalValue<QuoteInput>(legacyStorageKey)?.line_id ===
            line.line_id
        ) {
            localStorage.removeItem(legacyStorageKey)
        }
        setDraftCheckTick((tick) => tick + 1)
    }

    const specLine = line
        ? [
              `SKU ${line.sku_code}`,
              line.specification && line.specification !== "无规格"
                  ? line.specification
                  : null,
          ]
              .filter(Boolean)
              .join(" · ")
        : ""

    const prevLine =
        lineIndex > 0 ? inquiry.draft.lines[lineIndex - 1] : undefined
    const nextLine =
        lineIndex >= 0 && lineIndex < inquiry.draft.lines.length - 1
            ? inquiry.draft.lines[lineIndex + 1]
            : undefined

    return (
        <Dialog
            open={open && Boolean(line)}
            onOpenChange={(next) => {
                if (!next && editorPending) return
                onOpenChange(next)
            }}
        >
            <DialogContent
                id="procurement-inquiry-quotes-dialog"
                size="xl"
                scroll="body"
                closeButtonId="procurement-inquiry-quote-dialog-close"
                showCloseButton={!editorPending}
                finalFocus={() => {
                    const targetId = lastLineIdRef.current
                    if (!targetId) return true
                    return (
                        document.getElementById(
                            `procurement-inquiry-line-${toAutomationIdSegment(targetId)}-internal-quotes`,
                        ) ?? true
                    )
                }}
            >
                {line && (
                    <>
                        <DialogHeader className="gap-1">
                            <div className="flex flex-wrap items-center justify-between gap-2">
                                {view.mode === "edit" ? (
                                    <Button
                                        id="procurement-inquiry-quote-editor-back"
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        disabled={editorPending}
                                        className="-ml-2 h-6 gap-1 px-2 text-xs text-muted-foreground hover:text-foreground"
                                        onClick={() =>
                                            setView({ mode: "compare" })
                                        }
                                    >
                                        <ArrowLeftIcon
                                            aria-hidden="true"
                                            className="size-3.5"
                                        />
                                        返回比价列表
                                    </Button>
                                ) : (
                                    <p className="num text-xs text-muted-foreground">
                                        {specLine}
                                    </p>
                                )}

                                {view.mode === "compare" &&
                                inquiry.draft.lines.length > 1 ? (
                                    <div className="flex items-center gap-1">
                                        <Button
                                            id="procurement-inquiry-quote-prev-line"
                                            type="button"
                                            variant="ghost"
                                            size="xs"
                                            disabled={!prevLine}
                                            className="h-6 px-1.5 text-xs"
                                            onClick={() => {
                                                if (prevLine)
                                                    onLineIdChange(
                                                        prevLine.line_id,
                                                    )
                                            }}
                                        >
                                            <ChevronLeftIcon
                                                aria-hidden="true"
                                                className="size-3.5"
                                            />
                                            上一项
                                        </Button>
                                        <span className="num px-1 text-xs text-muted-foreground">
                                            {lineIndex + 1} /{" "}
                                            {inquiry.draft.lines.length}
                                        </span>
                                        <Button
                                            id="procurement-inquiry-quote-next-line"
                                            type="button"
                                            variant="ghost"
                                            size="xs"
                                            disabled={!nextLine}
                                            className="h-6 px-1.5 text-xs"
                                            onClick={() => {
                                                if (nextLine)
                                                    onLineIdChange(
                                                        nextLine.line_id,
                                                    )
                                            }}
                                        >
                                            下一项
                                            <ChevronRightIcon
                                                aria-hidden="true"
                                                className="size-3.5"
                                            />
                                        </Button>
                                    </div>
                                ) : view.mode === "edit" ? (
                                    <p className="num text-xs text-muted-foreground">
                                        {specLine}
                                    </p>
                                ) : null}
                            </div>

                            <DialogTitle className="text-page-title">
                                {view.mode === "edit"
                                    ? view.title
                                    : line.product_name}
                            </DialogTitle>
                            <DialogDescription>
                                {view.mode === "edit"
                                    ? `${line.product_name} · 仅当前责任采购可见，保存后生成新报价版本`
                                    : "供应商报价 · 仅采购可见，采用后回填含运费参考单价与税率（不代表已选定采购供应商）"}
                            </DialogDescription>

                            {buildQuoteSummaryHeader(line, unit, feedbackLine)}
                        </DialogHeader>

                        {view.mode === "edit" ? (
                            <QuoteEditorPanel
                                key={`${line.line_id}:${view.initial.quote_id}:${view.initial.revision_no}`}
                                initial={view.initial}
                                lockSupplier={view.lockSupplier}
                                inquiry={inquiry}
                                line={line}
                                feedbackLine={feedbackLine}
                                task={task}
                                userId={userId}
                                storageKey={storageKey}
                                offerings={skuOfferings}
                                offeringError={offerings.error}
                                unit={unit}
                                onPendingChange={setEditorPending}
                                onBack={() => setView({ mode: "compare" })}
                                onAdopt={onAdopt}
                                onSaved={({ adopted }) => {
                                    if (
                                        readInquiryLocalValue<QuoteInput>(
                                            legacyStorageKey,
                                        )?.line_id === line.line_id
                                    ) {
                                        localStorage.removeItem(
                                            legacyStorageKey,
                                        )
                                    }
                                    void quotes.refetch()
                                    onSaved()
                                    if (adopted) {
                                        onOpenChange(false)
                                    } else {
                                        setView({ mode: "compare" })
                                    }
                                }}
                            />
                        ) : (
                            <>
                                <DialogBody className="space-y-4">
                                    <div className="flex flex-wrap items-center justify-between gap-2">
                                        <div className="flex flex-wrap items-center gap-2">
                                            <h3 className="text-section-title font-semibold tracking-tight">
                                                候选报价
                                            </h3>
                                            <span className="num text-xs text-muted-foreground">
                                                {comparisonRows.length} 条
                                            </span>
                                        </div>
                                        <Button
                                            id="procurement-inquiry-add-internal-quote"
                                            type="button"
                                            size="sm"
                                            variant={
                                                comparisonRows.length === 0
                                                    ? "default"
                                                    : "outline"
                                            }
                                            disabled={
                                                offerings.isPending ||
                                                skuOfferings.length === 0
                                            }
                                            title={
                                                skuOfferings.length === 0 &&
                                                !offerings.isPending
                                                    ? "该商品暂无启用的供应商供给关系，请先维护供给资料"
                                                    : undefined
                                            }
                                            onClick={openNewQuote}
                                        >
                                            <PlusIcon
                                                aria-hidden="true"
                                                data-icon="inline-start"
                                            />
                                            登记供应商报价
                                        </Button>
                                    </div>

                                    {savedDraft && (
                                        <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-primary/25 bg-sidebar-accent/45 px-3.5 py-2 text-xs">
                                            <span className="text-sidebar-accent-foreground">
                                                检测到该商品有一份未提交的报价草稿，可随时继续填写。
                                            </span>
                                            <div className="flex items-center gap-1.5">
                                                <Button
                                                    id="procurement-inquiry-quote-resume-draft"
                                                    type="button"
                                                    size="xs"
                                                    variant="outline"
                                                    onClick={openDraftQuote}
                                                >
                                                    继续填写草稿
                                                </Button>
                                                <Button
                                                    id="procurement-inquiry-quote-discard-draft"
                                                    type="button"
                                                    size="xs"
                                                    variant="ghost"
                                                    onClick={discardDraft}
                                                >
                                                    丢弃
                                                </Button>
                                            </div>
                                        </div>
                                    )}

                                    {adoptError && (
                                        <Alert variant="warning">
                                            <AlertDescription>
                                                {adoptError}
                                            </AlertDescription>
                                        </Alert>
                                    )}

                                    {offerings.isError && (
                                        <BusinessFailureState
                                            id="procurement-inquiry-offerings-retry"
                                            title="供应商供给资料读取失败"
                                            error={offerings.error}
                                            onRetry={() =>
                                                void offerings.refetch()
                                            }
                                        />
                                    )}

                                    {quotes.isError ? (
                                        <BusinessFailureState
                                            id="procurement-inquiry-quotes-retry"
                                            title="供应商报价加载失败"
                                            error={quotes.error}
                                            onRetry={() =>
                                                void quotes.refetch()
                                            }
                                        />
                                    ) : quotes.isPending ? (
                                        <p
                                            role="status"
                                            className="py-10 text-center text-xs text-muted-foreground"
                                        >
                                            正在读取供应商报价…
                                        </p>
                                    ) : comparisonRows.length === 0 ? (
                                        <div className="rounded-lg border border-dashed border-border/80 bg-surface-sunken/40 px-4 py-10 text-center">
                                            <p className="text-body-compact text-muted-foreground">
                                                {skuOfferings.length === 0 &&
                                                !offerings.isPending
                                                    ? "该商品暂无启用的供应商供给关系，请先维护供给资料后再登记报价。"
                                                    : "暂无供应商报价或系统供给参考，点击右上角「登记供应商报价」录入。"}
                                            </p>
                                        </div>
                                    ) : (
                                        <QuoteComparisonTable
                                            rows={comparisonRows}
                                            unit={unit}
                                            currentRound={
                                                inquiry.feedback_round + 1
                                            }
                                            dueDate={inquiryDateInput(
                                                line.delivery_due_at,
                                            )}
                                            demandQuantity={line.quantity}
                                            currentPrice={
                                                feedbackLine?.prices[0]
                                                    ?.unit_price_gross
                                            }
                                            currentTax={
                                                feedbackLine?.prices[0]
                                                    ?.tax_rate
                                            }
                                            lineUnavailable={
                                                lineUnavailableInFeedback
                                            }
                                            fetching={quotes.isFetching}
                                            onAdopt={
                                                onAdopt
                                                    ? handleAdoptRow
                                                    : undefined
                                            }
                                            onRevise={openReviseQuote}
                                        />
                                    )}

                                    {hasSystemReference && (
                                        <p className="text-xs text-muted-foreground">
                                            注：系统供给参考价尚未核定运费与交期，可直接采用作参考基准，或点击右侧「核价登记」补全条件并保存为正式报价。
                                        </p>
                                    )}

                                    {historicalRows.length > 0 && (
                                        <div className="space-y-2.5 border-t border-border/60 pt-3">
                                            <Button
                                                id="procurement-inquiry-quote-history"
                                                type="button"
                                                size="sm"
                                                variant="ghost"
                                                className={
                                                    detailWorkspaceStyles.quietButton
                                                }
                                                onClick={() =>
                                                    setShowHistory(
                                                        (value) => !value,
                                                    )
                                                }
                                            >
                                                {showHistory ? (
                                                    <ChevronUpIcon
                                                        aria-hidden="true"
                                                        data-icon="inline-start"
                                                    />
                                                ) : (
                                                    <ChevronDownIcon
                                                        aria-hidden="true"
                                                        data-icon="inline-start"
                                                    />
                                                )}
                                                {showHistory
                                                    ? "收起历史报价"
                                                    : `查看历史报价（${historicalRows.length}）`}
                                            </Button>
                                            {showHistory && (
                                                <QuoteComparisonTable
                                                    rows={historicalRows}
                                                    unit={unit}
                                                    currentRound={
                                                        inquiry.feedback_round +
                                                        1
                                                    }
                                                    dueDate={inquiryDateInput(
                                                        line.delivery_due_at,
                                                    )}
                                                    demandQuantity={
                                                        line.quantity
                                                    }
                                                    lineUnavailable={
                                                        lineUnavailableInFeedback
                                                    }
                                                    fetching={quotes.isFetching}
                                                    historical
                                                />
                                            )}
                                        </div>
                                    )}
                                </DialogBody>

                                <DialogFooter className="items-center">
                                    <p className="mr-auto text-xs text-muted-foreground">
                                        {lineUnavailableInFeedback
                                            ? "主表当前行已设为「不供应」，如需采用报价请先调整供给结论。"
                                            : "仅展示满足需求条件且已确认运费的可比价格「最低」标记。"}
                                    </p>
                                    <Button
                                        id="procurement-inquiry-quote-dialog-dismiss"
                                        type="button"
                                        variant="outline"
                                        onClick={() => onOpenChange(false)}
                                    >
                                        关闭
                                    </Button>
                                </DialogFooter>
                            </>
                        )}
                    </>
                )}
            </DialogContent>
        </Dialog>
    )
}

function QuoteComparisonTable({
    rows,
    unit,
    currentRound,
    dueDate,
    demandQuantity,
    currentPrice,
    currentTax,
    lineUnavailable,
    fetching,
    historical = false,
    onAdopt,
    onRevise,
}: {
    rows: readonly QuoteRowModel[]
    unit: string
    currentRound: number
    dueDate: string
    demandQuantity: string
    currentPrice?: string | null
    currentTax?: string | null
    lineUnavailable: boolean
    fetching: boolean
    historical?: boolean
    onAdopt?: (row: QuoteRowModel) => void
    onRevise?: (quote: InternalQuote) => void
}) {
    const colCount = historical ? 6 : 7
    return (
        <DetailInlineTable
            frame="none"
            className="min-w-0 overflow-x-auto rounded-lg border border-border/75"
        >
            <Table className="min-w-[780px] table-fixed [--table-cell-inline:0.75rem]">
                <colgroup>
                    <col className="w-44" />
                    <col className="w-24" />
                    <col className="w-24" />
                    <col className="w-36" />
                    <col className="w-16" />
                    <col className="w-28" />
                    {!historical && <col className="w-32" />}
                </colgroup>
                <TableHeader>
                    <TableRow>
                        <TableHead>供应商与来源</TableHead>
                        <TableHead data-align="end" className="text-right">
                            商品单价
                        </TableHead>
                        <TableHead data-align="end" className="text-right">
                            本行运费
                        </TableHead>
                        <TableHead data-align="end" className="text-right">
                            含运费参考单价
                        </TableHead>
                        <TableHead data-align="end" className="text-right">
                            税率
                        </TableHead>
                        <TableHead>可供与交期</TableHead>
                        {!historical && (
                            <TableHead data-align="end" className="text-right">
                                操作
                            </TableHead>
                        )}
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {rows.map((row) => {
                        const { quote } = row
                        const prefix = `procurement-inquiry-quote-${toAutomationIdSegment(quote.id)}`
                        const isSystem = Boolean(quote.system_price_kind)
                        const isUnavailable = Boolean(quote.terms.unavailable)
                        const isAlreadyAdopted =
                            !historical &&
                            !row.excluded &&
                            row.refPrice != null &&
                            currentPrice != null &&
                            compareDecimal(row.refPrice, currentPrice, 4) ===
                                0 &&
                            (!currentTax ||
                                compareDecimal(
                                    quote.terms.input_tax_rate,
                                    currentTax,
                                    6,
                                ) === 0)
                        const delta =
                            !historical && !isSystem && row.refPrice
                                ? computePriceDelta(row.refPrice, currentPrice)
                                : null
                        const qtyShort =
                            !isUnavailable &&
                            compareDecimal(
                                quote.terms.available_quantity,
                                demandQuantity,
                                6,
                            ) < 0
                        const dateLate =
                            !isUnavailable &&
                            Boolean(dueDate) &&
                            Boolean(quote.terms.expected_delivery_date) &&
                            quote.terms.expected_delivery_date > dueDate
                        const hasSubRow = Boolean(row.excluded || row.note)

                        const sourceLabel = isSystem
                            ? quote.system_price_kind === "dropship"
                                ? "系统供给 · 一件代发价"
                                : "系统供给 · 集采价"
                            : `第 ${quote.revision_no} 版 · 正式报价${quote.feedback_round !== currentRound ? `（第 ${quote.feedback_round} 轮）` : ""}`

                        return (
                            <React.Fragment key={quote.id}>
                                <TableRow
                                    className={cn(
                                        "[&>td]:py-2.5 [&>td]:align-top",
                                        hasSubRow && "border-b-0!",
                                        (historical || isUnavailable) &&
                                            "bg-surface-sunken/35",
                                    )}
                                >
                                    <TableCell className="whitespace-normal">
                                        <p className="font-medium text-foreground wrap-anywhere">
                                            {row.supplierName}
                                        </p>
                                        <p className="mt-0.5 text-xs text-muted-foreground">
                                            {sourceLabel}
                                        </p>
                                    </TableCell>

                                    <TableCell
                                        data-align="end"
                                        className="num text-right"
                                    >
                                        {isUnavailable ? (
                                            <span className="text-muted-foreground">
                                                —
                                            </span>
                                        ) : (
                                            <MoneyValue
                                                display="exact"
                                                showHint={false}
                                                value={
                                                    quote.terms.unit_cost_gross
                                                }
                                            />
                                        )}
                                    </TableCell>

                                    <TableCell
                                        data-align="end"
                                        className="num text-right"
                                    >
                                        {isUnavailable ? (
                                            <span className="text-muted-foreground">
                                                —
                                            </span>
                                        ) : quote.terms.freight_gross ==
                                          null ? (
                                            <span className="text-xs text-warning-soft-foreground">
                                                未确认
                                            </span>
                                        ) : compareDecimal(
                                              quote.terms.freight_gross,
                                              "0",
                                              2,
                                          ) === 0 ? (
                                            <span className="text-xs text-muted-foreground">
                                                ¥0（已含）
                                            </span>
                                        ) : (
                                            <MoneyValue
                                                display="exact"
                                                showHint={false}
                                                value={
                                                    quote.terms.freight_gross
                                                }
                                            />
                                        )}
                                    </TableCell>

                                    <TableCell
                                        data-align="end"
                                        className="num text-right whitespace-normal"
                                    >
                                        {isUnavailable ? (
                                            <div className="flex justify-end">
                                                <StatusBadge
                                                    tone="neutral"
                                                    label="无法供货"
                                                />
                                            </div>
                                        ) : row.refPrice != null ? (
                                            <div className="space-y-0.5">
                                                <div className="flex flex-wrap items-center justify-end gap-1.5">
                                                    {row.isLowest && (
                                                        <span className="rounded-sm bg-success-soft px-1.5 py-0.5 text-tiny font-medium text-success-soft-foreground">
                                                            最低
                                                        </span>
                                                    )}
                                                    <span
                                                        className={
                                                            row.isLowest
                                                                ? "text-success-soft-foreground"
                                                                : undefined
                                                        }
                                                    >
                                                        <MoneyValue
                                                            display="exact"
                                                            showHint={false}
                                                            inheritColor={
                                                                row.isLowest
                                                            }
                                                            value={row.refPrice}
                                                        />
                                                    </span>
                                                </div>
                                                {isSystem ? (
                                                    <p className="text-xs text-muted-foreground">
                                                        运费待核定
                                                    </p>
                                                ) : delta ? (
                                                    <p
                                                        className={cn(
                                                            "text-xs",
                                                            delta.kind ===
                                                                "same" ||
                                                                delta.kind ===
                                                                    "lower"
                                                                ? "text-success-soft-foreground"
                                                                : "text-muted-foreground",
                                                        )}
                                                    >
                                                        {delta.label}
                                                    </p>
                                                ) : null}
                                            </div>
                                        ) : (
                                            <span className="text-xs text-muted-foreground">
                                                待核定运费
                                            </span>
                                        )}
                                    </TableCell>

                                    <TableCell
                                        data-align="end"
                                        className="num text-right"
                                    >
                                        {isUnavailable ? (
                                            <span className="text-muted-foreground">
                                                —
                                            </span>
                                        ) : (
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
                                        )}
                                    </TableCell>

                                    <TableCell className="whitespace-normal">
                                        {isUnavailable ? (
                                            <span className="text-xs text-muted-foreground">
                                                不供应
                                            </span>
                                        ) : (
                                            <div className="space-y-0.5">
                                                <div
                                                    className={cn(
                                                        "text-body-compact",
                                                        qtyShort &&
                                                            "font-medium text-warning-soft-foreground",
                                                    )}
                                                >
                                                    <QuantityValue
                                                        value={
                                                            quote.terms
                                                                .available_quantity
                                                        }
                                                        unit={unit}
                                                    />
                                                </div>
                                                <p
                                                    className={cn(
                                                        "num text-xs",
                                                        dateLate
                                                            ? "text-warning-soft-foreground"
                                                            : "text-muted-foreground",
                                                    )}
                                                >
                                                    {isSystem
                                                        ? "交期：待核对"
                                                        : quote.terms
                                                              .expected_delivery_date ||
                                                          "交期未确认"}
                                                </p>
                                            </div>
                                        )}
                                    </TableCell>

                                    {!historical && (
                                        <TableCell
                                            data-align="end"
                                            className="text-right"
                                        >
                                            <div className="flex items-center justify-end gap-1.5">
                                                {onAdopt && !isUnavailable && (
                                                    <Button
                                                        id={`${prefix}-adopt`}
                                                        type="button"
                                                        size="xs"
                                                        variant={
                                                            isAlreadyAdopted
                                                                ? "secondary"
                                                                : "outline"
                                                        }
                                                        disabled={
                                                            Boolean(
                                                                row.excluded,
                                                            ) ||
                                                            fetching ||
                                                            lineUnavailable
                                                        }
                                                        title={
                                                            lineUnavailable
                                                                ? "主表当前行已设为不供应"
                                                                : (row.excluded ??
                                                                  undefined)
                                                        }
                                                        onClick={() =>
                                                            onAdopt(row)
                                                        }
                                                    >
                                                        {isAlreadyAdopted ? (
                                                            <>
                                                                <CheckIcon
                                                                    aria-hidden="true"
                                                                    className="size-3"
                                                                />
                                                                已填入
                                                            </>
                                                        ) : (
                                                            "采用"
                                                        )}
                                                    </Button>
                                                )}
                                                {onRevise && (
                                                    <Button
                                                        id={`${prefix}-revise`}
                                                        type="button"
                                                        size="xs"
                                                        variant="ghost"
                                                        className={
                                                            detailWorkspaceStyles.inlineLinkButton
                                                        }
                                                        onClick={() =>
                                                            onRevise(quote)
                                                        }
                                                    >
                                                        {isSystem
                                                            ? "核价登记"
                                                            : "更新"}
                                                    </Button>
                                                )}
                                            </div>
                                        </TableCell>
                                    )}
                                </TableRow>
                                {hasSubRow && (
                                    <TableRow className="hover:bg-transparent">
                                        <TableCell
                                            colSpan={colCount}
                                            className={cn(
                                                "whitespace-normal py-1.5",
                                                row.excluded
                                                    ? "bg-warning-soft/35 text-warning-soft-foreground"
                                                    : "bg-surface-sunken/60 text-muted-foreground",
                                            )}
                                        >
                                            <div className="flex flex-wrap items-start gap-x-3 gap-y-1 text-xs">
                                                {row.excluded && (
                                                    <span className="inline-flex items-center gap-1 font-medium">
                                                        <CircleAlertIcon
                                                            aria-hidden="true"
                                                            className="size-3.5 shrink-0"
                                                        />
                                                        {row.excluded}
                                                    </span>
                                                )}
                                                {row.note && (
                                                    <span className="wrap-anywhere text-muted-foreground">
                                                        备注：{row.note}
                                                    </span>
                                                )}
                                            </div>
                                        </TableCell>
                                    </TableRow>
                                )}
                            </React.Fragment>
                        )
                    })}
                </TableBody>
            </Table>
        </DetailInlineTable>
    )
}

function validateEditorInput(
    value: QuoteInput,
    line: InquiryLine,
): string | null {
    if (!value.supplier_offering_id || !value.supplier_id) {
        return "请选择当前可用的供应商。"
    }
    if (value.terms.unavailable) {
        return null
    }
    try {
        parseDecimal(value.terms.unit_cost_gross, { maxScale: 4 })
        if (compareDecimal(value.terms.unit_cost_gross, "0", 4) <= 0) {
            return "请填写大于 0 的含税商品单价（最多 4 位小数）。"
        }
    } catch {
        return "含税商品单价格式有误，请填写大于 0 的数字（最多 4 位小数）。"
    }
    if (value.terms.freight_gross.trim() !== "") {
        try {
            parseDecimal(value.terms.freight_gross, { maxScale: 2 })
            if (compareDecimal(value.terms.freight_gross, "0", 2) < 0) {
                return "本行总运费不能为负数。"
            }
        } catch {
            return "本行总运费格式有误，请填写非负数（最多 2 位小数），或留空表示待确认。"
        }
    }
    if (!value.terms.input_tax_rate) {
        return "请填写进项税率。"
    }
    try {
        parseDecimal(value.terms.input_tax_rate, { maxScale: 6 })
        if (
            compareDecimal(value.terms.input_tax_rate, "0", 6) < 0 ||
            compareDecimal(value.terms.input_tax_rate, "1", 6) > 0
        ) {
            return "进项税率须在 0% 至 100% 之间。"
        }
    } catch {
        return "进项税率格式有误，请填写 0 至 100 之间的数字。"
    }
    try {
        parseDecimal(value.terms.available_quantity, {
            maxScale: line.quantity_scale,
        })
        if (
            compareDecimal(
                value.terms.available_quantity,
                "0",
                line.quantity_scale,
            ) <= 0
        ) {
            return "请填写大于 0 的可供数量。"
        }
    } catch {
        return `可供数量格式有误，最多支持 ${line.quantity_scale} 位小数。`
    }
    if (!isValidIsoDate(value.terms.expected_delivery_date)) {
        return "请选择有效的承诺交付日期。"
    }
    return null
}

function QuoteEditorPanel({
    initial,
    lockSupplier,
    inquiry,
    line,
    feedbackLine,
    task,
    userId,
    storageKey,
    offerings,
    offeringError,
    unit,
    onPendingChange,
    onBack,
    onAdopt,
    onSaved,
}: {
    initial: QuoteInput
    lockSupplier: boolean
    inquiry: ProcurementInquiry
    line: InquiryLine
    feedbackLine?: InquiryFeedbackLine
    task: InquiryTask
    userId: string
    storageKey: string
    offerings: readonly SupplierOfferingView[]
    offeringError: unknown
    unit: string
    onPendingChange: (pending: boolean) => void
    onBack: () => void
    onAdopt?: (quote: InternalQuote) => boolean
    onSaved: (result: { adopted: boolean }) => void
}) {
    const command = useInquiryCommand(userId, inquiry.id)
    const [error, setError] = React.useState<string | null>(null)
    const submitIntent = React.useRef<"save" | "saveAndAdopt">("save")

    React.useEffect(() => {
        onPendingChange(command.isPending)
        return () => onPendingChange(false)
    }, [command.isPending, onPendingChange])

    const form = useAppForm({
        defaultValues: normalizedQuote(initial),
        listeners: {
            onChange: ({ formApi }) => {
                try {
                    writeInquiryLocalValue(storageKey, formApi.state.values)
                } catch {
                    setError("浏览器无法保留输入，请先保存报价后离开。")
                }
            },
        },
        onSubmit: async ({ value }) => {
            setError(null)
            const validationError = validateEditorInput(value, line)
            if (validationError) {
                setError(validationError)
                return
            }

            const requiredDate =
                inquiryDateInput(line.delivery_due_at) || todayBeijingDate()
            const normalized = normalizedQuote(value)
            const terms = normalized.terms.unavailable
                ? {
                      unavailable: true,
                      freight_gross: null,
                      unit_cost_gross: "0",
                      input_tax_rate: normalized.terms.input_tax_rate || "0",
                      available_quantity: "0",
                      expected_delivery_date:
                          normalized.terms.expected_delivery_date &&
                          isValidIsoDate(
                              normalized.terms.expected_delivery_date,
                          )
                              ? normalized.terms.expected_delivery_date
                              : requiredDate,
                  }
                : {
                      unavailable: false,
                      freight_gross: normalized.terms.freight_gross.trim()
                          ? compactFixed(normalized.terms.freight_gross.trim())
                          : null,
                      unit_cost_gross: compactFixed(
                          normalized.terms.unit_cost_gross.trim(),
                      ),
                      input_tax_rate: compactFixed(
                          normalized.terms.input_tax_rate.trim(),
                      ),
                      available_quantity: compactFixed(
                          normalized.terms.available_quantity.trim(),
                      ),
                      expected_delivery_date:
                          normalized.terms.expected_delivery_date,
                  }

            const payloadQuote = {
                quote_id: normalized.quote_id,
                revision_no: normalized.revision_no,
                line_id: line.line_id,
                supplier_id: normalized.supplier_id,
                supplier_offering_id: normalized.supplier_offering_id,
                terms,
                internal_note: normalized.internal_note,
            }

            try {
                await command.run(
                    `${inquiryPath(true, inquiry.id)}/internal-quotes`,
                    {
                        expected_version: inquiry.version,
                        task_id: task.id,
                        task_version: task.version,
                        quote: payloadQuote,
                    },
                )
                localStorage.removeItem(storageKey)

                const supplierName =
                    offerings.find(
                        (item) => item.id === payloadQuote.supplier_offering_id,
                    )?.supplier_name ?? "供应商"

                if (
                    submitIntent.current === "saveAndAdopt" &&
                    onAdopt &&
                    !terms.unavailable
                ) {
                    const savedQuote: InternalQuote = {
                        id: payloadQuote.quote_id,
                        quote_id: payloadQuote.quote_id,
                        revision_no: payloadQuote.revision_no,
                        requirement_version: inquiry.requirement_version,
                        feedback_round: inquiry.feedback_round + 1,
                        created_at: Date.now(),
                        line_id: payloadQuote.line_id,
                        supplier_id: payloadQuote.supplier_id,
                        supplier_offering_id: payloadQuote.supplier_offering_id,
                        supplier_name: supplierName,
                        terms,
                        internal_note: payloadQuote.internal_note,
                    }
                    const adopted = onAdopt(savedQuote)
                    if (adopted) {
                        const refPrice = safeReferencePrice(
                            savedQuote,
                            line.quantity,
                        )
                        toast.add({
                            title: "已保存并采用报价",
                            description: refPrice
                                ? `已保存「${supplierName}」报价并将含运费参考单价 ¥${refPrice} 填入主表。`
                                : `已保存并带入「${supplierName}」报价。`,
                            type: "success",
                        })
                        onSaved({ adopted: true })
                        return
                    }
                }

                toast.add({
                    title: "已保存供应商报价",
                    description: `已记录「${supplierName}」第 ${payloadQuote.revision_no} 版报价。`,
                    type: "success",
                })
                onSaved({ adopted: false })
            } catch (failure) {
                setError(
                    getErrorMessage(
                        failure,
                        "保存供应商报价失败，请保留输入并重试。",
                    ),
                )
            }
        },
    })

    const values = useStore(form.store, (state) => state.values)
    const unavailable = Boolean(values.terms.unavailable)

    const livePreview = React.useMemo(() => {
        if (unavailable) return null
        const previewQuote: InternalQuote = {
            id: values.quote_id,
            quote_id: values.quote_id,
            revision_no: values.revision_no,
            requirement_version: inquiry.requirement_version,
            feedback_round: inquiry.feedback_round + 1,
            created_at: 0,
            line_id: line.line_id,
            supplier_id: values.supplier_id,
            supplier_offering_id: values.supplier_offering_id,
            terms: {
                unavailable: false,
                unit_cost_gross: values.terms.unit_cost_gross.trim(),
                input_tax_rate: values.terms.input_tax_rate.trim(),
                available_quantity: values.terms.available_quantity.trim(),
                expected_delivery_date: values.terms.expected_delivery_date,
                freight_gross: values.terms.freight_gross.trim() || null,
            },
            internal_note: values.internal_note,
        }
        const refPrice =
            previewQuote.terms.freight_gross != null &&
            previewQuote.terms.unit_cost_gross
                ? safeReferencePrice(previewQuote, line.quantity)
                : null
        const exclusion = quoteExclusion(
            previewQuote,
            line,
            inquiry.feedback_round + 1,
        )
        return { refPrice, exclusion }
    }, [
        inquiry.feedback_round,
        inquiry.requirement_version,
        line,
        unavailable,
        values,
    ])

    const lineUnavailableInFeedback = feedbackLine?.conclusion === "unavailable"
    const canSaveAndAdopt =
        Boolean(onAdopt) &&
        !unavailable &&
        !lineUnavailableInFeedback &&
        livePreview?.exclusion == null

    return (
        <form
            className="flex min-h-0 flex-1 flex-col overflow-hidden"
            onSubmit={(event) => {
                event.preventDefault()
                void form.handleSubmit()
            }}
        >
            <DialogBody>
                <fieldset
                    disabled={command.isPending}
                    className="min-w-0 space-y-5"
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
                                    "供应商读取失败，请返回重试。",
                                )}
                            </AlertDescription>
                        </Alert>
                    )}

                    <div className="grid gap-4 sm:grid-cols-2">
                        <form.Field
                            name="supplier_offering_id"
                            children={(field) => (
                                <Field className="gap-2">
                                    <FieldLabel htmlFor="procurement-inquiry-quote-offering">
                                        供应商
                                        <span className="text-destructive">
                                            *
                                        </span>
                                    </FieldLabel>
                                    <OptionCombobox
                                        id="procurement-inquiry-quote-offering"
                                        aria-label="供应商"
                                        allowClear={false}
                                        value={field.state.value}
                                        disabled={lockSupplier}
                                        options={offerings.map((item) => ({
                                            value: item.id,
                                            label:
                                                item.supplier_name ??
                                                "供应商名称未匹配",
                                        }))}
                                        onValueChange={(id) => {
                                            field.handleChange(id ?? "")
                                            const offering = offerings.find(
                                                (item) => item.id === id,
                                            )
                                            form.setFieldValue(
                                                "supplier_id",
                                                offering?.supplier_id ?? "",
                                            )
                                            if (offering?.input_tax_rate) {
                                                form.setFieldValue(
                                                    "terms.input_tax_rate",
                                                    offering.input_tax_rate,
                                                )
                                            }
                                        }}
                                    />
                                </Field>
                            )}
                        />

                        <Field className="gap-2">
                            <FieldLabel>供货状态</FieldLabel>
                            <div
                                role="radiogroup"
                                aria-label="供货状态"
                                className={cn(
                                    detailWorkspaceStyles.subTabs,
                                    "h-control w-full p-1",
                                )}
                            >
                                <button
                                    id="procurement-inquiry-quote-availability-available"
                                    type="button"
                                    role="radio"
                                    aria-checked={!unavailable}
                                    data-active={
                                        !unavailable ? "true" : undefined
                                    }
                                    className={cn(
                                        detailWorkspaceStyles.subTab,
                                        "flex-1",
                                    )}
                                    onClick={() => {
                                        if (!unavailable) return
                                        form.setFieldValue(
                                            "terms.unavailable",
                                            false,
                                        )
                                        if (
                                            !values.terms.available_quantity ||
                                            values.terms.available_quantity ===
                                                "0"
                                        ) {
                                            form.setFieldValue(
                                                "terms.available_quantity",
                                                line.quantity,
                                            )
                                        }
                                        if (
                                            values.terms.unit_cost_gross === "0"
                                        ) {
                                            form.setFieldValue(
                                                "terms.unit_cost_gross",
                                                "",
                                            )
                                        }
                                        if (
                                            !values.terms.expected_delivery_date
                                        ) {
                                            form.setFieldValue(
                                                "terms.expected_delivery_date",
                                                inquiryDateInput(
                                                    line.delivery_due_at,
                                                ),
                                            )
                                        }
                                        if (!values.terms.input_tax_rate) {
                                            const offering = offerings.find(
                                                (item) =>
                                                    item.id ===
                                                    values.supplier_offering_id,
                                            )
                                            if (offering?.input_tax_rate) {
                                                form.setFieldValue(
                                                    "terms.input_tax_rate",
                                                    offering.input_tax_rate,
                                                )
                                            }
                                        }
                                    }}
                                >
                                    正常供货
                                </button>
                                <button
                                    id="procurement-inquiry-quote-availability-unavailable"
                                    type="button"
                                    role="radio"
                                    aria-checked={unavailable}
                                    data-active={
                                        unavailable ? "true" : undefined
                                    }
                                    className={cn(
                                        detailWorkspaceStyles.subTab,
                                        "flex-1",
                                    )}
                                    onClick={() => {
                                        if (unavailable) return
                                        form.setFieldValue(
                                            "terms.unavailable",
                                            true,
                                        )
                                    }}
                                >
                                    无法供货
                                </button>
                            </div>
                        </Field>
                    </div>

                    {unavailable ? (
                        <Alert>
                            <AlertDescription>
                                登记为「无法供货」后将保留该供应商回复记录，但不可用于报价采用。建议在下方备注中注明原因（如缺货、停产、起订量不符等）。
                            </AlertDescription>
                        </Alert>
                    ) : (
                        <>
                            <div className="grid gap-4 sm:grid-cols-2">
                                <form.AppField
                                    name="terms.unit_cost_gross"
                                    children={(field) => (
                                        <field.TextField
                                            id="procurement-inquiry-quote-cost"
                                            label={
                                                unit
                                                    ? `含税商品单价（元 / ${unit}）`
                                                    : "含税商品单价（元）"
                                            }
                                            placeholder="填写不含分摊运费的含税单价"
                                            inputMode="decimal"
                                            inputClassName="num"
                                            required
                                        />
                                    )}
                                />

                                <div className="space-y-1.5">
                                    <div className="flex items-center justify-between gap-2">
                                        <FieldLabel htmlFor="procurement-inquiry-quote-freight">
                                            本行总运费（元）
                                        </FieldLabel>
                                        <button
                                            id="procurement-inquiry-quote-freight-free"
                                            type="button"
                                            className="text-xs text-primary underline-offset-4 hover:underline"
                                            onClick={() =>
                                                form.setFieldValue(
                                                    "terms.freight_gross",
                                                    "0",
                                                )
                                            }
                                        >
                                            已含/免运费填 0
                                        </button>
                                    </div>
                                    <form.AppField
                                        name="terms.freight_gross"
                                        children={(field) => (
                                            <field.TextField
                                                id="procurement-inquiry-quote-freight"
                                                label="本行总运费（元）"
                                                hideLabel
                                                placeholder="已含运费填 0，未确认可留空"
                                                inputMode="decimal"
                                                inputClassName="num"
                                            />
                                        )}
                                    />
                                </div>
                            </div>

                            <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-border/70 bg-surface-sunken/70 px-3.5 py-2.5 text-xs">
                                <div className="flex flex-wrap items-baseline gap-2">
                                    <span className="text-muted-foreground">
                                        含运费参考单价
                                    </span>
                                    {livePreview?.refPrice ? (
                                        <>
                                            <span className="text-body-compact">
                                                <MoneyValue
                                                    value={livePreview.refPrice}
                                                    display="exact"
                                                    showHint={false}
                                                />
                                            </span>
                                            <span className="text-muted-foreground">
                                                （按需求 {line.quantity} {unit}{" "}
                                                分摊本行运费）
                                            </span>
                                        </>
                                    ) : (
                                        <span className="text-muted-foreground">
                                            {values.terms.unit_cost_gross.trim() &&
                                            !values.terms.freight_gross.trim()
                                                ? "运费留空表示「未确认」，保存后暂不可直接采用"
                                                : "填写单价与运费后自动试算"}
                                        </span>
                                    )}
                                </div>
                                {livePreview?.exclusion &&
                                    values.terms.unit_cost_gross.trim() && (
                                        <span className="text-warning-soft-foreground">
                                            {livePreview.exclusion}
                                        </span>
                                    )}
                            </div>

                            <div className="grid gap-4 sm:grid-cols-3">
                                <form.Field
                                    name="terms.input_tax_rate"
                                    children={(field) => {
                                        const invalid =
                                            field.state.meta.isTouched &&
                                            !field.state.meta.isValid
                                        return (
                                            <Field
                                                data-invalid={
                                                    invalid || undefined
                                                }
                                                className="gap-2"
                                            >
                                                <FieldLabel htmlFor="procurement-inquiry-quote-tax">
                                                    进项税率
                                                    <span className="text-destructive">
                                                        *
                                                    </span>
                                                </FieldLabel>
                                                <InquiryTaxRateInput
                                                    id="procurement-inquiry-quote-tax"
                                                    value={field.state.value}
                                                    onValueChange={(rate) =>
                                                        field.handleChange(rate)
                                                    }
                                                    onBlur={field.handleBlur}
                                                    placeholder="如 13"
                                                    aria-invalid={
                                                        invalid || undefined
                                                    }
                                                />
                                                <FieldDescription>
                                                    输入 13 表示 13%
                                                </FieldDescription>
                                                {invalid && (
                                                    <FieldError
                                                        errors={toFieldErrors(
                                                            field.state.meta
                                                                .errors,
                                                        )}
                                                    />
                                                )}
                                            </Field>
                                        )
                                    }}
                                />

                                <form.AppField
                                    name="terms.available_quantity"
                                    children={(field) => (
                                        <field.TextField
                                            id="procurement-inquiry-quote-available"
                                            label={`可供数量${unit ? `（${unit}）` : ""}`}
                                            inputMode="decimal"
                                            inputClassName="num"
                                            required
                                        />
                                    )}
                                />

                                <form.AppField
                                    name="terms.expected_delivery_date"
                                    children={(field) => (
                                        <field.DateField
                                            id="procurement-inquiry-quote-delivery"
                                            label="承诺交付日期"
                                            required
                                        />
                                    )}
                                />
                            </div>
                        </>
                    )}

                    <form.AppField
                        name="internal_note"
                        children={(field) => (
                            <field.TextareaField
                                id="procurement-inquiry-quote-note"
                                label={
                                    unavailable
                                        ? "无法供货原因 / 备注（建议填写）"
                                        : "内部备注（选填）"
                                }
                                placeholder={
                                    unavailable
                                        ? "例如：工厂停产、交期无法满足、未达起订量等"
                                        : "有需要说明的报价条件、账期或包装要求时填写"
                                }
                                rows={2}
                                textareaClassName="min-h-20 resize-y"
                            />
                        )}
                    />
                </fieldset>
            </DialogBody>

            <DialogFooter className="flex-wrap items-center gap-2">
                <Button
                    id="procurement-inquiry-quote-cancel"
                    type="button"
                    variant="ghost"
                    className="sm:mr-auto"
                    disabled={command.isPending}
                    onClick={onBack}
                >
                    <ArrowLeftIcon
                        aria-hidden="true"
                        data-icon="inline-start"
                    />
                    返回比价列表
                </Button>
                <Button
                    id="procurement-inquiry-quote-save"
                    type="submit"
                    variant={onAdopt && !unavailable ? "outline" : "default"}
                    disabled={
                        command.isPending ||
                        command.unresolved ||
                        !command.ready
                    }
                    onClick={() => {
                        submitIntent.current = "save"
                    }}
                >
                    {command.isPending && submitIntent.current === "save"
                        ? "正在保存…"
                        : "保存报价"}
                </Button>
                {onAdopt && !unavailable && (
                    <Button
                        id="procurement-inquiry-quote-save-adopt"
                        type="submit"
                        disabled={
                            command.isPending ||
                            command.unresolved ||
                            !command.ready ||
                            !canSaveAndAdopt
                        }
                        title={
                            lineUnavailableInFeedback
                                ? "主表当前行已设为不供应"
                                : (livePreview?.exclusion ?? undefined)
                        }
                        onClick={() => {
                            submitIntent.current = "saveAndAdopt"
                        }}
                    >
                        {command.isPending &&
                        submitIntent.current === "saveAndAdopt"
                            ? "正在保存…"
                            : "保存并采用"}
                    </Button>
                )}
            </DialogFooter>
        </form>
    )
}
