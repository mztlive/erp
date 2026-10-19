"use client"
import { useEffect, useMemo, useRef, useState } from "react"
import Link from "next/link"
import { ArrowUpRightIcon } from "lucide-react"
import { cn } from "@/lib/utils"
import { usePathname, useSearchParams } from "next/navigation"
import {
    BusinessStatusBadge,
    MoneyValue,
    WorkspaceTaskFooter,
} from "@/components/business"
import {
    Popover,
    PopoverContent,
    PopoverTitle,
    PopoverTrigger,
} from "@/components/ui/popover"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { RequestDraftDialog } from "@/features/purchase-payment-requests/components/request-draft-dialog"
import { getErrorMessage } from "@/lib/api"
import { hasPermission } from "@/lib/permissions"
import { compareDecimal } from "@/lib/fixed-decimal"
import { usePaymentArrangement } from "../hooks/use-payment-arrangement"
import {
    purchasePaymentRequestStatusLabel,
    purchasePaymentRequestStatusTone,
} from "../lib/purchase-payment-request-display"
import { PurchasePaymentFollowUp } from "./purchase-payment-follow-up"

const titles = {
    inactive: "尚未形成付款安排",
    available: "采购付款待安排",
    blocked: "采购付款暂时受阻",
    pending: "付款申请已提交",
    settled: "采购应付已结清",
}

export function PurchasePaymentArrangement({
    purchaseOrderId,
    purchaseOrderNo,
    costMasked = false,
    workItemId,
    onTaskCompleted,
}: {
    purchaseOrderId: string
    purchaseOrderNo?: string
    costMasked?: boolean
    workItemId?: string
    onTaskCompleted?: (id: string) => void
}) {
    const query = usePaymentArrangement(purchaseOrderId)
    const profile = useAccountProfileQuery()
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const returnTo = useMemo(() => {
        const params = new URLSearchParams(searchParams.toString())
        params.delete("returnTo")
        const queryString = params.toString()
        return `${pathname}${queryString ? `?${queryString}` : ""}`
    }, [pathname, searchParams])
    const [followUpOpen, setFollowUpOpen] = useState(false)
    const [requestOpen, setRequestOpen] = useState(false)
    const allRequestsParams = new URLSearchParams({
        purchase_order_id: purchaseOrderId,
    })
    if (purchaseOrderNo)
        allRequestsParams.set("purchase_order_label", purchaseOrderNo)
    const allRequestsHref = `/procurement/payment-requests?${allRequestsParams.toString()}`
    const notified = useRef(false)
    const arrangement = query.data
    // 关闭申请结果弹窗后才按服务端余额状态推进队列；部分申请和跟进仍保留责任。
    useEffect(() => {
        if (
            !workItemId ||
            requestOpen ||
            !arrangement ||
            arrangement.task_id === workItemId ||
            query.isFetching ||
            query.isError ||
            notified.current
        )
            return
        if (
            !(["pending", "settled", "inactive"] as const).some(
                (state) => state === arrangement.state,
            )
        )
            return
        notified.current = true
        onTaskCompleted?.(workItemId)
    }, [
        arrangement,
        query.isFetching,
        query.isError,
        workItemId,
        requestOpen,
        onTaskCompleted,
    ])
    if (!arrangement && query.isPending)
        return (
            <p role="status" className="text-sm text-muted-foreground">
                正在读取付款安排…
            </p>
        )
    if (query.isError && !requestOpen)
        return (
            <div role="alert" className="rounded-lg border p-4 text-sm">
                {getErrorMessage(query.error, "付款安排读取失败")}
                <Button
                    id="purchase-payment-arrangement-retry"
                    variant="link"
                    onClick={() => void query.refetch()}
                >
                    重新读取
                </Button>
            </div>
        )
    if (!arrangement || (arrangement.state === "inactive" && !requestOpen))
        return null
    const canApply =
        hasPermission(
            profile.data?.permissions,
            "purchase_payment_request:create",
        ) &&
        hasPermission(
            profile.data?.permissions,
            "purchase_payment_request:submit",
        )
    const canOpenRequest = hasPermission(
        profile.data?.permissions,
        "purchase_payment_request:detail",
    )
    const canListRequests = hasPermission(
        profile.data?.permissions,
        "purchase_payment_request:list",
    )
    const money = (value: string, prominent = false) =>
        costMasked ? (
            "•••"
        ) : (
            <MoneyValue value={value} size={prominent ? "hero" : "inherit"} />
        )
    const followUp =
        arrangement.follow_up_at == null
            ? null
            : new Date(arrangement.follow_up_at * 1000).toLocaleDateString(
                  "zh-CN",
                  { timeZone: "Asia/Shanghai" },
              )
    const hasBalance =
        arrangement.state === "available" || arrangement.state === "blocked"
    const hasPrepaymentGap =
        arrangement.prepayment_gap != null &&
        compareDecimal(arrangement.prepayment_gap, "0", 2) > 0
    return (
        <section
            aria-label="采购付款安排"
            className={cn(
                "rounded-lg border bg-card p-5 sm:p-6",
                workItemId && "rounded-none border-0 bg-transparent",
            )}
        >
            <div className="flex flex-wrap items-center justify-between gap-3">
                <h2 className="text-sm font-semibold">
                    {workItemId && arrangement.state === "available"
                        ? "本次付款安排"
                        : titles[arrangement.state]}
                </h2>
                {workItemId && (
                    <Button
                        id="purchase-payment-arrangement-order"
                        variant="link"
                        size="sm"
                        render={
                            <Link
                                href={`/procurement/orders/${encodeURIComponent(purchaseOrderId)}`}
                            />
                        }
                    >
                        查看采购单
                        {arrangement.state === "blocked" ? "并处理依据" : ""}
                        <ArrowUpRightIcon aria-hidden="true" />
                    </Button>
                )}
            </div>
            <div className="mt-5 rounded-lg bg-muted/30 p-5">
                <div className="flex flex-wrap items-start justify-between gap-4">
                    <div className="min-w-0 space-y-2">
                        <p className="text-sm text-muted-foreground">
                            {hasPrepaymentGap
                                ? "履约前还需付款"
                                : arrangement.state === "settled"
                                  ? "有效已付"
                                  : "可申请金额"}
                        </p>
                        <div>
                            {money(
                                hasPrepaymentGap
                                    ? arrangement.prepayment_gap!
                                    : arrangement.state === "settled"
                                      ? arrangement.paid_amount
                                      : arrangement.available_amount,
                                true,
                            )}
                        </div>
                    </div>
                    <div className="space-y-1 text-sm">
                        <p className="text-xs text-muted-foreground">
                            付款条件
                        </p>
                        <p className="font-medium">
                            {arrangement.payment_term_label || "待核对"}
                        </p>
                    </div>
                </div>
                <p className="mt-3 text-xs leading-5 text-muted-foreground">
                    {hasPrepaymentGap
                        ? "按付款条件计算的实付缺口，审批中的申请不计入已付。"
                        : "采购审批已通过，付款申请须经财务审核后由出纳执行。"}
                </p>
            </div>
            <dl className="mt-5 grid grid-cols-1 gap-4 sm:grid-cols-3 sm:gap-6">
                {[
                    {
                        label: "可申请金额",
                        value: arrangement.available_amount,
                    },
                    { label: "申请占用", value: arrangement.reserved_amount },
                    { label: "有效已付", value: arrangement.paid_amount },
                ].map(({ label, value }) => (
                    <div key={label} className="min-w-0 space-y-1.5">
                        <dt className="text-xs text-muted-foreground">
                            {label}
                        </dt>
                        <dd className="text-lg">{money(value)}</dd>
                    </div>
                ))}
            </dl>
            <p className="mt-3 text-xs leading-5 text-muted-foreground">
                可申请金额为剩余应付扣除有效申请占用后的余额；申请占用不代表已付款。
            </p>
            {arrangement.payment_blocked && (
                <p
                    role="alert"
                    className="mt-4 rounded-lg bg-destructive/5 p-3 text-sm leading-6 text-destructive"
                >
                    销售变更影响尚未处理，本采购暂停付款申请和付款。请先处理采购依据。
                </p>
            )}
            {arrangement.state === "pending" && (
                <p className="mt-2 text-sm">
                    剩余应付已由有效申请覆盖，请跟踪下方审核和付款进度。
                </p>
            )}
            {hasBalance && (
                <div className="mt-5 border-t border-border/60 pt-5 text-sm leading-6">
                    {followUp && (
                        <p className="mb-1 font-medium">下次跟进：{followUp}</p>
                    )}
                    <p className="text-muted-foreground">
                        核对金额后申请付款，暂不申请可安排跟进。
                    </p>
                    <p className="mt-1 text-xs text-muted-foreground">
                        周期结算按约定对账后申请；预付款后的尾款金额和日期需另行确认。
                    </p>
                </div>
            )}
            <div className="mt-6 space-y-4 border-t border-border/60 pt-5 text-sm">
                <div className="flex items-center justify-between gap-3">
                    <h3 className="font-semibold">审核与付款进度</h3>
                    {canListRequests && arrangement.requests.length > 0 ? (
                        <Button
                            id="purchase-payment-arrangement-requests"
                            variant="link"
                            size="sm"
                            render={<Link href={allRequestsHref} />}
                        >
                            全部付款申请（{arrangement.requests.length}）
                        </Button>
                    ) : (
                        <span className="text-xs text-muted-foreground">
                            {arrangement.requests.length} 笔申请
                        </span>
                    )}
                </div>
                {arrangement.requests.length === 0 && (
                    <p className="rounded-lg bg-muted/20 px-4 py-5 text-sm text-muted-foreground">
                        暂无付款申请，提交后可在此查看审核与付款进度。
                    </p>
                )}
                {arrangement.requests.map((request) => (
                    <div
                        key={request.request_id}
                        className="flex flex-wrap items-center gap-x-4 gap-y-3 rounded-lg border border-border/60 p-4"
                    >
                        {canOpenRequest ? (
                            <Link
                                id={`purchase-payment-request-${request.request_id}`}
                                href={`/procurement/payment-requests/${encodeURIComponent(request.request_id)}?returnTo=${encodeURIComponent(returnTo)}`}
                                className="underline underline-offset-4"
                            >
                                {request.request_no}
                            </Link>
                        ) : (
                            <span className="num">{request.request_no}</span>
                        )}
                        <BusinessStatusBadge
                            context="list"
                            label={
                                purchasePaymentRequestStatusLabel[
                                    request.status
                                ]
                            }
                            tone={
                                purchasePaymentRequestStatusTone[request.status]
                            }
                        />
                        <span>本单申请 {money(request.requested_amount)}</span>
                        <span>本单已付 {money(request.paid_amount)}</span>
                        <span>希望付款日 {request.desired_payment_date}</span>
                    </div>
                ))}
            </div>
            {canApply &&
                (arrangement.state === "available" ||
                    (hasBalance && arrangement.can_schedule)) && (
                    <WorkspaceTaskFooter inline={!workItemId}>
                        <div
                            className={cn(
                                "flex flex-wrap items-center justify-end gap-2",
                                !workItemId &&
                                    "mt-5 border-t border-border/60 pt-4",
                            )}
                        >
                            {hasBalance && arrangement.can_schedule && (
                                <Popover
                                    open={followUpOpen}
                                    onOpenChange={setFollowUpOpen}
                                >
                                    <PopoverTrigger
                                        id="purchase-payment-arrangement-schedule"
                                        render={<Button variant="outline" />}
                                    >
                                        {followUp ? "调整跟进日期" : "安排跟进"}
                                    </PopoverTrigger>
                                    <PopoverContent
                                        side="top"
                                        align="end"
                                        sideOffset={8}
                                        className="w-96 max-w-[calc(100vw-2rem)]"
                                    >
                                        <PopoverTitle>
                                            安排下次跟进
                                        </PopoverTitle>
                                        {followUpOpen && (
                                            <PurchasePaymentFollowUp
                                                key={`${arrangement.task_id}:${arrangement.task_version}`}
                                                purchaseOrderId={
                                                    purchaseOrderId
                                                }
                                                arrangement={arrangement}
                                                onSaved={() =>
                                                    setFollowUpOpen(false)
                                                }
                                            />
                                        )}
                                    </PopoverContent>
                                </Popover>
                            )}
                            {arrangement.state === "available" && (
                                <Button
                                    id="purchase-payment-arrangement-apply"
                                    onClick={() => setRequestOpen(true)}
                                >
                                    申请采购付款
                                </Button>
                            )}
                        </div>
                    </WorkspaceTaskFooter>
                )}
            {requestOpen ? (
                <RequestDraftDialog
                    key={`payment-request-${purchaseOrderId}`}
                    open
                    onOpenChange={setRequestOpen}
                    fixedPurchaseOrderId={purchaseOrderId}
                />
            ) : null}
        </section>
    )
}
