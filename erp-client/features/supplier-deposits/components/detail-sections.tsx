"use client"

import Link from "next/link"
import { format } from "date-fns"
import { ArrowUpRightIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessStatusBadge,
    DetailWorkspaceSection,
    DetailWorkspaceSubTabs,
    MoneyValue,
    TableEmptyCell,
    TableIdentityCell,
    TableRowActions,
    TableTextCell,
    detailWorkspaceStyles,
} from "@/components/business"
import { Progress } from "@/components/ui/progress"
import type { StatusTone } from "@/components/ui/status-badge"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    compareDecimal,
    parseDecimal,
    subtractFixed,
} from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"

import { reservationRemainder } from "../lib/resolution"
import type {
    AccountDetail,
    DepositAuthorization,
    DepositLedgerEntry,
    DepositReconciliation,
    DepositReservation,
    RechargeRequest,
} from "../types"
import type { DepositAction } from "./command-dialog"
import { DepositResolutionEvidence } from "./resolution-evidence"
import type { DepositResolutionTarget } from "./resolution-dialog"

export type DepositDetailDialogTarget =
    | {
          action: DepositAction
          request?: RechargeRequest
          authorization?: DepositAuthorization
      }
    | { action: "pay"; request: RechargeRequest }
    | DepositResolutionTarget

const MONEY_OPTIONS = { maxScale: 2, outputScale: 2 } as const

export function displayDepositTime(value: number | undefined | null) {
    return value == null
        ? "未标注"
        : format(new Date(value * 1000), "yyyy-MM-dd HH:mm")
}

const RECHARGE_STATUS: Record<
    string,
    { label: string; tone: StatusTone; quiet?: boolean }
> = {
    draft: { label: "待批准", tone: "warning" },
    requested: { label: "待批准", tone: "warning" },
    pending: { label: "待批准", tone: "warning" },
    approved: { label: "待银行付款", tone: "info" },
    paid: { label: "待核对到账", tone: "info" },
    bank_paid: { label: "待核对到账", tone: "info" },
    received: { label: "已到账", tone: "success", quiet: true },
    reversed: { label: "已冲正", tone: "void" },
}

const EXPOSURE_STATUS: Record<
    DepositReservation["exposure"],
    { label: string; tone: StatusTone; quiet?: boolean }
> = {
    not_sent: { label: "尚未发送订单", tone: "neutral", quiet: true },
    possibly_sent: { label: "扣款结果待核实", tone: "destructive" },
    cleared: { label: "已核实无扣款风险", tone: "success", quiet: true },
}

const LEDGER_KIND: Record<
    DepositLedgerEntry["kind"],
    { label: string; tone: StatusTone; inflow: boolean }
> = {
    recharge: { label: "充值到账", tone: "success", inflow: true },
    return: { label: "账户返款", tone: "info", inflow: true },
    debit: { label: "订单扣款", tone: "neutral", inflow: false },
    recharge_reversal: { label: "充值冲正", tone: "warning", inflow: false },
}

function utilizationPercent(committed: string, limit: string): number {
    try {
        const limitNum = parseDecimal(limit, { maxScale: 2 }).unscaled
        if (limitNum <= BigInt(0)) return 0
        const committedNum = parseDecimal(committed, { maxScale: 2 }).unscaled
        if (committedNum <= BigInt(0)) return 0
        const pct = Number((committedNum * BigInt(100)) / limitNum)
        return Math.max(0, Math.min(100, pct))
    } catch {
        return 0
    }
}

function TableShell({
    children,
    empty,
    emptyTitle,
    emptyDescription,
}: {
    children: React.ReactNode
    empty: boolean
    emptyTitle: string
    emptyDescription?: string
}) {
    if (empty) {
        return (
            <BusinessEmptyState
                kind="no-data"
                title={emptyTitle}
                description={emptyDescription}
            />
        )
    }
    return (
        <div
            data-slot="detail-inline-table"
            className={cn(
                detailWorkspaceStyles.inlineTableWrapper,
                "overflow-x-auto",
            )}
        >
            <Table>{children}</Table>
        </div>
    )
}

export function DepositRechargesSection({
    requests,
    permitted,
    onOpenDialog,
    actions,
}: {
    requests: readonly RechargeRequest[]
    permitted: (action: string) => boolean
    onOpenDialog: (dialog: DepositDetailDialogTarget) => void
    actions?: React.ReactNode
}) {
    return (
        <DetailWorkspaceSection
            title="充值办理"
            subtitle={`共 ${requests.length} 条 · 按申请、审批、银行付款与到账核对分步推进`}
            width="full"
            actions={actions}
        >
            <TableShell
                empty={requests.length === 0}
                emptyTitle="暂无充值申请"
                emptyDescription="点击右上角「申请充值」发起预存款充值。"
            >
                <TableHeader>
                    <TableRow>
                        <TableHead>申请单号 / 用途</TableHead>
                        <TableHead data-align="end">申请金额</TableHead>
                        <TableHead>办理进度</TableHead>
                        <TableHead>批准及付款依据</TableHead>
                        <TableHead data-align="end">操作</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {requests.map((request) => {
                        const status = request.status.toLowerCase()
                        const statusMeta = RECHARGE_STATUS[status] ?? {
                            label: "需核对申请进度",
                            tone: "warning" as const,
                        }
                        const segment = toAutomationIdSegment(request.id)
                        const nextAction =
                            ["draft", "pending", "requested"].includes(
                                status,
                            ) && permitted("approve_recharge")
                                ? {
                                      id: `supplier-deposits-approve-${segment}`,
                                      label: "核对并批准",
                                      onClick: () =>
                                          onOpenDialog({
                                              action: "approve",
                                              request,
                                          }),
                                  }
                                : status === "approved" &&
                                    permitted("pay_recharge")
                                  ? {
                                        id: `supplier-deposits-pay-${segment}`,
                                        label: "登记银行付款",
                                        onClick: () =>
                                            onOpenDialog({
                                                action: "pay",
                                                request,
                                            }),
                                    }
                                  : ["paid", "bank_paid"].includes(status) &&
                                      permitted("receive_recharge")
                                    ? {
                                          id: `supplier-deposits-receive-${segment}`,
                                          label: "核对供应商到账",
                                          onClick: () =>
                                              onOpenDialog({
                                                  action: "receive",
                                                  request,
                                              }),
                                      }
                                    : null
                        return (
                            <TableRow key={request.id}>
                                <TableCell>
                                    <TableIdentityCell
                                        primary={
                                            <span className="num">
                                                {request.request_no ||
                                                    "充值申请"}
                                            </span>
                                        }
                                        primaryTitle={
                                            request.request_no || "充值申请"
                                        }
                                        secondary={request.reason}
                                        secondaryTitle={request.reason}
                                    />
                                </TableCell>
                                <TableCell data-align="end">
                                    <MoneyValue
                                        value={request.amount}
                                        display="exact"
                                        className="font-medium"
                                    />
                                </TableCell>
                                <TableCell>
                                    <BusinessStatusBadge
                                        context="list"
                                        label={statusMeta.label}
                                        tone={statusMeta.tone}
                                        appearance={
                                            statusMeta.quiet ? "quiet" : "badge"
                                        }
                                    />
                                </TableCell>
                                <TableCell>
                                    <div className="flex min-w-0 flex-col gap-0.5">
                                        <TableTextCell
                                            value={
                                                request.approval_reference ||
                                                "尚未批准"
                                            }
                                            className={
                                                request.approval_reference
                                                    ? undefined
                                                    : "text-muted-foreground"
                                            }
                                        />
                                        <span className="num text-xs text-muted-foreground">
                                            {request.bank_reference
                                                ? `银行流水：${request.bank_reference}`
                                                : request.supplier_payment_id
                                                  ? "银行付款已登记"
                                                  : "尚未登记银行付款"}
                                        </span>
                                    </div>
                                </TableCell>
                                <TableCell data-align="end">
                                    {nextAction ? (
                                        <TableRowActions
                                            moreId={`supplier-deposits-recharge-${segment}-more`}
                                            moreLabel="充值记录更多操作"
                                            actions={[nextAction]}
                                        />
                                    ) : (
                                        <TableEmptyCell />
                                    )}
                                </TableCell>
                            </TableRow>
                        )
                    })}
                </TableBody>
            </TableShell>
        </DetailWorkspaceSection>
    )
}

export function DepositReservationsAndUsagesSection({
    data,
    subTab,
    onSubTabChange,
    permitted,
    onOpenDialog,
}: {
    data: AccountDetail
    subTab: "reservations" | "usages"
    onSubTabChange: (next: "reservations" | "usages") => void
    permitted: (action: string) => boolean
    onOpenDialog: (dialog: DepositDetailDialogTarget) => void
}) {
    return (
        <DetailWorkspaceSection
            title="订单占用与扣款"
            subtitle={
                subTab === "reservations"
                    ? `共 ${data.reservations.length} 条 · 最近 100 条及全部仍有剩余占用的记录`
                    : `共 ${data.usages.length} 条 · 最近 100 条订单实际扣款与核销记录`
            }
            width="full"
            actions={
                <DetailWorkspaceSubTabs
                    className="pt-0"
                    value={subTab}
                    onValueChange={(next) =>
                        onSubTabChange(
                            next === "usages" ? "usages" : "reservations",
                        )
                    }
                    idPrefix="supplier-deposits-usage-subtab"
                    items={[
                        {
                            id: "reservations",
                            label: `订单占用 (${data.reservations.length})`,
                        },
                        {
                            id: "usages",
                            label: `扣款及核销 (${data.usages.length})`,
                        },
                    ]}
                />
            }
        >
            {subTab === "reservations" ? (
                <TableShell
                    empty={data.reservations.length === 0}
                    emptyTitle="暂无订单占用"
                    emptyDescription="采购付款申请批准使用预存款后，将在此追踪预算占用、扣款消耗与剩余释放。"
                >
                    <TableHeader>
                        <TableRow>
                            <TableHead>来源采购单 / 付款申请</TableHead>
                            <TableHead data-align="end">占用预算</TableHead>
                            <TableHead data-align="end">已扣款消耗</TableHead>
                            <TableHead data-align="end">已释放</TableHead>
                            <TableHead data-align="end">未使用占用</TableHead>
                            <TableHead>扣款核实进度</TableHead>
                            <TableHead data-align="end">操作</TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {data.reservations.map((reservation) => {
                            const remainder = reservationRemainder(reservation)
                            const hasRemainder =
                                compareDecimal(remainder, "0", 2) > 0
                            const segment = toAutomationIdSegment(
                                reservation.id,
                            )
                            const exposure = EXPOSURE_STATUS[
                                reservation.exposure
                            ] ?? {
                                label: "待核对",
                                tone: "warning" as const,
                            }
                            const primaryLabel =
                                reservation.purchase_order_no ||
                                "查看采购及付款依据"
                            const secondaryParts = [
                                reservation.payment_request_no
                                    ? `付款申请 ${reservation.payment_request_no}`
                                    : null,
                                `货款 ${reservation.basis.approved_goods_amount}`,
                                compareDecimal(
                                    reservation.basis.approved_freight,
                                    "0",
                                    2,
                                ) > 0
                                    ? `运费 ${reservation.basis.approved_freight}`
                                    : null,
                            ]
                                .filter(Boolean)
                                .join(" · ")

                            return (
                                <TableRow key={reservation.id}>
                                    <TableCell>
                                        <div className="flex min-w-0 flex-col gap-0.5">
                                            <Link
                                                id={`supplier-deposits-purchase-${segment}`}
                                                href={`/procurement/orders/${encodeURIComponent(reservation.basis.purchase_order_id)}`}
                                                className="num inline-flex items-center gap-1 font-medium text-primary underline-offset-4 hover:underline"
                                            >
                                                <span>{primaryLabel}</span>
                                                <ArrowUpRightIcon
                                                    className="size-3.5 shrink-0"
                                                    aria-hidden="true"
                                                />
                                            </Link>
                                            <span className="num text-xs text-muted-foreground">
                                                {secondaryParts}
                                            </span>
                                        </div>
                                    </TableCell>
                                    <TableCell data-align="end">
                                        <MoneyValue
                                            value={reservation.amount}
                                            display="exact"
                                        />
                                    </TableCell>
                                    <TableCell data-align="end">
                                        <MoneyValue
                                            value={reservation.consumed_amount}
                                            display="exact"
                                        />
                                    </TableCell>
                                    <TableCell data-align="end">
                                        <MoneyValue
                                            value={reservation.released_amount}
                                            display="exact"
                                        />
                                    </TableCell>
                                    <TableCell data-align="end">
                                        <MoneyValue
                                            value={remainder}
                                            display="exact"
                                            className={
                                                hasRemainder
                                                    ? "font-semibold"
                                                    : "text-muted-foreground"
                                            }
                                        />
                                    </TableCell>
                                    <TableCell>
                                        <BusinessStatusBadge
                                            context="list"
                                            label={exposure.label}
                                            tone={exposure.tone}
                                            appearance={
                                                exposure.quiet
                                                    ? "quiet"
                                                    : "badge"
                                            }
                                        />
                                    </TableCell>
                                    <TableCell data-align="end">
                                        {hasRemainder &&
                                        permitted("release") ? (
                                            <TableRowActions
                                                moreId={`supplier-deposits-release-${segment}-more`}
                                                moreLabel="占用记录更多操作"
                                                actions={[
                                                    {
                                                        id: `supplier-deposits-release-${segment}`,
                                                        label: "核实剩余占用",
                                                        onClick: () =>
                                                            onOpenDialog({
                                                                action: "release",
                                                                reservation,
                                                            }),
                                                    },
                                                ]}
                                            />
                                        ) : hasRemainder ? (
                                            <TableEmptyCell />
                                        ) : (
                                            <TableEmptyCell
                                                variant="label"
                                                label="无剩余占用"
                                            />
                                        )}
                                    </TableCell>
                                </TableRow>
                            )
                        })}
                    </TableBody>
                </TableShell>
            ) : (
                <TableShell
                    empty={data.usages.length === 0}
                    emptyTitle="暂无订单扣款及核销记录"
                >
                    <TableHeader>
                        <TableRow>
                            <TableHead>扣款时间 / 供应商订单</TableHead>
                            <TableHead data-align="end">实际扣款</TableHead>
                            <TableHead data-align="end">已核销</TableHead>
                            <TableHead data-align="end">已到账返款</TableHead>
                            <TableHead data-align="end">已冲回核销</TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {data.usages.map((usage) => {
                            const evidence = data.ledger.find(
                                (entry) => entry.id === usage.ledger_entry_id,
                            )?.evidence
                            return (
                                <TableRow key={usage.id}>
                                    <TableCell>
                                        <TableIdentityCell
                                            primary={
                                                <span className="num">
                                                    {displayDepositTime(
                                                        evidence?.occurred_at,
                                                    )}
                                                </span>
                                            }
                                            secondary={
                                                evidence?.external_order_id ||
                                                "原订单核实中"
                                            }
                                            numericSecondary
                                        />
                                    </TableCell>
                                    <TableCell data-align="end">
                                        <MoneyValue
                                            value={usage.amount}
                                            display="exact"
                                            className="font-medium"
                                        />
                                    </TableCell>
                                    <TableCell data-align="end">
                                        <MoneyValue
                                            value={usage.settled_amount}
                                            display="exact"
                                        />
                                    </TableCell>
                                    <TableCell data-align="end">
                                        <MoneyValue
                                            value={usage.returned_amount}
                                            display="exact"
                                        />
                                    </TableCell>
                                    <TableCell data-align="end">
                                        <MoneyValue
                                            value={
                                                usage.reversed_settlement_amount
                                            }
                                            display="exact"
                                        />
                                    </TableCell>
                                </TableRow>
                            )
                        })}
                    </TableBody>
                </TableShell>
            )}
        </DetailWorkspaceSection>
    )
}

export function DepositLedgerAndBatchesSection({
    data,
    subTab,
    onSubTabChange,
}: {
    data: AccountDetail
    subTab: "ledger" | "batches"
    onSubTabChange: (next: "ledger" | "batches") => void
}) {
    return (
        <DetailWorkspaceSection
            title="资金流水与批次"
            subtitle={
                subTab === "ledger"
                    ? `最近 ${data.ledger.length} 条 · 不可变资金明细（最多显示 100 条）`
                    : `最近 ${data.batches.length} 条 · 按到账顺序追踪每批资金的占用与消耗`
            }
            width="full"
            actions={
                <DetailWorkspaceSubTabs
                    className="pt-0"
                    value={subTab}
                    onValueChange={(next) =>
                        onSubTabChange(
                            next === "batches" ? "batches" : "ledger",
                        )
                    }
                    idPrefix="supplier-deposits-ledger-subtab"
                    items={[
                        {
                            id: "ledger",
                            label: `资金流水 (${data.ledger.length})`,
                        },
                        {
                            id: "batches",
                            label: `充值来源批次 (${data.batches.length})`,
                        },
                    ]}
                />
            }
        >
            {subTab === "ledger" ? (
                <TableShell
                    empty={data.ledger.length === 0}
                    emptyTitle="暂无资金流水"
                >
                    <TableHeader>
                        <TableRow>
                            <TableHead>发生时间</TableHead>
                            <TableHead>业务类型</TableHead>
                            <TableHead data-align="end">变动金额</TableHead>
                            <TableHead>供应商交易号</TableHead>
                            <TableHead>来源依据</TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {data.ledger.map((entry) => {
                            const kind = LEDGER_KIND[entry.kind] ?? {
                                label: "需核对资金类型",
                                tone: "warning" as const,
                                inflow: false,
                            }
                            return (
                                <TableRow key={entry.id}>
                                    <TableCell className="num">
                                        {displayDepositTime(
                                            entry.evidence.occurred_at,
                                        )}
                                    </TableCell>
                                    <TableCell>
                                        <BusinessStatusBadge
                                            context="list"
                                            label={kind.label}
                                            tone={kind.tone}
                                            appearance="quiet"
                                        />
                                    </TableCell>
                                    <TableCell data-align="end">
                                        <span
                                            className={cn(
                                                "num inline-flex items-baseline gap-0.5 font-medium",
                                                kind.inflow
                                                    ? "text-success-soft-foreground"
                                                    : "text-foreground",
                                            )}
                                        >
                                            <span>
                                                {kind.inflow ? "+" : "−"}
                                            </span>
                                            <MoneyValue
                                                value={entry.amount}
                                                display="exact"
                                            />
                                        </span>
                                    </TableCell>
                                    <TableCell className="num">
                                        <TableTextCell
                                            value={
                                                entry.evidence
                                                    .external_transaction_id
                                            }
                                            numeric
                                        />
                                    </TableCell>
                                    <TableCell className="max-w-72 whitespace-normal">
                                        <TableTextCell
                                            value={
                                                entry.evidence
                                                    .evidence_reference
                                            }
                                            clamp={2}
                                        />
                                    </TableCell>
                                </TableRow>
                            )
                        })}
                    </TableBody>
                </TableShell>
            ) : (
                <TableShell
                    empty={data.batches.length === 0}
                    emptyTitle="暂无充值来源批次"
                >
                    <TableHeader>
                        <TableRow>
                            <TableHead>到账时间 / 银行流水</TableHead>
                            <TableHead data-align="end">到账金额</TableHead>
                            <TableHead data-align="end">已占用</TableHead>
                            <TableHead data-align="end">累计使用</TableHead>
                            <TableHead data-align="end">已到账返款</TableHead>
                            <TableHead data-align="end">冲正</TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {data.batches.map((batch) => (
                            <TableRow key={batch.id}>
                                <TableCell>
                                    <TableIdentityCell
                                        primary={
                                            <span className="num">
                                                {displayDepositTime(
                                                    batch.evidence.received_at,
                                                )}
                                            </span>
                                        }
                                        secondary={
                                            batch.evidence.bank_reference
                                        }
                                        numericSecondary
                                    />
                                </TableCell>
                                <TableCell data-align="end">
                                    <MoneyValue
                                        value={batch.amount}
                                        display="exact"
                                        className="font-medium"
                                    />
                                </TableCell>
                                <TableCell data-align="end">
                                    <MoneyValue
                                        value={batch.reserved_amount}
                                        display="exact"
                                    />
                                </TableCell>
                                <TableCell data-align="end">
                                    <MoneyValue
                                        value={batch.consumed_amount}
                                        display="exact"
                                    />
                                </TableCell>
                                <TableCell data-align="end">
                                    <MoneyValue
                                        value={batch.returned_amount}
                                        display="exact"
                                    />
                                </TableCell>
                                <TableCell data-align="end">
                                    <MoneyValue
                                        value={batch.reversed_amount}
                                        display="exact"
                                    />
                                </TableCell>
                            </TableRow>
                        ))}
                    </TableBody>
                </TableShell>
            )}
        </DetailWorkspaceSection>
    )
}

export function DepositReconciliationsSection({
    data,
    permitted,
    onOpenDialog,
    actions,
}: {
    data: AccountDetail
    permitted: (action: string) => boolean
    onOpenDialog: (dialog: DepositDetailDialogTarget) => void
    actions?: React.ReactNode
}) {
    return (
        <DetailWorkspaceSection
            title="资金对账"
            subtitle={`共 ${data.reconciliations.length} 条 · 最近 100 条及全部未结案差异，历史待处理记录继续保留`}
            width="full"
            actions={actions}
        >
            <TableShell
                empty={data.reconciliations.length === 0}
                emptyTitle="暂无资金对账记录"
                emptyDescription="定期按同一截止时点核对本地余额与供应商平台余额，发现差异后可在此结案。"
            >
                <TableHeader>
                    <TableRow>
                        <TableHead>核对期间</TableHead>
                        <TableHead data-align="end">供应商余额</TableHead>
                        <TableHead data-align="end">本地余额</TableHead>
                        <TableHead data-align="end">余额差额</TableHead>
                        <TableHead>未核实交易</TableHead>
                        <TableHead>状态与核实依据</TableHead>
                        <TableHead data-align="end">操作</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {data.reconciliations.map(
                        (record: DepositReconciliation) => {
                            const diff = subtractFixed(
                                record.remote_balance,
                                record.local_balance_at_cutoff,
                                MONEY_OPTIONS,
                            )
                            const diffSign = compareDecimal(diff, "0", 2)
                            const balanced =
                                diffSign === 0 &&
                                record.unresolved_transaction_ids.length === 0
                            const segment = toAutomationIdSegment(record.id)
                            const statusBadge = record.resolved_by
                                ? {
                                      label: "已核实结案",
                                      tone: "success" as const,
                                      quiet: true,
                                  }
                                : balanced
                                  ? {
                                        label: "双方余额一致",
                                        tone: "success" as const,
                                        quiet: true,
                                    }
                                  : {
                                        label: "差异待核实",
                                        tone: "destructive" as const,
                                        quiet: false,
                                    }
                            const canResolve =
                                !record.resolved_by &&
                                permitted("resolve") &&
                                (data.account.reconciliation_required ||
                                    !balanced)
                            const resolveLabel =
                                data.account.reconciliation_required && balanced
                                    ? "核实后恢复使用"
                                    : "核实对账差异"

                            return (
                                <TableRow key={record.id}>
                                    <TableCell className="num">
                                        <div>
                                            {displayDepositTime(
                                                record.covered_from,
                                            )}
                                        </div>
                                        <div className="text-xs text-muted-foreground">
                                            至{" "}
                                            {displayDepositTime(
                                                record.covered_until,
                                            )}
                                        </div>
                                    </TableCell>
                                    <TableCell data-align="end">
                                        <MoneyValue
                                            value={record.remote_balance}
                                            display="exact"
                                        />
                                    </TableCell>
                                    <TableCell data-align="end">
                                        <MoneyValue
                                            value={
                                                record.local_balance_at_cutoff
                                            }
                                            display="exact"
                                        />
                                    </TableCell>
                                    <TableCell data-align="end">
                                        <MoneyValue
                                            value={diff}
                                            display="exact"
                                            className={
                                                diffSign !== 0
                                                    ? "font-semibold text-destructive"
                                                    : "text-muted-foreground"
                                            }
                                        />
                                    </TableCell>
                                    <TableCell className="max-w-52 whitespace-normal">
                                        {record.unresolved_transaction_ids
                                            .length > 0 ? (
                                            <span className="num text-xs text-destructive">
                                                {record.unresolved_transaction_ids.join(
                                                    "、",
                                                )}
                                            </span>
                                        ) : (
                                            <TableEmptyCell
                                                variant="label"
                                                label="无"
                                            />
                                        )}
                                    </TableCell>
                                    <TableCell className="max-w-64 whitespace-normal">
                                        <div className="flex flex-col items-start gap-1">
                                            <BusinessStatusBadge
                                                context="list"
                                                label={statusBadge.label}
                                                tone={statusBadge.tone}
                                                appearance={
                                                    statusBadge.quiet
                                                        ? "quiet"
                                                        : "badge"
                                                }
                                            />
                                            <span className="text-xs text-muted-foreground">
                                                {record.evidence_reference}
                                            </span>
                                            {record.resolution
                                                ?.evidence_asset_id ? (
                                                <DepositResolutionEvidence
                                                    reconciliationId={record.id}
                                                    assetId={
                                                        record.resolution
                                                            .evidence_asset_id
                                                    }
                                                />
                                            ) : null}
                                        </div>
                                    </TableCell>
                                    <TableCell data-align="end">
                                        {record.resolved_by ? (
                                            <TableEmptyCell
                                                variant="label"
                                                label="已结案"
                                            />
                                        ) : canResolve ? (
                                            <TableRowActions
                                                moreId={`supplier-deposits-resolve-${segment}-more`}
                                                moreLabel="对账记录更多操作"
                                                actions={[
                                                    {
                                                        id: `supplier-deposits-resolve-${segment}`,
                                                        label: resolveLabel,
                                                        onClick: () =>
                                                            onOpenDialog({
                                                                action: "resolve",
                                                                reconciliation:
                                                                    record,
                                                            }),
                                                    },
                                                ]}
                                            />
                                        ) : (
                                            <TableEmptyCell />
                                        )}
                                    </TableCell>
                                </TableRow>
                            )
                        },
                    )}
                </TableBody>
            </TableShell>
        </DetailWorkspaceSection>
    )
}

export function DepositAuthorizationsSection({
    authorizations,
    permitted,
    onOpenDialog,
    actions,
}: {
    authorizations: readonly DepositAuthorization[]
    permitted: (action: string) => boolean
    onOpenDialog: (dialog: DepositDetailDialogTarget) => void
    actions?: React.ReactNode
}) {
    const now = Date.now()
    return (
        <DetailWorkspaceSection
            title="余额执行授权"
            subtitle={`共 ${authorizations.length} 条 · 自动执行须同时满足有效期限、单次限额与累计限额`}
            width="full"
            actions={actions}
        >
            <TableShell
                empty={authorizations.length === 0}
                emptyTitle="暂无余额执行授权"
                emptyDescription="如需系统在采购付款审批通过后自动使用余额，请先登记财务授权。"
            >
                <TableHeader>
                    <TableRow>
                        <TableHead>授权方式 / 依据</TableHead>
                        <TableHead>有效期</TableHead>
                        <TableHead data-align="end">单次上限</TableHead>
                        <TableHead data-align="end">累计上限与已用</TableHead>
                        <TableHead>状态</TableHead>
                        <TableHead data-align="end">操作</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {authorizations.map((authorization) => {
                        const segment = toAutomationIdSegment(authorization.id)
                        const statusMeta = authorization.revoked
                            ? {
                                  label: "已撤销",
                                  tone: "void" as const,
                                  quiet: false,
                              }
                            : authorization.valid_until * 1000 <= now
                              ? {
                                    label: "已到期",
                                    tone: "neutral" as const,
                                    quiet: true,
                                }
                              : authorization.valid_from * 1000 > now
                                ? {
                                      label: "尚未生效",
                                      tone: "info" as const,
                                      quiet: false,
                                  }
                                : {
                                      label: "有效",
                                      tone: "success" as const,
                                      quiet: true,
                                  }
                        const percent = utilizationPercent(
                            authorization.committed_amount,
                            authorization.cumulative_limit,
                        )

                        return (
                            <TableRow key={authorization.id}>
                                <TableCell>
                                    <TableIdentityCell
                                        primary={
                                            authorization.mode?.type ===
                                            "manual"
                                                ? "本次人工执行"
                                                : "自动执行"
                                        }
                                        secondary={
                                            authorization.approval_reference
                                        }
                                        secondaryTitle={
                                            authorization.approval_reference
                                        }
                                    />
                                </TableCell>
                                <TableCell className="num">
                                    <div>
                                        {displayDepositTime(
                                            authorization.valid_from,
                                        )}
                                    </div>
                                    <div className="text-xs text-muted-foreground">
                                        至{" "}
                                        {displayDepositTime(
                                            authorization.valid_until,
                                        )}
                                    </div>
                                </TableCell>
                                <TableCell data-align="end">
                                    <MoneyValue
                                        value={
                                            authorization.per_execution_limit
                                        }
                                        display="exact"
                                    />
                                </TableCell>
                                <TableCell data-align="end">
                                    <div className="ml-auto flex w-44 flex-col items-end gap-1">
                                        <MoneyValue
                                            value={
                                                authorization.cumulative_limit
                                            }
                                            display="exact"
                                            className="font-medium"
                                        />
                                        <Progress
                                            value={percent}
                                            className="h-1.5 w-full"
                                        />
                                        <span className="num text-xs text-muted-foreground">
                                            已用{" "}
                                            <MoneyValue
                                                value={
                                                    authorization.committed_amount
                                                }
                                                display="exact"
                                            />{" "}
                                            ({percent}%)
                                        </span>
                                    </div>
                                </TableCell>
                                <TableCell>
                                    <BusinessStatusBadge
                                        context="list"
                                        label={statusMeta.label}
                                        tone={statusMeta.tone}
                                        appearance={
                                            statusMeta.quiet ? "quiet" : "badge"
                                        }
                                    />
                                </TableCell>
                                <TableCell data-align="end">
                                    {permitted("revoke") &&
                                    !authorization.revoked ? (
                                        <TableRowActions
                                            moreId={`supplier-deposits-revoke-${segment}-more`}
                                            moreLabel="授权记录更多操作"
                                            actions={[
                                                {
                                                    id: `supplier-deposits-revoke-${segment}`,
                                                    label: "撤销授权",
                                                    destructive: true,
                                                    onClick: () =>
                                                        onOpenDialog({
                                                            action: "revoke",
                                                            authorization,
                                                        }),
                                                },
                                            ]}
                                        />
                                    ) : (
                                        <TableEmptyCell />
                                    )}
                                </TableCell>
                            </TableRow>
                        )
                    })}
                </TableBody>
            </TableShell>
        </DetailWorkspaceSection>
    )
}
