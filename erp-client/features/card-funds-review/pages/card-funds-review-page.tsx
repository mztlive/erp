"use client"

import * as React from "react"
import Link from "next/link"
import {
    CircleCheckIcon,
    PauseIcon,
    TriangleAlertIcon,
    XIcon,
} from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessStatusBadge,
    DataFreshness,
    DiscardConfirmDialog,
    FormalActionConfirmDialog,
    FormalActionResult,
    PageHeader,
    PageScaffold,
    SequentialProcessBar,
    surfacePanelClassName,
} from "@/components/business"
import { cn } from "@/lib/utils"
import { type ResultState as SharedResultState } from "@/components/business/feedback"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Textarea } from "@/components/ui/textarea"
import type {
    AllocationDraftLine,
    ApproveConclusion,
    CardFundsReviewDecision,
    FormalOutcome,
    ReviewType,
} from "@/features/card-funds-review/types"
import {
    APPROVE_CONCLUSION_LABEL,
    REJECT_FOLLOW_UP_COLLABORATION,
    REVIEW_TYPE_LABEL,
    WORK_ITEM_TYPE_LABEL,
} from "@/features/card-funds-review/types"
import {
    useCardFundsReviewQueueQuery,
    useClaimCardFundsMutation,
    useCompleteCardFundsMutation,
    useHoldCardFundsMutation,
    useRegisterInvoiceMutation,
    useRegisterReceiptMutation,
    useSaveCardFundsEvidenceMutation,
} from "../hooks/queries"
import {
    useCardFundsReviewDefaultUrlSync,
    useCardFundsReviewUrlState,
} from "../hooks/use-card-funds-review-url-state"
import { freshnessText, openWorkspaceLabel, versionText } from "@/lib/ui-text"
import { formatDateTime } from "@/lib/datetime"
import { getErrorMessage } from "@/lib/api/errors"
import { CardFundsAllocationEditor } from "../components/card-funds-allocation-editor"
import { CardFundsOverview } from "../components/card-funds-overview"
import { CardFundsRecords } from "../components/card-funds-records"
import { EvidenceNavPanel } from "../components/evidence-nav-panel"
import { QueueFilterToolbar } from "../components/queue-filter-toolbar"
import { RejectReviewDialog } from "../components/reject-review-dialog"
import type { RejectReviewValue } from "../components/reject-review-dialog"
import { ReviewChainPanel } from "../components/review-chain-panel"
import { formatMoney, moneyStrSafe, shortHash } from "../lib/presentation"

type SessionLease = {
    workItemId: string
    subjectVersion: string
}

type ResultState = SharedResultState<FormalOutcome>

type ConfirmMode =
    | { kind: "approve"; conclusion: ApproveConclusion; advance: boolean }
    | { kind: "zero"; advance: boolean }
    | { kind: "reject" }
    | { kind: "hold" }
    | null

export function CardFundsReviewPage() {
    const {
        scope,
        type,
        status,
        due,
        q,
        currentWorkItemId,
        queueContextId,
        autoNext,
        searchInput,
        setSearchInput,
        setAutoNext,
        replaceUrl,
        pathname,
        searchParams,
        router,
    } = useCardFundsReviewUrlState()

    const filters = React.useMemo(
        () => ({
            scope,
            type,
            status,
            due,
            q,
            currentWorkItemId,
            queueContextId,
        }),
        [scope, type, status, due, q, currentWorkItemId, queueContextId],
    )

    const queueQuery = useCardFundsReviewQueueQuery(filters)
    const claimMutation = useClaimCardFundsMutation()
    const completeMutation = useCompleteCardFundsMutation()
    const holdMutation = useHoldCardFundsMutation()
    const registerReceiptMutation = useRegisterReceiptMutation()
    const registerInvoiceMutation = useRegisterInvoiceMutation()
    const saveEvidenceMutation = useSaveCardFundsEvidenceMutation()

    const view = queueQuery.data
    const tasks = React.useMemo(() => view?.tasks ?? [], [view?.tasks])
    const context = view?.context
    const task =
        tasks.find((t) => t.workItem.workItemId === currentWorkItemId) ??
        view?.current ??
        tasks[0]
    const currentIndex = task
        ? Math.max(
              0,
              tasks.findIndex(
                  (t) => t.workItem.workItemId === task.workItem.workItemId,
              ),
          )
        : 0

    useCardFundsReviewDefaultUrlSync({
        queuePending: queueQuery.isPending,
        view,
        task,
        taskCount: tasks.length,
        scope,
        type,
        queueContextId,
    })

    const completed = Boolean(view) && tasks.length === 0

    const [confirmMode, setConfirmMode] = React.useState<ConfirmMode>(null)
    const [lastResult, setLastResult] = React.useState<ResultState>(null)
    const [actionError, setActionError] = React.useState<string | null>(null)
    const [allocationMode, setAllocationMode] = React.useState<
        null | "receipt" | "invoice"
    >(null)
    const [evidenceRef, setEvidenceRef] = React.useState("")
    const [evidenceDocId, setEvidenceDocId] = React.useState("")
    const [comment, setComment] = React.useState("")
    const [receiptForm, setReceiptForm] = React.useState({
        receiptNo: "",
        receivedAt: "2026-07-01",
        grossAmount: "",
    })
    const [invoiceForm, setInvoiceForm] = React.useState({
        invoiceNo: "",
        issuedAt: "2026-07-01",
        grossAmount: "",
        netAmount: "",
        taxAmount: "",
    })
    const [allocLines, setAllocLines] = React.useState<AllocationDraftLine[]>(
        [],
    )
    const [evidenceSavedAt, setEvidenceSavedAt] = React.useState<string | null>(
        null,
    )
    const [evidenceDirty, setEvidenceDirty] = React.useState(false)
    const [pendingNav, setPendingNav] = React.useState<number | null>(null)
    const [keyHint, setKeyHint] = React.useState<string | null>(null)

    const evidenceOk = Boolean(evidenceDocId.trim() || evidenceRef.trim())

    const headingRef = React.useRef<HTMLHeadingElement>(null)
    const resultRef = React.useRef<HTMLDivElement>(null)
    const leaseRef = React.useRef<SessionLease | null>(null)
    const [leaseEpoch, setLeaseEpoch] = React.useState(0)

    React.useEffect(() => {
        if (!task) return
        setEvidenceRef(task.currentEvidence.evidenceReferences[0] ?? "")
        setEvidenceDocId(task.currentEvidence.evidenceDocumentIds[0] ?? "")
        setComment(task.currentEvidence.comment ?? "")
        setActionError(null)
        setAllocationMode(null)
        setReceiptForm({
            receiptNo: "",
            receivedAt: "2026-07-01",
            grossAmount: "",
        })
        setInvoiceForm({
            invoiceNo: "",
            issuedAt: "2026-07-01",
            grossAmount: "",
            netAmount: "",
            taxAmount: "",
        })
        setAllocLines([])
        setEvidenceSavedAt(null)
        setEvidenceDirty(false)
    }, [task])

    // 自动领取（含登记票款后版本刷新：任务 subjectVersion 变化时重新领取）
    React.useEffect(() => {
        if (!task) return
        const held = leaseRef.current
        if (
            held?.workItemId === task.workItem.workItemId &&
            held.subjectVersion === task.workItem.subjectVersion
        ) {
            return
        }
        if (claimMutation.isPending) return
        let cancelled = false
        void claimMutation
            .mutateAsync(task.workItem.workItemId)
            .then((lease) => {
                if (cancelled) return
                leaseRef.current = {
                    workItemId: lease.workItemId,
                    subjectVersion: lease.subjectVersion,
                }
                setLeaseEpoch((n) => n + 1)
            })
            .catch((error) => {
                if (cancelled) return
                setActionError(getErrorMessage(error, "任务领取失败，请重试"))
            })
        return () => {
            cancelled = true
        }
        // eslint-disable-next-line react-hooks/exhaustive-deps -- 仅任务或版本变化时领取
    }, [task?.workItem.workItemId, task?.workItem.subjectVersion])

    // 焦点：结果区 / 对象标题；位置播报由 SequentialProcessBar aria-live
    React.useEffect(() => {
        if (lastResult) {
            resultRef.current?.focus()
        } else if (task) {
            headingRef.current?.focus()
        }
    }, [task, lastResult])

    const goToWorkItem = React.useCallback(
        (workItemId: string | undefined | null) => {
            setLastResult(null)
            setActionError(null)
            replaceUrl({
                currentWorkItemId: workItemId ?? null,
                queueContextId,
            })
        },
        [queueContextId, replaceUrl],
    )

    // 清除筛选：清 type/status/due/q + 焦点，保留 scope/queueContextId（P4）。
    // type 不写默认值「all」，遵循 URL 最小化（默认值省略语义，D18）
    const hasActiveQueueFilters = Boolean(
        q || status === "held" || due !== "all" || type !== "all",
    )

    const clearFilters = React.useCallback(() => {
        setSearchInput("")
        replaceUrl({
            type: null,
            status: null,
            due: null,
            q: null,
            currentWorkItemId: null,
        })
    }, [replaceUrl, setSearchInput])

    const neighborId = React.useCallback(
        (delta: number) => {
            const idx = currentIndex + delta
            return tasks[idx]?.workItem.workItemId
        },
        [currentIndex, tasks],
    )

    const activeLease =
        leaseRef.current?.workItemId === task?.workItem.workItemId
            ? leaseRef.current
            : null
    void leaseEpoch

    const leaseStatus: "active" | "unclaimed" | "lost" | "expiring" =
        activeLease ? "active" : "unclaimed"
    const leaseLabel = activeLease ? "已领取" : "未领取"

    const ensureLease = React.useCallback(async () => {
        if (!task) throw new Error("无当前任务")
        const held = leaseRef.current
        if (held?.workItemId === task.workItem.workItemId) {
            // 登记回款/发票后任务版本已刷新：继续用旧版本提交会被阻断，须重新领取
            if (held.subjectVersion === task.workItem.subjectVersion) {
                return held
            }
            const lease = await claimMutation.mutateAsync(
                task.workItem.workItemId,
            )
            const session: SessionLease = {
                workItemId: lease.workItemId,
                subjectVersion: lease.subjectVersion,
            }
            leaseRef.current = session
            setLeaseEpoch((n) => n + 1)
            return session
        }
        const lease = await claimMutation.mutateAsync(task.workItem.workItemId)
        const session: SessionLease = {
            workItemId: lease.workItemId,
            subjectVersion: lease.subjectVersion,
        }
        leaseRef.current = session
        setLeaseEpoch((n) => n + 1)
        return session
    }, [claimMutation, task])

    const buildDecisionBase = React.useCallback(
        (reviewResult: "APPROVED" | "REJECTED") => {
            if (!task) throw new Error("无当前任务")
            const evidenceDocumentIds = evidenceDocId.trim()
                ? [evidenceDocId.trim()]
                : []
            const evidenceReferences = evidenceRef.trim()
                ? [evidenceRef.trim()]
                : []
            return {
                receivableAccountId: task.account.id,
                expectedAccountSeq: task.account.accountSeq,
                expectedAccountDomainVersion: task.account.domainVersion,
                expectedReviewChainTailId: task.reviewChain.tailReviewId,
                expectedReviewChainVersion: task.reviewChain.chainVersion,
                expectedNextReviewNo: task.reviewChain.nextReviewNo,
                expectedSalesOrderRevisionId: task.currentSalesOrderRevisionId,
                expectedFundsFactVersion: task.fundsFactVersion,
                reviewType: task.reviewType as ReviewType,
                evidenceDocumentIds,
                evidenceReferences,
                comment: comment.trim() || undefined,
                expectedSubjectHash: task.workItem.subjectHash,
                reviewResult,
            }
        },
        [comment, evidenceDocId, evidenceRef, task],
    )

    const advanceIfNeeded = React.useCallback(
        (shouldAdvance: boolean) => {
            if (!shouldAdvance) return
            const nextId =
                context?.nextWorkItemId ??
                neighborId(1) ??
                tasks.find(
                    (t) => t.workItem.workItemId !== task?.workItem.workItemId,
                )?.workItem.workItemId
            leaseRef.current = null
            setLeaseEpoch((n) => n + 1)
            if (nextId) goToWorkItem(nextId)
            else replaceUrl({ currentWorkItemId: null, queueContextId })
        },
        [
            context?.nextWorkItemId,
            goToWorkItem,
            neighborId,
            queueContextId,
            replaceUrl,
            task?.workItem.workItemId,
            tasks,
        ],
    )

    const runApprove = React.useCallback(
        async (conclusion: ApproveConclusion, advance: boolean) => {
            if (!task) return
            setActionError(null)
            try {
                const lease = await ensureLease()
                const base = buildDecisionBase("APPROVED")
                const decision: CardFundsReviewDecision = {
                    ...base,
                    reviewResult: "APPROVED",
                    conclusion,
                }
                const response = await completeMutation.mutateAsync({
                    workItemId: task.workItem.workItemId,
                    expectedSubjectVersion: lease.subjectVersion,
                    decision,
                })
                setConfirmMode(null)

                if (response.status !== "succeeded") {
                    if (response.status === "failed") {
                        setActionError(response.message)
                        throw new Error(response.message)
                    }
                    return
                }
                if (response.outcome.kind !== "APPROVED") return
                const biz = response.outcome.business
                leaseRef.current = null
                setLeaseEpoch((n) => n + 1)
                setLastResult({
                    status: "succeeded",
                    title: `复核通过 · 复核号 ${biz.reviewNo}`,
                    description: `${APPROVE_CONCLUSION_LABEL[biz.conclusion as ApproveConclusion]} · ${advance && autoNext ? "自动下一项" : "手动继续"}`,
                    reference: biz.reference,
                    outcome: response.outcome,
                    stayOnItem: !(advance && autoNext),
                })
                // 成功先展示固定复核号；若 autoNext 则短暂停留后前进
                if (advance && autoNext) {
                    window.setTimeout(() => advanceIfNeeded(true), 2200)
                }
            } catch (error) {
                setActionError(getErrorMessage(error, "完成失败"))
            }
        },
        [
            advanceIfNeeded,
            autoNext,
            buildDecisionBase,
            completeMutation,
            ensureLease,
            task,
        ],
    )

    const submitReject = React.useCallback(
        async (value: RejectReviewValue) => {
            if (!task) return
            setActionError(null)
            try {
                const lease = await ensureLease()
                const base = buildDecisionBase("REJECTED")
                const decision: CardFundsReviewDecision = {
                    ...base,
                    reviewResult: "REJECTED",
                    conclusion: "REJECTED",
                    reasonCode: value.reasonCode,
                    comment: value.comment.trim(),
                    evidenceDocumentIds:
                        base.evidenceDocumentIds.length > 0
                            ? base.evidenceDocumentIds
                            : ["doc_reject_note"],
                    evidenceReferences:
                        base.evidenceReferences.length > 0
                            ? base.evidenceReferences
                            : [`驳回说明:${value.comment.trim().slice(0, 40)}`],
                }
                const response = await completeMutation.mutateAsync({
                    workItemId: task.workItem.workItemId,
                    expectedSubjectVersion: lease.subjectVersion,
                    decision,
                })
                setConfirmMode(null)
                if (response.status !== "succeeded") {
                    if (response.status === "failed") {
                        setActionError(response.message)
                    }
                    return
                }
                if (response.outcome.kind !== "REJECTED") return
                const biz = response.outcome.business
                leaseRef.current = null
                setLeaseEpoch((n) => n + 1)
                setLastResult({
                    status: "rejected",
                    title: `已驳回 · 复核号 ${biz.reviewNo}`,
                    description: `${REJECT_FOLLOW_UP_COLLABORATION}`,
                    reference: biz.reference,
                    outcome: response.outcome,
                    stayOnItem: !autoNext,
                })
                if (autoNext) {
                    window.setTimeout(() => advanceIfNeeded(true), 2200)
                }
            } catch (error) {
                setActionError(getErrorMessage(error, "驳回失败"))
            }
        },
        [
            advanceIfNeeded,
            autoNext,
            buildDecisionBase,
            completeMutation,
            ensureLease,
            task,
        ],
    )

    const handleHold = React.useCallback(async () => {
        if (!task) return
        setActionError(null)
        try {
            const lease = await ensureLease()
            const response = await holdMutation.mutateAsync({
                workItemId: task.workItem.workItemId,
                expectedSubjectVersion: lease.subjectVersion,
                reasonCode: "NEED_MORE_EVIDENCE",
                note: comment.trim() || "跳过：待补充票款证据",
                nextWorkItemId: neighborId(1),
            })
            setConfirmMode(null)
            if (
                response.status !== "succeeded" ||
                response.outcome.kind !== "HELD"
            ) {
                if (response.status === "failed")
                    setActionError(response.message)
                return
            }
            leaseRef.current = null
            setLeaseEpoch((n) => n + 1)
            setLastResult({
                status: "blocked",
                title: "当前项已跳过 · 仍在待处理列表",
                description: response.outcome.resumeHint,
                reference: response.outcome.reference,
                outcome: response.outcome,
            })
            // 暂挂不自动移动；结果面板给出可见反馈，用户按「下一项」或 j/k 继续
        } catch (error) {
            setActionError(getErrorMessage(error, "跳过失败"))
        }
    }, [comment, ensureLease, holdMutation, neighborId, task])

    const openAllocation = React.useCallback(
        (mode: "receipt" | "invoice") => {
            if (!task) return
            setAllocationMode(mode)
            setAllocLines([
                {
                    lineId: "al_1",
                    targetAccountId: task.account.id,
                    targetLabel: `${task.salesOrder.orderNo} · ${task.account.customerName}`,
                    amount:
                        mode === "receipt"
                            ? receiptForm.grossAmount || "0.00"
                            : invoiceForm.grossAmount || "0.00",
                },
            ])
        },
        [invoiceForm.grossAmount, receiptForm.grossAmount, task],
    )

    const submitReceipt = React.useCallback(async () => {
        if (!task) return
        setActionError(null)
        try {
            const lease = await ensureLease()
            const result = await registerReceiptMutation.mutateAsync({
                workItemId: task.workItem.workItemId,
                expectedSubjectVersion: lease.subjectVersion,
                receiptNo:
                    receiptForm.receiptNo.trim() ||
                    `SK-${Date.now().toString(36).toUpperCase()}`,
                receivedAt: receiptForm.receivedAt,
                grossAmount: receiptForm.grossAmount,
                allocations: allocLines,
                evidenceReference: evidenceRef.trim() || "银行回单-本次登记",
            })
            // 登记后停留当前项，刷新金额/指纹（invalidate 后 query 更新）
            setAllocationMode(null)
            setLastResult({
                status: "succeeded",
                title: "历史回款已登记",
                description: `已形成回款与分配；净已收 ${formatMoney(result.settledTotal)}。复核完成前指标仍可能不可靠。`,
                reference: result.fundsFactVersion,
                stayOnItem: true,
            })
            // 租约仍有效但 subject 已变：刷新 lease 展示
            leaseRef.current = {
                ...lease,
            }
        } catch (error) {
            setActionError(getErrorMessage(error, "登记回款失败"))
        }
    }, [
        allocLines,
        ensureLease,
        evidenceRef,
        receiptForm,
        registerReceiptMutation,
        task,
    ])

    const submitInvoice = React.useCallback(async () => {
        if (!task) return
        setActionError(null)
        try {
            const lease = await ensureLease()
            const gross = invoiceForm.grossAmount
            const net =
                invoiceForm.netAmount || moneyStrSafe(Number(gross) / 1.13)
            const tax =
                invoiceForm.taxAmount ||
                moneyStrSafe(Number(gross) - Number(net))
            const result = await registerInvoiceMutation.mutateAsync({
                workItemId: task.workItem.workItemId,
                expectedSubjectVersion: lease.subjectVersion,
                invoiceNo:
                    invoiceForm.invoiceNo.trim() ||
                    `FP-${Date.now().toString(36).toUpperCase()}`,
                issuedAt: invoiceForm.issuedAt,
                grossAmount: gross,
                netAmount: net,
                taxAmount: tax,
                allocations: allocLines,
                evidenceReference: evidenceRef.trim() || "发票扫描件-本次登记",
            })
            setAllocationMode(null)
            setLastResult({
                status: "succeeded",
                title: "历史发票已登记",
                description: `已形成发票与分配；版本 ${shortHash(result.subjectHash)}，净已开票 ${formatMoney(result.invoicedTotal)}。`,
                reference: result.fundsFactVersion,
                stayOnItem: true,
            })
        } catch (error) {
            setActionError(getErrorMessage(error, "登记发票失败"))
        }
    }, [
        allocLines,
        ensureLease,
        evidenceRef,
        invoiceForm,
        registerInvoiceMutation,
        task,
    ])

    const saveEvidence = React.useCallback(async () => {
        if (!task) return
        try {
            const lease = await ensureLease()
            await saveEvidenceMutation.mutateAsync({
                workItemId: task.workItem.workItemId,
                expectedSubjectVersion: lease.subjectVersion,
                evidenceDocumentIds: evidenceDocId.trim()
                    ? [evidenceDocId.trim()]
                    : [],
                evidenceReferences: evidenceRef.trim()
                    ? [evidenceRef.trim()]
                    : [],
                comment: comment.trim() || undefined,
            })
            setEvidenceSavedAt(
                new Date().toLocaleString("zh-CN", {
                    hour: "2-digit",
                    minute: "2-digit",
                }),
            )
            setEvidenceDirty(false)
        } catch (error) {
            setActionError(getErrorMessage(error, "保存证据失败"))
        }
    }, [
        comment,
        ensureLease,
        evidenceDocId,
        evidenceRef,
        saveEvidenceMutation,
        task,
    ])

    // 键盘：j/k 导航；⌘↵ 打开正式确认（未领取时给出提示）
    React.useEffect(() => {
        const onKey = (event: KeyboardEvent) => {
            const target = event.target as HTMLElement | null
            const tag = target?.tagName
            const inField =
                tag === "INPUT" ||
                tag === "TEXTAREA" ||
                tag === "SELECT" ||
                target?.isContentEditable

            if (
                (event.metaKey || event.ctrlKey) &&
                event.key === "Enter" &&
                !inField
            ) {
                event.preventDefault()
                if (activeLease && task) {
                    if (!evidenceOk) {
                        setKeyHint(
                            "请先填写凭证编号或证据说明并保存证据，再通过复核。",
                        )
                        window.setTimeout(() => setKeyHint(null), 3000)
                        return
                    }
                    const zeroOk =
                        task.reviewType === "OPENING" &&
                        Number(task.account.settledTotal) === 0 &&
                        Number(task.account.invoicedTotal) === 0
                    setConfirmMode(
                        zeroOk
                            ? { kind: "zero", advance: autoNext }
                            : {
                                  kind: "approve",
                                  conclusion: "RECORDED_FACTS_RECONCILED",
                                  advance: autoNext,
                              },
                    )
                } else if (task) {
                    setKeyHint("任务尚未领取，无法提交；请先点击「领取任务」。")
                    window.setTimeout(() => setKeyHint(null), 3000)
                }
                return
            }
            if (inField) return
            if (event.key === "j" || event.key === "ArrowDown") {
                event.preventDefault()
                const next = neighborId(1)
                if (!next) return
                if (evidenceDirty) {
                    setPendingNav(1)
                    return
                }
                goToWorkItem(next)
            }
            if (event.key === "k" || event.key === "ArrowUp") {
                event.preventDefault()
                const prev = neighborId(-1)
                if (!prev) return
                if (evidenceDirty) {
                    setPendingNav(-1)
                    return
                }
                goToWorkItem(prev)
            }
        }
        window.addEventListener("keydown", onKey)
        return () => window.removeEventListener("keydown", onKey)
    }, [
        activeLease,
        autoNext,
        evidenceDirty,
        evidenceOk,
        goToWorkItem,
        neighborId,
        task,
    ])

    const formalPending =
        completeMutation.isPending ||
        holdMutation.isPending ||
        claimMutation.isPending

    const canConfirmZero =
        task?.reviewType === "OPENING" &&
        Number(task.account.settledTotal) === 0 &&
        Number(task.account.invoicedTotal) === 0

    const w05Href = task
        ? `/sales/orders/${task.salesOrder.id}?from=W13&returnTo=${encodeURIComponent(`${pathname}?${searchParams.toString()}`)}&sourceWorkItemId=${task.workItem.workItemId}`
        : "#"
    const w11Href = task
        ? `/finance/customer-accounts?customer=${task.account.customerId}&from=W13&returnTo=${encodeURIComponent(`${pathname}?queueContextId=${queueContextId}&currentWorkItemId=${task.workItem.workItemId}&type=${type}&scope=${scope}`)}`
        : "/finance/customer-accounts"

    const allocatedSum = allocLines.reduce(
        (s, l) => s + (Number(l.amount) || 0),
        0,
    )
    const allocTarget =
        allocationMode === "receipt"
            ? Number(receiptForm.grossAmount) || 0
            : Number(invoiceForm.grossAmount) || 0

    if (queueQuery.isPending) {
        return (
            <PageScaffold>
                <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
                <div className="h-24 animate-pulse rounded-lg bg-muted" />
                <div className="grid gap-4 xl:grid-cols-[minmax(0,64fr)_minmax(16rem,36fr)]">
                    <div className="h-80 animate-pulse rounded-lg bg-muted" />
                    <div className="h-64 animate-pulse rounded-lg bg-muted" />
                </div>
            </PageScaffold>
        )
    }

    if (queueQuery.isError) {
        return (
            <PageScaffold>
                <PageHeader title="卡券票款复核" />
                <BusinessFailureState
                    error={queueQuery.error}
                    onRetry={() => void queueQuery.refetch()}
                />
            </PageScaffold>
        )
    }

    return (
        <PageScaffold>
            <PageHeader
                title="卡券票款复核"
                breadcrumbs={[
                    {
                        id: "fin",
                        label: "财务",
                        href: "/finance/card-funds-review",
                    },
                    { id: "card", label: "卡券票款复核", current: true },
                ]}
                metadata={
                    <div className="flex flex-wrap items-center gap-3">
                        <DataFreshness
                            updatedAt={
                                context?.queueContextUpdatedAt
                                    ? formatDateTime(
                                          context.queueContextUpdatedAt,
                                          "full",
                                      )
                                    : "刚刚"
                            }
                            dateTime={context?.queueContextUpdatedAt}
                            state="fresh"
                            label={freshnessText.queueUpdatedAt}
                        />
                        <span
                            className="text-xs text-muted-foreground"
                            aria-live="polite"
                        >
                            {context?.filterSummary ?? "仅我的"} · 第{" "}
                            {context?.position ?? 0}/{context?.total ?? 0} 项
                        </span>
                    </div>
                }
            />

            <QueueFilterToolbar
                scope={scope}
                type={type}
                due={due}
                status={status}
                searchInput={searchInput}
                onSearchInputChange={setSearchInput}
                autoNext={autoNext}
                setAutoNext={setAutoNext}
                replaceUrl={replaceUrl}
            />

            {lastResult ? (
                <div ref={resultRef} tabIndex={-1} className="outline-none">
                    <FormalActionResult
                        status={
                            lastResult.status === "failed"
                                ? "blocked"
                                : lastResult.status
                        }
                        title={lastResult.title}
                        description={lastResult.description}
                        reference={lastResult.reference}
                        facts={buildResultFacts(lastResult.outcome)}
                        actions={
                            <div className="flex flex-wrap gap-2">
                                <Button
                                    type="button"
                                    onClick={() => {
                                        const next =
                                            context?.nextWorkItemId ??
                                            neighborId(1) ??
                                            tasks[0]?.workItem.workItemId
                                        goToWorkItem(next)
                                    }}
                                >
                                    下一项
                                </Button>
                                {task ? (
                                    <Button
                                        type="button"
                                        variant="outline"
                                        render={<Link href={w05Href} />}
                                    >
                                        {openWorkspaceLabel("W05")}
                                    </Button>
                                ) : null}
                            </div>
                        }
                    />
                    {lastResult.outcome?.kind === "REJECTED" &&
                    lastResult.outcome.business.followUpConfiguration ? (
                        <Alert className="mt-3" variant="destructive">
                            <TriangleAlertIcon aria-hidden="true" />
                            <AlertTitle>驳回后继流程未配置</AlertTitle>
                            <AlertDescription>
                                {
                                    lastResult.outcome.business
                                        .followUpConfiguration
                                        .collaborationMessage
                                }
                            </AlertDescription>
                        </Alert>
                    ) : null}
                </div>
            ) : null}

            {actionError ? (
                <Alert variant="destructive" role="alert">
                    <TriangleAlertIcon aria-hidden="true" />
                    <AlertTitle>操作未生效</AlertTitle>
                    <AlertDescription>{actionError}</AlertDescription>
                </Alert>
            ) : null}

            {completed ? (
                <BusinessEmptyState
                    kind="no-tasks"
                    title="当前筛选项已处理完"
                    description="卡券票款复核有效队列已清空。可清除筛选、切换类型/跳过范围，或返回工作台。"
                    action={
                        <div className="flex flex-wrap gap-2">
                            {hasActiveQueueFilters ? (
                                <Button
                                    type="button"
                                    variant="secondary"
                                    className="rounded-lg shadow-none"
                                    onClick={clearFilters}
                                >
                                    清除筛选
                                </Button>
                            ) : null}
                            <Button
                                variant="secondary"
                                className="rounded-lg shadow-none"
                                render={<Link href="/workspace" />}
                            >
                                返回今日工作台
                            </Button>
                        </div>
                    }
                />
            ) : task ? (
                <>
                    <SequentialProcessBar
                        current={context?.position ?? currentIndex + 1}
                        total={context?.total ?? tasks.length}
                        leaseStatus={leaseStatus}
                        leaseStatusLabel={leaseLabel}
                        processLabel="复核通过"
                        processNextLabel="通过并打开下一条"
                        processDisabled={
                            formalPending ||
                            Boolean(lastResult?.status === "unknown")
                        }
                        pending={formalPending}
                        backLabel="返回工作台"
                        onBack={() => router.push("/workspace")}
                        onProcess={() => {
                            if (!evidenceOk) {
                                setActionError(
                                    "请先填写凭证编号或证据说明并保存证据，再通过复核。",
                                )
                                return
                            }
                            setConfirmMode({
                                kind: "approve",
                                conclusion: "RECORDED_FACTS_RECONCILED",
                                advance: false,
                            })
                        }}
                        onProcessNext={() => {
                            if (!evidenceOk) {
                                setActionError(
                                    "请先填写凭证编号或证据说明并保存证据，再通过复核。",
                                )
                                return
                            }
                            setConfirmMode({
                                kind: "approve",
                                conclusion: "RECORDED_FACTS_RECONCILED",
                                advance: true,
                            })
                        }}
                        onReclaim={() => {
                            void ensureLease().catch((error) => {
                                setActionError(
                                    getErrorMessage(error, "领取失败"),
                                )
                            })
                        }}
                    />

                    <div className="grid min-w-0 gap-4 xl:grid-cols-[minmax(0,64fr)_minmax(17rem,36fr)]">
                        <div className="min-w-0 space-y-4">
                            <Card size="sm" className={surfacePanelClassName}>
                                <CardHeader className="border-b border-border/30">
                                    <div className="flex flex-wrap items-center gap-2">
                                        <CardTitle>
                                            <h2
                                                ref={headingRef}
                                                tabIndex={-1}
                                                className="outline-none"
                                                aria-live="polite"
                                            >
                                                {task.salesOrder.orderNo} ·{" "}
                                                {task.account.customerName}
                                            </h2>
                                        </CardTitle>
                                        <BusinessStatusBadge
                                            context="list"
                                            label={
                                                REVIEW_TYPE_LABEL[
                                                    task.reviewType
                                                ]
                                            }
                                            tone={
                                                task.reviewType === "OPENING"
                                                    ? "info"
                                                    : "warning"
                                            }
                                        />
                                        <Badge variant="secondary">
                                            {
                                                WORK_ITEM_TYPE_LABEL[
                                                    task.workItem.workItemType
                                                ]
                                            }
                                        </Badge>
                                        {task.workItem.held ? (
                                            <BusinessStatusBadge
                                                context="list"
                                                label="已跳过 · 仍在待处理列表"
                                                tone="warning"
                                            />
                                        ) : null}
                                    </div>
                                    <CardDescription>
                                        数据版本 r{task.salesOrder.revisionNo} ·
                                        同步于 {task.salesOrder.snapshotAt} ·{" "}
                                        {task.account.mallName} · 往来{" "}
                                        {task.account.counterpartyPartyName}
                                    </CardDescription>
                                </CardHeader>
                                <CardContent className="space-y-4">
                                    <CardFundsOverview task={task} />
                                    <CardFundsRecords
                                        task={task}
                                        w11Href={w11Href}
                                        openAllocation={openAllocation}
                                    />

                                    <CardFundsAllocationEditor
                                        allocationMode={allocationMode}
                                        task={task}
                                        receiptForm={receiptForm}
                                        setReceiptForm={setReceiptForm}
                                        invoiceForm={invoiceForm}
                                        setInvoiceForm={setInvoiceForm}
                                        allocLines={allocLines}
                                        setAllocLines={setAllocLines}
                                        allocTarget={allocTarget}
                                        allocatedSum={allocatedSum}
                                        receiptPending={
                                            registerReceiptMutation.isPending
                                        }
                                        invoicePending={
                                            registerInvoiceMutation.isPending
                                        }
                                        setAllocationMode={setAllocationMode}
                                        submitReceipt={submitReceipt}
                                        submitInvoice={submitInvoice}
                                    />
                                </CardContent>
                            </Card>

                            {/* sticky 决策区 */}
                            <Card
                                size="sm"
                                className={cn(
                                    surfacePanelClassName,
                                    "sticky bottom-2 z-10",
                                )}
                            >
                                <CardHeader className="border-b border-border/30 py-3">
                                    <CardTitle className="text-base">
                                        结论区
                                    </CardTitle>
                                    <CardDescription>
                                        提交时将核对账户、历史复核记录与数据版本。快捷键：j/k
                                        切换任务 · ⌘↵ 复核通过
                                    </CardDescription>
                                </CardHeader>
                                <CardContent className="space-y-3 pt-4">
                                    <div className="grid gap-3 sm:grid-cols-2">
                                        <div className="space-y-1.5">
                                            <Label htmlFor="ev-doc">
                                                凭证编号
                                            </Label>
                                            <Input
                                                id="ev-doc"
                                                value={evidenceDocId}
                                                onChange={(e) => {
                                                    setEvidenceDocId(
                                                        e.target.value,
                                                    )
                                                    setEvidenceDirty(true)
                                                }}
                                                placeholder="银行回单号 / 发票号"
                                            />
                                        </div>
                                        <div className="space-y-1.5">
                                            <Label htmlFor="ev-ref">
                                                证据说明
                                            </Label>
                                            <Input
                                                id="ev-ref"
                                                value={evidenceRef}
                                                onChange={(e) => {
                                                    setEvidenceRef(
                                                        e.target.value,
                                                    )
                                                    setEvidenceDirty(true)
                                                }}
                                                placeholder="如记账凭证、商城对账记录"
                                            />
                                        </div>
                                    </div>
                                    {!evidenceOk ? (
                                        <p
                                            className="text-xs text-destructive"
                                            role="alert"
                                        >
                                            完成复核前须至少填写一项凭证编号或证据说明（保存证据后生效）。
                                        </p>
                                    ) : null}
                                    <div className="space-y-1.5">
                                        <Label htmlFor="ev-comment">备注</Label>
                                        <Textarea
                                            id="ev-comment"
                                            value={comment}
                                            onChange={(e) => {
                                                setComment(e.target.value)
                                                setEvidenceDirty(true)
                                            }}
                                            rows={2}
                                        />
                                    </div>
                                    <div className="flex flex-wrap items-center gap-2">
                                        <Button
                                            type="button"
                                            variant="outline"
                                            size="sm"
                                            disabled={
                                                saveEvidenceMutation.isPending ||
                                                formalPending
                                            }
                                            onClick={() => void saveEvidence()}
                                        >
                                            {saveEvidenceMutation.isPending
                                                ? "正在保存…"
                                                : "保存证据"}
                                        </Button>
                                        {evidenceSavedAt ? (
                                            <span
                                                className="text-xs text-muted-foreground"
                                                aria-live="polite"
                                            >
                                                证据已保存（{evidenceSavedAt}）
                                            </span>
                                        ) : null}
                                        {keyHint ? (
                                            <span
                                                className="text-xs text-destructive"
                                                role="alert"
                                            >
                                                {keyHint}
                                            </span>
                                        ) : null}
                                        {canConfirmZero ? (
                                            <Button
                                                type="button"
                                                variant="secondary"
                                                disabled={
                                                    formalPending || !evidenceOk
                                                }
                                                title={
                                                    evidenceOk
                                                        ? undefined
                                                        : "须先填写并保存凭证编号或证据说明"
                                                }
                                                onClick={() =>
                                                    setConfirmMode({
                                                        kind: "zero",
                                                        advance: autoNext,
                                                    })
                                                }
                                            >
                                                <CircleCheckIcon data-icon="inline-start" />
                                                无历史票款，从 0 起
                                            </Button>
                                        ) : null}
                                        <Button
                                            type="button"
                                            disabled={
                                                formalPending || !evidenceOk
                                            }
                                            title={
                                                evidenceOk
                                                    ? undefined
                                                    : "须先填写并保存凭证编号或证据说明"
                                            }
                                            onClick={() =>
                                                setConfirmMode({
                                                    kind: "approve",
                                                    conclusion:
                                                        "RECORDED_FACTS_RECONCILED",
                                                    advance: autoNext,
                                                })
                                            }
                                        >
                                            复核通过
                                        </Button>
                                        <Button
                                            type="button"
                                            variant="destructive"
                                            disabled={formalPending}
                                            onClick={() =>
                                                setConfirmMode({
                                                    kind: "reject",
                                                })
                                            }
                                        >
                                            <XIcon data-icon="inline-start" />
                                            驳回
                                        </Button>
                                        <Button
                                            type="button"
                                            variant="outline"
                                            disabled={formalPending}
                                            onClick={() =>
                                                setConfirmMode({ kind: "hold" })
                                            }
                                        >
                                            <PauseIcon data-icon="inline-start" />
                                            先跳过
                                        </Button>
                                    </div>
                                </CardContent>
                            </Card>
                        </div>

                        <aside className="min-w-0 space-y-4 xl:sticky xl:top-4 xl:self-start">
                            <ReviewChainPanel task={task} />

                            <EvidenceNavPanel
                                task={task}
                                w05Href={w05Href}
                                w11Href={w11Href}
                            />
                        </aside>
                    </div>
                </>
            ) : (
                <BusinessEmptyState
                    kind="filter"
                    title="筛选无结果"
                    description="当前类型/范围没有任务，可清除筛选。"
                    action={
                        <Button
                            type="button"
                            variant="secondary"
                            className="rounded-lg shadow-none"
                            onClick={clearFilters}
                        >
                            清除筛选
                        </Button>
                    }
                />
            )}

            {/* 从 0 起 / 通过 强确认 */}
            <FormalActionConfirmDialog
                open={
                    confirmMode?.kind === "approve" ||
                    confirmMode?.kind === "zero"
                }
                onOpenChange={(open) => {
                    if (!open) setConfirmMode(null)
                }}
                title={
                    confirmMode?.kind === "zero"
                        ? "确认无历史票款，从 0 起"
                        : "确认复核通过"
                }
                description={
                    confirmMode?.kind === "zero"
                        ? `将提交「期初净额为零、无历史票款」结论：销售单 ${task?.salesOrder.orderNo ?? ""}、应收子账 #${task?.account.accountSeq ?? ""}（${task?.account.customerName ?? ""}）。不创建 0 元回款/发票。须证据完整；提交时将核对数据版本。`
                        : `将提交「复核通过并核对票款记录」。复核类型 ${task ? REVIEW_TYPE_LABEL[task.reviewType] : ""}，当前数据版本 ${task ? shortHash(task.workItem.subjectHash) : ""}。`
                }
                actionLabel={
                    confirmMode?.kind === "zero" ? "从 0 起并完成" : "复核通过"
                }
                confirmLabel={
                    confirmMode?.kind === "zero"
                        ? "确认从 0 起并完成"
                        : "确认通过"
                }
                fromStatus={{ label: "待复核", tone: "warning" }}
                toStatus={
                    confirmMode?.kind === "zero"
                        ? { label: "从 0 起已通过", tone: "success" }
                        : { label: "复核已通过", tone: "success" }
                }
                lockedFields={
                    task
                        ? [
                              `销售单 ${task.salesOrder.orderNo}`,
                              `应收子账 #${task.account.accountSeq}（${task.account.customerName}）`,
                              "数据版本（短校验码）",
                              `复核类型 ${REVIEW_TYPE_LABEL[task.reviewType]}`,
                              "票款版本（仅显示，不可改）",
                          ]
                        : []
                }
                effects={
                    confirmMode?.kind === "zero"
                        ? [
                              "记录期初通过结论：无历史票款",
                              "不创建 0 元回款单或 0 元发票",
                              "记录复核结论并完成任务",
                          ]
                        : [
                              "记录本次复核并完成任务",
                              "提交时核对数据版本，不一致将阻断",
                              "同本次提交完成当前任务",
                          ]
                }
                pending={completeMutation.isPending}
                onConfirm={async () => {
                    if (confirmMode?.kind === "zero") {
                        await runApprove(
                            "NO_HISTORY_FROM_ZERO",
                            confirmMode.advance,
                        )
                    } else if (confirmMode?.kind === "approve") {
                        await runApprove(
                            confirmMode.conclusion,
                            confirmMode.advance,
                        )
                    }
                }}
            />

            <FormalActionConfirmDialog
                open={confirmMode?.kind === "hold"}
                onOpenChange={(open) => {
                    if (!open) setConfirmMode(null)
                }}
                title="先跳过当前复核任务"
                description="跳过当前任务后，任务仍保留在待处理列表与「已跳过」范围；不生成复核记录。可手动浏览下一项。"
                actionLabel="先跳过"
                confirmLabel="确认跳过"
                fromStatus={{ label: "处理中", tone: "info" }}
                toStatus={{
                    label: "已跳过（仍在待处理列表）",
                    tone: "warning",
                }}
                effects={[
                    "任务保留在待处理列表",
                    "不生成复核记录",
                    "不自动切换下一项，结果面板提示后手动继续",
                ]}
                pending={holdMutation.isPending}
                onConfirm={() => void handleHold()}
            />

            <RejectReviewDialog
                open={confirmMode?.kind === "reject"}
                onOpenChange={(open) => {
                    if (!open) setConfirmMode(null)
                }}
                pending={completeMutation.isPending}
                onSubmit={submitReject}
            />

            {/* 证据/备注未保存时切换任务确认 */}
            <DiscardConfirmDialog
                open={pendingNav != null}
                onOpenChange={(open) => {
                    if (!open) setPendingNav(null)
                }}
                title="放弃未保存的证据或备注？"
                description="当前凭证编号、证据说明或备注尚未保存，切换任务后将丢失。"
                confirmLabel="放弃并切换"
                cancelLabel="继续编辑"
                onConfirm={() => {
                    const delta = pendingNav
                    setPendingNav(null)
                    if (delta != null) {
                        const target = neighborId(delta)
                        if (target) goToWorkItem(target)
                    }
                }}
            />
        </PageScaffold>
    )
}

function buildResultFacts(
    outcome?: FormalOutcome,
): { label: string; value: React.ReactNode }[] {
    if (!outcome) return []
    if (outcome.kind === "HELD") {
        return [
            {
                label: "任务状态",
                value:
                    outcome.workItemStatus === "IN_PROGRESS"
                        ? "处理中"
                        : outcome.workItemStatus === "PENDING"
                          ? "待处理"
                          : "已处理",
            },
            { label: "跳过时间", value: outcome.heldAt },
            { label: "恢复提示", value: outcome.resumeHint },
        ]
    }
    const biz = outcome.business
    const facts = [
        { label: "复核号", value: String(biz.reviewNo) },
        {
            label: "结论",
            value:
                biz.conclusion === "REJECTED"
                    ? "驳回"
                    : APPROVE_CONCLUSION_LABEL[
                          biz.conclusion as ApproveConclusion
                      ],
        },
        {
            label: "完成时间",
            value: new Date(biz.completedAt).toLocaleString("zh-CN", {
                hour12: false,
            }),
        },
        {
            label: versionText.dataVersion,
            value: (
                <span className="font-mono text-xs">
                    {shortHash(biz.subjectHash)}
                </span>
            ),
        },
    ]
    return facts
}
