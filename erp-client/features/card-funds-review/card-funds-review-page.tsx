"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import {
    CircleCheckIcon,
    PauseIcon,
    ReceiptIcon,
    SearchIcon,
    TriangleAlertIcon,
    XIcon,
} from "lucide-react"
import { z } from "zod"

import {
    AllocationWorkspace,
    BusinessDiffPanel,
    BusinessEmptyState,
    BusinessFailureState,
    BusinessStatusBadge,
    DataFreshness,
    DiscardConfirmDialog,
    DocumentSummary,
    FormalActionConfirmDialog,
    FormalActionResult,
    MetricItem,
    MetricStrip,
    ListToolbar,
    OptionCombobox,
    PageHeader,
    PageScaffold,
    SequentialProcessBar,
    surfaceInsetClassName,
    surfacePanelClassName,
} from "@/components/business"
import { cn } from "@/lib/utils"
import { type ResultState as SharedResultState } from "@/components/business/feedback"
import { useAppForm } from "@/components/form"
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
import {
    Dialog,
    DialogClose,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { DatePicker } from "@/components/ui/date-picker"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Separator } from "@/components/ui/separator"
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"
import type {
    AllocationDraftLine,
    ApproveConclusion,
    CardFundsReviewDecision,
    FormalOutcome,
    RejectReasonCode,
    ReviewType,
} from "@/features/card-funds-review/types"
import {
    APPROVE_CONCLUSION_LABEL,
    REJECT_FOLLOW_UP_COLLABORATION,
    REJECT_REASON_LABEL,
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
} from "@/features/card-funds-review/queries"
import { freshnessText, openWorkspaceLabel, versionText } from "@/lib/ui-text"
import { formatDateTime } from "@/lib/datetime"
import { getErrorMessage } from "@/lib/api/errors"

const money = new Intl.NumberFormat("zh-CN", {
    style: "currency",
    currency: "CNY",
    minimumFractionDigits: 2,
})

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

function shortHash(hash: string) {
    if (hash.length <= 20) return hash
    return `${hash.slice(0, 12)}…${hash.slice(-6)}`
}

function formatMoney(value: string) {
    return money.format(Number(value) || 0)
}

const rejectSchema = z.object({
    reasonCode: z.enum([
        "EVIDENCE_INSUFFICIENT",
        "FACTS_MISMATCH",
        "COUNTERPARTY_UNCLEAR",
        "OTHER",
    ]),
    comment: z.string().trim().min(5, "请填写至少 5 个字的驳回说明"),
})

export function CardFundsReviewPage() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const scope: "mine" | "role_pool" =
        searchParams.get("scope") === "role_pool" ? "role_pool" : "mine"
    const typeParam = searchParams.get("type")
    const type: "all" | "opening" | "delta" =
        typeParam === "opening" || typeParam === "delta" ? typeParam : "all"
    const statusParam = searchParams.get("status")
    const status: "pending" | "held" =
        statusParam === "held" ? "held" : "pending"
    const dueParam = searchParams.get("due")
    const due: "all" | "today" | "overdue" =
        dueParam === "today" || dueParam === "overdue" ? dueParam : "all"
    const q = searchParams.get("q") ?? undefined
    const currentWorkItemId = searchParams.get("currentWorkItemId") ?? undefined
    const queueContextId =
        searchParams.get("queueContextId") ?? `queue:card-funds-review:${scope}`

    const autoNextExplicit = searchParams.get("autoNext")
    const [sessionAutoNext, setSessionAutoNext] = React.useState(true)
    const autoNext =
        autoNextExplicit === "0"
            ? false
            : autoNextExplicit === "1"
              ? true
              : sessionAutoNext

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
    const [searchInput, setSearchInput] = React.useState(q ?? "")
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

    // 搜索输入（q）与 URL 对齐
    React.useEffect(() => {
        setSearchInput(q ?? "")
    }, [q])

    // URL 默认：保留 queueContextId / scope / currentWorkItemId；
    // type 默认「all」不写 URL（默认值省略，D18）
    React.useEffect(() => {
        if (queueQuery.isPending || !view) return
        const hasScope = searchParams.has("scope")
        const hasType = searchParams.has("type")
        const hasItem = searchParams.has("currentWorkItemId")
        const hasCtx = searchParams.has("queueContextId")
        if (hasScope && hasType && hasCtx && (hasItem || tasks.length === 0))
            return
        const params = new URLSearchParams(searchParams.toString())
        if (!hasScope) params.set("scope", scope)
        if (!hasType && type !== "all") params.set("type", type)
        if (!hasCtx) params.set("queueContextId", queueContextId)
        if (!hasItem && task) {
            params.set("currentWorkItemId", task.workItem.workItemId)
        }
        const qs = params.toString()
        if (qs === searchParams.toString()) return
        router.replace(qs ? `${pathname}?${qs}` : pathname, { scroll: false })
    }, [
        queueQuery.isPending,
        view,
        searchParams,
        scope,
        type,
        queueContextId,
        task,
        tasks.length,
        pathname,
        router,
    ])

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

    const replaceUrl = React.useCallback(
        (patch: Record<string, string | null | undefined>) => {
            const params = new URLSearchParams(searchParams.toString())
            for (const [key, value] of Object.entries(patch)) {
                if (value == null || value === "") params.delete(key)
                else params.set(key, value)
            }
            // 跨 W05/W11 返回时不丢 queueContextId
            if (!params.has("queueContextId")) {
                params.set("queueContextId", queueContextId)
            }
            const qs = params.toString()
            router.replace(qs ? `${pathname}?${qs}` : pathname, {
                scroll: false,
            })
        },
        [pathname, queueContextId, router, searchParams],
    )

    // 300ms 防抖写 URL；q/replaceUrl 入依赖保证闭包不陈旧，
    // 避免防抖期间切换 scope/type 等参数后被旧 URL 快照覆盖（D18 竞态）
    React.useEffect(() => {
        const handle = globalThis.setTimeout(() => {
            if (searchInput.trim() === (q ?? "")) return
            replaceUrl({
                q: searchInput.trim() || null,
                currentWorkItemId: null,
            })
        }, 300)
        return () => globalThis.clearTimeout(handle)
    }, [q, replaceUrl, searchInput])

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
    }, [replaceUrl])

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

    const rejectForm = useAppForm({
        defaultValues: {
            reasonCode: "EVIDENCE_INSUFFICIENT" as RejectReasonCode,
            comment: "",
        },
        validators: { onChange: rejectSchema },
        onSubmit: async ({ value }) => {
            if (!task) return
            setActionError(null)
            try {
                const lease = await ensureLease()
                const base = buildDecisionBase("REJECTED")
                const decision: CardFundsReviewDecision = {
                    ...base,
                    reviewResult: "REJECTED",
                    conclusion: "REJECTED",
                    reasonCode: value.reasonCode as RejectReasonCode,
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
    })

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

            <div
                className={`${surfacePanelClassName} sticky top-0 z-10 space-y-2.5 px-3 py-2.5 text-sm`}
            >
                <div className="flex flex-wrap items-center gap-3">
                    <div
                        role="group"
                        aria-label="责任范围"
                        className="inline-flex items-center rounded-lg bg-muted p-0.5 ring-1 ring-foreground/10"
                    >
                        {(
                            [
                                { value: "mine" as const, label: "我的待办" },
                                {
                                    value: "role_pool" as const,
                                    label: "团队待认领",
                                },
                            ] as const
                        ).map((opt) => (
                            <button
                                key={opt.value}
                                type="button"
                                aria-pressed={scope === opt.value}
                                onClick={() =>
                                    replaceUrl({
                                        scope:
                                            opt.value === "mine"
                                                ? null
                                                : opt.value,
                                        queueContextId: null,
                                        currentWorkItemId: null,
                                    })
                                }
                                className={cn(
                                    "inline-flex h-7 items-center gap-1.5 rounded-md px-2.5 text-sm transition-all outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                    scope === opt.value
                                        ? "bg-card font-medium text-foreground shadow-sm ring-1 ring-foreground/10"
                                        : "font-normal text-muted-foreground hover:bg-foreground/5 hover:text-foreground",
                                )}
                            >
                                {opt.label}
                            </button>
                        ))}
                    </div>
                    <div
                        role="group"
                        aria-label="任务类型"
                        className="inline-flex items-center rounded-lg bg-muted p-0.5 ring-1 ring-foreground/10"
                    >
                        {(
                            [
                                { value: "all" as const, label: "全部类型" },
                                { value: "opening" as const, label: "期初" },
                                { value: "delta" as const, label: "同步差额" },
                            ] as const
                        ).map((opt) => (
                            <button
                                key={opt.value}
                                type="button"
                                aria-pressed={type === opt.value}
                                onClick={() =>
                                    replaceUrl({
                                        type: opt.value,
                                        currentWorkItemId: null,
                                    })
                                }
                                className={cn(
                                    "inline-flex h-7 items-center gap-1.5 rounded-md px-2.5 text-sm transition-all outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                    type === opt.value
                                        ? "bg-card font-medium text-foreground shadow-sm ring-1 ring-foreground/10"
                                        : "font-normal text-muted-foreground hover:bg-foreground/5 hover:text-foreground",
                                )}
                            >
                                {opt.label}
                            </button>
                        ))}
                    </div>
                    <div
                        role="group"
                        aria-label="到期时限"
                        className="inline-flex items-center rounded-lg bg-muted p-0.5 ring-1 ring-foreground/10"
                    >
                        {(
                            [
                                { value: "all" as const, label: "全部时限" },
                                { value: "today" as const, label: "今日到期" },
                                { value: "overdue" as const, label: "已超期" },
                            ] as const
                        ).map((opt) => (
                            <button
                                key={opt.value}
                                type="button"
                                aria-pressed={due === opt.value}
                                onClick={() =>
                                    replaceUrl({
                                        due:
                                            opt.value === "all"
                                                ? null
                                                : opt.value,
                                        currentWorkItemId: null,
                                    })
                                }
                                className={cn(
                                    "inline-flex h-7 items-center gap-1.5 rounded-md px-2.5 text-sm transition-all outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                    due === opt.value
                                        ? "bg-card font-medium text-foreground shadow-sm ring-1 ring-foreground/10"
                                        : "font-normal text-muted-foreground hover:bg-foreground/5 hover:text-foreground",
                                )}
                            >
                                {opt.label}
                            </button>
                        ))}
                    </div>
                    <div
                        role="group"
                        aria-label="队列范围"
                        className="inline-flex items-center rounded-lg bg-muted p-0.5 ring-1 ring-foreground/10"
                    >
                        {(
                            [
                                { value: "pending" as const, label: "待处理" },
                                { value: "held" as const, label: "已跳过" },
                            ] as const
                        ).map((opt) => (
                            <button
                                key={opt.value}
                                type="button"
                                aria-pressed={status === opt.value}
                                onClick={() =>
                                    replaceUrl({
                                        status:
                                            opt.value === "pending"
                                                ? null
                                                : opt.value,
                                        currentWorkItemId: null,
                                    })
                                }
                                className={cn(
                                    "inline-flex h-7 items-center gap-1.5 rounded-md px-2.5 text-sm transition-all outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                    status === opt.value
                                        ? "bg-card font-medium text-foreground shadow-sm ring-1 ring-foreground/10"
                                        : "font-normal text-muted-foreground hover:bg-foreground/5 hover:text-foreground",
                                )}
                            >
                                {opt.label}
                            </button>
                        ))}
                    </div>
                </div>
                <ListToolbar
                    aria-label="票款复核筛选"
                    search={
                        <InputGroup>
                            <InputGroupAddon>
                                <SearchIcon
                                    className="size-4"
                                    aria-hidden="true"
                                />
                            </InputGroupAddon>
                            <InputGroupInput
                                value={searchInput}
                                onChange={(e) => setSearchInput(e.target.value)}
                                placeholder="搜索单号 / 客户 / 往来主体"
                                aria-label="搜索复核队列"
                            />
                        </InputGroup>
                    }
                    actions={
                        <div className="flex items-center gap-2">
                            <Label
                                htmlFor="auto-next"
                                className="text-muted-foreground"
                            >
                                自动下一项
                            </Label>
                            <Switch
                                id="auto-next"
                                checked={autoNext}
                                onCheckedChange={(on) => {
                                    setSessionAutoNext(on)
                                    replaceUrl({ autoNext: on ? "1" : "0" })
                                }}
                            />
                        </div>
                    }
                />
            </div>

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
                                    <DocumentSummary
                                        columns="two"
                                        items={[
                                            {
                                                id: "order",
                                                label: "卡券销售单",
                                                value: task.salesOrder.orderNo,
                                                emphasized: true,
                                            },
                                            {
                                                id: "hash",
                                                label: "当前数据版本",
                                                value: (
                                                    <span className="num font-mono text-sm">
                                                        {shortHash(
                                                            task.workItem
                                                                .subjectHash,
                                                        )}
                                                    </span>
                                                ),
                                                description:
                                                    task.workItem.subjectHash,
                                            },
                                            {
                                                id: "counterparty",
                                                label: "收款/开票往来主体",
                                                value: task.account
                                                    .counterpartyPartyName,
                                            },
                                            {
                                                id: "reason",
                                                label: "任务原因",
                                                value: task.workItem.reason,
                                            },
                                        ]}
                                    />

                                    <MetricStrip
                                        columns={5}
                                        aria-label="票款记录指标"
                                    >
                                        <MetricItem
                                            label="同步成交额"
                                            value={formatMoney(
                                                task.account.syncedGrossAmount,
                                            )}
                                            detail="商城当前版本"
                                        />
                                        <MetricItem
                                            label="当前应收"
                                            value={formatMoney(
                                                task.account.grossTotal,
                                            )}
                                            detail={`开放 ${formatMoney(task.account.openTotal)}`}
                                        />
                                        <MetricItem
                                            label="净已收"
                                            value={formatMoney(
                                                task.account.settledTotal,
                                            )}
                                            detail="净额（已收减冲正）"
                                        />
                                        <MetricItem
                                            label="净已开票"
                                            value={formatMoney(
                                                task.account.invoicedTotal,
                                            )}
                                            detail={`可开 ${formatMoney(task.account.openInvoiceableTotal)}`}
                                        />
                                        <MetricItem
                                            label={versionText.versionStatus}
                                            value={task.fingerprintStatus.label}
                                            detail={
                                                task.fingerprintStatus.detail
                                            }
                                            status={{
                                                label: task.fingerprintStatus
                                                    .label,
                                                tone: task.fingerprintStatus
                                                    .tone,
                                            }}
                                        />
                                    </MetricStrip>

                                    <Alert
                                        variant={
                                            task.account.fundsReliability ===
                                            "VERIFIED"
                                                ? "default"
                                                : "destructive"
                                        }
                                    >
                                        <TriangleAlertIcon aria-hidden="true" />
                                        <AlertTitle>
                                            {task.account.fundsReliability ===
                                            "UNRELIABLE_PENDING_REVIEW"
                                                ? "票款指标不可靠（复核未完成）"
                                                : task.account
                                                        .fundsReliability ===
                                                    "STALE_FINGERPRINT"
                                                  ? "数据已变更 · 指标不可靠"
                                                  : "可靠性"}
                                        </AlertTitle>
                                        <AlertDescription>
                                            {task.account.reliabilityNote}
                                            复核未完成前，指标不可视为已核实。
                                        </AlertDescription>
                                    </Alert>

                                    {task.reviewType === "SYNC_DELTA" &&
                                    task.difference ? (
                                        <div className="space-y-2">
                                            {(() => {
                                                const moneyChanges =
                                                    task.difference!.changes.filter(
                                                        (c) =>
                                                            /成交额|应收|已收|已开票/.test(
                                                                c.field,
                                                            ) &&
                                                            Number.isFinite(
                                                                Number(
                                                                    c.before,
                                                                ),
                                                            ) &&
                                                            Number.isFinite(
                                                                Number(c.after),
                                                            ),
                                                    )
                                                if (moneyChanges.length === 0)
                                                    return null
                                                const totalDelta =
                                                    moneyChanges.reduce(
                                                        (s, c) =>
                                                            s +
                                                            (Number(c.after) -
                                                                Number(
                                                                    c.before,
                                                                )),
                                                        0,
                                                    )
                                                return (
                                                    <p className="text-sm text-muted-foreground">
                                                        金额类字段合计差额：{" "}
                                                        <span
                                                            className={
                                                                totalDelta >= 0
                                                                    ? "num text-foreground"
                                                                    : "num text-destructive"
                                                            }
                                                        >
                                                            {formatMoney(
                                                                Math.abs(
                                                                    totalDelta,
                                                                ).toFixed(2),
                                                            )}
                                                        </span>
                                                        {totalDelta >= 0
                                                            ? "（增加）"
                                                            : "（减少）"}
                                                    </p>
                                                )
                                            })()}
                                            <BusinessDiffPanel
                                                title={task.difference.title}
                                                caption="上一有效复核与当前记录对比（系统最新数据）"
                                                changes={task.difference.changes.map(
                                                    (c) => ({
                                                        id: c.id,
                                                        field: c.field,
                                                        before: c.before,
                                                        after: c.after,
                                                        note: [
                                                            c.note,
                                                            c.sourceObject,
                                                            c.occurredAt,
                                                        ]
                                                            .filter(Boolean)
                                                            .join(" · "),
                                                    }),
                                                )}
                                            />
                                        </div>
                                    ) : null}

                                    <Card
                                        size="sm"
                                        className={surfacePanelClassName}
                                    >
                                        <CardHeader className="border-b border-border/30 py-3">
                                            <CardTitle className="text-base">
                                                回款与发票明细
                                            </CardTitle>
                                            <CardDescription>
                                                仅展示客户往来业务记录；登记为新增分配，不覆盖已有金额
                                            </CardDescription>
                                        </CardHeader>
                                        <CardContent className="space-y-3 pt-4">
                                            {task.receiptFacts.length === 0 &&
                                            task.invoiceFacts.length === 0 ? (
                                                <p className="text-sm text-muted-foreground">
                                                    尚无回款/发票。可登记历史记录，或确认期初净额为
                                                    0 时选择从 0 起。
                                                </p>
                                            ) : null}
                                            {task.receiptFacts.map((r) => (
                                                <div
                                                    key={r.receiptId}
                                                    className={`${surfaceInsetClassName} px-3 py-2 text-sm`}
                                                >
                                                    <div className="flex flex-wrap gap-2 font-medium">
                                                        <ReceiptIcon className="size-4 text-muted-foreground" />
                                                        回款 {r.receiptNo}
                                                        {r.reversed ? (
                                                            <Badge variant="destructive">
                                                                已冲正
                                                            </Badge>
                                                        ) : null}
                                                    </div>
                                                    <p className="mt-1 text-muted-foreground">
                                                        {r.receivedAt} · 含税{" "}
                                                        {formatMoney(
                                                            r.grossAmount,
                                                        )}{" "}
                                                        · 分配本应收{" "}
                                                        {formatMoney(
                                                            r.allocatedToAccount,
                                                        )}
                                                        {r.otherAllocationSummary
                                                            ? ` · ${r.otherAllocationSummary}`
                                                            : ""}
                                                    </p>
                                                </div>
                                            ))}
                                            {task.invoiceFacts.map((inv) => (
                                                <div
                                                    key={inv.invoiceId}
                                                    className={`${surfaceInsetClassName} px-3 py-2 text-sm`}
                                                >
                                                    <div className="flex flex-wrap gap-2 font-medium">
                                                        发票 {inv.invoiceNo}
                                                        <Badge variant="outline">
                                                            {inv.direction ===
                                                            "BLUE"
                                                                ? "蓝字"
                                                                : "红字"}
                                                        </Badge>
                                                        {inv.reversed ? (
                                                            <Badge variant="destructive">
                                                                已红冲
                                                            </Badge>
                                                        ) : null}
                                                    </div>
                                                    <p className="mt-1 text-muted-foreground">
                                                        {inv.issuedAt} · 含税{" "}
                                                        {formatMoney(
                                                            inv.grossAmount,
                                                        )}{" "}
                                                        · 分配本子账{" "}
                                                        {formatMoney(
                                                            inv.allocatedToAccount,
                                                        )}
                                                    </p>
                                                </div>
                                            ))}
                                            <div className="flex flex-wrap gap-2 pt-1">
                                                <Button
                                                    type="button"
                                                    variant="secondary"
                                                    size="sm"
                                                    onClick={() =>
                                                        openAllocation(
                                                            "receipt",
                                                        )
                                                    }
                                                >
                                                    登记历史回款
                                                </Button>
                                                <Button
                                                    type="button"
                                                    variant="secondary"
                                                    size="sm"
                                                    onClick={() =>
                                                        openAllocation(
                                                            "invoice",
                                                        )
                                                    }
                                                >
                                                    登记历史发票
                                                </Button>
                                                <Button
                                                    type="button"
                                                    variant="outline"
                                                    size="sm"
                                                    render={
                                                        <Link href={w11Href} />
                                                    }
                                                >
                                                    {openWorkspaceLabel("W11")}
                                                </Button>
                                            </div>
                                        </CardContent>
                                    </Card>

                                    {allocationMode ? (
                                        <div className="space-y-3">
                                            <Card
                                                size="sm"
                                                className={
                                                    surfacePanelClassName
                                                }
                                            >
                                                <CardHeader className="border-b border-border/30 py-3">
                                                    <CardTitle className="text-base">
                                                        {allocationMode ===
                                                        "receipt"
                                                            ? "登记历史回款"
                                                            : "登记历史发票"}
                                                    </CardTitle>
                                                    <CardDescription>
                                                        登记为新增分配，不覆盖已有金额；禁止
                                                        0 元单据
                                                    </CardDescription>
                                                </CardHeader>
                                                <CardContent className="grid gap-3 pt-4 sm:grid-cols-2">
                                                    {allocationMode ===
                                                    "receipt" ? (
                                                        <>
                                                            <div className="space-y-1.5">
                                                                <Label htmlFor="rcpt-no">
                                                                    回款单号
                                                                </Label>
                                                                <Input
                                                                    id="rcpt-no"
                                                                    value={
                                                                        receiptForm.receiptNo
                                                                    }
                                                                    onChange={(
                                                                        e,
                                                                    ) =>
                                                                        setReceiptForm(
                                                                            (
                                                                                f,
                                                                            ) => ({
                                                                                ...f,
                                                                                receiptNo:
                                                                                    e
                                                                                        .target
                                                                                        .value,
                                                                            }),
                                                                        )
                                                                    }
                                                                    placeholder="可空则系统生成"
                                                                />
                                                            </div>
                                                            <div className="space-y-1.5">
                                                                <Label htmlFor="rcpt-amt">
                                                                    含税金额
                                                                </Label>
                                                                <Input
                                                                    id="rcpt-amt"
                                                                    className="num"
                                                                    value={
                                                                        receiptForm.grossAmount
                                                                    }
                                                                    onChange={(
                                                                        e,
                                                                    ) => {
                                                                        const grossAmount =
                                                                            e
                                                                                .target
                                                                                .value
                                                                        setReceiptForm(
                                                                            (
                                                                                f,
                                                                            ) => ({
                                                                                ...f,
                                                                                grossAmount,
                                                                            }),
                                                                        )
                                                                        setAllocLines(
                                                                            (
                                                                                lines,
                                                                            ) =>
                                                                                lines.map(
                                                                                    (
                                                                                        l,
                                                                                        i,
                                                                                    ) =>
                                                                                        i ===
                                                                                        0
                                                                                            ? {
                                                                                                  ...l,
                                                                                                  amount:
                                                                                                      grossAmount ||
                                                                                                      "0.00",
                                                                                              }
                                                                                            : l,
                                                                                ),
                                                                        )
                                                                    }}
                                                                    placeholder="须 > 0"
                                                                />
                                                            </div>
                                                            <div className="space-y-1.5">
                                                                <Label htmlFor="rcpt-at">
                                                                    到账日期
                                                                </Label>
                                                                <DatePicker
                                                                    value={
                                                                        receiptForm.receivedAt ||
                                                                        undefined
                                                                    }
                                                                    onValueChange={(
                                                                        next,
                                                                    ) =>
                                                                        setReceiptForm(
                                                                            (
                                                                                f,
                                                                            ) => ({
                                                                                ...f,
                                                                                receivedAt:
                                                                                    next ??
                                                                                    "",
                                                                            }),
                                                                        )
                                                                    }
                                                                />
                                                            </div>
                                                        </>
                                                    ) : (
                                                        <>
                                                            <div className="space-y-1.5">
                                                                <Label htmlFor="inv-no">
                                                                    发票号码
                                                                </Label>
                                                                <Input
                                                                    id="inv-no"
                                                                    value={
                                                                        invoiceForm.invoiceNo
                                                                    }
                                                                    onChange={(
                                                                        e,
                                                                    ) =>
                                                                        setInvoiceForm(
                                                                            (
                                                                                f,
                                                                            ) => ({
                                                                                ...f,
                                                                                invoiceNo:
                                                                                    e
                                                                                        .target
                                                                                        .value,
                                                                            }),
                                                                        )
                                                                    }
                                                                />
                                                            </div>
                                                            <div className="space-y-1.5">
                                                                <Label htmlFor="inv-amt">
                                                                    含税金额
                                                                </Label>
                                                                <Input
                                                                    id="inv-amt"
                                                                    className="num"
                                                                    value={
                                                                        invoiceForm.grossAmount
                                                                    }
                                                                    onChange={(
                                                                        e,
                                                                    ) => {
                                                                        const grossAmount =
                                                                            e
                                                                                .target
                                                                                .value
                                                                        setInvoiceForm(
                                                                            (
                                                                                f,
                                                                            ) => ({
                                                                                ...f,
                                                                                grossAmount,
                                                                            }),
                                                                        )
                                                                        setAllocLines(
                                                                            (
                                                                                lines,
                                                                            ) =>
                                                                                lines.map(
                                                                                    (
                                                                                        l,
                                                                                        i,
                                                                                    ) =>
                                                                                        i ===
                                                                                        0
                                                                                            ? {
                                                                                                  ...l,
                                                                                                  amount:
                                                                                                      grossAmount ||
                                                                                                      "0.00",
                                                                                              }
                                                                                            : l,
                                                                                ),
                                                                        )
                                                                    }}
                                                                    placeholder="须 > 0"
                                                                />
                                                            </div>
                                                            <div className="space-y-1.5">
                                                                <Label htmlFor="inv-at">
                                                                    开票日期
                                                                </Label>
                                                                <DatePicker
                                                                    value={
                                                                        invoiceForm.issuedAt ||
                                                                        undefined
                                                                    }
                                                                    onValueChange={(
                                                                        next,
                                                                    ) =>
                                                                        setInvoiceForm(
                                                                            (
                                                                                f,
                                                                            ) => ({
                                                                                ...f,
                                                                                issuedAt:
                                                                                    next ??
                                                                                    "",
                                                                            }),
                                                                        )
                                                                    }
                                                                />
                                                            </div>
                                                        </>
                                                    )}
                                                </CardContent>
                                            </Card>

                                            <AllocationWorkspace
                                                title="多对多分配"
                                                description="分配合计须等于本次单据含税金额；登记不覆盖已有金额，差额以提交后系统结果为准。"
                                                summary={{
                                                    totalToAllocate:
                                                        formatMoney(
                                                            moneyStrSafe(
                                                                allocTarget,
                                                            ),
                                                        ),
                                                    allocated: formatMoney(
                                                        moneyStrSafe(
                                                            allocatedSum,
                                                        ),
                                                    ),
                                                    difference: formatMoney(
                                                        moneyStrSafe(
                                                            allocTarget -
                                                                allocatedSum,
                                                        ),
                                                    ),
                                                }}
                                                allocations={allocLines}
                                                getRowId={(row) => row.lineId}
                                                columns={[
                                                    {
                                                        id: "target",
                                                        header: "分配对象",
                                                        renderValue: ({
                                                            item,
                                                        }) => item.targetLabel,
                                                        renderEditor: ({
                                                            item,
                                                        }) => (
                                                            <span className="text-sm">
                                                                {
                                                                    item.targetLabel
                                                                }
                                                            </span>
                                                        ),
                                                    },
                                                    {
                                                        id: "amount",
                                                        header: "分配金额",
                                                        numeric: true,
                                                        align: "end",
                                                        renderValue: ({
                                                            item,
                                                        }) =>
                                                            formatMoney(
                                                                item.amount,
                                                            ),
                                                        renderEditor: ({
                                                            item,
                                                            rowIndex,
                                                        }) => (
                                                            <Input
                                                                className="num"
                                                                value={
                                                                    item.amount
                                                                }
                                                                onChange={(
                                                                    e,
                                                                ) => {
                                                                    const amount =
                                                                        e.target
                                                                            .value
                                                                    setAllocLines(
                                                                        (
                                                                            lines,
                                                                        ) =>
                                                                            lines.map(
                                                                                (
                                                                                    l,
                                                                                    i,
                                                                                ) =>
                                                                                    i ===
                                                                                    rowIndex
                                                                                        ? {
                                                                                              ...l,
                                                                                              amount,
                                                                                          }
                                                                                        : l,
                                                                            ),
                                                                    )
                                                                }}
                                                            />
                                                        ),
                                                    },
                                                ]}
                                                onAddAllocation={() => {
                                                    if (!task) return
                                                    setAllocLines((lines) => [
                                                        ...lines,
                                                        {
                                                            lineId: `al_${Date.now().toString(36)}`,
                                                            targetAccountId:
                                                                task.account.id,
                                                            targetLabel: `${task.salesOrder.orderNo} · 本应收`,
                                                            amount: "0.00",
                                                        },
                                                    ])
                                                }}
                                                onRemoveAllocation={(
                                                    _row,
                                                    _id,
                                                    rowIndex,
                                                ) => {
                                                    setAllocLines((lines) =>
                                                        lines.length <= 1
                                                            ? lines
                                                            : lines.filter(
                                                                  (_, i) =>
                                                                      i !==
                                                                      rowIndex,
                                                              ),
                                                    )
                                                }}
                                                actions={
                                                    <>
                                                        <Button
                                                            type="button"
                                                            variant="outline"
                                                            onClick={() =>
                                                                setAllocationMode(
                                                                    null,
                                                                )
                                                            }
                                                        >
                                                            取消
                                                        </Button>
                                                        <Button
                                                            type="button"
                                                            disabled={
                                                                registerReceiptMutation.isPending ||
                                                                registerInvoiceMutation.isPending
                                                            }
                                                            onClick={() => {
                                                                if (
                                                                    allocationMode ===
                                                                    "receipt"
                                                                ) {
                                                                    void submitReceipt()
                                                                } else {
                                                                    void submitInvoice()
                                                                }
                                                            }}
                                                        >
                                                            提交分配
                                                        </Button>
                                                    </>
                                                }
                                            />
                                        </div>
                                    ) : null}
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
                            <Card size="sm" className={surfacePanelClassName}>
                                <CardHeader className="border-b border-border/30 py-3">
                                    <CardTitle className="text-base">
                                        复核记录（只读）
                                    </CardTitle>
                                    <CardDescription>
                                        历史复核记录只读，不可修改或删除；本次将形成复核号{" "}
                                        {task.reviewChain.nextReviewNo}
                                    </CardDescription>
                                </CardHeader>
                                <CardContent className="space-y-3 pt-4">
                                    {task.reviewChain.items.length === 0 ? (
                                        <p className="text-sm text-muted-foreground">
                                            尚无历史复核。本次通过/驳回将形成首条复核记录。
                                        </p>
                                    ) : (
                                        task.reviewChain.items.map((item) => (
                                            <div
                                                key={item.reviewId}
                                                className="rounded-lg border border-border px-3 py-2 text-sm"
                                            >
                                                <div className="flex flex-wrap items-center gap-2">
                                                    <span className="font-medium">
                                                        复核号 {item.reviewNo}
                                                    </span>
                                                    <Badge variant="outline">
                                                        {
                                                            REVIEW_TYPE_LABEL[
                                                                item.reviewType
                                                            ]
                                                        }
                                                    </Badge>
                                                    <BusinessStatusBadge
                                                        context="list"
                                                        label={
                                                            item.reviewResult ===
                                                            "APPROVED"
                                                                ? "通过"
                                                                : "驳回"
                                                        }
                                                        tone={
                                                            item.reviewResult ===
                                                            "APPROVED"
                                                                ? "success"
                                                                : "destructive"
                                                        }
                                                    />
                                                    <Badge variant="secondary">
                                                        只读
                                                    </Badge>
                                                </div>
                                                <p className="mt-1 text-muted-foreground">
                                                    {item.reviewerLabel} ·{" "}
                                                    {item.completedAt}
                                                </p>
                                            </div>
                                        ))
                                    )}
                                </CardContent>
                            </Card>

                            <Card size="sm" className={surfacePanelClassName}>
                                <CardHeader className="border-b border-border/30 py-3">
                                    <CardTitle className="text-base">
                                        证据与导航
                                    </CardTitle>
                                </CardHeader>
                                <CardContent className="space-y-3 pt-4 text-sm">
                                    <p className="text-muted-foreground">
                                        {task.workItem.impact}
                                    </p>
                                    <Separator />
                                    <div className="flex flex-col gap-2">
                                        <Button
                                            type="button"
                                            variant="outline"
                                            size="sm"
                                            render={<Link href={w05Href} />}
                                        >
                                            打开销售单
                                        </Button>
                                        <Button
                                            type="button"
                                            variant="outline"
                                            size="sm"
                                            render={<Link href={w11Href} />}
                                        >
                                            {openWorkspaceLabel("W11")}
                                        </Button>
                                    </div>
                                </CardContent>
                            </Card>
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

            <Dialog
                open={confirmMode?.kind === "reject"}
                onOpenChange={(open) => {
                    if (!open) setConfirmMode(null)
                }}
            >
                <DialogContent className="sm:max-w-lg">
                    <DialogHeader>
                        <DialogTitle>驳回复核</DialogTitle>
                        <DialogDescription>
                            仅记录本次驳回并完成任务；未决问题不会自动创建后续任务。
                        </DialogDescription>
                    </DialogHeader>
                    <form
                        className="space-y-3"
                        onSubmit={(e) => {
                            e.preventDefault()
                            void rejectForm.handleSubmit()
                        }}
                    >
                        <rejectForm.AppField
                            name="reasonCode"
                            children={(field) => (
                                <div className="space-y-1.5">
                                    <Label>驳回原因</Label>
                                    <OptionCombobox
                                        value={field.state.value}
                                        onValueChange={(v) =>
                                            field.handleChange(
                                                v as RejectReasonCode,
                                            )
                                        }
                                        options={(
                                            Object.keys(
                                                REJECT_REASON_LABEL,
                                            ) as RejectReasonCode[]
                                        ).map((code) => ({
                                            value: code,
                                            label: REJECT_REASON_LABEL[code],
                                        }))}
                                        className="w-full"
                                        allowClear={false}
                                    />
                                </div>
                            )}
                        />
                        <rejectForm.AppField
                            name="comment"
                            children={(field) => (
                                <div className="space-y-1.5">
                                    <Label htmlFor="reject-comment">
                                        补充说明
                                    </Label>
                                    <Textarea
                                        id="reject-comment"
                                        value={field.state.value}
                                        onChange={(e) =>
                                            field.handleChange(e.target.value)
                                        }
                                        onBlur={field.handleBlur}
                                        rows={3}
                                    />
                                    {field.state.meta.errors?.[0] ? (
                                        <p className="text-xs text-destructive">
                                            {String(field.state.meta.errors[0])}
                                        </p>
                                    ) : null}
                                </div>
                            )}
                        />
                        <DialogFooter>
                            <DialogClose
                                render={
                                    <Button type="button" variant="outline" />
                                }
                            >
                                取消
                            </DialogClose>
                            <Button
                                type="submit"
                                variant="destructive"
                                disabled={completeMutation.isPending}
                            >
                                确认驳回
                            </Button>
                        </DialogFooter>
                    </form>
                </DialogContent>
            </Dialog>

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

function moneyStrSafe(n: number): string {
    if (!Number.isFinite(n)) return "0.00"
    return n.toFixed(2)
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
