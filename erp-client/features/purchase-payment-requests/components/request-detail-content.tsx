"use client"

import { useState, type ReactNode } from "react"
import { ArrowRightIcon } from "lucide-react"
import { MoneyValue } from "@/components/business"
import {
    ObjectSectionTabs,
    ObjectSectionTabsPanel,
} from "@/components/business/object-section-tabs"
import { Button } from "@/components/ui/button"
import type { PaymentRequest } from "../types"
import { RequestOrderDetails } from "./request-order-details"
import { RequestMaterials } from "./request-materials"
import { RequestPaymentHistory } from "./request-payment-history"
import { RequestSubmissionHistory } from "./request-submission-history"

function Field({ label, children }: { label: string; children: ReactNode }) {
    return (
        <div className="grid min-w-0 grid-cols-[7rem_minmax(0,1fr)] items-baseline gap-x-4 gap-y-1">
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
    return (
        <section className="space-y-5" aria-label="付款信息">
            <h3 className="border-b pb-3 text-base font-semibold">付款信息</h3>
            <dl className="grid gap-x-12 gap-y-5 text-sm lg:grid-cols-2">
                <Field label="付款申请人">{request.applicant_name}</Field>
                <Field label="我方付款主体">{request.our_company_name}</Field>
                <Field label="收款户名">
                    {request.payment_recipient.account_name}
                </Field>
                <Field label="开户行">
                    {request.payment_recipient.bank_name}
                </Field>
                <Field label="申请说明">{request.description}</Field>
                <Field label="收款账号">{account}</Field>
            </dl>
        </section>
    )
}

export function RequestDetailContent({
    request,
    account,
    approval,
    embedded,
    showHistory = true,
    workItemId,
}: {
    request: PaymentRequest
    account: ReactNode
    approval: ReactNode
    embedded: boolean
    showHistory?: boolean
    workItemId?: string
}) {
    const [section, setSection] = useState("details")
    const overview = (
        <>
            <PaymentInformation request={request} account={account} />
            <RequestOrderDetails request={request} />
        </>
    )
    return (
        <>
            <section className="space-y-5" aria-label="付款申请摘要">
                <dl className="grid grid-cols-2 items-center gap-x-6 gap-y-5 rounded-lg bg-muted/45 px-6 py-6 sm:grid-cols-3 xl:grid-cols-[1.2fr_1fr_1fr_1fr_1.1fr] xl:gap-x-0">
                    <div className="col-span-2 space-y-2 sm:col-span-1 xl:pr-6">
                        <dt className="text-sm text-muted-foreground">
                            本次申请
                        </dt>
                        <dd>
                            <MoneyValue
                                value={request.total_amount}
                                size="hero"
                            />
                        </dd>
                    </div>
                    {(
                        [
                            ["有效已付", request.paid_amount],
                            ["剩余待付", request.unpaid_amount],
                            ["已关闭未付", request.closed_amount],
                        ] as const
                    ).map(([label, value]) => (
                        <div
                            key={label}
                            className="space-y-2 xl:border-l xl:px-6"
                        >
                            <dt className="text-xs text-muted-foreground">
                                {label}
                            </dt>
                            <dd className="text-xl">
                                <MoneyValue value={value} />
                            </dd>
                        </div>
                    ))}
                    <div className="space-y-2 xl:border-l xl:pl-6">
                        <dt className="text-xs text-muted-foreground">
                            希望付款日
                        </dt>
                        <dd className="num text-lg font-medium">
                            {request.desired_payment_date}
                        </dd>
                    </div>
                </dl>
                {request.status === "closed" && (
                    <p className="rounded-md bg-muted p-3 text-sm">
                        付款授权已终止：{request.close_reason || "未记录原因"}
                        。已付记录继续保留，冲正后不恢复付款授权。
                    </p>
                )}
            </section>
            {embedded ? (
                <div className="min-w-0 space-y-7">
                    {overview}
                    <RequestMaterials
                        request={request}
                        workItemId={workItemId}
                    />
                    {showHistory && (
                        <>
                            <RequestPaymentHistory
                                request={request}
                                workItemId={workItemId}
                            />
                            {approval}
                        </>
                    )}
                </div>
            ) : (
                <ObjectSectionTabs
                    id="ppr-detail-sections"
                    value={section}
                    onValueChange={setSection}
                    listLabel="付款申请分区"
                    items={[
                        { id: "details", label: "申请详情" },
                        { id: "materials", label: "审核材料" },
                        { id: "payments", label: "付款记录" },
                        { id: "history", label: "流转记录" },
                    ]}
                >
                    <ObjectSectionTabsPanel
                        value="details"
                        className="space-y-8"
                    >
                        {overview}
                        <RequestMaterials
                            request={request}
                            summary
                            action={
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
                            }
                        />
                    </ObjectSectionTabsPanel>
                    <ObjectSectionTabsPanel
                        value="materials"
                        className="[&>section]:border-t-0 [&>section]:pt-0"
                    >
                        <RequestMaterials request={request} />
                    </ObjectSectionTabsPanel>
                    <ObjectSectionTabsPanel
                        value="payments"
                        className="[&>section]:border-t-0 [&>section]:pt-0"
                    >
                        <RequestPaymentHistory request={request} />
                    </ObjectSectionTabsPanel>
                    <ObjectSectionTabsPanel
                        value="history"
                        className="[&>section:first-child]:border-t-0 [&>section:first-child]:pt-0"
                    >
                        {approval ?? (
                            <p className="rounded-md bg-muted/40 px-5 py-6 text-sm text-muted-foreground">
                                尚未提交财务审核。
                            </p>
                        )}
                        <RequestSubmissionHistory request={request} />
                    </ObjectSectionTabsPanel>
                </ObjectSectionTabs>
            )}
        </>
    )
}
