"use client"

import { useCallback, useState, type ReactNode } from "react"
import { ArrowRightIcon } from "lucide-react"
import {
    WorkspaceTaskPageSlot,
    useWorkspaceTaskPage,
} from "@/components/business/workspace-task-pane"
import {
    DetailWorkspaceBody,
    DetailWorkspaceMetricItem,
    DetailWorkspaceMetrics,
    DetailWorkspaceSection,
    DetailWorkspaceTabs,
    MoneyValue,
    detailWorkspaceStyles,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import { compareDecimal } from "@/lib/fixed-decimal"
import { usePathname, useRouter, useSearchParams } from "@/lib/navigation"
import { cn } from "@/lib/utils"
import { requestStatusLabel, requestStatusTone } from "../lib/display"
import type { PaymentRequest } from "../types"
import { RequestMaterials } from "./request-materials"
import { RequestOrderDetails } from "./request-order-details"
import { RequestPaymentHistory } from "./request-payment-history"
import { RequestReviewContent } from "./request-review-content"
import { RequestSubmissionHistory } from "./request-submission-history"

const detailSections = [
    { id: "details", label: "申请内容" },
    { id: "materials", label: "审核材料" },
    { id: "payments", label: "付款记录" },
    { id: "history", label: "审批记录" },
] as const

type DetailSectionId = (typeof detailSections)[number]["id"]

const sectionsId = "ppr-detail-sections"
const sectionTabId = (id: string) => `${sectionsId}-tab-${id}`

function resolveDetailSection(
    value: string | null | undefined,
): DetailSectionId {
    return detailSections.find((item) => item.id === value)?.id ?? "details"
}

function Field({ label, children }: { label: string; children: ReactNode }) {
    return (
        <div className="grid min-w-0 grid-cols-[6rem_minmax(0,1fr)] items-baseline gap-x-4 gap-y-1">
            <dt className="text-xs text-muted-foreground">{label}</dt>
            <dd className="min-w-0 whitespace-pre-wrap break-words text-body-compact">
                {children}
            </dd>
        </div>
    )
}

function PaymentInformation({
    request,
    account,
    detailWorkspace = false,
}: {
    request: PaymentRequest
    account: ReactNode
    detailWorkspace?: boolean
}) {
    const fullPage = useWorkspaceTaskPage()
    const Heading = detailWorkspace ? "h2" : "h3"
    const headingClassName = detailWorkspace
        ? detailWorkspaceStyles.sectionTitle
        : "font-semibold"
    return (
        <section className="space-y-5" aria-label="申请与收款信息">
            {!fullPage && (
                <div className="space-y-3">
                    <Heading className={headingClassName}>申请信息</Heading>
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
                        fullPage
                            ? "space-y-3"
                            : "space-y-3 border-t border-border/60 pt-5"
                    }
                >
                    <Heading className={headingClassName}>
                        付款方式：供应商预存款
                    </Heading>
                    <p className="text-body-compact">
                        人民币预存款
                        {request.deposit_account?.identity
                            .external_account_ref ? (
                            <>
                                {" "}
                                · 账户标识{" "}
                                <span className="num">
                                    ****
                                    {request.deposit_account.identity.external_account_ref.slice(
                                        -4,
                                    )}
                                </span>
                            </>
                        ) : (
                            " · 账户已冻结"
                        )}
                    </p>
                    <p className="text-xs text-muted-foreground">
                        使用提交时已冻结的同主体、同供应商预存款账户。批准后按采购执行组占用，实际扣款核实后才核销，不登记本次银行付款。
                    </p>
                </div>
            ) : (
                <div
                    className={
                        fullPage
                            ? "space-y-3"
                            : "space-y-3 border-t border-border/60 pt-5"
                    }
                >
                    <Heading className={headingClassName}>收款信息</Heading>
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
    const [section, setSection] = useState<DetailSectionId>("details")
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
    const items = showHistory ? detailSections : detailSections.slice(0, 2)
    return (
        <div className="@container/payment-detail min-w-0 space-y-5">
            <WorkspaceTaskPageSlot slot="aside">
                <RequestPaymentSummary
                    request={request}
                    submissionVersion={submissionVersion}
                />
            </WorkspaceTaskPageSlot>
            <div className="border-b border-border/60">
                <DetailWorkspaceTabs
                    value={section}
                    onValueChange={(next) =>
                        setSection(resolveDetailSection(next))
                    }
                    items={items}
                    ariaLabel="付款申请分区"
                    tabIdPrefix={`${sectionsId}-tab`}
                />
            </div>
            {section === "details" && (
                <div className="space-y-6">
                    <PaymentInformation request={request} account={account} />
                    <div className="border-t border-border/60 pt-5">
                        <RequestOrderDetails
                            request={request}
                            compact={embedded}
                        />
                    </div>
                    <RequestMaterialsLink
                        request={request}
                        onOpen={() => setSection("materials")}
                    />
                </div>
            )}
            {section === "materials" && (
                <div className="[&>section]:border-t-0 [&>section]:pt-0">
                    <RequestMaterials
                        request={request}
                        workItemId={workItemId}
                        submissionVersion={submissionVersion}
                    />
                </div>
            )}
            {showHistory && section === "payments" && (
                <div className="[&>section]:border-t-0 [&>section]:pt-0">
                    <RequestPaymentHistory
                        request={request}
                        workItemId={workItemId}
                    />
                </div>
            )}
            {showHistory && section === "history" && (
                <div className="space-y-5">
                    {approval ?? (
                        <p className="text-xs text-muted-foreground">
                            尚未提交财务审核。
                        </p>
                    )}
                    {!embedded && (
                        <RequestSubmissionHistory request={request} />
                    )}
                </div>
            )}
        </div>
    )
}

function RequestMaterialsLink({
    request,
    onOpen,
    detailWorkspace = false,
}: {
    request: PaymentRequest
    onOpen: () => void
    detailWorkspace?: boolean
}) {
    return (
        <div
            className={cn(
                "flex flex-wrap items-center justify-between gap-3",
                !detailWorkspace && "border-t border-border/60 pt-4",
            )}
        >
            <p className="text-xs text-muted-foreground">
                {request.status === "draft"
                    ? "采购单、销售单及附件"
                    : "采购单、销售单及附件保留提交时版本"}
            </p>
            <Button
                id="ppr-detail-open-materials"
                variant="ghost"
                size="sm"
                className={detailWorkspaceStyles.subtleButton}
                onClick={() => {
                    onOpen()
                    document.getElementById(sectionTabId("materials"))?.focus()
                }}
            >
                查看审核材料
                <ArrowRightIcon data-icon="inline-end" aria-hidden="true" />
            </Button>
        </div>
    )
}

function ClosedPaymentNotice({
    request,
    detailWorkspace = false,
}: {
    request: PaymentRequest
    detailWorkspace?: boolean
}) {
    if (
        request.status !== "closed" &&
        compareDecimal(request.closed_amount, "0", 4) <= 0
    )
        return null
    return (
        <div
            className={
                detailWorkspace
                    ? "space-y-1 border-t border-border/60 py-3 text-body-compact"
                    : "space-y-1 border-t border-border/60 pt-3 text-body-compact"
            }
        >
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
    )
}

function RequestPaymentSummary({
    request,
    submissionVersion,
}: {
    request: PaymentRequest
    submissionVersion?: number
}) {
    return (
        <section
            className="space-y-3 border-b border-border/60 pb-4"
            aria-label="付款申请摘要"
        >
            <div className="flex flex-wrap items-center justify-between gap-2">
                <span className="num text-xs text-muted-foreground">
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
            <DetailWorkspaceMetrics columns={3} variant="inline">
                <DetailWorkspaceMetricItem
                    label="申请金额"
                    value={<MoneyValue value={request.total_amount} />}
                    emphasis="primary"
                />
                <DetailWorkspaceMetricItem
                    label={
                        <span title="扣除已冲正金额后的有效付款">已付金额</span>
                    }
                    value={<MoneyValue value={request.paid_amount} />}
                />
                <DetailWorkspaceMetricItem
                    label="待付金额"
                    value={<MoneyValue value={request.unpaid_amount} />}
                />
            </DetailWorkspaceMetrics>
            <ClosedPaymentNotice request={request} />
        </section>
    )
}

export function RequestDetailWorkspaceContent({
    request,
    account,
    approval,
    alerts,
    renderChrome,
}: {
    request: PaymentRequest
    account: ReactNode
    approval: ReactNode
    alerts: ReactNode
    renderChrome: (tabs: ReactNode) => ReactNode
}) {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const section = resolveDetailSection(searchParams.get("section"))

    const selectSection = useCallback(
        (next: string) => {
            const resolved = resolveDetailSection(next)
            const params = new URLSearchParams(searchParams.toString())
            if (resolved === "details") params.delete("section")
            else params.set("section", resolved)
            const qs = params.toString()
            router.replace(qs ? `${pathname}?${qs}` : pathname, {
                scroll: false,
            })
        },
        [pathname, router, searchParams],
    )

    return (
        <>
            {renderChrome(
                <DetailWorkspaceTabs
                    value={section}
                    onValueChange={selectSection}
                    items={detailSections}
                    ariaLabel="付款申请分区"
                    tabIdPrefix={`${sectionsId}-tab`}
                />,
            )}
            <DetailWorkspaceBody
                className="@container/payment-detail"
                alerts={alerts}
                summary={
                    <section aria-label="付款申请摘要">
                        <p className="pt-3 text-xs text-muted-foreground">
                            当前付款情况
                        </p>
                        <DetailWorkspaceMetrics columns={3} className="pt-2">
                            <DetailWorkspaceMetricItem
                                label="申请金额"
                                value={
                                    <MoneyValue value={request.total_amount} />
                                }
                                emphasis="primary"
                            />
                            <DetailWorkspaceMetricItem
                                label={
                                    <span title="扣除已冲正金额后的有效付款">
                                        已付金额
                                    </span>
                                }
                                value={
                                    <MoneyValue value={request.paid_amount} />
                                }
                            />
                            <DetailWorkspaceMetricItem
                                label="待付金额"
                                value={
                                    <MoneyValue value={request.unpaid_amount} />
                                }
                            />
                        </DetailWorkspaceMetrics>
                        <ClosedPaymentNotice
                            request={request}
                            detailWorkspace
                        />
                    </section>
                }
            >
                {section === "details" && (
                    <>
                        <DetailWorkspaceSection width="form" divider={false}>
                            <PaymentInformation
                                request={request}
                                account={account}
                                detailWorkspace
                            />
                        </DetailWorkspaceSection>
                        <RequestOrderDetails
                            request={request}
                            detailWorkspace
                        />
                        <DetailWorkspaceSection width="form">
                            <RequestMaterialsLink
                                request={request}
                                onOpen={() => selectSection("materials")}
                                detailWorkspace
                            />
                        </DetailWorkspaceSection>
                    </>
                )}
                {section === "materials" && (
                    <RequestMaterials request={request} detailWorkspace />
                )}
                {section === "payments" && (
                    <DetailWorkspaceSection width="form" divider={false}>
                        <RequestPaymentHistory
                            request={request}
                            detailWorkspace
                        />
                    </DetailWorkspaceSection>
                )}
                {section === "history" && (
                    <DetailWorkspaceSection
                        title="审批记录"
                        width="form"
                        divider={false}
                    >
                        {approval ?? (
                            <p className="text-xs text-muted-foreground">
                                尚未提交财务审核。
                            </p>
                        )}
                        <RequestSubmissionHistory
                            request={request}
                            detailWorkspace
                        />
                    </DetailWorkspaceSection>
                )}
            </DetailWorkspaceBody>
        </>
    )
}
