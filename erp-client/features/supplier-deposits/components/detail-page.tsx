"use client"

import { useState } from "react"
import {
    ArrowRightIcon,
    PlusIcon,
    RefreshCwIcon,
    ScaleIcon,
    ShieldCheckIcon,
} from "lucide-react"

import {
    BusinessFailureState,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspaceMeta,
    DetailWorkspaceMetricItem,
    DetailWorkspaceMetrics,
    DetailWorkspacePage,
    DetailWorkspaceSkeleton,
    DetailWorkspaceTabs,
    MoneyValue,
    detailWorkspaceStyles,
    type DetailWorkspaceTabItem,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { compareDecimal, subtractFixed, sumFixed } from "@/lib/fixed-decimal"
import { useParams } from "@/lib/navigation"
import { hasPermission } from "@/lib/permissions"

import { useDepositAccount } from "../hooks/queries"
import {
    balancedReconciliations,
    reservationRemainder,
} from "../lib/resolution"
import type { AccountDetail } from "../types"
import { DepositCommandDialog } from "./command-dialog"
import {
    DepositAuthorizationsSection,
    DepositLedgerAndBatchesSection,
    DepositRechargesSection,
    DepositReconciliationsSection,
    DepositReservationsAndUsagesSection,
    displayDepositTime,
    type DepositDetailDialogTarget,
} from "./detail-sections"
import { RechargePaymentDialog } from "./payment-dialog"
import { DepositResolutionDialog } from "./resolution-dialog"

export { displayDepositTime as displayTime }

type DepositTab =
    | "recharges"
    | "reservations"
    | "ledger"
    | "reconciliations"
    | "authorizations"

const MONEY_OPTIONS = { maxScale: 2, outputScale: 2 } as const

function TabCount({
    count,
    tone = "neutral",
}: {
    count: number
    tone?: "neutral" | "warning" | "destructive"
}) {
    if (count <= 0) return null
    return (
        <span
            className={
                tone === "destructive"
                    ? "num ml-1.5 rounded-sm bg-destructive-soft px-1.5 text-tiny leading-4 font-semibold text-destructive-soft-foreground"
                    : tone === "warning"
                      ? "num ml-1.5 rounded-sm bg-warning-soft px-1.5 text-tiny leading-4 font-semibold text-warning-soft-foreground"
                      : "num ml-1.5 text-xs text-muted-foreground/80"
            }
        >
            {count}
        </span>
    )
}

export function SupplierDepositDetailPage() {
    const params = useParams<{ accountId: string }>()
    const profile = useAccountProfileQuery()
    const permissions = profile.data?.permissions
    const canDetail = hasPermission(permissions, "supplier_deposit:detail")
    const query = useDepositAccount(params.accountId, canDetail)
    const [dialog, setDialog] = useState<DepositDetailDialogTarget>()
    const [activeTab, setActiveTab] = useState<DepositTab>("recharges")
    const [usageSubTab, setUsageSubTab] = useState<"reservations" | "usages">(
        "reservations",
    )
    const [ledgerSubTab, setLedgerSubTab] = useState<"ledger" | "batches">(
        "ledger",
    )

    const permitted = (action: string) =>
        hasPermission(permissions, `supplier_deposit:${action}`)
    const actionPermission: Partial<
        Record<DepositDetailDialogTarget["action"], string>
    > = {
        approve: "approve_recharge",
        receive: "receive_recharge",
        pay: "pay_recharge",
    }

    const data = query.data

    if (profile.isPending || (canDetail && query.isPending && !data)) {
        return (
            <DetailWorkspaceSkeleton
                title="预存款账户"
                description="正在读取账户余额与资金记录…"
                summary
            />
        )
    }

    if (profile.isError) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome
                    title="预存款账户"
                    back={{
                        id: "supplier-deposits-detail-back",
                        label: "返回账户列表",
                        href: "/finance/supplier-deposits",
                    }}
                />
                <DetailWorkspaceBody>
                    <div className="py-6">
                        <BusinessFailureState
                            idPrefix="supplier-deposits-detail-profile"
                            error={profile.error}
                            onRetry={() => void profile.refetch()}
                        />
                    </div>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    if (!canDetail) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome
                    title="预存款账户"
                    back={{
                        id: "supplier-deposits-detail-back",
                        label: "返回账户列表",
                        href: "/finance/supplier-deposits",
                    }}
                />
                <DetailWorkspaceBody>
                    <div className="py-6">
                        <BusinessFailureState
                            kind="permission"
                            description="当前账号不能查看该预存款账户。"
                        />
                    </div>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    if (query.isError || !data) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome
                    title="预存款账户"
                    back={{
                        id: "supplier-deposits-detail-back",
                        label: "返回账户列表",
                        href: "/finance/supplier-deposits",
                    }}
                />
                <DetailWorkspaceBody>
                    <div className="py-6">
                        <BusinessFailureState
                            idPrefix="supplier-deposits-detail"
                            error={query.error}
                            onRetry={() => void query.refetch()}
                        />
                    </div>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    const pendingRechargeCount = data.recharge_requests.filter((item) =>
        [
            "draft",
            "pending",
            "requested",
            "approved",
            "paid",
            "bank_paid",
        ].includes(item.status.toLowerCase()),
    ).length
    const openReservationCount = data.reservations.filter(
        (item) =>
            compareDecimal(reservationRemainder(item), "0", 2) > 0 ||
            item.exposure === "possibly_sent",
    ).length
    const unresolvedReconciliationCount = data.reconciliations.filter(
        (record) =>
            !record.resolved_by &&
            (compareDecimal(
                record.remote_balance,
                record.local_balance_at_cutoff,
                2,
            ) !== 0 ||
                record.unresolved_transaction_ids.length > 0),
    ).length
    const now = Date.now()
    const activeAuthCount = data.authorizations.filter(
        (auth) =>
            !auth.revoked &&
            auth.valid_from * 1000 <= now &&
            auth.valid_until * 1000 > now,
    ).length

    const tabItems: DetailWorkspaceTabItem[] = [
        {
            id: "recharges",
            label: "充值办理",
            badge: (
                <TabCount
                    count={pendingRechargeCount}
                    tone={pendingRechargeCount > 0 ? "warning" : "neutral"}
                />
            ),
        },
        {
            id: "reservations",
            label: "订单占用与扣款",
            badge: (
                <TabCount
                    count={openReservationCount}
                    tone={
                        data.reservations.some(
                            (r) => r.exposure === "possibly_sent",
                        )
                            ? "destructive"
                            : "neutral"
                    }
                />
            ),
        },
        {
            id: "ledger",
            label: "资金流水与批次",
            badge: <TabCount count={data.ledger.length} />,
        },
        {
            id: "reconciliations",
            label: "资金对账",
            badge: (
                <TabCount
                    count={
                        unresolvedReconciliationCount ||
                        (data.account.reconciliation_required ? 1 : 0)
                    }
                    tone={
                        data.account.reconciliation_required ||
                        unresolvedReconciliationCount > 0
                            ? "destructive"
                            : "neutral"
                    }
                />
            ),
        },
        {
            id: "authorizations",
            label: "执行授权",
            badge: <TabCount count={activeAuthCount} />,
        },
    ]

    const primaryStatus = !data.account.enabled
        ? { label: "已停用", tone: "neutral" as const }
        : data.account.reconciliation_required
          ? { label: "待核对（已暂停使用）", tone: "destructive" as const }
          : { label: "可使用", tone: "success" as const }

    return (
        <DetailWorkspacePage>
            <DetailWorkspaceChrome
                title={data.supplier_name}
                back={{
                    id: "supplier-deposits-detail-back",
                    label: "返回账户列表",
                    href: "/finance/supplier-deposits",
                }}
                primaryStatus={primaryStatus}
                meta={
                    <DetailWorkspaceMeta
                        items={[
                            `我方付款主体：${data.payer_party_name}`,
                            "币种：人民币",
                            data.external_account_label,
                        ]}
                    />
                }
                secondaryActions={
                    <>
                        <LoadingButton
                            id="supplier-deposits-detail-refresh"
                            type="button"
                            variant="ghost"
                            size="sm"
                            loading={query.isFetching}
                            className={detailWorkspaceStyles.quietButton}
                            onClick={() => void query.refetch()}
                        >
                            {!query.isFetching ? (
                                <RefreshCwIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                            ) : null}
                            刷新账户
                        </LoadingButton>
                        {permitted("authorize") ? (
                            <Button
                                id="supplier-deposits-authorize"
                                type="button"
                                variant="ghost"
                                size="sm"
                                className={detailWorkspaceStyles.quietButton}
                                onClick={() =>
                                    setDialog({ action: "authorize" })
                                }
                            >
                                <ShieldCheckIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                授权自动余额执行
                            </Button>
                        ) : null}
                        {permitted("reconcile") ? (
                            <Button
                                id="supplier-deposits-reconcile"
                                type="button"
                                variant="ghost"
                                size="sm"
                                className={detailWorkspaceStyles.quietButton}
                                onClick={() =>
                                    setDialog({ action: "reconcile" })
                                }
                            >
                                <ScaleIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                登记资金对账
                            </Button>
                        ) : null}
                    </>
                }
                primaryAction={
                    permitted("recharge_request") ? (
                        <Button
                            id="supplier-deposits-recharge-request"
                            type="button"
                            size="sm"
                            className="px-3.5"
                            onClick={() =>
                                setDialog({ action: "recharge_request" })
                            }
                        >
                            <PlusIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            申请充值
                        </Button>
                    ) : undefined
                }
                tabs={
                    <DetailWorkspaceTabs
                        value={activeTab}
                        onValueChange={(next) =>
                            setActiveTab(next as DepositTab)
                        }
                        items={tabItems}
                        ariaLabel="预存款账户分区"
                        tabIdPrefix="supplier-deposits-detail-tab"
                    />
                }
            />

            <DetailWorkspaceBody
                summary={<AccountSummaryMetrics data={data} />}
                alerts={
                    data.account.reconciliation_required ? (
                        <Alert variant="destructive">
                            <AlertTitle>
                                账户资金待核对，已暂停新增使用
                            </AlertTitle>
                            <AlertDescription className="flex flex-wrap items-center justify-between gap-3">
                                <span className="min-w-0 flex-1">
                                    {balancedReconciliations(
                                        data.reconciliations,
                                    ).length
                                        ? "请前往「资金对账」提交双方余额一致的对账记录与核实凭证；系统确认无待处理扣款后自动恢复账户使用。"
                                        : "请先「登记资金对账」形成双方余额一致且无未核实交易的最新记录，再上传核实凭证办理结案。"}
                                </span>
                                {activeTab !== "reconciliations" ? (
                                    <Button
                                        id="supplier-deposits-goto-reconciliations"
                                        type="button"
                                        variant="outline"
                                        size="sm"
                                        onClick={() =>
                                            setActiveTab("reconciliations")
                                        }
                                    >
                                        前往资金对账处理
                                        <ArrowRightIcon
                                            data-icon="inline-end"
                                            aria-hidden="true"
                                        />
                                    </Button>
                                ) : null}
                            </AlertDescription>
                        </Alert>
                    ) : null
                }
            >
                {activeTab === "recharges" ? (
                    <DepositRechargesSection
                        requests={data.recharge_requests}
                        permitted={permitted}
                        onOpenDialog={setDialog}
                    />
                ) : activeTab === "reservations" ? (
                    <DepositReservationsAndUsagesSection
                        data={data}
                        subTab={usageSubTab}
                        onSubTabChange={setUsageSubTab}
                        permitted={permitted}
                        onOpenDialog={setDialog}
                    />
                ) : activeTab === "ledger" ? (
                    <DepositLedgerAndBatchesSection
                        data={data}
                        subTab={ledgerSubTab}
                        onSubTabChange={setLedgerSubTab}
                    />
                ) : activeTab === "reconciliations" ? (
                    <DepositReconciliationsSection
                        data={data}
                        permitted={permitted}
                        onOpenDialog={setDialog}
                    />
                ) : (
                    <DepositAuthorizationsSection
                        authorizations={data.authorizations}
                        permitted={permitted}
                        onOpenDialog={setDialog}
                    />
                )}
            </DetailWorkspaceBody>

            {dialog &&
                permitted(actionPermission[dialog.action] ?? dialog.action) &&
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
        </DetailWorkspacePage>
    )
}

function AccountSummaryMetrics({ data }: { data: AccountDetail }) {
    const pendingExposure = sumFixed(
        data.reservations
            .filter((item) => item.exposure === "possibly_sent")
            .map((item) =>
                subtractFixed(
                    subtractFixed(
                        item.amount,
                        item.consumed_amount,
                        MONEY_OPTIONS,
                    ),
                    item.released_amount,
                    MONEY_OPTIONS,
                ),
            ),
        MONEY_OPTIONS,
    )
    const hasPendingExposure = compareDecimal(pendingExposure, "0", 2) > 0
    const hasUnavailable =
        compareDecimal(data.account.unavailable_amount, "0", 2) > 0

    return (
        <DetailWorkspaceMetrics
            columns={4}
            variant="bar"
            ariaLabel="预存款余额摘要"
        >
            <DetailWorkspaceMetricItem
                label="可用余额"
                emphasis="primary"
                value={
                    <MoneyValue value={data.available_amount} display="exact" />
                }
                detail="已扣除占用及其他不可用额度"
            />
            <DetailWorkspaceMetricItem
                label="账面余额"
                value={
                    <MoneyValue
                        value={data.account.book_balance}
                        display="exact"
                    />
                }
                detail="充值仅记一次现金支出，订单使用独立核销"
            />
            <DetailWorkspaceMetricItem
                label="已占用"
                value={
                    <MoneyValue
                        value={data.account.reserved_amount}
                        display="exact"
                    />
                }
                detail={
                    hasPendingExposure ? (
                        <span className="text-destructive">
                            含结果待确认冻结{" "}
                            <MoneyValue
                                value={pendingExposure}
                                display="exact"
                            />
                        </span>
                    ) : (
                        "无结果待确认冻结金额"
                    )
                }
            />
            <DetailWorkspaceMetricItem
                label="其他不可用额度"
                value={
                    <MoneyValue
                        value={data.account.unavailable_amount}
                        display="exact"
                        className={
                            hasUnavailable
                                ? "text-warning-soft-foreground"
                                : "text-muted-foreground"
                        }
                    />
                }
                detail="不含已占用；可用余额已扣除该额度"
            />
        </DetailWorkspaceMetrics>
    )
}
