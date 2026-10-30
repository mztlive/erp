"use client"

import { useState } from "react"
import Link from "next/link"
import { format } from "date-fns"
import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessTableFrame,
    MetricItem,
    MetricStrip,
    MoneyValue,
    PageScaffold,
} from "@/components/business"
import { ListWorkspaceHeader } from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import { Alert, AlertTitle, AlertDescription } from "@/components/ui/alert"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { useParams } from "@/lib/navigation"
import { hasPermission } from "@/lib/permissions"
import { compareDecimal, subtractFixed, sumFixed } from "@/lib/fixed-decimal"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { useDepositAccount } from "../hooks/queries"
import {
    balancedReconciliations,
    reservationRemainder,
} from "../lib/resolution"
import type {
    AccountDetail,
    RechargeRequest,
    DepositAuthorization,
} from "../types"
import { DepositCommandDialog, type DepositAction } from "./command-dialog"
import { RechargePaymentDialog } from "./payment-dialog"
import { DepositResolutionEvidence } from "./resolution-evidence"
import {
    DepositResolutionDialog,
    type DepositResolutionTarget,
} from "./resolution-dialog"

export function displayTime(value: number | undefined | null) {
    return value == null
        ? "未标注"
        : format(new Date(value * 1000), "yyyy-MM-dd HH:mm")
}
const moneyOptions = { maxScale: 2, outputScale: 2 } as const
const exposureLabels = {
    not_sent: "尚未发送订单",
    possibly_sent: "扣款结果待核实",
    cleared: "已核实无后续扣款风险",
}
const ledgerLabels = {
    recharge: "充值到账",
    debit: "订单扣款",
    return: "账户返款",
    recharge_reversal: "充值冲正",
}
const statusLabels: Record<string, string> = {
    draft: "待批准",
    approved: "待银行付款",
    paid: "待核对到账",
    bank_paid: "待核对到账",
    received: "已到账",
    reversed: "已冲正",
    requested: "待批准",
    pending: "待批准",
}
function Section({
    title,
    columns,
    children,
    empty,
    count,
    description,
}: {
    title: string
    columns: string[]
    children: React.ReactNode
    empty?: boolean
    count: number
    description?: string
}) {
    return (
        <BusinessTableFrame
            title={title}
            showHeader
            description={
                description ?? `最近 ${count} 条 · 每类最多显示 100 条`
            }
            table={
                empty ? (
                    <BusinessEmptyState kind="no-data" title={`暂无${title}`} />
                ) : (
                    <Table>
                        <TableHeader>
                            <TableRow>
                                {columns.map((column) => (
                                    <TableHead key={column}>{column}</TableHead>
                                ))}
                            </TableRow>
                        </TableHeader>
                        <TableBody>{children}</TableBody>
                    </Table>
                )
            }
        />
    )
}

type ActiveDialog =
    | {
          action: DepositAction
          request?: RechargeRequest
          authorization?: DepositAuthorization
      }
    | { action: "pay"; request: RechargeRequest }
    | DepositResolutionTarget

export function SupplierDepositDetailPage() {
    const params = useParams<{ accountId: string }>()
    const profile = useAccountProfileQuery()
    const permissions = profile.data?.permissions
    const canDetail = hasPermission(permissions, "supplier_deposit:detail")
    const query = useDepositAccount(params.accountId, canDetail)
    const [dialog, setDialog] = useState<ActiveDialog>()
    const permitted = (action: string) =>
        hasPermission(permissions, `supplier_deposit:${action}`)
    const data = query.data
    return (
        <PageScaffold density="compact">
            <ListWorkspaceHeader
                eyebrow="财务 / 供应商预存款"
                title={data?.supplier_name ?? "预存款账户"}
                description={
                    data
                        ? `${data.payer_party_name} · 人民币${data.external_account_label ? ` · ${data.external_account_label}` : ""}`
                        : undefined
                }
            >
                <Button
                    id="supplier-deposits-detail-back"
                    variant="outline"
                    render={<Link href="/finance/supplier-deposits" />}
                >
                    返回账户列表
                </Button>
                <Button
                    id="supplier-deposits-detail-refresh"
                    variant="outline"
                    disabled={query.isFetching || !canDetail}
                    onClick={() => void query.refetch()}
                >
                    刷新账户
                </Button>
                <Button
                    id="supplier-deposits-recharge-request"
                    disabled={!permitted("recharge_request") || !data}
                    onClick={() => setDialog({ action: "recharge_request" })}
                >
                    申请充值
                </Button>
            </ListWorkspaceHeader>
            {profile.isPending ? (
                <div className="h-40 animate-pulse bg-muted" aria-busy />
            ) : profile.isError ? (
                <BusinessFailureState
                    idPrefix="supplier-deposits-detail-profile"
                    error={profile.error}
                    onRetry={() => void profile.refetch()}
                />
            ) : !canDetail ? (
                <BusinessFailureState
                    kind="permission"
                    description="当前账号不能查看该预存款账户。"
                />
            ) : query.isError ? (
                <BusinessFailureState
                    idPrefix="supplier-deposits-detail"
                    error={query.error}
                    onRetry={() => void query.refetch()}
                />
            ) : !data ? (
                <div className="h-40 animate-pulse bg-muted" aria-busy />
            ) : (
                <>
                    <AccountSummary data={data} />
                    <div className="flex flex-wrap justify-end gap-2">
                        <Button
                            id="supplier-deposits-authorize"
                            variant="outline"
                            disabled={!permitted("authorize")}
                            onClick={() => setDialog({ action: "authorize" })}
                        >
                            授权自动余额执行
                        </Button>
                        <Button
                            id="supplier-deposits-reconcile"
                            variant="outline"
                            disabled={!permitted("reconcile")}
                            onClick={() => setDialog({ action: "reconcile" })}
                        >
                            登记资金对账
                        </Button>
                    </div>
                    <Section
                        title="充值办理"
                        columns={[
                            "申请用途",
                            "申请金额",
                            "办理进度",
                            "批准及付款依据",
                            "下一步",
                        ]}
                        count={data.recharge_requests.length}
                        empty={!data.recharge_requests.length}
                    >
                        {data.recharge_requests.map((request) => {
                            const status = request.status.toLowerCase()
                            const segment = toAutomationIdSegment(request.id)
                            return (
                                <TableRow key={request.id}>
                                    <TableCell>
                                        <p>
                                            {request.request_no || "充值申请"}
                                        </p>
                                        <p className="text-xs text-muted-foreground whitespace-normal">
                                            {request.reason}
                                        </p>
                                    </TableCell>
                                    <TableCell>
                                        <MoneyValue
                                            value={request.amount}
                                            display="exact"
                                        />
                                    </TableCell>
                                    <TableCell>
                                        <Badge variant="secondary">
                                            {statusLabels[status] ??
                                                "需核对申请进度"}
                                        </Badge>
                                    </TableCell>
                                    <TableCell>
                                        <p>
                                            {request.approval_reference ||
                                                "尚未批准"}
                                        </p>
                                        <p className="text-xs text-muted-foreground">
                                            {request.bank_reference
                                                ? `银行流水：${request.bank_reference}`
                                                : request.supplier_payment_id
                                                  ? "银行付款已登记"
                                                  : "尚未登记银行付款"}
                                        </p>
                                    </TableCell>
                                    <TableCell>
                                        {[
                                            "draft",
                                            "pending",
                                            "requested",
                                        ].includes(status) ? (
                                            <Button
                                                id={`supplier-deposits-approve-${segment}`}
                                                variant="outline"
                                                size="sm"
                                                disabled={
                                                    !permitted(
                                                        "approve_recharge",
                                                    )
                                                }
                                                onClick={() =>
                                                    setDialog({
                                                        action: "approve",
                                                        request,
                                                    })
                                                }
                                            >
                                                核对并批准
                                            </Button>
                                        ) : status === "approved" ? (
                                            <Button
                                                id={`supplier-deposits-pay-${segment}`}
                                                variant="outline"
                                                size="sm"
                                                disabled={
                                                    !permitted("pay_recharge")
                                                }
                                                onClick={() =>
                                                    setDialog({
                                                        action: "pay",
                                                        request,
                                                    })
                                                }
                                            >
                                                登记银行付款
                                            </Button>
                                        ) : ["paid", "bank_paid"].includes(
                                              status,
                                          ) ? (
                                            <Button
                                                id={`supplier-deposits-receive-${segment}`}
                                                variant="outline"
                                                size="sm"
                                                disabled={
                                                    !permitted(
                                                        "receive_recharge",
                                                    )
                                                }
                                                onClick={() =>
                                                    setDialog({
                                                        action: "receive",
                                                        request,
                                                    })
                                                }
                                            >
                                                核对供应商到账
                                            </Button>
                                        ) : (
                                            "—"
                                        )}
                                    </TableCell>
                                </TableRow>
                            )
                        })}
                    </Section>
                    <Section
                        title="订单占用"
                        columns={[
                            "来源采购",
                            "占用预算",
                            "已扣款消耗",
                            "已释放",
                            "未使用占用",
                            "进度",
                            "下一步",
                        ]}
                        count={data.reservations.length}
                        description={`共 ${data.reservations.length} 条 · 最近 100 条及全部仍有剩余占用的记录`}
                        empty={!data.reservations.length}
                    >
                        {data.reservations.map((reservation) => (
                            <TableRow key={reservation.id}>
                                <TableCell>
                                    <Button
                                        id={`supplier-deposits-purchase-${toAutomationIdSegment(reservation.id)}`}
                                        variant="link"
                                        render={
                                            <Link
                                                href={`/procurement/orders/${encodeURIComponent(reservation.basis.purchase_order_id)}`}
                                            />
                                        }
                                    >
                                        查看采购及付款依据
                                    </Button>
                                </TableCell>
                                <TableCell>
                                    <MoneyValue
                                        value={reservation.amount}
                                        display="exact"
                                    />
                                </TableCell>
                                <TableCell>
                                    <MoneyValue
                                        value={reservation.consumed_amount}
                                        display="exact"
                                    />
                                </TableCell>
                                <TableCell>
                                    <MoneyValue
                                        value={reservation.released_amount}
                                        display="exact"
                                    />
                                </TableCell>
                                <TableCell>
                                    <MoneyValue
                                        value={reservationRemainder(
                                            reservation,
                                        )}
                                        display="exact"
                                    />
                                </TableCell>
                                <TableCell>
                                    {exposureLabels[reservation.exposure] ??
                                        "待核对"}
                                </TableCell>
                                <TableCell>
                                    {compareDecimal(
                                        reservationRemainder(reservation),
                                        "0",
                                        2,
                                    ) > 0 ? (
                                        <Button
                                            id={`supplier-deposits-release-${toAutomationIdSegment(reservation.id)}`}
                                            variant="outline"
                                            size="sm"
                                            disabled={!permitted("release")}
                                            onClick={() =>
                                                setDialog({
                                                    action: "release",
                                                    reservation,
                                                })
                                            }
                                        >
                                            核实剩余占用
                                        </Button>
                                    ) : (
                                        "无剩余占用"
                                    )}
                                </TableCell>
                            </TableRow>
                        ))}
                    </Section>
                    <Section
                        title="订单扣款及核销"
                        columns={[
                            "扣款时间 / 供应商订单",
                            "实际扣款",
                            "已核销",
                            "已到账返款",
                            "已冲回核销",
                        ]}
                        count={data.usages.length}
                        empty={!data.usages.length}
                    >
                        {data.usages.map((usage) => {
                            const evidence = data.ledger.find(
                                (entry) => entry.id === usage.ledger_entry_id,
                            )?.evidence
                            return (
                                <TableRow key={usage.id}>
                                    <TableCell>
                                        <p>
                                            {displayTime(evidence?.occurred_at)}
                                        </p>
                                        <p className="text-xs text-muted-foreground">
                                            {evidence?.external_order_id ||
                                                "原订单调查中"}
                                        </p>
                                    </TableCell>
                                    <TableCell>
                                        <MoneyValue
                                            value={usage.amount}
                                            display="exact"
                                        />
                                    </TableCell>
                                    <TableCell>
                                        <MoneyValue
                                            value={usage.settled_amount}
                                            display="exact"
                                        />
                                    </TableCell>
                                    <TableCell>
                                        <MoneyValue
                                            value={usage.returned_amount}
                                            display="exact"
                                        />
                                    </TableCell>
                                    <TableCell>
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
                    </Section>
                    <Section
                        title="余额执行授权"
                        columns={[
                            "授权依据",
                            "有效期",
                            "单次上限",
                            "累计上限 / 已使用额度",
                            "状态",
                            "操作",
                        ]}
                        count={data.authorizations.length}
                        empty={!data.authorizations.length}
                    >
                        {data.authorizations.map((authorization) => (
                            <TableRow key={authorization.id}>
                                <TableCell>
                                    <p>
                                        {authorization.mode?.type === "manual"
                                            ? "本次人工执行"
                                            : "自动执行"}
                                    </p>
                                    <p className="text-xs text-muted-foreground">
                                        {authorization.approval_reference}
                                    </p>
                                </TableCell>
                                <TableCell>
                                    {displayTime(authorization.valid_from)}
                                    <br />至{" "}
                                    {displayTime(authorization.valid_until)}
                                </TableCell>
                                <TableCell>
                                    <MoneyValue
                                        value={
                                            authorization.per_execution_limit
                                        }
                                        display="exact"
                                    />
                                </TableCell>
                                <TableCell>
                                    <MoneyValue
                                        value={authorization.cumulative_limit}
                                        display="exact"
                                    />
                                    <p className="text-xs">
                                        已使用{" "}
                                        <MoneyValue
                                            value={
                                                authorization.committed_amount
                                            }
                                            display="exact"
                                        />
                                    </p>
                                </TableCell>
                                <TableCell>
                                    {authorization.revoked
                                        ? "已撤销"
                                        : authorization.valid_until * 1000 <=
                                            Date.now()
                                          ? "已到期"
                                          : authorization.valid_from * 1000 >
                                              Date.now()
                                            ? "尚未生效"
                                            : "有效"}
                                </TableCell>
                                <TableCell>
                                    <Button
                                        id={`supplier-deposits-revoke-${toAutomationIdSegment(authorization.id)}`}
                                        variant="outline"
                                        size="sm"
                                        disabled={
                                            authorization.revoked ||
                                            !permitted("revoke")
                                        }
                                        onClick={() =>
                                            setDialog({
                                                action: "revoke",
                                                authorization,
                                            })
                                        }
                                    >
                                        撤销授权
                                    </Button>
                                </TableCell>
                            </TableRow>
                        ))}
                    </Section>
                    <Section
                        title="资金流水"
                        columns={[
                            "发生时间",
                            "业务类型",
                            "金额",
                            "供应商交易号",
                            "来源依据",
                        ]}
                        count={data.ledger.length}
                        empty={!data.ledger.length}
                    >
                        {data.ledger.map((entry) => (
                            <TableRow key={entry.id}>
                                <TableCell>
                                    {displayTime(entry.evidence.occurred_at)}
                                </TableCell>
                                <TableCell>
                                    {ledgerLabels[entry.kind] ??
                                        "需核对资金类型"}
                                </TableCell>
                                <TableCell>
                                    <MoneyValue
                                        value={entry.amount}
                                        display="exact"
                                    />
                                </TableCell>
                                <TableCell>
                                    {entry.evidence.external_transaction_id}
                                </TableCell>
                                <TableCell className="max-w-64 whitespace-normal">
                                    {entry.evidence.evidence_reference}
                                </TableCell>
                            </TableRow>
                        ))}
                    </Section>
                    <Section
                        title="充值来源批次"
                        columns={[
                            "到账时间 / 银行流水",
                            "到账金额",
                            "占用",
                            "累计使用",
                            "已到账返款",
                            "冲正",
                        ]}
                        count={data.batches.length}
                        empty={!data.batches.length}
                    >
                        {data.batches.map((batch) => (
                            <TableRow key={batch.id}>
                                <TableCell>
                                    <p>
                                        {displayTime(
                                            batch.evidence.received_at,
                                        )}
                                    </p>
                                    <p className="text-xs text-muted-foreground">
                                        {batch.evidence.bank_reference}
                                    </p>
                                </TableCell>
                                {[
                                    batch.amount,
                                    batch.reserved_amount,
                                    batch.consumed_amount,
                                    batch.returned_amount,
                                    batch.reversed_amount,
                                ].map((amount, column) => (
                                    <TableCell key={column}>
                                        <MoneyValue
                                            value={amount}
                                            display="exact"
                                        />
                                    </TableCell>
                                ))}
                            </TableRow>
                        ))}
                    </Section>
                    <Section
                        title="资金对账"
                        columns={[
                            "核对范围",
                            "供应商余额",
                            "同一时点本地余额",
                            "未核实交易",
                            "依据 / 处理结果",
                            "下一步",
                        ]}
                        count={data.reconciliations.length}
                        description={`共 ${data.reconciliations.length} 条 · 最近 100 条及全部未结案差异，历史待处理记录继续保留`}
                        empty={!data.reconciliations.length}
                    >
                        {data.reconciliations.map((record) => (
                            <TableRow key={record.id}>
                                <TableCell>
                                    {displayTime(record.covered_from)}
                                    <br />至 {displayTime(record.covered_until)}
                                </TableCell>
                                <TableCell>
                                    <MoneyValue
                                        value={record.remote_balance}
                                        display="exact"
                                    />
                                </TableCell>
                                <TableCell>
                                    <MoneyValue
                                        value={record.local_balance_at_cutoff}
                                        display="exact"
                                    />
                                </TableCell>
                                <TableCell className="max-w-56 whitespace-normal">
                                    {record.unresolved_transaction_ids.join(
                                        "、",
                                    ) || "无未核实交易"}
                                </TableCell>
                                <TableCell className="max-w-64 whitespace-normal">
                                    <p>{record.evidence_reference}</p>
                                    <p className="text-xs text-muted-foreground">
                                        {record.resolved_by
                                            ? "差异已核实结案"
                                            : compareDecimal(
                                                    record.remote_balance,
                                                    record.local_balance_at_cutoff,
                                                    2,
                                                ) === 0 &&
                                                !record
                                                    .unresolved_transaction_ids
                                                    .length
                                              ? "双方余额一致，无未核实交易"
                                              : "差异待核实"}
                                    </p>
                                    {record.resolution?.evidence_asset_id ? (
                                        <DepositResolutionEvidence
                                            reconciliationId={record.id}
                                            assetId={
                                                record.resolution
                                                    .evidence_asset_id
                                            }
                                        />
                                    ) : null}
                                </TableCell>
                                <TableCell>
                                    {record.resolved_by ? (
                                        "已核实结案"
                                    ) : (
                                        <Button
                                            id={`supplier-deposits-resolve-${toAutomationIdSegment(record.id)}`}
                                            variant="outline"
                                            size="sm"
                                            disabled={
                                                !permitted("resolve") ||
                                                (!data.account
                                                    .reconciliation_required &&
                                                    compareDecimal(
                                                        record.remote_balance,
                                                        record.local_balance_at_cutoff,
                                                        2,
                                                    ) === 0 &&
                                                    !record
                                                        .unresolved_transaction_ids
                                                        .length)
                                            }
                                            onClick={() =>
                                                setDialog({
                                                    action: "resolve",
                                                    reconciliation: record,
                                                })
                                            }
                                        >
                                            {data.account
                                                .reconciliation_required &&
                                            compareDecimal(
                                                record.remote_balance,
                                                record.local_balance_at_cutoff,
                                                2,
                                            ) === 0 &&
                                            !record.unresolved_transaction_ids
                                                .length
                                                ? "核实后恢复使用"
                                                : "核实对账差异"}
                                        </Button>
                                    )}
                                </TableCell>
                            </TableRow>
                        ))}
                    </Section>
                </>
            )}
            {dialog &&
                data &&
                (dialog.action === "pay" ? (
                    <RechargePaymentDialog
                        account={data}
                        request={dialog.request}
                        onOpenChange={(open) => {
                            if (!open) setDialog(undefined)
                        }}
                    />
                ) : dialog.action === "resolve" ||
                  dialog.action === "release" ? (
                    <DepositResolutionDialog
                        account={data}
                        target={dialog}
                        onOpenChange={(open) => {
                            if (!open) setDialog(undefined)
                        }}
                    />
                ) : (
                    <DepositCommandDialog
                        action={dialog.action}
                        account={data}
                        request={dialog.request}
                        authorization={dialog.authorization}
                        open
                        onOpenChange={(open) => {
                            if (!open) setDialog(undefined)
                        }}
                    />
                ))}
        </PageScaffold>
    )
}
function AccountSummary({ data }: { data: AccountDetail }) {
    const pending = sumFixed(
        data.reservations
            .filter((item) => item.exposure === "possibly_sent")
            .map((item) =>
                subtractFixed(
                    subtractFixed(
                        item.amount,
                        item.consumed_amount,
                        moneyOptions,
                    ),
                    item.released_amount,
                    moneyOptions,
                ),
            ),
        moneyOptions,
    )
    return (
        <>
            <MetricStrip columns={4}>
                <MetricItem
                    label="账面余额"
                    value={
                        <MoneyValue
                            value={data.account.book_balance}
                            display="exact"
                        />
                    }
                />
                <MetricItem
                    label="已占用"
                    value={
                        <MoneyValue
                            value={data.account.reserved_amount}
                            display="exact"
                        />
                    }
                />
                <MetricItem
                    label="可用余额"
                    value={
                        <MoneyValue
                            value={data.available_amount}
                            display="exact"
                        />
                    }
                />
                <MetricItem
                    label="最近记录待核实占用"
                    value={<MoneyValue value={pending} display="exact" />}
                    detail="已包含在占用中，不重复扣减可用余额"
                />
            </MetricStrip>
            {data.account.reconciliation_required ? (
                <Alert variant="destructive">
                    <AlertTitle>账户资金待核对，已暂停新增使用</AlertTitle>
                    <AlertDescription>
                        请核对下方资金交易、余额采样范围及原订单，并从原对账记录办理“核实对账差异”。
                        {balancedReconciliations(data.reconciliations).length
                            ? "请以最新平衡对账和附件说明差异处理结果；服务端核实全部未决资金后才恢复新增使用。"
                            : "请先登记双方余额一致且无未核实交易的最新对账，再上传核实附件办理结案。"}
                        原占用继续保护可能发生的扣款，既有订单调查和真实资金记录继续处理。
                    </AlertDescription>
                </Alert>
            ) : null}
            <p className="text-sm text-muted-foreground">
                其他不可用额度：
                <MoneyValue
                    value={data.account.unavailable_amount}
                    display="exact"
                />
                。可用余额已扣除占用及该额度。充值只登记一次现金支出，订单余额使用单独核销，不重复登记银行付款。
            </p>
        </>
    )
}
