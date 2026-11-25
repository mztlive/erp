"use client"

import {
    useEffect,
    useImperativeHandle,
    useRef,
    useState,
    type ReactNode,
    type RefObject,
} from "react"
import {
    ChevronLeftIcon,
    ChevronRightIcon,
    CircleHelpIcon,
    SearchIcon,
    TriangleAlertIcon,
    XIcon,
} from "lucide-react"

import {
    MoneyValue,
    QuantityValue,
    RateValue,
    WorkspaceFactGrid,
    WorkspaceFactGroup,
    WorkspaceFactItem,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
} from "@/components/ui/select"
import {
    Table,
    TableBody,
    TableCell,
    TableFooter,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import {
    Tooltip,
    TooltipContent,
    TooltipProvider,
    TooltipTrigger,
} from "@/components/ui/tooltip"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { compareDecimal, divideFixed, multiplyFixed } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import type { ApprovalMaterials } from "../api/materials"
import { displayReadableName } from "../display"
import { sectionValue } from "./approval-submitted-summary"

const FINANCIAL_LABELS = {
    customer_refund: "客户退款单",
    supplier_refund: "供应商退款单",
    receipt_reversal: "收款红字冲正",
    payment_reversal: "付款红字冲正",
}

export type FinancialApprovalDocumentType = keyof typeof FINANCIAL_LABELS

export function isFinancialApprovalDocumentType(
    type?: string | null,
): type is FinancialApprovalDocumentType {
    return type != null && Object.hasOwn(FINANCIAL_LABELS, type)
}

export function isReversalDocumentType(type?: string | null): boolean {
    return type === "receipt_reversal" || type === "payment_reversal"
}

type BriefSource = ApprovalMaterials["display"]["source"]
type BriefSection = BriefSource["extra_sections"][number]

export interface FinancialApprovalRendererHandle {
    locateLines: () => void
}

export type FinancialApprovalRendererProps = Readonly<{
    documentType: FinancialApprovalDocumentType
    source: BriefSource
    submitterName?: string | null
    impactSummary?: string | null
    documentNo?: string | null
    counterpartyLabel?: string | null
    instanceId?: string | null
    resubmission?: ReactNode
    hideRiskStrip?: boolean
    rendererHandleRef?: RefObject<FinancialApprovalRendererHandle | null>
}>

const AMOUNT_LABELS = [
    "本次退款金额",
    "本次冲正金额",
    "退款金额",
    "冲正金额",
    "红字冲正金额",
    "转留抵金额",
    "含税金额",
    "申请金额",
    "金额",
]
const COUNTERPARTY_LABELS = [
    "客户",
    "供应商",
    "往来单位",
    "往来主体",
    "退款对象",
    "付款主体",
    "收款主体",
]
const SUBMITTER_LABELS = ["提交人", "申请人"]
const ORIGINAL_DOC_PATTERN =
    /^(?:原始资金单据|原(?:回款|付款|收款)(?:单(?:据|号)?|编号)?|原单(?:据|号)?|(?:关联(?:单据|回款单?|付款单?|销售单|采购单)|来源单据)(?:号)?|原凭证)$/
const MISSING_ORIGINAL_PATTERN = /待补全|未记录|未找到|未关联|^[-—]+$/
const ORIGINAL_AMOUNT_PATTERN =
    /^原(?:单据?|回款(?:单据?)?|付款(?:单据?)?|收款(?:单据?)?|应收|应付)金额$/
const POST_ACTION_BALANCE_PATTERN =
    /(?:冲正后.*(?:余额|金额)|剩余可退|剩余可冲)/
const LONG_REASON_PATTERN = /(?:原因|备注|说明|事由)/
const MONEY_SCALE = 28
const HIGH_RISK_REFUND_THRESHOLD = "100000"
const LINE_FILTER_THRESHOLD = 8
const LINE_PAGE_SIZE = 20
const LINE_HIGHLIGHT_MS = 2200

function currencyDecimal(value?: string | null): string | undefined {
    const match = /^([+-]?)[¥￥]?([+-]?)(\d{1,3}(?:,\d{3})+|\d+)(\.\d+)?$/.exec(
        value?.trim() ?? "",
    )
    if (
        !match ||
        (match[1] && match[2]) ||
        (match[4]?.slice(1).length ?? 0) > MONEY_SCALE
    ) {
        return undefined
    }

    const sign = (match[1] || match[2]) === "-" ? "-" : ""
    const integer = match[3].replaceAll(",", "").replace(/^0+(?=\d)/, "")
    return `${sign}${integer}${match[4] ?? ""}`
}

function FinancialAmount({ value }: { value?: string | null }) {
    const decimal = currencyDecimal(value)
    return decimal != null ? (
        <MoneyValue
            value={decimal}
            display="exact"
            inheritColor
            className="font-semibold [&_[data-slot=money-value-number]]:wrap-anywhere [&_[data-slot=money-value-number]]:whitespace-normal"
        />
    ) : (
        <span className="num wrap-anywhere">{value || "—"}</span>
    )
}

function sameAmount(value: string, other?: string): boolean {
    if (value === other) return true
    const left = currencyDecimal(value)
    const right = currencyDecimal(other)
    return (
        left != null &&
        right != null &&
        compareDecimal(left, right, MONEY_SCALE) === 0
    )
}

function FinancialLineValue({ value }: { value: string | null }) {
    if (value && /[¥￥]/.test(value)) return <FinancialAmount value={value} />
    const decimal = currencyDecimal(value)
    return decimal != null ? (
        <QuantityValue value={decimal} unit={null} />
    ) : (
        value || "—"
    )
}

function readFinancialSource(
    documentType: FinancialApprovalDocumentType,
    source: BriefSource,
) {
    const reversal = isReversalDocumentType(documentType)
    const sections = source.extra_sections
        .map((section) => ({
            ...section,
            label: section.label.trim(),
            value: sectionValue(section).trim(),
        }))
        .filter(({ label, value }) => label && value)

    const amountTitle = reversal ? "本次冲正金额" : "本次退款金额"
    const reasonLabel = reversal ? "冲正原因" : "退款原因"
    const amountField = [
        amountTitle,
        reversal ? "冲正金额" : "退款金额",
        ...AMOUNT_LABELS,
    ]
        .map((label) => sections.find((section) => section.label === label))
        .find(Boolean)
    const amount = source.amount_label?.trim() || amountField?.value
    const amountDecimal = currencyDecimal(amount)
    const largeRefund =
        !reversal &&
        amountDecimal != null &&
        compareDecimal(
            amountDecimal.replace(/^-/, ""),
            HIGH_RISK_REFUND_THRESHOLD,
            MONEY_SCALE,
        ) >= 0

    const originalDocuments = sections.filter(({ label }) =>
        ORIGINAL_DOC_PATTERN.test(label),
    )
    const originalDocument = originalDocuments.find(
        ({ value, object_id }) =>
            displayReadableName(value, object_id) &&
            !MISSING_ORIGINAL_PATTERN.test(value),
    )
    const originalAmount = sections.find(({ label }) =>
        ORIGINAL_AMOUNT_PATTERN.test(label),
    )
    const balances = sections.filter(
        ({ label }) =>
            (POST_ACTION_BALANCE_PATTERN.test(label) ||
                label.includes("余额")) &&
            !LONG_REASON_PATTERN.test(label),
    )
    const balance =
        balances.find(({ label }) => POST_ACTION_BALANCE_PATTERN.test(label)) ??
        balances[0]
    const impacts = sections
        .filter(({ label }) => label === "核销影响")
        .map(({ value }) => value)
    const hasReason = sections.some(
        ({ label, value }) =>
            (label === "原因" || label === reasonLabel || label === "事由") &&
            !/^[-—]+$|^未记录/.test(value),
    )

    return {
        reversal,
        sections,
        amountTitle,
        reasonLabel,
        amountField,
        amount,
        amountDecimal,
        largeRefund,
        originalDocument,
        originalAmount,
        balance,
        impacts,
        hasReason,
    }
}

export function buildFinancialDynamicRejectPhrases(
    documentType: FinancialApprovalDocumentType,
    source: BriefSource,
    attachmentCount?: number,
): string[] {
    const facts = readFinancialSource(documentType, source)
    const operation = facts.reversal ? "冲正" : "退款"
    const evidenceMissing =
        attachmentCount === 0 ||
        (attachmentCount == null &&
            facts.sections.some(
                ({ label, value }) =>
                    label === "本单凭证附件" && value === "未上传",
            ))

    return [
        !facts.originalDocument && "请补充可核对的原单号及原单依据",
        !facts.hasReason && `请补充${operation}原因及处理依据`,
        evidenceMissing && `请补充本次${operation}的复核凭证`,
        facts.largeRefund && "大额退款，请补充金额依据和核销说明",
        !facts.amountDecimal && `请核实本次${operation}金额`,
        source.more_count > 0 && "请补充完整的原单核销或分配明细",
        (facts.impacts.length > 0 || facts.balance) &&
            "请复核往来余额及核销影响",
        facts.originalDocument &&
            `请核对本次${operation}金额与原单及凭证是否一致`,
        documentType === "supplier_refund" && "请核实供应商退款结算方式",
    ]
        .filter((phrase): phrase is string => typeof phrase === "string")
        .slice(0, 6)
}

export type FinancialApprovalRiskSummary = Readonly<{
    severe: boolean
    riskLabels: readonly string[]
    impactDescriptions: readonly string[]
    missingOriginalDocument: boolean
    evidenceMissing: boolean
    hasLines: boolean
}>

/** 提取退款/红字冲正单据的前置体检风险摘要，供行动指挥卡与审查条共用。 */
export function inspectFinancialApprovalRisks(
    documentType: FinancialApprovalDocumentType,
    source: BriefSource,
    impactSummary?: string | null,
    attachmentCount?: number,
): FinancialApprovalRiskSummary {
    const facts = readFinancialSource(documentType, source)
    const impactDescriptions = [
        ...new Set([...facts.impacts, impactSummary?.trim() ?? ""]),
    ].filter(Boolean)
    const severe = facts.reversal || facts.largeRefund
    const evidenceMissing =
        attachmentCount === 0 ||
        (attachmentCount == null &&
            facts.sections.some(
                ({ label, value }) =>
                    label === "本单凭证附件" && value === "未上传",
            ))
    const riskLabels = [
        facts.reversal && "红字冲正",
        facts.largeRefund && "大额退款",
        !facts.originalDocument && "原单号待补全",
        impactDescriptions.length > 0 && "核销影响",
    ].filter((value): value is string => Boolean(value))
    return {
        severe,
        riskLabels,
        impactDescriptions,
        missingOriginalDocument: !facts.originalDocument,
        evidenceMissing,
        hasLines: source.lines.length + source.more_count > 0,
    }
}

function balancePercent(balance?: string, originalAmount?: string) {
    const value = currencyDecimal(balance)
    const total = currencyDecimal(originalAmount)
    if (
        value == null ||
        total == null ||
        compareDecimal(total, "0", MONEY_SCALE) <= 0 ||
        compareDecimal(value, "0", MONEY_SCALE) < 0 ||
        compareDecimal(value, total, MONEY_SCALE) > 0
    ) {
        return undefined
    }
    const ratio = divideFixed(value, total, {
        numeratorMaxScale: MONEY_SCALE,
        denominatorMaxScale: MONEY_SCALE,
        outputScale: 4,
        allowNegative: true,
    })
    return multiplyFixed(ratio, "100", {
        leftMaxScale: 4,
        rightMaxScale: 0,
        outputScale: 2,
    })
}

function KpiHelp({
    id,
    label,
    children,
}: {
    id: string
    label: string
    children: ReactNode
}) {
    return (
        <Tooltip>
            <TooltipTrigger
                render={
                    <button
                        id={id}
                        type="button"
                        aria-label={label}
                        className="inline-flex size-4 cursor-help items-center justify-center rounded-sm text-muted-foreground hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring"
                    />
                }
            >
                <CircleHelpIcon aria-hidden="true" className="size-3" />
            </TooltipTrigger>
            <TooltipContent>{children}</TooltipContent>
        </Tooltip>
    )
}

function FinancialMetaStream({
    supplier,
    counterparty,
    originalDocument,
    facts,
    kpiColumns,
    hasLinesBelow,
}: {
    supplier: boolean
    counterparty?: string
    originalDocument?: BriefSection
    facts: BriefSection[]
    kpiColumns: 2 | 3
    hasLinesBelow: boolean
}) {
    const seen = new Set<string>()
    const uniqueFacts = facts.filter(({ label, value }) => {
        const key = `${label}\0${value}`
        if (seen.has(key)) return false
        seen.add(key)
        return true
    })
    const shortCount =
        2 +
        uniqueFacts.filter(
            ({ label, value }) =>
                !(
                    LONG_REASON_PATTERN.test(label) ||
                    value.length > 30 ||
                    value.includes("\n")
                ),
        ).length
    const columns: 2 | 3 =
        shortCount <= 2 || (kpiColumns === 2 && shortCount === 4) ? 2 : 3

    return (
        <WorkspaceFactGroup
            title="核心业务与原单依据"
            className={cn(
                "pt-1",
                hasLinesBelow && "border-b border-border/60 pb-4",
            )}
        >
            <WorkspaceFactGrid columns={columns} ariaLabel="核心业务与原单依据">
                <WorkspaceFactItem
                    label={supplier ? "往来供应商" : "往来客户"}
                    value={counterparty ?? "未记录往来单位"}
                    tone={counterparty ? "default" : "muted"}
                />
                <WorkspaceFactItem
                    label="原关联单据"
                    value={originalDocument?.value ?? "原单号待补全"}
                    numeric
                    selectable={Boolean(originalDocument)}
                    tone={originalDocument ? "default" : "warning"}
                />
                {uniqueFacts.map(({ label, value, numeric }) => {
                    const longReason =
                        LONG_REASON_PATTERN.test(label) ||
                        value.length > 30 ||
                        value.includes("\n")
                    const isNumeric = Boolean(
                        numeric ||
                        /日期|发生日|时间|单号|流水|金额|数量|比例|笔数/.test(
                            label,
                        ),
                    )
                    return (
                        <WorkspaceFactItem
                            key={`${label}\0${value}`}
                            label={label}
                            value={value}
                            numeric={isNumeric}
                            selectable={isNumeric}
                            span={longReason ? "full" : 1}
                        />
                    )
                })}
            </WorkspaceFactGrid>
        </WorkspaceFactGroup>
    )
}

/** 财务审批根据提交快照展示金额、原单及核销依据。 */
export function FinancialApprovalRenderer(
    props: FinancialApprovalRendererProps,
) {
    const prefix =
        `financial-approval-${toAutomationIdSegment(props.documentType)}-` +
        toAutomationIdSegment(props.instanceId || props.documentNo)

    return <FinancialApprovalContent key={prefix} prefix={prefix} {...props} />
}

function FinancialApprovalContent({
    prefix,
    documentType,
    source,
    submitterName,
    impactSummary,
    documentNo,
    counterpartyLabel,
    resubmission,
    hideRiskStrip = false,
    rendererHandleRef,
}: FinancialApprovalRendererProps & { prefix: string }) {
    const facts = readFinancialSource(documentType, source)
    const { reversal, amount, originalDocument, originalAmount, balance } =
        facts
    const supplier =
        documentType === "supplier_refund" ||
        documentType === "payment_reversal"
    const counterparty =
        displayReadableName(counterpartyLabel) ||
        displayReadableName(source.customer) ||
        facts.sections.find(
            ({ label, value, object_id }) =>
                COUNTERPARTY_LABELS.includes(label) &&
                displayReadableName(value, object_id),
        )?.value
    const submitter =
        displayReadableName(submitterName) ||
        displayReadableName(source.submitter_name) ||
        facts.sections.find(
            ({ label, value, object_id }) =>
                SUBMITTER_LABELS.includes(label) &&
                displayReadableName(value, object_id),
        )?.value
    const ratio = balancePercent(balance?.value, originalAmount?.value)
    const impactDescriptions = [
        ...new Set([...facts.impacts, impactSummary?.trim() ?? ""]),
    ].filter(Boolean)
    const severe = reversal || facts.largeRefund
    const riskLabels = [
        reversal && "红字冲正",
        facts.largeRefund && "大额退款",
        !originalDocument && "原单号待补全",
        impactDescriptions.length > 0 && "核销影响",
    ].filter(Boolean)
    const amountBadge = reversal
        ? "红字冲回"
        : facts.amountField?.label === "转留抵金额"
          ? "转留抵"
          : supplier
            ? originalDocument?.value.startsWith("应付分录")
                ? "应付冲减"
                : "退款入账"
            : originalDocument?.value.startsWith("应收分录")
              ? "应收冲减"
              : "退款出账"
    const amountTone = severe
        ? "text-destructive-soft-foreground"
        : supplier
          ? "text-info-soft-foreground"
          : "text-warning-soft-foreground"

    const metadata = facts.sections
        .filter(({ label, value }) => {
            if (label === "核销影响") return false
            if (AMOUNT_LABELS.includes(label) && sameAmount(value, amount))
                return false
            if (
                ORIGINAL_AMOUNT_PATTERN.test(label) &&
                sameAmount(value, originalAmount?.value)
            )
                return false
            if (label === balance?.label && sameAmount(value, balance.value))
                return false
            if (COUNTERPARTY_LABELS.includes(label) && value === counterparty)
                return false
            if (SUBMITTER_LABELS.includes(label) && value === submitter)
                return false
            if (
                /^(?:本单)?(?:单号|单据编号)$/.test(label) &&
                value === documentNo
            )
                return false
            return !(
                ORIGINAL_DOC_PATTERN.test(label) &&
                (value === originalDocument?.value ||
                    MISSING_ORIGINAL_PATTERN.test(value))
            )
        })
        .map((section) =>
            section.label === "原因"
                ? { ...section, label: facts.reasonLabel }
                : section,
        )
    if (submitter) {
        metadata.unshift({
            label: "提交人",
            value: submitter,
            numeric: false,
            object_id: null,
        })
    }

    const [search, setSearch] = useState("")
    const [page, setPage] = useState(1)
    const [pageSize, setPageSize] = useState(LINE_PAGE_SIZE)
    const [highlighted, setHighlighted] = useState(false)
    const lineSectionRef = useRef<HTMLElement>(null)
    const timerRef = useRef<number | undefined>(undefined)
    const scrollFrameRef = useRef<number | undefined>(undefined)
    useEffect(
        () => () => {
            window.clearTimeout(timerRef.current)
            if (scrollFrameRef.current != null) {
                window.cancelAnimationFrame(scrollFrameRef.current)
            }
        },
        [],
    )

    const linesId = `${prefix}-lines`
    const lineTitle = reversal
        ? "冲正涉及的原单核销明细"
        : "退款涉及的原单核销明细"
    const totalLineCount = source.lines.length + source.more_count
    const showLineSearch =
        source.lines.length > LINE_FILTER_THRESHOLD || Boolean(search.trim())
    const query = showLineSearch ? search.trim().toLowerCase() : ""
    const rows = source.lines.map((line, index) => ({
        ...line,
        title: displayReadableName(line.title) || "关联单据待补全",
        lineNo: index + 1,
    }))
    const filteredRows = query
        ? rows.filter(({ title, quantity, due_label }) =>
              [title, quantity, due_label].some((value) =>
                  value?.toLowerCase().includes(query),
              ),
          )
        : rows
    const pageCount = Math.max(1, Math.ceil(filteredRows.length / pageSize))
    const currentPage = Math.min(page, pageCount)
    const pageRows = filteredRows.slice(
        (currentPage - 1) * pageSize,
        currentPage * pageSize,
    )

    function updateSearch(value: string) {
        setSearch(value)
        setPage(1)
    }

    function locateLines() {
        updateSearch("")
        window.clearTimeout(timerRef.current)
        if (scrollFrameRef.current != null) {
            window.cancelAnimationFrame(scrollFrameRef.current)
        }
        scrollFrameRef.current = window.requestAnimationFrame(() => {
            scrollFrameRef.current = undefined
            lineSectionRef.current?.focus({ preventScroll: true })
            lineSectionRef.current?.scrollIntoView({
                behavior: window.matchMedia("(prefers-reduced-motion: reduce)")
                    .matches
                    ? "instant"
                    : "smooth",
                block: "start",
            })
            setHighlighted(true)
            timerRef.current = window.setTimeout(
                () => setHighlighted(false),
                LINE_HIGHLIGHT_MS,
            )
        })
    }

    useImperativeHandle(rendererHandleRef, () => ({
        locateLines,
    }))

    return (
        <div
            data-slot="financial-approval-renderer"
            data-document-type={documentType}
            role="region"
            aria-label={`${FINANCIAL_LABELS[documentType]}审批核验`}
            className="@container/financial-approval flex min-w-0 flex-col gap-3 pt-3 pb-5"
        >
            {resubmission}
            {!hideRiskStrip && riskLabels.length > 0 ? (
                <div
                    data-slot="financial-approval-risk-strip"
                    className={cn(
                        "flex flex-wrap items-center justify-between gap-x-3 gap-y-1 rounded-md border-l-2 px-3 py-1.5 text-xs",
                        severe
                            ? "border-l-destructive bg-destructive-soft/65 text-destructive-soft-foreground"
                            : "border-l-warning bg-warning-soft/75 text-warning-soft-foreground",
                    )}
                >
                    <div
                        role="alert"
                        className="flex min-w-0 flex-1 items-start gap-2"
                    >
                        <TriangleAlertIcon
                            aria-hidden="true"
                            className="mt-0.5 size-3.5 shrink-0"
                        />
                        <div className="min-w-0 wrap-anywhere">
                            <p>
                                <span className="font-semibold">
                                    {severe ? "高危复核" : "需复核"}
                                </span>
                                <span className="num ml-1.5">
                                    {riskLabels.length} 项
                                </span>
                                <span className="ml-1.5">
                                    {riskLabels.join(" · ")}
                                </span>
                            </p>
                            {impactDescriptions.map((description) => (
                                <p key={description}>{description}</p>
                            ))}
                        </div>
                    </div>
                    <Button
                        id={`${prefix}-locate-lines`}
                        type="button"
                        variant="ghost"
                        size="xs"
                        aria-controls={linesId}
                        className={cn(
                            "h-control-xs shrink-0 gap-0.5 px-1.5 text-xs font-medium shadow-none",
                            severe
                                ? "text-destructive-soft-foreground hover:bg-destructive-soft hover:text-destructive-soft-foreground"
                                : "text-warning-soft-foreground hover:bg-warning-soft hover:text-warning-soft-foreground",
                        )}
                        onClick={locateLines}
                    >
                        {reversal ? "定位冲正明细" : "定位核销明细"}
                        <ChevronRightIcon aria-hidden="true" />
                    </Button>
                </div>
            ) : null}

            <TooltipProvider>
                <section
                    aria-label="财务审批决策指标"
                    className={cn(
                        "grid grid-cols-1 gap-x-6 gap-y-3.5 rounded-lg bg-surface-sunken px-4 py-3.5 @[480px]/financial-approval:grid-cols-2",
                        balance && "@[840px]/financial-approval:grid-cols-3",
                    )}
                >
                    <div className="min-w-0">
                        <div className="flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-xs text-muted-foreground">
                            <span>{facts.amountTitle}</span>
                            <span className="text-tiny">提交金额</span>
                            <KpiHelp
                                id={`${prefix}-amount-help`}
                                label={`${facts.amountTitle}口径`}
                            >
                                {reversal
                                    ? "取本次提交的冲正金额。通过后按该金额冲回原收付款并调整相关核销。"
                                    : "取本次提交的退款金额。过账影响以本单退款依据及结算方式为准。"}
                            </KpiHelp>
                        </div>
                        <div
                            className={cn(
                                "mt-1 text-value-section font-semibold tracking-tight",
                                amountTone,
                            )}
                        >
                            <FinancialAmount value={amount} />
                        </div>
                        <Badge
                            variant={
                                severe
                                    ? "destructive"
                                    : supplier
                                      ? "info"
                                      : "warning"
                            }
                            className="mt-1 text-tiny"
                        >
                            {amountBadge}
                        </Badge>
                    </div>

                    <div className="min-w-0">
                        <div className="flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-xs text-muted-foreground">
                            <span>{originalAmount?.label ?? "原单金额"}</span>
                            <span className="text-tiny">原单口径</span>
                            <KpiHelp
                                id={`${prefix}-original-amount-help`}
                                label="原单金额口径"
                            >
                                取快照所记录的原关联资金单据或往来分录金额，作为本次
                                {reversal ? "冲正" : "退款"}核对基准。
                            </KpiHelp>
                        </div>
                        <div className="mt-1 text-value-compact font-semibold">
                            <FinancialAmount value={originalAmount?.value} />
                        </div>
                        {!originalAmount ? (
                            <p className="text-tiny text-muted-foreground">
                                快照未记录
                            </p>
                        ) : null}
                    </div>

                    {balance ? (
                        <div className="min-w-0 @[480px]/financial-approval:col-span-2 @[840px]/financial-approval:col-span-1">
                            <div className="flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-xs text-muted-foreground">
                                <span>{balance.label}</span>
                                <span className="text-tiny">快照余额</span>
                                <KpiHelp
                                    id={`${prefix}-balance-help`}
                                    label="余额占比口径"
                                >
                                    {ratio != null
                                        ? "占比 = 快照所列余额 ÷ 原单金额 × 100%。"
                                        : "取本次提交快照所列往来余额，请结合原单与核销记录复核。"}
                                </KpiHelp>
                            </div>
                            <div className="mt-1 text-value-compact font-semibold">
                                <FinancialAmount value={balance.value} />
                            </div>
                            {ratio != null ? (
                                <div className="mt-1 space-y-0.5">
                                    <div
                                        aria-hidden="true"
                                        className="h-1.5 overflow-hidden rounded-full bg-muted"
                                    >
                                        <div
                                            className="h-full rounded-full bg-info"
                                            style={{ width: `${ratio}%` }}
                                        />
                                    </div>
                                    <p className="text-tiny text-muted-foreground">
                                        占原单金额{" "}
                                        <RateValue
                                            value={ratio}
                                            precision={2}
                                        />
                                    </p>
                                </div>
                            ) : null}
                        </div>
                    ) : null}
                </section>
            </TooltipProvider>

            <FinancialMetaStream
                supplier={supplier}
                counterparty={counterparty}
                originalDocument={originalDocument}
                facts={metadata}
                kpiColumns={balance ? 3 : 2}
                hasLinesBelow={totalLineCount > 0 || riskLabels.length > 0}
            />

            {totalLineCount > 0 || riskLabels.length > 0 ? (
                <section
                    ref={lineSectionRef}
                    id={linesId}
                    tabIndex={-1}
                    aria-labelledby={`${linesId}-title`}
                    className="relative min-w-0 scroll-mt-16 space-y-2.5 pt-1 focus:outline-none"
                >
                    <div className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-1">
                        <h2
                            id={`${linesId}-title`}
                            className="text-section-title font-semibold tracking-tight"
                        >
                            {lineTitle}
                        </h2>
                        {totalLineCount > 0 ? (
                            <p className="num text-xs text-muted-foreground">
                                共 {totalLineCount} 项
                            </p>
                        ) : null}
                    </div>
                    {showLineSearch ? (
                        <div className="flex flex-wrap items-center justify-between gap-2">
                            <div className="relative w-full @[480px]/financial-approval:w-64">
                                <SearchIcon
                                    aria-hidden="true"
                                    className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
                                />
                                <Input
                                    id={`${prefix}-line-search`}
                                    value={search}
                                    onChange={(event) =>
                                        updateSearch(event.target.value)
                                    }
                                    placeholder="搜索原单号、日期或金额"
                                    aria-label="搜索核销明细"
                                    className="h-control-sm pr-8 pl-8 text-xs"
                                />
                                {search ? (
                                    <Button
                                        id={`${prefix}-clear-line-search`}
                                        type="button"
                                        variant="ghost"
                                        size="icon-xs"
                                        aria-label="清除搜索"
                                        className="absolute top-1/2 right-0.5 -translate-y-1/2"
                                        onClick={() => updateSearch("")}
                                    >
                                        <XIcon aria-hidden="true" />
                                    </Button>
                                ) : null}
                            </div>
                            {query ? (
                                <p
                                    role="status"
                                    aria-live="polite"
                                    className="num text-xs text-muted-foreground"
                                >
                                    命中 {filteredRows.length} / {rows.length}{" "}
                                    项
                                </p>
                            ) : null}
                        </div>
                    ) : null}

                    <div className="relative">
                        <Table
                            aria-labelledby={`${linesId}-title`}
                            className="table-fixed"
                        >
                            <TableHeader className="bg-transparent [&_th]:bg-surface-sunken [&_th]:hover:bg-surface-sunken [&_th:first-child]:rounded-l-md [&_th:last-child]:rounded-r-md [&_tr]:border-b-0">
                                <TableRow>
                                    <TableHead
                                        scope="col"
                                        className="w-10 px-2"
                                    >
                                        序号
                                    </TableHead>
                                    <TableHead
                                        scope="col"
                                        className="whitespace-normal"
                                    >
                                        关联单据 / 明细
                                    </TableHead>
                                    <TableHead
                                        scope="col"
                                        data-align="end"
                                        className="w-2/5 whitespace-normal @[480px]/financial-approval:w-1/3"
                                    >
                                        原金额 / 数量
                                    </TableHead>
                                </TableRow>
                            </TableHeader>
                            <TableBody>
                                {pageRows.map((line) => (
                                    <TableRow key={line.lineNo}>
                                        <TableCell className="num px-2 align-top text-muted-foreground">
                                            {line.lineNo}
                                        </TableCell>
                                        <TableCell className="align-top whitespace-normal">
                                            <span className="num font-medium wrap-anywhere select-all">
                                                {line.title}
                                            </span>
                                            {line.due_label ? (
                                                <p className="num mt-0.5 text-xs wrap-anywhere text-muted-foreground">
                                                    {line.due_label}
                                                </p>
                                            ) : null}
                                        </TableCell>
                                        <TableCell
                                            data-align="end"
                                            className="num align-top whitespace-normal wrap-anywhere"
                                        >
                                            <FinancialLineValue
                                                value={line.quantity}
                                            />
                                        </TableCell>
                                    </TableRow>
                                ))}
                                {pageRows.length === 0 ? (
                                    <TableRow>
                                        <TableCell
                                            colSpan={3}
                                            className="text-xs whitespace-normal text-muted-foreground"
                                        >
                                            {query
                                                ? "没有匹配的明细"
                                                : "快照未展开原单核销明细，请核对原单或凭证。"}
                                        </TableCell>
                                    </TableRow>
                                ) : null}
                            </TableBody>
                            <TableFooter className="border-t-0 bg-transparent [&_td]:bg-surface-sunken [&_td:first-child]:rounded-l-md [&_td:last-child]:rounded-r-md">
                                <TableRow className="border-b-0">
                                    <TableCell
                                        colSpan={2}
                                        className="text-xs whitespace-normal"
                                    >
                                        {reversal
                                            ? "整单冲正合计"
                                            : "整单退款合计"}
                                        {query || pageCount > 1 ? (
                                            <span className="num font-normal text-muted-foreground">
                                                （当前显示 {pageRows.length}{" "}
                                                项，合计按整单）
                                            </span>
                                        ) : null}
                                    </TableCell>
                                    <TableCell
                                        data-align="end"
                                        className="whitespace-normal"
                                    >
                                        <FinancialAmount value={amount} />
                                    </TableCell>
                                </TableRow>
                            </TableFooter>
                        </Table>
                        {highlighted ? (
                            <div
                                aria-hidden="true"
                                className="pointer-events-none absolute inset-0 rounded-md ring-2 ring-inset ring-info-border motion-safe:animate-pulse"
                            />
                        ) : null}
                    </div>

                    {source.more_count > 0 ? (
                        <p className="num text-xs text-warning-soft-foreground">
                            快照仅展开 {source.lines.length} 项，另有{" "}
                            {source.more_count} 项。请核对原单或附件凭证。
                        </p>
                    ) : null}
                    {filteredRows.length > LINE_PAGE_SIZE ||
                    pageSize !== LINE_PAGE_SIZE ? (
                        <nav
                            aria-label="核销明细分页"
                            className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground"
                        >
                            <div className="flex items-center gap-2">
                                <span id={`${prefix}-page-size-label`}>
                                    每页
                                </span>
                                <Select
                                    value={String(pageSize)}
                                    onValueChange={(next) => {
                                        if (!next) return
                                        setPageSize(
                                            next === "50" ? 50 : LINE_PAGE_SIZE,
                                        )
                                        setPage(1)
                                    }}
                                >
                                    <SelectTrigger
                                        id={`${prefix}-page-size`}
                                        size="sm"
                                        aria-labelledby={`${prefix}-page-size-label`}
                                        className="w-24"
                                    >
                                        <SelectValue>{pageSize} 项</SelectValue>
                                    </SelectTrigger>
                                    <SelectContent
                                        align="start"
                                        alignItemWithTrigger={false}
                                    >
                                        <SelectItem
                                            id={`${prefix}-page-size-20`}
                                            value="20"
                                        >
                                            20 项
                                        </SelectItem>
                                        <SelectItem
                                            id={`${prefix}-page-size-50`}
                                            value="50"
                                        >
                                            50 项
                                        </SelectItem>
                                    </SelectContent>
                                </Select>
                                <span role="status" className="num">
                                    第 {currentPage} / {pageCount} 页 · 共{" "}
                                    {filteredRows.length} 项
                                </span>
                            </div>
                            <div className="flex items-center gap-1">
                                <Button
                                    id={`${prefix}-previous-page`}
                                    type="button"
                                    variant="outline"
                                    size="icon-sm"
                                    aria-label="上一页明细"
                                    disabled={currentPage === 1}
                                    onClick={() => setPage(currentPage - 1)}
                                >
                                    <ChevronLeftIcon aria-hidden="true" />
                                </Button>
                                <Button
                                    id={`${prefix}-next-page`}
                                    type="button"
                                    variant="outline"
                                    size="icon-sm"
                                    aria-label="下一页明细"
                                    disabled={currentPage === pageCount}
                                    onClick={() => setPage(currentPage + 1)}
                                >
                                    <ChevronRightIcon aria-hidden="true" />
                                </Button>
                            </div>
                        </nav>
                    ) : null}
                </section>
            ) : null}
        </div>
    )
}
