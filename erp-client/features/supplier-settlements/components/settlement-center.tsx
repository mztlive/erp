"use client"

import * as React from "react"
import Link from "next/link"
import {
    ArrowLeftIcon,
    CheckIcon,
    ExternalLinkIcon,
    RefreshCwIcon,
    SendIcon,
} from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DocumentHeader,
    DocumentTotals,
    FormalActionConfirmDialog,
    FormalActionResult,
    GuardedBusinessAction,
    MoneyValue,
    OptionCombobox,
    PageHeader,
    PageScaffold,
    surfaceInsetClassName,
    surfacePanelClassName,
} from "@/components/business"
import type { ResultState } from "@/components/business/feedback"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Label } from "@/components/ui/label"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { Textarea } from "@/components/ui/textarea"
import { CrossEntryBanner } from "@/features/supplier-settlements/components/cross-entry-banner"
import { DifferencesWorkspace } from "@/features/supplier-settlements/components/differences-workspace"
import {
    blockerOf,
    newKey,
    outcomeToResult,
} from "@/features/supplier-settlements/operations"
import {
    useAppendEvidenceMutation,
    useClaimReviewMutation,
    useRefreshTrialMutation,
    useResolveDifferenceMutation,
    useReviewDecisionMutation,
    useSettlementDetailQuery,
    useSubmitReviewMutation,
} from "@/features/supplier-settlements/queries"
import type {
    DifferenceResolution,
    SettlementSection,
} from "@/features/supplier-settlements/types"
import {
    AUDIT_ACTION_LABEL,
    REASON_CODE_LABEL,
    RESOLUTION_LABEL,
    SECTION_LABEL,
    SECTIONS,
} from "@/features/supplier-settlements/types"
import type { SettlementsUrlState } from "@/features/supplier-settlements/url-state"
import { getErrorMessage } from "@/lib/api/errors"
import { formatDateTime } from "@/lib/datetime"
import { openWorkspaceLabel } from "@/lib/ui-text"
import { cn } from "@/lib/utils"

function SettlementCenter({
    statementId,
    urlState,
    patchUrl,
    returnTo,
    onBack,
}: {
    statementId: string
    urlState: SettlementsUrlState
    patchUrl: (patch: Partial<SettlementsUrlState>) => void
    returnTo?: string
    onBack: () => void
}) {
    const detailQuery = useSettlementDetailQuery(statementId)
    const refreshMutation = useRefreshTrialMutation()
    const resolveMutation = useResolveDifferenceMutation()
    const evidenceMutation = useAppendEvidenceMutation()
    const submitMutation = useSubmitReviewMutation()
    const decisionMutation = useReviewDecisionMutation()
    const claimMutation = useClaimReviewMutation()

    const [result, setResult] = React.useState<ResultState>(null)
    const [resolveOpen, setResolveOpen] = React.useState(false)
    const [evidenceOpen, setEvidenceOpen] = React.useState(false)
    const [submitOpen, setSubmitOpen] = React.useState(false)
    const [confirmOpen, setConfirmOpen] = React.useState(false)
    const [rejectOpen, setRejectOpen] = React.useState(false)
    const [resolution, setResolution] =
        React.useState<DifferenceResolution>("ERP_ACCEPTED")
    const [reasonCode, setReasonCode] = React.useState("ACCEPT_BILL")
    const [evidenceComment, setEvidenceComment] = React.useState("")
    const [rejectReason, setRejectReason] = React.useState("")
    const resultRef = React.useRef<HTMLDivElement | null>(null)

    const data = detailQuery.data
    const section = urlState.section

    React.useEffect(() => {
        if (result?.status === "succeeded" || result?.status === "unknown") {
            resultRef.current?.focus()
        }
    }, [result])

    // keyboard: d opens differences when center focused
    React.useEffect(() => {
        const onKey = (event: KeyboardEvent) => {
            const target = event.target as HTMLElement | null
            if (
                target &&
                (target.tagName === "INPUT" ||
                    target.tagName === "TEXTAREA" ||
                    target.tagName === "SELECT" ||
                    target.isContentEditable)
            ) {
                return
            }
            if (event.key === "d" && !event.metaKey && !event.ctrlKey) {
                event.preventDefault()
                patchUrl({ section: "differences" })
            }
        }
        window.addEventListener("keydown", onKey)
        return () => window.removeEventListener("keydown", onKey)
    }, [patchUrl])

    if (detailQuery.isPending) {
        return (
            <PageScaffold>
                <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
                <div className="h-24 animate-pulse rounded-lg bg-muted" />
                <div className="h-96 animate-pulse rounded-lg bg-muted" />
                <p className="text-sm text-muted-foreground">
                    正在加载结算单，请稍候…
                </p>
            </PageScaffold>
        )
    }

    if (detailQuery.isError) {
        return (
            <PageScaffold>
                <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    onClick={onBack}
                >
                    <ArrowLeftIcon className="size-4" />
                    返回列表
                </Button>
                <BusinessFailureState
                    title="结算单加载失败"
                    error={detailQuery.error}
                    action={
                        <Button
                            type="button"
                            onClick={() => void detailQuery.refetch()}
                        >
                            重试
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    if (!data) {
        return (
            <PageScaffold>
                <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    onClick={onBack}
                >
                    <ArrowLeftIcon className="size-4" />
                    返回列表
                </Button>
                <BusinessEmptyState
                    kind="no-data"
                    className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                    title="结算单不存在"
                    description="该结算单不存在或已被作废。可返回列表重新选择，或检查分享链接是否正确。"
                />
            </PageScaffold>
        )
    }

    const detail = data
    const st = detail.statement
    const allowed = new Set(detail.allowedActions)
    const blockers = detail.actionBlockers
    const activeDiff =
        detail.differences.find((d) => d.differenceId === urlState.diff) ??
        detail.differences[0] ??
        null

    const confirmBlocker = blockerOf(blockers, "CONFIRM")
    const submitBlocker = blockerOf(blockers, "SUBMIT_REVIEW")

    async function onRefresh() {
        try {
            const outcome = await refreshMutation.mutateAsync({
                statementId: st.id,
                expectedLockVersion: st.lockVersion,
                expectedSourceSnapshotHash: st.sourceSnapshotHash,
                requestId: newKey("req"),
                idempotencyKey: newKey("refresh"),
            })
            setResult(outcomeToResult(outcome))
        } catch (error) {
            setResult({
                status: "rejected",
                title: "刷新试算未完成",
                description: getErrorMessage(error, "刷新失败，请稍后重试"),
            })
        }
    }

    async function onResolve() {
        if (!activeDiff) return
        try {
            const outcome = await resolveMutation.mutateAsync({
                statementId: st.id,
                differenceId: activeDiff.differenceId,
                expectedLockVersion: st.lockVersion,
                expectedDifferenceVersion: activeDiff.version,
                resolution,
                reasonCode,
                operationId: newKey("op"),
                idempotencyKey: newKey("resolve"),
            })
            setResult(outcomeToResult(outcome))
            if (outcome.status === "succeeded") setResolveOpen(false)
        } catch (error) {
            setResult({
                status: "rejected",
                title: "结论登记未完成",
                description: getErrorMessage(error, "提交失败，请稍后重试"),
            })
        }
    }

    async function onEvidence() {
        if (!activeDiff) return
        try {
            const outcome = await evidenceMutation.mutateAsync({
                statementId: st.id,
                differenceId: activeDiff.differenceId,
                expectedDifferenceVersion: activeDiff.version,
                opinionCode: "PROCUREMENT_NOTE",
                comment: evidenceComment,
                requestId: newKey("req"),
                idempotencyKey: newKey("ev"),
            })
            setResult(outcomeToResult(outcome))
            if (outcome.status === "succeeded") {
                setEvidenceOpen(false)
                setEvidenceComment("")
            }
        } catch (error) {
            setResult({
                status: "rejected",
                title: "证据保存未完成",
                description: getErrorMessage(error, "保存失败，请稍后重试"),
            })
        }
    }

    async function onSubmitReview() {
        const outcome = await submitMutation.mutateAsync({
            statementId: st.id,
            expectedLockVersion: st.lockVersion,
            subjectHash: st.subjectHash ?? `sh_${st.id}`,
            operationId: newKey("op"),
            idempotencyKey: newKey("submit"),
        })
        setResult(outcomeToResult(outcome))
        if (outcome.status === "succeeded") {
            setSubmitOpen(false)
            patchUrl({ section: "review" })
        }
    }

    async function onConfirm() {
        if (!detail.workItem) {
            setResult({
                status: "blocked",
                title: "无复核任务",
                description: "请先领取任务后再确认",
            })
            return
        }
        const outcome = await decisionMutation.mutateAsync({
            statementId: st.id,
            workItemId: detail.workItem.workItemId,
            expectedSubjectVersion: detail.workItem.subjectVersion,
            expectedLockVersion: st.lockVersion,
            action: "CONFIRM",
            operationId: newKey("op"),
            idempotencyKey: newKey("confirm"),
        })
        setResult(outcomeToResult(outcome))
        if (outcome.status === "succeeded") {
            setConfirmOpen(false)
            patchUrl({ section: "payable" })
        }
    }

    async function onReject() {
        if (!detail.workItem) return
        const outcome = await decisionMutation.mutateAsync({
            statementId: st.id,
            workItemId: detail.workItem.workItemId,
            expectedSubjectVersion: detail.workItem.subjectVersion,
            expectedLockVersion: st.lockVersion,
            action: "REJECT",
            operationId: newKey("op"),
            idempotencyKey: newKey("reject"),
            reasonCode: rejectReason || "NEEDS_MORE_EVIDENCE",
        })
        setResult(outcomeToResult(outcome))
        if (outcome.status === "rejected" || outcome.status === "succeeded") {
            setRejectOpen(false)
        }
    }

    return (
        <PageScaffold>
            <PageHeader
                variant="object-chrome"
                breadcrumbs={[
                    {
                        id: "api",
                        label: "供应商 API",
                        href: "/supplier-api/settlements",
                    },
                    {
                        id: "list",
                        label: "API 供应商结算",
                        href: "/supplier-api/settlements",
                    },
                    {
                        id: "detail",
                        label: st.statementNo,
                        current: true,
                    },
                ]}
                actions={
                    <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        onClick={onBack}
                    >
                        <ArrowLeftIcon className="size-4" />
                        返回列表
                    </Button>
                }
            />

            {returnTo ? <CrossEntryBanner returnTo={returnTo} /> : null}

            <DocumentHeader
                density="compact"
                title={`${st.supplierName} · ${st.periodLabel}`}
                documentNumber={st.statementNo}
                primaryStatus={{ label: st.statusLabel, tone: st.statusTone }}
                version={st.lockVersion}
                meta={
                    <span className="inline-flex flex-wrap items-center gap-x-1.5 gap-y-0.5">
                        <span>
                            经办{" "}
                            <span className="font-medium text-foreground">
                                {st.preparedBy?.displayName ?? "—"}
                            </span>
                        </span>
                        <span className="text-border" aria-hidden="true">
                            ·
                        </span>
                        <span>
                            复核{" "}
                            <span className="font-medium text-foreground">
                                {st.reviewedBy?.displayName ?? "待复核人"}
                            </span>
                        </span>
                        <span className="text-border" aria-hidden="true">
                            ·
                        </span>
                        <span className="text-muted-foreground">
                            记录{" "}
                            {formatDateTime(
                                detail.freshness.immutableFactsAsOf,
                                "default",
                            )}
                        </span>
                    </span>
                }
                primaryAction={
                    <div className="flex flex-wrap gap-2">
                        {allowed.has("REFRESH_TRIAL") ? (
                            <Button
                                type="button"
                                variant="outline"
                                size="sm"
                                disabled={refreshMutation.isPending}
                                onClick={() => void onRefresh()}
                            >
                                <RefreshCwIcon className="size-3.5" />
                                刷新试算
                            </Button>
                        ) : null}
                        {allowed.has("SUBMIT_REVIEW") ? (
                            <Button
                                type="button"
                                size="sm"
                                onClick={() => setSubmitOpen(true)}
                            >
                                <SendIcon className="size-3.5" />
                                提交复核
                            </Button>
                        ) : submitBlocker ? (
                            <GuardedBusinessAction
                                type="button"
                                size="sm"
                                disabled
                                reason={submitBlocker.message}
                            >
                                提交复核
                            </GuardedBusinessAction>
                        ) : null}
                        {allowed.has("CONFIRM") ? (
                            <Button
                                type="button"
                                size="sm"
                                onClick={() => setConfirmOpen(true)}
                            >
                                <CheckIcon className="size-3.5" />
                                确认结算
                            </Button>
                        ) : confirmBlocker ? (
                            <GuardedBusinessAction
                                type="button"
                                size="sm"
                                disabled
                                reason={confirmBlocker.message}
                            >
                                确认结算
                            </GuardedBusinessAction>
                        ) : null}
                        {allowed.has("REJECT") ? (
                            <Button
                                type="button"
                                size="sm"
                                variant="outline"
                                onClick={() => setRejectOpen(true)}
                            >
                                驳回
                            </Button>
                        ) : null}
                    </div>
                }
            />

            {blockers.filter(
                (b) =>
                    ["CONFIRM", "SUBMIT_REVIEW", "SOD_VIOLATION"].includes(
                        b.action,
                    ) ||
                    b.code === "SOD_VIOLATION" ||
                    b.code === "BLOCKING_DIFFERENCES",
            ).length > 0 ? (
                <Alert variant="warning">
                    <AlertTitle>动作门禁</AlertTitle>
                    <AlertDescription>
                        <ul className="list-inside list-disc text-sm">
                            {blockers
                                .filter(
                                    (b) =>
                                        b.action === "CONFIRM" ||
                                        b.action === "SUBMIT_REVIEW" ||
                                        b.code === "SOD_VIOLATION" ||
                                        b.code === "BLOCKING_DIFFERENCES",
                                )
                                .map((b) => (
                                    <li key={`${b.action}-${b.code}`}>
                                        {b.message}
                                    </li>
                                ))}
                        </ul>
                    </AlertDescription>
                </Alert>
            ) : null}

            <div ref={resultRef} tabIndex={-1} className="outline-none">
                {result ? (
                    <FormalActionResult
                        status={
                            result.status === "failed"
                                ? "blocked"
                                : result.status
                        }
                        title={result.title}
                        description={result.description}
                        reference={result.reference}
                        facts={result.facts}
                        actions={
                            <div className="flex flex-wrap gap-2">
                                {result.w12Href ? (
                                    <Button
                                        type="button"
                                        size="sm"
                                        render={<Link href={result.w12Href} />}
                                    >
                                        去供应商往来 处理应付
                                        <ExternalLinkIcon className="size-3.5" />
                                    </Button>
                                ) : null}
                            </div>
                        }
                    />
                ) : null}
            </div>

            <Card size="sm" className={surfacePanelClassName}>
                <CardHeader className="rounded-t-lg border-b border-border/30 py-3">
                    <CardTitle className="text-base">金额摘要</CardTitle>
                    <CardDescription>
                        订单、运费、服务费、退款与 ERP
                        计算金额、供应商账单金额、差异方向对比 · 全部
                        {detail.totals.taxBasisLabel}
                    </CardDescription>
                </CardHeader>
                <CardContent className="pt-4">
                    <DocumentTotals
                        title={null}
                        items={[
                            {
                                id: "order",
                                label: "订单结算价",
                                value: (
                                    <MoneyValue
                                        value={detail.totals.orderAmountGross}
                                        taxBasis="gross"
                                    />
                                ),
                                basis: "含税",
                            },
                            {
                                id: "freight",
                                label: "运费",
                                value: (
                                    <MoneyValue
                                        value={detail.totals.freightGross}
                                        taxBasis="gross"
                                    />
                                ),
                                basis: "含税",
                            },
                            {
                                id: "service",
                                label: "服务费",
                                value: (
                                    <MoneyValue
                                        value={detail.totals.serviceFeeGross}
                                        taxBasis="gross"
                                    />
                                ),
                                basis: "含税",
                            },
                            {
                                id: "refund",
                                label: "供应商退款",
                                value: (
                                    <MoneyValue
                                        value={detail.totals.refundGross}
                                        taxBasis="gross"
                                    />
                                ),
                                basis: "含税",
                            },
                            {
                                id: "erp",
                                label: "ERP 计算金额",
                                value: (
                                    <MoneyValue
                                        value={detail.totals.erpAmountGross}
                                        taxBasis="gross"
                                    />
                                ),
                                basis: "含税",
                            },
                            {
                                id: "supplier",
                                label: "供应商账单金额",
                                value: detail.totals.supplierAmountGross ? (
                                    <MoneyValue
                                        value={
                                            detail.totals.supplierAmountGross
                                        }
                                        taxBasis="gross"
                                    />
                                ) : (
                                    "账单未同步 · 刷新试算后以 ERP 金额预填"
                                ),
                                basis: "含税",
                            },
                            {
                                id: "diff",
                                label: "差异金额",
                                value: detail.totals.differenceAmountGross ? (
                                    <MoneyValue
                                        value={
                                            detail.totals.differenceAmountGross
                                        }
                                        taxBasis="gross"
                                    />
                                ) : (
                                    "—"
                                ),
                                warning: detail.totals.differenceDirectionLabel,
                                basis: "含税",
                            },
                            {
                                id: "cost",
                                label:
                                    st.status === "CONFIRMED"
                                        ? "已确认成本差额"
                                        : "待确认成本差额预览",
                                value: (
                                    <MoneyValue
                                        value={
                                            detail.totals
                                                .confirmedCostDeltaGross ??
                                            detail.totals
                                                .pendingCostDeltaGross ??
                                            "0.00"
                                        }
                                        taxBasis="gross"
                                    />
                                ),
                                basis: "含税",
                            },
                        ]}
                    />
                </CardContent>
            </Card>

            <div
                className={cn(
                    surfaceInsetClassName,
                    "px-3 py-2 text-xs text-muted-foreground",
                )}
            >
                <span className="font-medium text-foreground">来源数据 </span>
                更新时间 {formatDateTime(st.sourceAsOf, "default")}
                {st.externalBillNo ? (
                    <>
                        {" "}
                        · 账单 {st.externalBillNo}（第{" "}
                        {String(st.externalBillVersion ?? "").replace(
                            /^v/i,
                            "",
                        )}{" "}
                        版）
                    </>
                ) : null}
                <span className="ml-2">以下数据仅供参考，不进入结算结果</span>
            </div>

            <div
                className={cn(surfacePanelClassName, "min-w-0 overflow-hidden")}
            >
                <Tabs
                    value={section}
                    onValueChange={(v) =>
                        patchUrl({ section: v as SettlementSection })
                    }
                >
                    <TabsList
                        variant="line"
                        className="sticky top-0 z-10 h-auto w-full flex-wrap justify-start gap-1 overflow-x-auto rounded-none border-b border-border/30 bg-card/95 px-3 py-1.5 backdrop-blur supports-backdrop-filter:bg-card/80"
                    >
                        {SECTIONS.map((s) => (
                            <TabsTrigger key={s} value={s}>
                                {SECTION_LABEL[s]}
                                {s === "differences" &&
                                detail.differenceSummary.blocking > 0
                                    ? ` (${detail.differenceSummary.blocking})`
                                    : null}
                            </TabsTrigger>
                        ))}
                    </TabsList>
                </Tabs>
                <div className="space-y-4 p-3 md:p-4">
                    <p className="text-xs text-muted-foreground">
                        快捷键 d 可直达差异处理
                    </p>

                    {section === "overview" ? (
                        <Card
                            size="sm"
                            className={cn(
                                surfaceInsetClassName,
                                "shadow-none ring-0",
                            )}
                        >
                            <CardHeader className="rounded-t-lg border-b border-border/30 py-3">
                                <CardTitle className="text-base">
                                    概览
                                </CardTitle>
                            </CardHeader>
                            <CardContent className="space-y-2 pt-4 text-sm">
                                <p>
                                    供应商：{st.supplierName}
                                    （记录时，不受后续更名影响）
                                </p>
                                <p className="num">
                                    期间：{st.periodStart} ~ {st.periodEnd}
                                </p>
                                <p>状态：{st.statusLabel}</p>
                                <p>
                                    未决阻断差异：
                                    {detail.differenceSummary.blocking} /
                                    差异合计 {detail.differenceSummary.total}
                                </p>
                                <p className="text-muted-foreground">
                                    账单/订单/成本原值只读，不可在本页改写以消差。
                                </p>
                                <div className="flex flex-wrap gap-2 pt-2">
                                    <Button
                                        type="button"
                                        size="sm"
                                        variant="secondary"
                                        onClick={() =>
                                            patchUrl({ section: "differences" })
                                        }
                                    >
                                        打开差异处理
                                    </Button>
                                    <Button
                                        type="button"
                                        size="sm"
                                        variant="outline"
                                        onClick={() =>
                                            patchUrl({ section: "items" })
                                        }
                                    >
                                        查看结算明细
                                    </Button>
                                </div>
                            </CardContent>
                        </Card>
                    ) : null}

                    {section === "items" ? (
                        <Card
                            size="sm"
                            className={cn(
                                surfaceInsetClassName,
                                "shadow-none ring-0",
                            )}
                        >
                            <CardHeader className="rounded-t-lg border-b border-border/30 py-3">
                                <CardTitle className="text-base">
                                    结算明细
                                </CardTitle>
                                <CardDescription>
                                    冻结数据 + 不可变完成/取消/退款记录 ·
                                    金额只读，不可修改
                                </CardDescription>
                            </CardHeader>
                            <CardContent className="overflow-x-auto pt-0">
                                <table className="w-full min-w-[48rem] text-left text-sm">
                                    <thead className="border-b text-xs text-muted-foreground">
                                        <tr>
                                            <th className="px-2 py-2">
                                                供应商订单
                                            </th>
                                            <th className="px-2 py-2">
                                                采购单号
                                            </th>
                                            <th className="px-2 py-2">
                                                外部单号
                                            </th>
                                            <th className="px-2 py-2">商品</th>
                                            <th className="px-2 py-2 text-right">
                                                数量
                                            </th>
                                            <th className="px-2 py-2">记录</th>
                                            <th className="px-2 py-2 text-right">
                                                订单
                                            </th>
                                            <th className="px-2 py-2 text-right">
                                                运费
                                            </th>
                                            <th className="px-2 py-2 text-right">
                                                服务费
                                            </th>
                                            <th className="px-2 py-2 text-right">
                                                退款
                                            </th>
                                            <th className="px-2 py-2 text-right">
                                                ERP
                                            </th>
                                            <th className="px-2 py-2 text-right">
                                                账单行
                                            </th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {detail.items.map((it) => (
                                            <tr
                                                key={it.itemId}
                                                className="border-b border-border/60"
                                            >
                                                <td className="px-2 py-2">
                                                    <Link
                                                        href={`/supplier-api/orders?q=${encodeURIComponent(it.supplierOrderNo)}`}
                                                        className="num font-medium text-primary underline-offset-2 hover:underline"
                                                    >
                                                        {it.supplierOrderNo}
                                                    </Link>
                                                </td>
                                                <td className="px-2 py-2">
                                                    {it.purchaseNo ? (
                                                        <Link
                                                            href={
                                                                it.purchaseOrderId
                                                                    ? `/procurement/orders/${it.purchaseOrderId}?returnTo=${encodeURIComponent(`/supplier-api/settlements/${statementId}`)}`
                                                                    : `/procurement/orders?q=${encodeURIComponent(it.purchaseNo)}`
                                                            }
                                                            className="num font-medium text-primary underline-offset-2 hover:underline"
                                                        >
                                                            {it.purchaseNo}
                                                        </Link>
                                                    ) : (
                                                        <span className="text-xs text-muted-foreground">
                                                            —
                                                        </span>
                                                    )}
                                                </td>
                                                <td className="num px-2 py-2 text-muted-foreground">
                                                    {it.externalOrderNo}
                                                </td>
                                                <td className="px-2 py-2">
                                                    {it.productName}
                                                </td>
                                                <td className="num px-2 py-2 text-right">
                                                    {it.quantity}
                                                </td>
                                                <td className="px-2 py-2 text-xs">
                                                    {it.factLabel}
                                                </td>
                                                <td className="px-2 py-2 text-right">
                                                    <MoneyValue
                                                        value={
                                                            it.orderAmountGross
                                                        }
                                                        taxBasis="gross"
                                                    />
                                                </td>
                                                <td className="px-2 py-2 text-right">
                                                    <MoneyValue
                                                        value={it.freightGross}
                                                        taxBasis="gross"
                                                    />
                                                </td>
                                                <td className="px-2 py-2 text-right">
                                                    <MoneyValue
                                                        value={
                                                            it.serviceFeeGross
                                                        }
                                                        taxBasis="gross"
                                                    />
                                                </td>
                                                <td className="px-2 py-2 text-right">
                                                    <MoneyValue
                                                        value={it.refundGross}
                                                        taxBasis="gross"
                                                    />
                                                </td>
                                                <td className="px-2 py-2 text-right">
                                                    <MoneyValue
                                                        value={
                                                            it.erpAmountGross
                                                        }
                                                        taxBasis="gross"
                                                    />
                                                </td>
                                                <td className="px-2 py-2 text-right">
                                                    {it.supplierBillLineGross !=
                                                    null ? (
                                                        <MoneyValue
                                                            value={
                                                                it.supplierBillLineGross
                                                            }
                                                            taxBasis="gross"
                                                        />
                                                    ) : (
                                                        "—"
                                                    )}
                                                </td>
                                            </tr>
                                        ))}
                                        {detail.items.length === 0 ? (
                                            <tr>
                                                <td
                                                    colSpan={12}
                                                    className="px-2 py-6 text-center text-muted-foreground"
                                                >
                                                    暂无明细；可在草稿态刷新试算纳入不可变记录
                                                </td>
                                            </tr>
                                        ) : null}
                                    </tbody>
                                </table>
                                <p className="mt-2 text-xs text-muted-foreground">
                                    输入控件未提供金额编辑路径；账单原值与订单记录不可覆盖。
                                </p>
                            </CardContent>
                        </Card>
                    ) : null}

                    {section === "differences" ? (
                        <DifferencesWorkspace
                            differences={detail.differences}
                            activeDiff={activeDiff}
                            onSelect={(id) => patchUrl({ diff: id })}
                            allowed={allowed}
                            onResolve={() => setResolveOpen(true)}
                            onEvidence={() => setEvidenceOpen(true)}
                        />
                    ) : null}

                    {section === "review" ? (
                        <Card
                            size="sm"
                            className={cn(
                                surfaceInsetClassName,
                                "shadow-none ring-0",
                            )}
                        >
                            <CardHeader className="rounded-t-lg border-b border-border/30 py-3">
                                <CardTitle className="text-base">
                                    复核记录
                                </CardTitle>
                                <CardDescription>
                                    提交 / 驳回 /
                                    确认追加式记录；岗位分离由系统校验
                                </CardDescription>
                            </CardHeader>
                            <CardContent className="space-y-3 pt-4">
                                {detail.workItem ? (
                                    <Alert variant="info">
                                        <AlertTitle>复核任务</AlertTitle>
                                        <AlertDescription>
                                            {detail.statement.statementNo} ·
                                            供应商{" "}
                                            {detail.statement.supplierName}
                                            {detail.workItem.claimedBy
                                                ? ` · 领取人 ${detail.workItem.claimedBy.displayName}`
                                                : " · 待领取"}
                                        </AlertDescription>
                                        {detail.workItem.claimedBy == null &&
                                        allowed.has("CLAIM_REVIEW") ? (
                                            <Button
                                                type="button"
                                                size="sm"
                                                variant="outline"
                                                disabled={
                                                    claimMutation.isPending
                                                }
                                                onClick={async () => {
                                                    try {
                                                        const outcome =
                                                            await claimMutation.mutateAsync(
                                                                {
                                                                    statementId:
                                                                        st.id,
                                                                    workItemId:
                                                                        detail
                                                                            .workItem!
                                                                            .workItemId,
                                                                    expectedSubjectVersion:
                                                                        detail
                                                                            .workItem!
                                                                            .subjectVersion,
                                                                    idempotencyKey:
                                                                        newKey(
                                                                            "claim",
                                                                        ),
                                                                },
                                                            )
                                                        setResult(
                                                            outcomeToResult(
                                                                outcome,
                                                            ),
                                                        )
                                                    } catch (error) {
                                                        setResult({
                                                            status: "rejected",
                                                            title: "领取任务未完成",
                                                            description:
                                                                getErrorMessage(
                                                                    error,
                                                                    "领取失败，请稍后重试",
                                                                ),
                                                        })
                                                    }
                                                }}
                                            >
                                                领取任务
                                            </Button>
                                        ) : null}
                                    </Alert>
                                ) : null}
                                {detail.reviewRecords.length === 0 ? (
                                    <p className="text-sm text-muted-foreground">
                                        尚无复核记录
                                    </p>
                                ) : (
                                    detail.reviewRecords.map((r) => (
                                        <div
                                            key={r.recordId}
                                            className={cn(
                                                surfaceInsetClassName,
                                                "px-3 py-2 text-sm",
                                            )}
                                        >
                                            <div className="font-medium">
                                                {r.actionLabel} ·{" "}
                                                {r.by.displayName}
                                            </div>
                                            <div className="text-xs text-muted-foreground">
                                                {formatDateTime(
                                                    r.at,
                                                    "default",
                                                )}
                                                {r.reasonCode
                                                    ? ` · ${REASON_CODE_LABEL[r.reasonCode] ?? r.reasonCode}`
                                                    : ""}
                                                {r.comment
                                                    ? ` · ${r.comment}`
                                                    : ""}
                                            </div>
                                        </div>
                                    ))
                                )}
                            </CardContent>
                        </Card>
                    ) : null}

                    {section === "payable" ? (
                        <Card
                            size="sm"
                            className={cn(
                                surfaceInsetClassName,
                                "shadow-none ring-0",
                            )}
                        >
                            <CardHeader className="rounded-t-lg border-b border-border/30 py-3">
                                <CardTitle className="text-base">
                                    应付与票款
                                </CardTitle>
                                <CardDescription>
                                    确认后形成唯一应付；付款/进项发票/核销进入供应商往来，不在本页复制
                                </CardDescription>
                            </CardHeader>
                            <CardContent className="space-y-3 pt-4">
                                {detail.payable ? (
                                    <>
                                        <p className="text-sm">
                                            应付编号{" "}
                                            <span className="num font-medium">
                                                {detail.payable.payableNo}
                                            </span>
                                        </p>
                                        <p className="text-sm">
                                            含税金额{" "}
                                            <MoneyValue
                                                value={
                                                    detail.payable.grossAmount
                                                }
                                                taxBasis="gross"
                                            />{" "}
                                            · 到期 {detail.payable.dueDate} ·{" "}
                                            {detail.payable.statusLabel}
                                        </p>
                                        <Button
                                            type="button"
                                            size="sm"
                                            render={
                                                <Link
                                                    href={
                                                        detail.payable.w12Href
                                                    }
                                                />
                                            }
                                        >
                                            {openWorkspaceLabel("W12")}
                                            <ExternalLinkIcon className="size-3.5" />
                                        </Button>
                                    </>
                                ) : (
                                    <p className="text-sm text-muted-foreground">
                                        尚未确认结算，无应付编号。确认成功后此处展示应付与成本差额结果。
                                    </p>
                                )}
                            </CardContent>
                        </Card>
                    ) : null}

                    {section === "audit" ? (
                        <Card
                            size="sm"
                            className={cn(
                                surfaceInsetClassName,
                                "shadow-none ring-0",
                            )}
                        >
                            <CardHeader className="rounded-t-lg border-b border-border/30 py-3">
                                <CardTitle className="text-base">
                                    审计
                                </CardTitle>
                            </CardHeader>
                            <CardContent className="space-y-2 pt-4">
                                {detail.auditEvents.map((e) => (
                                    <div
                                        key={e.eventId}
                                        className={cn(
                                            surfaceInsetClassName,
                                            "px-3 py-2 text-sm",
                                        )}
                                    >
                                        <div className="flex flex-wrap gap-2">
                                            <span className="font-medium">
                                                {AUDIT_ACTION_LABEL[e.action] ??
                                                    e.summary.split("·")[0]}
                                            </span>
                                            <span className="text-muted-foreground">
                                                {e.actor}
                                            </span>
                                            {e.auditNo ? (
                                                <span className="num text-xs">
                                                    审计号 {e.auditNo}
                                                </span>
                                            ) : null}
                                        </div>
                                        <p className="text-muted-foreground">
                                            {e.summary}
                                        </p>
                                        <p className="text-xs text-muted-foreground">
                                            {formatDateTime(e.at, "default")}
                                        </p>
                                    </div>
                                ))}
                            </CardContent>
                        </Card>
                    ) : null}
                </div>
            </div>

            {/* Resolve difference */}
            <Dialog open={resolveOpen} onOpenChange={setResolveOpen}>
                <DialogContent>
                    <DialogHeader>
                        <DialogTitle>登记差异处理结论</DialogTitle>
                        <DialogDescription>
                            财务经办追加式结论；不修改左右证据原值或历史成本。结论一经登记不可撤回，将写入审计并改变待确认成本差额。
                        </DialogDescription>
                    </DialogHeader>
                    <div className="space-y-3">
                        <div className="space-y-1.5">
                            <Label>受控结论</Label>
                            <OptionCombobox
                                value={resolution}
                                onValueChange={(v) => {
                                    if (v)
                                        setResolution(v as DifferenceResolution)
                                }}
                                options={(
                                    Object.keys(
                                        RESOLUTION_LABEL,
                                    ) as DifferenceResolution[]
                                ).map((k) => ({
                                    value: k,
                                    label: RESOLUTION_LABEL[k],
                                }))}
                                allowClear={false}
                            />
                        </div>
                        <div className="space-y-1.5">
                            <Label>原因码</Label>
                            <OptionCombobox
                                value={reasonCode}
                                onValueChange={(v) => {
                                    if (v) setReasonCode(v)
                                }}
                                options={[
                                    {
                                        value: "BILL_ALIGNED",
                                        label: "账单已对齐",
                                    },
                                    {
                                        value: "ACCEPT_BILL",
                                        label: "接受供应商账单",
                                    },
                                    {
                                        value: "NO_BUSINESS_IMPACT",
                                        label: "无需业务调整",
                                    },
                                    {
                                        value: "COMPENSATED_ELSEWHERE",
                                        label: "已另行补偿",
                                    },
                                ]}
                                allowClear={false}
                            />
                        </div>
                    </div>
                    <DialogFooter>
                        <Button
                            type="button"
                            variant="outline"
                            onClick={() => setResolveOpen(false)}
                        >
                            取消
                        </Button>
                        <Button
                            type="button"
                            disabled={resolveMutation.isPending}
                            onClick={() => void onResolve()}
                        >
                            提交结论
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>

            <Dialog open={evidenceOpen} onOpenChange={setEvidenceOpen}>
                <DialogContent>
                    <DialogHeader>
                        <DialogTitle>追加采购协同证据</DialogTitle>
                        <DialogDescription>
                            只追加供应商证据或业务意见和审计，不改变差异结论、试算金额或成本基线。
                        </DialogDescription>
                    </DialogHeader>
                    <div className="space-y-1.5">
                        <Label htmlFor="ev-comment">业务说明</Label>
                        <Textarea
                            id="ev-comment"
                            value={evidenceComment}
                            onChange={(e) => setEvidenceComment(e.target.value)}
                            rows={3}
                        />
                    </div>
                    <DialogFooter>
                        <Button
                            type="button"
                            variant="outline"
                            onClick={() => setEvidenceOpen(false)}
                        >
                            取消
                        </Button>
                        <Button
                            type="button"
                            disabled={evidenceMutation.isPending}
                            onClick={() => void onEvidence()}
                        >
                            保存证据
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>

            <FormalActionConfirmDialog
                open={submitOpen}
                onOpenChange={setSubmitOpen}
                title="提交复核"
                description="将冻结来源更新时间、明细与差异结论，并创建唯一复核待办。"
                actionLabel="提交复核"
                confirmLabel="确认提交"
                fromStatus={{ label: st.statusLabel, tone: st.statusTone }}
                toStatus={{ label: "待复核", tone: "warning" }}
                lockedFields={[
                    st.statementNo,
                    "来源数据、明细与差异结论已锁定",
                ]}
                effects={["冻结来源数据与差异结论", "创建结算复核待办"]}
                pending={submitMutation.isPending}
                onConfirm={async () => {
                    await onSubmitReview()
                }}
            />

            <FormalActionConfirmDialog
                open={confirmOpen}
                onOpenChange={setConfirmOpen}
                title="确认结算（不可逆）"
                description="同一次提交追加成本差额、形成唯一应付并锁定处理结果。经办人不可确认本单。"
                actionLabel="确认结算"
                confirmLabel="确认结算"
                fromStatus={{ label: st.statusLabel, tone: st.statusTone }}
                toStatus={{ label: "已确认", tone: "success" }}
                lockedFields={[
                    st.statementNo,
                    `应付金额预览 ${st.supplierAmountGross ?? st.erpAmountGross}`,
                    `成本差额预览 ${detail.totals.pendingCostDeltaGross ?? "0.00"}`,
                    `经办 ${st.preparedBy?.displayName ?? "—"}`,
                ]}
                effects={[
                    "追加成本差额记录",
                    "形成唯一供应商结算应付",
                    "锁定处理结果，不可撤回确认",
                ]}
                irreversibleEffects={["确认后付款/进项发票/核销进入供应商往来"]}
                nextDepartment="供应商往来"
                pending={decisionMutation.isPending}
                onConfirm={async () => {
                    await onConfirm()
                }}
            />

            <Dialog open={rejectOpen} onOpenChange={setRejectOpen}>
                <DialogContent>
                    <DialogHeader>
                        <DialogTitle>驳回复核</DialogTitle>
                        <DialogDescription>
                            原因必填，退回经办并保留记录。
                        </DialogDescription>
                    </DialogHeader>
                    <div className="space-y-1.5">
                        <Label>原因码</Label>
                        <OptionCombobox
                            value={rejectReason || null}
                            onValueChange={(v) => setRejectReason(v ?? "")}
                            options={[
                                { value: "", label: "请选择" },
                                {
                                    value: "NEEDS_MORE_EVIDENCE",
                                    label: "证据不足",
                                },
                                {
                                    value: "AMOUNT_MISMATCH",
                                    label: "金额仍不一致",
                                },
                                { value: "OTHER", label: "其他" },
                            ]}
                            placeholder="请选择"
                            allowClear={false}
                        />
                    </div>
                    <DialogFooter>
                        <Button
                            type="button"
                            variant="ghost"
                            onClick={() => setRejectOpen(false)}
                        >
                            取消
                        </Button>
                        <Button
                            type="button"
                            disabled={
                                !rejectReason || decisionMutation.isPending
                            }
                            onClick={() => void onReject()}
                        >
                            确认驳回
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>
        </PageScaffold>
    )
}

export { SettlementCenter }
