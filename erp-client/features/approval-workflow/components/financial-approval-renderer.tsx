"use client"

import { InfoIcon, TriangleAlertIcon } from "lucide-react"

import {
    DetailWorkspaceFact,
    DetailWorkspaceFacts,
    DetailWorkspaceMetricItem,
    DetailWorkspaceMetrics,
    DetailWorkspaceSection,
    MoneyValue,
    detailWorkspaceStyles,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { cn } from "@/lib/utils"
import type { ApprovalMaterials } from "../api/materials"
import { displayReadableName } from "../display"
import { sectionValue } from "./approval-submitted-summary"

export type FinancialApprovalDocumentType =
    | "customer_refund"
    | "supplier_refund"
    | "receipt_reversal"
    | "payment_reversal"

const FINANCIAL_DOCUMENT_TYPES = new Set<string>([
    "customer_refund",
    "supplier_refund",
    "receipt_reversal",
    "payment_reversal",
])

const REVERSAL_DOCUMENT_TYPES = new Set<string>([
    "receipt_reversal",
    "payment_reversal",
])

export function isFinancialApprovalDocumentType(
    type?: string | null,
): type is FinancialApprovalDocumentType {
    return Boolean(type && FINANCIAL_DOCUMENT_TYPES.has(type))
}

export function isReversalDocumentType(type?: string | null): boolean {
    return Boolean(type && REVERSAL_DOCUMENT_TYPES.has(type))
}

export type FinancialApprovalRendererProps = Readonly<{
    documentType: FinancialApprovalDocumentType
    source: ApprovalMaterials["display"]["source"]
    submitterName?: string | null
    impactSummary?: string | null
    documentNo?: string | null
    counterpartyLabel?: string | null
}>

type BriefSource = ApprovalMaterials["display"]["source"]
type BriefSection = BriefSource["extra_sections"][number]

type FinancialDocMeta = Readonly<{
    label: string
    badgeVariant: "warning" | "info" | "destructive"
    amountTitle: string
    counterpartyTitle: string
    originalDocTitle: string
    lineSectionTitle: string
    lineColumnLabel: string
}>

const FINANCIAL_META: Record<FinancialApprovalDocumentType, FinancialDocMeta> =
    {
        customer_refund: {
            label: "客户退款单",
            badgeVariant: "warning",
            amountTitle: "本次退款金额",
            counterpartyTitle: "退款客户",
            originalDocTitle: "原回款 / 关联单据",
            lineSectionTitle: "退款核销与分配明细",
            lineColumnLabel: "关联单据 / 明细",
        },
        supplier_refund: {
            label: "供应商退款单",
            badgeVariant: "info",
            amountTitle: "本次退款金额",
            counterpartyTitle: "退款供应商",
            originalDocTitle: "原付款 / 关联单据",
            lineSectionTitle: "退款核销与分配明细",
            lineColumnLabel: "关联单据 / 明细",
        },
        receipt_reversal: {
            label: "收款红字冲正",
            badgeVariant: "destructive",
            amountTitle: "本次冲正金额",
            counterpartyTitle: "往来客户",
            originalDocTitle: "原回款单据",
            lineSectionTitle: "冲正涉及核销明细",
            lineColumnLabel: "冲正关联单据",
        },
        payment_reversal: {
            label: "付款红字冲正",
            badgeVariant: "destructive",
            amountTitle: "本次冲正金额",
            counterpartyTitle: "往来供应商",
            originalDocTitle: "原付款单据",
            lineSectionTitle: "冲正涉及核销明细",
            lineColumnLabel: "冲正关联单据",
        },
    }

const AMOUNT_LABELS = [
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

const ORIGINAL_DOC_PATTERN =
    /^(?:原回款|原付款|原收款|原单据|原单号|关联单据|来源单据|关联回款|关联付款|关联销售单|关联采购单|原凭证)/

const BALANCE_OR_IMPACT_PATTERN =
    /(?:余额|留抵|原单金额|原回款金额|原付款金额|冲正后|剩余可退)/

const REASON_PATTERN = /(?:原因|备注|说明|事由)/

const WIDE_FACT_LENGTH = 30

function extractCurrencyDecimal(value?: string | null): string | null {
    if (!value) return null
    const match =
        /^(?:(增加|减少)\s+)?(-?)[¥￥]?(-?)(\d{1,3}(?:,\d{3})+|\d+)(\.\d+)?$/.exec(
            value.trim(),
        )
    if (!match || (match[2] && match[3])) return null
    const hasCurrencySymbol = /[¥￥]/.test(value)
    if (!hasCurrencySymbol && !match[5]) return null
    return `${match[2] || match[3]}${match[4].replaceAll(",", "")}${match[5] ?? ""}`
}

function FinancialMoneyDisplay({
    value,
    className,
}: {
    value: string
    className?: string
}) {
    const decimal = extractCurrencyDecimal(value)
    if (decimal != null) {
        return <MoneyValue value={decimal} className={className} />
    }
    return <span className={cn("num", className)}>{value}</span>
}

function findMatchingSection(
    sections: readonly BriefSection[],
    matcher: (section: BriefSection) => boolean,
): BriefSection | undefined {
    return sections.find(matcher)
}

/**
 * 面向客户退款、供应商退款、收款冲正、付款冲正的财务审批渲染器。
 * 遵循飞书纯白通栏规范：冲正单顶部强警示红条 + 金额/原单/余额/账户核验条 + 结构化明细表。
 */
export function FinancialApprovalRenderer({
    documentType,
    source,
    submitterName,
    impactSummary,
    documentNo,
    counterpartyLabel,
}: FinancialApprovalRendererProps) {
    const meta = FINANCIAL_META[documentType]
    const isReversal = isReversalDocumentType(documentType)
    const sections = source.extra_sections
    const consumedLabels = new Set<string>()

    const amountSection = findMatchingSection(sections, (section) =>
        AMOUNT_LABELS.includes(section.label),
    )
    const primaryAmount =
        source.amount_label ||
        (amountSection ? sectionValue(amountSection) : undefined)
    if (amountSection && primaryAmount === sectionValue(amountSection)) {
        consumedLabels.add(amountSection.label)
    }

    const counterpartySection = findMatchingSection(sections, (section) =>
        COUNTERPARTY_LABELS.includes(section.label),
    )
    const resolvedCounterparty =
        displayReadableName(counterpartyLabel) ||
        displayReadableName(source.customer) ||
        (counterpartySection ? sectionValue(counterpartySection) : undefined)
    if (counterpartySection) {
        consumedLabels.add(counterpartySection.label)
    }

    const originalDocSection = findMatchingSection(
        sections,
        (section) =>
            !consumedLabels.has(section.label) &&
            ORIGINAL_DOC_PATTERN.test(section.label),
    )
    if (originalDocSection) {
        consumedLabels.add(originalDocSection.label)
    }

    const balanceSection = findMatchingSection(
        sections,
        (section) =>
            !consumedLabels.has(section.label) &&
            BALANCE_OR_IMPACT_PATTERN.test(section.label),
    )
    if (balanceSection) {
        consumedLabels.add(balanceSection.label)
    }

    const resolvedSubmitter =
        displayReadableName(submitterName) ||
        displayReadableName(source.submitter_name)
    const submitterSection = findMatchingSection(
        sections,
        (section) =>
            !consumedLabels.has(section.label) &&
            (section.label === "申请人" || section.label === "提交人"),
    )
    if (submitterSection) {
        consumedLabels.add(submitterSection.label)
    }
    const finalSubmitter =
        resolvedSubmitter ||
        (submitterSection ? sectionValue(submitterSection) : undefined)

    const remainingSections = sections.filter(
        (section) => !consumedLabels.has(section.label),
    )
    const reasonSections = remainingSections.filter((section) =>
        REASON_PATTERN.test(section.label),
    )
    const verificationSections = remainingSections.filter(
        (section) => !REASON_PATTERN.test(section.label),
    )

    const metricColumns: 3 | 4 = balanceSection ? 4 : 3
    const hasDueLabel = source.lines.some((line) => Boolean(line.due_label))

    return (
        <div
            data-slot="financial-approval-renderer"
            data-document-type={documentType}
            aria-label={`${meta.label}审批核验`}
        >
            {isReversal ? (
                <div className="pt-4">
                    <Alert variant="destructive" role="alert">
                        <TriangleAlertIcon aria-hidden="true" />
                        <AlertTitle>红字冲正高危复核</AlertTitle>
                        <AlertDescription>
                            红字冲正高危复核：通过后将冲回原收付款记录并同步调整往来余额，提交通过前须二次核对。
                        </AlertDescription>
                    </Alert>
                </div>
            ) : null}

            <DetailWorkspaceSection divider={false} ariaLabel="财务核验概览">
                <div className="flex flex-wrap items-center gap-2">
                    <Badge variant={meta.badgeVariant}>{meta.label}</Badge>
                    {isReversal ? (
                        <Badge variant="destructive">红字冲回</Badge>
                    ) : null}
                    {documentNo ? (
                        <span className="num ml-auto text-xs text-muted-foreground">
                            {documentNo}
                        </span>
                    ) : null}
                </div>

                <DetailWorkspaceMetrics
                    columns={metricColumns}
                    variant="inline"
                    ariaLabel="金额与原单核验指标"
                >
                    <DetailWorkspaceMetricItem
                        label={amountSection?.label ?? meta.amountTitle}
                        emphasis="primary"
                        value={
                            primaryAmount ? (
                                <FinancialMoneyDisplay
                                    value={primaryAmount}
                                    className={
                                        isReversal
                                            ? "text-destructive-soft-foreground"
                                            : undefined
                                    }
                                />
                            ) : (
                                "金额未记录"
                            )
                        }
                        detail={
                            isReversal
                                ? "审批通过后将冲回对应入账金额"
                                : "审批通过后按本单口径流转"
                        }
                    />
                    <DetailWorkspaceMetricItem
                        label={meta.counterpartyTitle}
                        value={
                            <span className="text-body-compact font-semibold">
                                {resolvedCounterparty || "未记录往来单位"}
                            </span>
                        }
                        detail={
                            finalSubmitter
                                ? `申请人：${finalSubmitter}`
                                : undefined
                        }
                    />
                    <DetailWorkspaceMetricItem
                        label={
                            originalDocSection?.label ?? meta.originalDocTitle
                        }
                        value={
                            originalDocSection ? (
                                <span className="num text-body-compact font-semibold">
                                    {sectionValue(originalDocSection)}
                                </span>
                            ) : source.lines.length > 0 ? (
                                `${source.lines.length + source.more_count} 笔关联明细`
                            ) : (
                                "未记录原单"
                            )
                        }
                        detail={
                            originalDocSection &&
                            source.lines.length + source.more_count > 0
                                ? `涉及 ${source.lines.length + source.more_count} 笔核销/分配`
                                : undefined
                        }
                    />
                    {balanceSection ? (
                        <DetailWorkspaceMetricItem
                            label={balanceSection.label}
                            value={
                                balanceSection.numeric ? (
                                    <FinancialMoneyDisplay
                                        value={sectionValue(balanceSection)}
                                    />
                                ) : (
                                    <span className="text-body-compact font-semibold">
                                        {sectionValue(balanceSection)}
                                    </span>
                                )
                            }
                        />
                    ) : null}
                </DetailWorkspaceMetrics>

                {verificationSections.length > 0 ||
                reasonSections.length > 0 ||
                finalSubmitter ? (
                    <div className="border-t border-border/60 pt-3.5">
                        <DetailWorkspaceFacts
                            columns={3}
                            ariaLabel="账户、结算与原因核验"
                        >
                            {verificationSections.map((section, index) => {
                                const value = sectionValue(section)
                                return (
                                    <DetailWorkspaceFact
                                        key={`${section.label}:${index}`}
                                        label={section.label}
                                        numeric={section.numeric}
                                        span={
                                            value.length > WIDE_FACT_LENGTH
                                                ? "full"
                                                : 1
                                        }
                                        value={value}
                                    />
                                )
                            })}
                            {finalSubmitter ? (
                                <DetailWorkspaceFact
                                    label="申请人"
                                    value={finalSubmitter}
                                />
                            ) : null}
                            {reasonSections.map((section, index) => (
                                <DetailWorkspaceFact
                                    key={`reason:${section.label}:${index}`}
                                    label={section.label}
                                    span="full"
                                    value={
                                        <span className="whitespace-pre-wrap">
                                            {sectionValue(section)}
                                        </span>
                                    }
                                />
                            ))}
                        </DetailWorkspaceFacts>
                    </div>
                ) : null}
            </DetailWorkspaceSection>

            {source.lines.length > 0 || source.more_count > 0 ? (
                <DetailWorkspaceSection
                    title={meta.lineSectionTitle}
                    subtitle={
                        source.lines.length > 0
                            ? `共 ${source.lines.length + source.more_count} 笔`
                            : undefined
                    }
                    ariaLabel={meta.lineSectionTitle}
                >
                    {source.lines.length > 0 ? (
                        <div
                            data-slot="detail-inline-table"
                            className={detailWorkspaceStyles.inlineTableWrapper}
                        >
                            <Table
                                aria-label={meta.lineSectionTitle}
                                className="w-full [&_:is(th,td):first-child]:ps-0"
                            >
                                <TableHeader>
                                    <TableRow>
                                        <TableHead scope="col" className="w-12">
                                            序号
                                        </TableHead>
                                        <TableHead scope="col">
                                            {meta.lineColumnLabel}
                                        </TableHead>
                                        {hasDueLabel ? (
                                            <TableHead scope="col">
                                                日期 / 说明
                                            </TableHead>
                                        ) : null}
                                        <TableHead scope="col" data-align="end">
                                            金额 / 数量
                                        </TableHead>
                                    </TableRow>
                                </TableHeader>
                                <TableBody>
                                    {source.lines.map((line, index) => (
                                        <TableRow
                                            key={`${line.title}:${index}`}
                                        >
                                            <TableCell className="num text-muted-foreground">
                                                {index + 1}
                                            </TableCell>
                                            <TableCell className="whitespace-normal wrap-anywhere font-medium">
                                                {displayReadableName(
                                                    line.title,
                                                ) || line.title}
                                            </TableCell>
                                            {hasDueLabel ? (
                                                <TableCell className="num text-xs text-muted-foreground">
                                                    {line.due_label || "—"}
                                                </TableCell>
                                            ) : null}
                                            <TableCell
                                                data-align="end"
                                                className="num font-semibold"
                                            >
                                                {line.quantity ? (
                                                    <FinancialMoneyDisplay
                                                        value={line.quantity}
                                                    />
                                                ) : (
                                                    "—"
                                                )}
                                            </TableCell>
                                        </TableRow>
                                    ))}
                                </TableBody>
                            </Table>
                        </div>
                    ) : null}

                    {source.more_count > 0 ? (
                        <Alert variant="warning">
                            <AlertDescription>
                                当前仅展示 {source.lines.length} 笔明细，另有{" "}
                                {source.more_count}{" "}
                                笔未在摘要中展开。请核对附件凭证；如有疑问请退回申请人补充。
                            </AlertDescription>
                        </Alert>
                    ) : null}
                </DetailWorkspaceSection>
            ) : null}

            {impactSummary ? (
                <DetailWorkspaceSection ariaLabel="审批影响说明">
                    <Alert
                        variant={isReversal ? "warning" : "info"}
                        role="note"
                    >
                        <InfoIcon aria-hidden="true" />
                        <AlertDescription>{impactSummary}</AlertDescription>
                    </Alert>
                </DetailWorkspaceSection>
            ) : null}
        </div>
    )
}
