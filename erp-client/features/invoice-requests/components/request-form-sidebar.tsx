"use client"

import { useEffect, useState } from "react"
import {
    Building2Icon,
    CheckIcon,
    CircleAlertIcon,
    CopyIcon,
    FileTextIcon,
    RefreshCwIcon,
} from "lucide-react"

import { MoneyValue } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { formatCurrencyFixed } from "@/lib/fixed-decimal"
import { describeMoney } from "@/lib/money-display"
import { cn } from "@/lib/utils"

import type { RequestAmounts } from "../api"
import {
    invoiceExceedsAvailable,
    invoiceOverAmount,
    invoiceRemainingAfterRequest,
} from "../line-amounts"

export type InvoiceArchiveSnapshot = {
    invoiceTitle: string
    taxNumber: string
    sellerName: string
    sellerTaxNumber: string
}

function CopyValueButton({
    id,
    label,
    value,
}: {
    id: string
    label: string
    value: string
}) {
    const [copied, setCopied] = useState(false)

    useEffect(() => {
        if (!copied) return
        const timer = window.setTimeout(() => setCopied(false), 1500)
        return () => window.clearTimeout(timer)
    }, [copied])

    if (!value.trim()) return null

    return (
        <Button
            id={id}
            type="button"
            variant="ghost"
            size="icon-xs"
            className="size-6 shrink-0 text-muted-foreground hover:text-foreground"
            aria-label={copied ? `已复制${label}` : `复制${label}`}
            title={copied ? "已复制" : `复制${label}`}
            onClick={() => {
                if (typeof navigator === "undefined" || !navigator.clipboard) {
                    return
                }
                void navigator.clipboard.writeText(value.trim()).then(
                    () => setCopied(true),
                    () => setCopied(false),
                )
            }}
        >
            {copied ? (
                <CheckIcon
                    className="size-3 text-success-soft-foreground"
                    aria-hidden="true"
                />
            ) : (
                <CopyIcon className="size-3" aria-hidden="true" />
            )}
        </Button>
    )
}

function ArchiveFieldRow({
    id,
    copyId,
    label,
    value,
    numeric = false,
}: {
    id: string
    copyId: string
    label: string
    value: string
    numeric?: boolean
}) {
    const trimmed = value.trim()
    return (
        <div className="min-w-0 space-y-0.5">
            <dt className="text-xs text-muted-foreground">{label}</dt>
            <dd className="flex min-w-0 items-start justify-between gap-1.5">
                <span
                    id={id}
                    className={cn(
                        "min-w-0 break-all text-body-compact font-medium",
                        numeric && "num",
                        trimmed ? "text-foreground" : "text-muted-foreground",
                    )}
                >
                    {trimmed || "未维护"}
                </span>
                <CopyValueButton id={copyId} label={label} value={trimmed} />
            </dd>
        </div>
    )
}

function QuotaLedgerRow({
    label,
    value,
    emphasis = "default",
    destructive = false,
}: {
    label: string
    value: string | null
    emphasis?: "default" | "strong"
    destructive?: boolean
}) {
    return (
        <div className="flex items-baseline justify-between gap-3 py-1.5 text-body-compact">
            <dt className="text-xs text-muted-foreground">{label}</dt>
            <dd className="text-right">
                {value === null ? (
                    <span className="num text-muted-foreground">—</span>
                ) : destructive ? (
                    <span className="num font-semibold text-destructive">
                        {formatCurrencyFixed(value, {
                            maxScale: 2,
                            minimumFractionDigits: 2,
                        })}
                    </span>
                ) : (
                    <MoneyValue
                        value={value}
                        display="exact"
                        className={
                            emphasis === "strong"
                                ? "font-semibold"
                                : "font-normal"
                        }
                    />
                )}
            </dd>
        </div>
    )
}

/**
 * 独立工作面右侧参考栏：
 * - 实时展示本单 4 项开票额度与扣除本次申请后的剩余额度；
 * - 以简洁描述列表展示自动带入的购销方开票档案，支持一键复制抬头与税号。
 */
export function InvoiceRequestFormSidebar({
    hasOrder,
    hasAccount,
    isLoading,
    isError,
    onRetry,
    amounts,
    archive,
    linesTotal,
}: {
    hasOrder: boolean
    hasAccount: boolean
    isLoading: boolean
    isError: boolean
    onRetry: () => void
    amounts?: RequestAmounts
    archive: InvoiceArchiveSnapshot
    linesTotal: string | null
}) {
    const hasArchiveValues = Boolean(
        archive.invoiceTitle ||
        archive.taxNumber ||
        archive.sellerName ||
        archive.sellerTaxNumber,
    )
    const exceeds = invoiceExceedsAvailable(
        linesTotal,
        amounts?.available_amount,
    )
    const overAmount = invoiceOverAmount(linesTotal, amounts?.available_amount)
    const remaining = invoiceRemainingAfterRequest(
        linesTotal,
        amounts?.available_amount,
    )
    const totalReadout = linesTotal ? describeMoney(linesTotal)?.readout : null

    return (
        <div className="space-y-6 xl:sticky xl:top-20">
            {/* 区块 1：本单开票额度 */}
            <section aria-label="本单开票额度" className="space-y-3">
                <div className="flex items-center justify-between gap-2">
                    <h2 className="text-section-title font-semibold tracking-tight text-foreground">
                        本单开票额度
                    </h2>
                </div>

                {isError ? (
                    <Alert variant="destructive">
                        <CircleAlertIcon aria-hidden="true" />
                        <AlertTitle>开票额度读取失败</AlertTitle>
                        <AlertDescription className="space-y-2">
                            <p>无法核对可申请额度，请刷新重试。</p>
                            <Button
                                id="invoice-request-amounts-retry"
                                type="button"
                                variant="outline"
                                size="xs"
                                onClick={onRetry}
                            >
                                <RefreshCwIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                重新读取
                            </Button>
                        </AlertDescription>
                    </Alert>
                ) : isLoading ? (
                    <div className="space-y-3" aria-busy="true">
                        <div className="h-9 w-36 animate-pulse rounded-md bg-muted" />
                        <div className="space-y-2 pt-1">
                            <div className="h-5 animate-pulse rounded bg-muted" />
                            <div className="h-5 animate-pulse rounded bg-muted" />
                            <div className="h-5 animate-pulse rounded bg-muted" />
                        </div>
                    </div>
                ) : !amounts ? (
                    <div className="flex items-start gap-2.5 py-2 text-xs text-muted-foreground">
                        <FileTextIcon
                            className="mt-0.5 size-4 shrink-0 text-muted-foreground/70"
                            aria-hidden="true"
                        />
                        <p className="leading-relaxed">
                            {!hasOrder
                                ? "选择已生效的销售单后，此处自动核对本单可申请额度与开票档案。"
                                : !hasAccount
                                  ? "请在左侧选择本次开票对应的结算主体，以核对可用额度。"
                                  : "尚未获取到本单可申请额度。"}
                        </p>
                    </div>
                ) : (
                    <div className="space-y-3">
                        <div>
                            <div className="text-xs text-muted-foreground">
                                本单可申请
                            </div>
                            <div className="mt-1">
                                <MoneyValue
                                    value={amounts.available_amount}
                                    size="section"
                                    display="exact"
                                />
                            </div>
                        </div>

                        <dl className="divide-y divide-border/50 border-y border-border/60">
                            <QuotaLedgerRow
                                label="已开票"
                                value={amounts.invoiced_amount}
                            />
                            <QuotaLedgerRow
                                label="审批中占用"
                                value={amounts.pending_amount}
                            />
                            <QuotaLedgerRow
                                label="已批待开票"
                                value={amounts.approved_remaining_amount}
                            />
                        </dl>

                        <dl className="space-y-0.5">
                            <div className="flex items-baseline justify-between gap-3 py-1">
                                <dt className="text-xs text-muted-foreground">
                                    本次申请合计
                                </dt>
                                <dd className="text-right">
                                    {linesTotal ? (
                                        <div className="flex flex-col items-end">
                                            <MoneyValue
                                                value={linesTotal}
                                                display="exact"
                                                className="font-semibold"
                                            />
                                            {totalReadout ? (
                                                <span className="text-xs text-muted-foreground">
                                                    {totalReadout}
                                                </span>
                                            ) : null}
                                        </div>
                                    ) : (
                                        <span className="num text-muted-foreground">
                                            —
                                        </span>
                                    )}
                                </dd>
                            </div>
                            <QuotaLedgerRow
                                label={
                                    exceeds ? "超出可申请额度" : "提交后剩余"
                                }
                                value={exceeds ? overAmount : remaining}
                                emphasis="strong"
                                destructive={exceeds}
                            />
                        </dl>
                    </div>
                )}
            </section>

            {/* 区块 2：开票档案（自动带入） */}
            <section
                aria-label="开票档案"
                className="space-y-4 border-t border-border/60 pt-5"
            >
                <div className="flex items-baseline justify-between gap-2">
                    <h2 className="text-section-title font-semibold tracking-tight text-foreground">
                        开票档案
                    </h2>
                    <span className="text-xs text-muted-foreground">
                        自动带入 · 只读
                    </span>
                </div>

                {!amounts && !hasArchiveValues ? (
                    <div className="flex items-start gap-2.5 py-1 text-xs text-muted-foreground">
                        <Building2Icon
                            className="mt-0.5 size-4 shrink-0 text-muted-foreground/70"
                            aria-hidden="true"
                        />
                        <p className="leading-relaxed">
                            选定销售单及结算主体后，自动带入购方与销方开票抬头及纳税人识别号。
                        </p>
                    </div>
                ) : (
                    <div className="space-y-4">
                        <div className="space-y-2.5">
                            <h3 className="text-xs font-medium text-muted-foreground">
                                购方信息
                            </h3>
                            <dl className="space-y-2.5">
                                <ArchiveFieldRow
                                    id="invoice-request-title"
                                    copyId="invoice-request-copy-title"
                                    label="开票抬头"
                                    value={archive.invoiceTitle}
                                />
                                <ArchiveFieldRow
                                    id="invoice-request-tax-number"
                                    copyId="invoice-request-copy-tax-number"
                                    label="纳税人识别号"
                                    value={archive.taxNumber}
                                    numeric
                                />
                            </dl>
                        </div>

                        <div className="space-y-2.5 border-t border-border/50 pt-4">
                            <h3 className="text-xs font-medium text-muted-foreground">
                                销方信息
                            </h3>
                            <dl className="space-y-2.5">
                                <ArchiveFieldRow
                                    id="invoice-request-seller-name"
                                    copyId="invoice-request-copy-seller-name"
                                    label="开票主体名称"
                                    value={archive.sellerName}
                                />
                                <ArchiveFieldRow
                                    id="invoice-request-seller-tax-number"
                                    copyId="invoice-request-copy-seller-tax-number"
                                    label="主体纳税人识别号"
                                    value={archive.sellerTaxNumber}
                                    numeric
                                />
                            </dl>
                        </div>
                    </div>
                )}
            </section>
        </div>
    )
}

/**
 * 弹窗态紧凑摘要带：无灰底卡片套娃，以细线清晰呈现本单额度与购销方档案。
 */
export function InvoiceRequestDialogContext({
    isLoading,
    isError,
    onRetry,
    amounts,
    archive,
}: {
    isLoading: boolean
    isError: boolean
    onRetry: () => void
    amounts?: RequestAmounts
    archive: InvoiceArchiveSnapshot
}) {
    if (isError) {
        return (
            <Alert variant="destructive">
                <CircleAlertIcon aria-hidden="true" />
                <AlertTitle>销售应收与开票档案读取失败</AlertTitle>
                <AlertDescription className="flex flex-wrap items-center gap-2">
                    <span>请刷新重试后再提交。</span>
                    <Button
                        id="invoice-request-amounts-retry"
                        type="button"
                        variant="outline"
                        size="xs"
                        onClick={onRetry}
                    >
                        重新读取
                    </Button>
                </AlertDescription>
            </Alert>
        )
    }

    if (isLoading && !amounts) {
        return (
            <div
                className="space-y-2 border-y border-border/60 py-3"
                aria-busy="true"
            >
                <div className="h-5 w-48 animate-pulse rounded bg-muted" />
                <div className="grid gap-3 sm:grid-cols-2">
                    <div className="h-9 animate-pulse rounded bg-muted" />
                    <div className="h-9 animate-pulse rounded bg-muted" />
                </div>
            </div>
        )
    }

    return (
        <div className="space-y-3 border-y border-border/60 py-3.5">
            {amounts ? (
                <div className="flex flex-wrap items-baseline justify-between gap-x-6 gap-y-1.5 border-b border-border/50 pb-2.5">
                    <div className="flex items-baseline gap-2">
                        <span className="text-xs text-muted-foreground">
                            本单可申请
                        </span>
                        <MoneyValue
                            value={amounts.available_amount}
                            display="exact"
                            className="font-semibold"
                        />
                    </div>
                    <dl className="flex flex-wrap items-baseline gap-x-4 gap-y-1 text-xs text-muted-foreground">
                        <div className="flex items-baseline gap-1.5">
                            <dt>已开票</dt>
                            <dd>
                                <MoneyValue
                                    value={amounts.invoiced_amount}
                                    display="exact"
                                />
                            </dd>
                        </div>
                        <div className="flex items-baseline gap-1.5">
                            <dt>审批中</dt>
                            <dd>
                                <MoneyValue
                                    value={amounts.pending_amount}
                                    display="exact"
                                />
                            </dd>
                        </div>
                        <div className="flex items-baseline gap-1.5">
                            <dt>已批待开票</dt>
                            <dd>
                                <MoneyValue
                                    value={amounts.approved_remaining_amount}
                                    display="exact"
                                />
                            </dd>
                        </div>
                    </dl>
                </div>
            ) : null}

            <dl className="grid gap-x-6 gap-y-2.5 sm:grid-cols-2">
                <ArchiveFieldRow
                    id="invoice-request-title"
                    copyId="invoice-request-copy-title"
                    label="购方开票抬头"
                    value={archive.invoiceTitle}
                />
                <ArchiveFieldRow
                    id="invoice-request-tax-number"
                    copyId="invoice-request-copy-tax-number"
                    label="购方纳税人识别号"
                    value={archive.taxNumber}
                    numeric
                />
                <ArchiveFieldRow
                    id="invoice-request-seller-name"
                    copyId="invoice-request-copy-seller-name"
                    label="开票主体名称（销方）"
                    value={archive.sellerName}
                />
                <ArchiveFieldRow
                    id="invoice-request-seller-tax-number"
                    copyId="invoice-request-copy-seller-tax-number"
                    label="开票主体纳税人识别号"
                    value={archive.sellerTaxNumber}
                    numeric
                />
            </dl>
        </div>
    )
}
