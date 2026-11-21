"use client"

import { useState, type ReactNode } from "react"
import { ArrowRightIcon } from "lucide-react"
import {
    WorkspaceTaskPageSlot,
    useWorkspaceTaskPage,
} from "@/components/business/workspace-task-pane"
import { MoneyValue } from "@/components/business"
import {
    ObjectSectionTabs,
    ObjectSectionTabsPanel,
} from "@/components/business/object-section-tabs"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import { compareDecimal } from "@/lib/fixed-decimal"
import { requestStatusLabel, requestStatusTone } from "../lib/display"
import type { PaymentRequest } from "../types"
import { RequestOrderDetails } from "./request-order-details"
import { RequestMaterials } from "./request-materials"
import { RequestPaymentHistory } from "./request-payment-history"
import { RequestSubmissionHistory } from "./request-submission-history"
import { RequestReviewContent } from "./request-review-content"

function Field({ label, children }: { label: string; children: ReactNode }) {
    return (
        <div className="grid min-w-0 grid-cols-[6rem_minmax(0,1fr)] items-baseline gap-x-4 gap-y-1">
            <dt className="text-muted-foreground">{label}</dt>
            <dd className="min-w-0 whitespace-pre-wrap break-words">
                {children}
            </dd>
        </div>
    )
}

function PaymentInformation({
    request,
    account,
}: {
    request: PaymentRequest
    account: ReactNode
}) {
    const fullPage = useWorkspaceTaskPage()
    return (
        <section className="space-y-5" aria-label="申请与收款信息">
            {!fullPage && (
                <div className="space-y-3">
                    <h3 className="font-semibold">申请信息</h3>
                    <dl className="grid gap-x-8 gap-y-3 text-body-compact @min-[640px]/payment-detail:grid-cols-2">
                        <Field label="申请人">{request.applicant_name}</Field>
                        <Field label="希望付款日">
                            <span className="num">
                                {request.desired_payment_date}
                            </span>
                        </Field>
                        <Field label="付款主体">
                            {request.our_company_name}
                        </Field>
                        <Field label="申请说明">
                            {request.description || "—"}
                        </Field>
                    </dl>
                </div>
            )}
            {request.execution_method.type === "supplier_deposit" ? (
                <div
                    className={
                        fullPage ? "space-y-3" : "space-y-3 border-t pt-5"
                    }
                >
                    <h3 className="font-semibold">付款方式：供应商预存款</h3>
                    <p className="text-body-compact">
                        人民币预存款
                        {request.deposit_account?.identity.external_account_ref
                            ? ` · 账户标识 ****${request.deposit_account.identity.external_account_ref.slice(-4)}`
                            : "账户已冻结"}
                    </p>
                    <p className="text-xs text-muted-foreground">
                        使用提交时已冻结的同主体、同供应商预存款账户。批准后按采购执行组占用，实际扣款核实后才核销，不登记本次银行付款。
                    </p>
                </div>
            ) : (
                <div
                    className={
                        fullPage ? "space-y-3" : "space-y-3 border-t pt-5"
                    }
                >
                    <h3 className="font-semibold">收款信息</h3>
                    <dl className="grid gap-x-8 gap-y-3 text-body-compact @min-[640px]/payment-detail:grid-cols-2">
                        <Field label="收款户名">
                            {request.payment_recipient?.account_name ??
                                "未记录"}
                        </Field>
                        <Field label="开户行">
                            {request.payment_recipient?.bank_name ?? "未记录"}
                        </Field>
                        <Field label="收款账号">{account}</Field>
                    </dl>
                </div>
            )}
        </section>
    )
}

export function RequestDetailContent({
    request,
    account,
    approval,
    embedded,
    showHistory = true,
    submissionVersion,
    workItemId,
}: {
    request: PaymentRequest
    account: ReactNode
    approval: ReactNode
    embedded: boolean
    showHistory?: boolean
    submissionVersion?: number
    workItemId?: string
}) {
    const fullPage = useWorkspaceTaskPage()
    const [section, setSection] = useState("details")
    if (fullPage)
        return (
            <RequestReviewContent
                request={request}
                account={account}
                approval={approval}
                embedded={embedded}
                showHistory={showHistory}
                submissionVersion={submissionVersion}
                workItemId={workItemId}
            />
        )
    const showClosed =
        request.status === "closed" ||
        compareDecimal(request.closed_amount, "0", 4) > 0
    return (
        <div className="@container/payment-detail min-w-0 space-y-5">
            <WorkspaceTaskPageSlot slot="aside">
                <section
                    className="space-y-3 rounded-lg bg-muted/40 px-5 py-4"
                    aria-label="付款申请摘要"
                >
                    <div className="flex flex-wrap items-center justify-between gap-2">
                        <span className="text-xs text-muted-foreground">
                            {submissionVersion
                                ? `第 ${submissionVersion} 次提交时的付款情况`
                                : "当前付款情况"}
                        </span>
                        {!submissionVersion && (
                            <StatusBadge
                                label={requestStatusLabel[request.status]}
                                tone={requestStatusTone[request.status]}
                            />
                        )}
                    </div>
                    <dl className="grid grid-cols-1 gap-4 sm:grid-cols-3 xl:grid-cols-1 2xl:grid-cols-3">
                        {(
                            [
                                ["申请金额", request.total_amount],
                                ["已付金额", request.paid_amount],
                                ["待付金额", request.unpaid_amount],
                            ] as const
                        ).map(([label, value]) => (
                            <div key={label} className="min-w-0 space-y-1">
                                <dt
                                    className="text-xs text-muted-foreground"
                                    title={
                                        label === "已付金额"
                                            ? "扣除已冲正金额后的有效付款"
                                            : undefined
                                    }
                                >
                                    {label}
                                </dt>
                                <dd className="text-body-compact font-medium">
                                    <MoneyValue value={value} />
                                </dd>
                            </div>
                        ))}
                    </dl>
                    {showClosed && (
                        <div className="space-y-1 border-t pt-3 text-body-compact">
                            <p>
                                已关闭未付：
                                <MoneyValue value={request.closed_amount} />
                            </p>
                            {request.status === "closed" && (
                                <p className="text-xs text-muted-foreground">
                                    本申请已终止后续付款：
                                    {request.close_reason || "未记录原因"}
                                    。已付记录继续保留，后续即使冲正也不会恢复付款资格。
                                </p>
                            )}
                        </div>
                    )}
                </section>
            </WorkspaceTaskPageSlot>
            <ObjectSectionTabs
                id="ppr-detail-sections"
                value={section}
                onValueChange={setSection}
                listLabel="付款申请分区"
                items={[
                    { id: "details", label: "申请内容" },
                    { id: "materials", label: "审核材料" },
                    ...(showHistory
                        ? [
                              { id: "payments", label: "付款记录" },
                              { id: "history", label: "审批记录" },
                          ]
                        : []),
                ]}
            >
                <ObjectSectionTabsPanel value="details" className="space-y-6">
                    <PaymentInformation request={request} account={account} />
                    <div className="border-t pt-5">
                        <RequestOrderDetails
                            request={request}
                            compact={embedded}
                        />
                    </div>
                    <div className="flex flex-wrap items-center justify-between gap-3 border-t pt-4">
                        <p className="text-xs text-muted-foreground">
                            {request.status === "draft"
                                ? "采购单、销售单及附件"
                                : "采购单、销售单及附件保留提交时版本"}
                        </p>
                        <Button
                            id="ppr-detail-open-materials"
                            variant="ghost"
                            size="sm"
                            onClick={() => {
                                setSection("materials")
                                document
                                    .getElementById(
                                        "ppr-detail-sections-tab-materials",
                                    )
                                    ?.focus()
                            }}
                        >
                            查看审核材料
                            <ArrowRightIcon />
                        </Button>
                    </div>
                </ObjectSectionTabsPanel>
                <ObjectSectionTabsPanel
                    value="materials"
                    className="[&>section]:border-t-0 [&>section]:pt-0"
                >
                    <RequestMaterials
                        request={request}
                        workItemId={workItemId}
                        submissionVersion={submissionVersion}
                    />
                </ObjectSectionTabsPanel>
                {showHistory && (
                    <>
                        <ObjectSectionTabsPanel
                            value="payments"
                            className="[&>section]:border-t-0 [&>section]:pt-0"
                        >
                            <RequestPaymentHistory
                                request={request}
                                workItemId={workItemId}
                            />
                        </ObjectSectionTabsPanel>
                        <ObjectSectionTabsPanel
                            value="history"
                            className="[&>section:first-child]:border-t-0 [&>section:first-child]:pt-0"
                        >
                            {approval ?? (
                                <p className="text-xs text-muted-foreground">
                                    尚未提交财务审核。
                                </p>
                            )}
                            {!embedded && (
                                <RequestSubmissionHistory request={request} />
                            )}
                        </ObjectSectionTabsPanel>
                    </>
                )}
            </ObjectSectionTabs>
        </div>
    )
}
