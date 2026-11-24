"use client"

import {
    useCallback,
    useEffect,
    useMemo,
    useRef,
    useState,
    type ReactNode,
} from "react"
import { useQueryClient } from "@tanstack/react-query"
import { useStore } from "@tanstack/react-form"
import { TriangleAlertIcon } from "lucide-react"
import { FormalActionConfirmDialog } from "@/components/business"
import { DetailWorkspaceSection } from "@/components/business/detail-workspace"
import { useAppForm } from "@/components/form"
import { toast } from "@/components/ui/toast"
import type { WorkItemDto } from "@/features/work-items/types"
import { PortalAdminCommandConflict } from "@/features/supplier-portal/components/admin-command-conflict"
import { PortalError } from "@/features/supplier-portal/components/surface"
import { kindLabels } from "@/features/supplier-portal/lib/presentation"
import type {
    PortalApplication,
    PortalDictionary,
} from "@/features/supplier-portal/types"
import { isApiError } from "@/lib/api"
import { commandFailureDisposition } from "@/lib/api/command-recovery"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    FormalCommandKeyLedger,
    type FormalCommandIdentity,
} from "@/lib/formal-command"
import {
    BPM_REASON_MAX_UTF8_BYTES,
    utf8ByteLength,
} from "@/lib/validation/reason-length"
import { subscribeWorkspaceHotkeyAction } from "@/lib/workspace-hotkeys"
import { useWorkspaceDirtyState } from "@/lib/workspace-form-guard"
import { reviewPortalApplication } from "../api"
import { PortalReviewFooter } from "../components/review-footer"
import { PortalReviewOverview } from "../components/review-overview"
import {
    clearPortalReviewAttempt,
    clearPortalReviewDraft,
    portalReviewStorageKey,
    readPortalReviewAttempt,
    readPortalReviewDraft,
    savePortalReviewAttempt,
    savePortalReviewDraft,
    type ReviewCommand,
    type ReviewDecision,
} from "../lib/review-draft"
import { portalReviewOverview } from "../lib/review-overview"
import { reviewProduct } from "../lib/review-mapping-plan"
import { portalAdminApplicationKey } from "../hooks"
import { PortalDictionaryCreateDialog } from "../components/dictionary-create-dialog"
import {
    BasicDuplicateSection,
    BrandCategorySection,
    CollapsibleReviewSection,
    DecisionSubmitSection,
    ReviewProgressRail,
    SkuMappingSection,
    buildApproveReviewPayload,
    createReviewDefaultValues,
    evaluateReviewSections,
    fieldIdToSectionId,
    reviewFormBaseOptions,
    type CreateDictionaryRequest,
    type ReviewSectionId,
} from "../components/review-sections"
import {
    portalAdminKeys,
    usePortalAdminAccess,
    usePortalAdminCommand,
    usePortalDuplicates,
    useReviewDictionary,
} from "../hooks"

const DEFAULT_EXPANDED_SECTIONS: Record<ReviewSectionId, boolean> = {
    basic: true,
    "brand-category": true,
    "sku-mapping": true,
    decision: true,
}

export function PortalReviewForm({
    application,
    task,
    onSuccess,
    onReload,
    onPendingChange,
    workspace,
}: {
    application: PortalApplication
    task: WorkItemDto
    onSuccess: () => void
    onReload: () => Promise<void>
    onPendingChange?: (pending: boolean) => void
    workspace?: { details: ReactNode; fallbackSupplier?: string }
}) {
    const access = usePortalAdminAccess()
    const rawProduct = useMemo(() => reviewProduct(application), [application])
    const actorId = access.profile.data?.userid
    const canReview = access.can("supplier_portal_request:review")
    const matchesTask =
        task.handler_key === "supplier_portal_review" &&
        task.business_object_type === "supplier_portal_request" &&
        task.business_object_id === application.id
    const [completed, setCompleted] = useState(false)
    const canProcess =
        !completed &&
        canReview &&
        Boolean(actorId) &&
        matchesTask &&
        application.status === "pending" &&
        task.status === "OPEN" &&
        !!task.allowed_actions?.includes("PROCESS")
    const storageKey =
        actorId && canReview && matchesTask
            ? portalReviewStorageKey(actorId, application.id, task.id)
            : undefined
    const [draft] = useState(() =>
        storageKey
            ? readPortalReviewDraft(storageKey, application.version)
            : undefined,
    )
    const [attempt, setAttempt] = useState<
        FormalCommandIdentity<ReviewCommand> | undefined
    >(() =>
        storageKey ? readPortalReviewAttempt(storageKey, task.id) : undefined,
    )
    const retryDecision = attempt?.payload.decision ?? null
    const canRecover = Boolean(
        attempt &&
        storageKey &&
        (canProcess || (task.owner_user?.id ?? task.owner_user_id) === actorId),
    )
    const [ledger] = useState(() => new FormalCommandKeyLedger())
    const running = useRef(false)
    const [basis, setBasis] = useState({
        version: application.version,
        taskVersion: task.task_version,
    })
    const basisChanged =
        basis.version !== application.version ||
        basis.taskVersion !== task.task_version
    const prefix = `supplier-portal-review-${toAutomationIdSegment(task.id)}`

    const brands = useReviewDictionary("brand", canProcess && !!rawProduct)
    const categories = useReviewDictionary(
        "category",
        canProcess && !!rawProduct,
    )
    const units = useReviewDictionary("unit", canProcess && !!rawProduct)
    const [search, setSearch] = useState(
        draft?.search ?? rawProduct?.name ?? "",
    )
    const [q, setQ] = useState(draft?.search ?? rawProduct?.name ?? "")
    const duplicates = usePortalDuplicates(
        application.id,
        q,
        canProcess && !!rawProduct,
    )

    const [error, setError] = useState<unknown>(null)
    const [needsReload, setNeedsReload] = useState(false)
    const conflict = !attempt && (needsReload || basisChanged)
    const [confirmApprove, setConfirmApprove] = useState(false)
    const [returnOpen, setReturnOpen] = useState(false)
    const [highlightedFieldId, setHighlightedFieldId] = useState<string | null>(
        null,
    )
    const highlightTimerRef = useRef<number | null>(null)
    const [nowMs, setNowMs] = useState<number | null>(null)
    const client = useQueryClient()
    const [createDictionary, setCreateDictionary] =
        useState<CreateDictionaryRequest | null>(null)
    const [reportedConfirmed, setReportedConfirmed] = useState(
        draft?.reportedConfirmed ?? false,
    )
    const [expandedSections, setExpandedSections] = useState<
        Record<ReviewSectionId, boolean>
    >(DEFAULT_EXPANDED_SECTIONS)
    const [activeSectionId, setActiveSectionId] =
        useState<ReviewSectionId>("basic")

    const existingCandidates = useMemo(
        () =>
            duplicates.data?.existing_offerings ??
            application.existing_offerings ??
            [],
        [application.existing_offerings, duplicates.data?.existing_offerings],
    )
    const initialAvailability =
        application.kind === "new_product" || application.kind === "quote"
    const mutation = usePortalAdminCommand((body: Record<string, unknown>) =>
        reviewPortalApplication(application.id, body),
    )

    useEffect(() => {
        if (!initialAvailability) return
        const tick = () => setNowMs(Date.now())
        tick()
        const timer = window.setInterval(tick, 60_000)
        return () => window.clearInterval(timer)
    }, [initialAvailability])

    useEffect(() => {
        onPendingChange?.(mutation.isPending)
    }, [mutation.isPending, onPendingChange])

    const expandAndScrollToSection = useCallback(
        (sectionId: ReviewSectionId) => {
            setExpandedSections((prev) =>
                prev[sectionId] ? prev : { ...prev, [sectionId]: true },
            )
            setActiveSectionId(sectionId)
            window.requestAnimationFrame(() => {
                const target = document.getElementById(
                    `supplier-portal-review-step-${sectionId}`,
                )
                target?.scrollIntoView({
                    behavior: "smooth",
                    block: "start",
                })
            })
        },
        [],
    )

    useEffect(() => {
        return () => {
            if (highlightTimerRef.current != null) {
                window.clearTimeout(highlightTimerRef.current)
            }
        }
    }, [])

    const focusInvalidField = useCallback((fieldId: string) => {
        const targetSection = fieldIdToSectionId(fieldId)
        setExpandedSections((prev) =>
            prev[targetSection] ? prev : { ...prev, [targetSection]: true },
        )
        setActiveSectionId(targetSection)
        setHighlightedFieldId(fieldId)
        if (highlightTimerRef.current != null) {
            window.clearTimeout(highlightTimerRef.current)
        }
        highlightTimerRef.current = window.setTimeout(() => {
            setHighlightedFieldId(null)
        }, 2200)
        window.requestAnimationFrame(() => {
            window.requestAnimationFrame(() => {
                const target = document.getElementById(fieldId)
                if (!target) return
                const reducedMotion = window.matchMedia(
                    "(prefers-reduced-motion: reduce)",
                ).matches
                target.scrollIntoView({
                    behavior: reducedMotion ? "instant" : "smooth",
                    block: "center",
                })
                if (typeof target.focus === "function") {
                    target.focus({ preventScroll: true })
                }
            })
        })
    }, [])

    const defaultValues = draft?.values ?? createReviewDefaultValues(rawProduct)

    const form = useAppForm({
        ...reviewFormBaseOptions,
        defaultValues,
        onSubmit: async () => {
            // 审核决定使用底栏的业务校验与提交入口。
        },
    })

    const isFormDirty = useStore(form.store, (state) => state.isDirty)
    const formValues = useStore(form.store, (state) => state.values)
    const returnReasonValue = formValues.returnReason

    useEffect(() => {
        if (
            !storageKey ||
            !canProcess ||
            conflict ||
            attempt ||
            running.current
        )
            return
        if (
            !isFormDirty &&
            !reportedConfirmed &&
            q === (draft?.search ?? rawProduct?.name ?? "")
        )
            return
        try {
            savePortalReviewDraft(storageKey, {
                version: basis.version,
                values: formValues,
                reportedConfirmed,
                search: q,
            })
        } catch {
            setError(new Error("草稿暂存失败，请保留当前页面。"))
        }
    }, [
        attempt,
        basis.version,
        canProcess,
        conflict,
        draft?.search,
        formValues,
        isFormDirty,
        q,
        rawProduct?.name,
        reportedConfirmed,
        storageKey,
    ])

    useWorkspaceDirtyState({
        dirty: (canProcess && (isFormDirty || reportedConfirmed)) || canRecover,
        busy: mutation.isPending,
    })

    const canListProcurementPeople = access.can("procurement_person:list")

    const sections = useMemo(
        () =>
            evaluateReviewSections({
                values: formValues,
                rawProduct,
                brands: brands.data,
                categories: categories.data,
                units: units.data,
                duplicates: duplicates.data?.duplicates,
                existingCandidates,
                initialAvailability,
                reportedConfirmed,
                canListProcurementPeople,
            }),
        [
            brands.data,
            canListProcurementPeople,
            categories.data,
            duplicates.data?.duplicates,
            existingCandidates,
            formValues,
            initialAvailability,
            rawProduct,
            reportedConfirmed,
            units.data,
        ],
    )

    const sectionMap = useMemo(
        () =>
            new Map(sections.map((section) => [section.id, section] as const)),
        [sections],
    )

    const allIssues = useMemo(
        () => sections.flatMap((section) => section.issues),
        [sections],
    )

    const allExpanded = useMemo(
        () => sections.every((section) => expandedSections[section.id]),
        [expandedSections, sections],
    )

    const toggleAllSections = useCallback(() => {
        const nextOpen = !allExpanded
        setExpandedSections({
            basic: nextOpen,
            "brand-category": nextOpen,
            "sku-mapping": nextOpen,
            decision: nextOpen,
        })
    }, [allExpanded])

    const sectionIdsKey = sections.map((section) => section.id).join(",")

    // 滚动监听高亮当前可见区块
    useEffect(() => {
        if (typeof IntersectionObserver === "undefined") return
        const sectionIds = sectionIdsKey
            ? (sectionIdsKey.split(",") as ReviewSectionId[])
            : []
        const nodes = sectionIds
            .map((sectionId) =>
                document.getElementById(
                    `supplier-portal-review-step-${sectionId}`,
                ),
            )
            .filter((node): node is HTMLElement => node !== null)
        if (!nodes.length) return

        const observer = new IntersectionObserver(
            (entries) => {
                const visible = entries
                    .filter((entry) => entry.isIntersecting)
                    .sort(
                        (left, right) =>
                            left.boundingClientRect.top -
                            right.boundingClientRect.top,
                    )
                const topEntry = visible[0]
                const id = topEntry?.target.getAttribute(
                    "data-review-section",
                ) as ReviewSectionId | null
                if (id) {
                    setActiveSectionId(id)
                }
            },
            { rootMargin: "-15% 0px -65% 0px", threshold: 0 },
        )

        for (const node of nodes) {
            observer.observe(node)
        }
        return () => observer.disconnect()
    }, [sectionIdsKey])

    const buildApprovePayload = useCallback(
        (value: typeof formValues) => {
            if (application.kind === "new_product" && !rawProduct) {
                return {
                    ok: false as const,
                    fieldId: workspace
                        ? `${prefix}-details`
                        : "supplier-portal-review-section",
                    message:
                        "新品提报规格资料不完整，请重新读取或退回供应商补充。",
                }
            }
            return buildApproveReviewPayload({
                value,
                canProcess,
                canListProcurementPeople,
                application,
                taskId: task.id,
                taskVersion: task.task_version,
                rawProduct,
                brands: brands.data,
                categories: categories.data,
                units: units.data,
                duplicates: duplicates.data?.duplicates,
                existingCandidates,
                initialAvailability,
                reportedConfirmed,
            })
        },
        [
            application,
            brands.data,
            canListProcurementPeople,
            canProcess,
            categories.data,
            duplicates.data?.duplicates,
            existingCandidates,
            initialAvailability,
            prefix,
            rawProduct,
            reportedConfirmed,
            task.id,
            task.task_version,
            units.data,
            workspace,
        ],
    )

    const requestDecision = useCallback(
        (decision: ReviewDecision) => {
            if (
                running.current ||
                mutation.isPending ||
                conflict ||
                !canProcess ||
                attempt
            )
                return
            setError(null)
            setHighlightedFieldId(null)
            if (decision === "return") {
                document
                    .getElementById("supplier-portal-review-return-reason")
                    ?.focus()
                setReturnOpen(true)
                return
            }
            const check = buildApprovePayload(form.state.values)
            if (!check.ok) {
                setError(new Error(check.message))
                focusInvalidField(check.fieldId)
                return
            }
            setReturnOpen(false)
            setConfirmApprove(true)
        },
        [
            attempt,
            buildApprovePayload,
            canProcess,
            conflict,
            focusInvalidField,
            form,
            mutation.isPending,
        ],
    )

    useEffect(() => {
        if (!workspace || !canProcess) return
        return subscribeWorkspaceHotkeyAction((action) => {
            if (action === "approve") requestDecision("approve")
            if (action === "reject") requestDecision("return")
        })
    }, [canProcess, requestDecision, workspace])

    const returnReason = returnReasonValue.trim()
    const returnReasonBytes = utf8ByteLength(returnReason)
    const returnReasonChars = Array.from(returnReason).length
    const returnReasonError =
        returnReasonBytes > BPM_REASON_MAX_UTF8_BYTES
            ? `退回原因超出长度限制（当前 ${returnReasonBytes} 字节，上限 ${BPM_REASON_MAX_UTF8_BYTES} 字节）`
            : returnReasonChars > 500
              ? "退回原因最多 500 字符"
              : undefined
    const returnReasonValid = Boolean(returnReason) && !returnReasonError

    const executeDecision = async (decision?: ReviewDecision) => {
        if (!storageKey || running.current || mutation.isPending || conflict)
            return
        if (decision ? !canProcess || attempt : !canRecover) return
        setError(null)
        let identity = readPortalReviewAttempt(storageKey, task.id) ?? attempt
        if (identity && decision) {
            setAttempt(identity)
            setConfirmApprove(false)
            setReturnOpen(false)
            return
        }
        if (!identity) {
            if (!decision) return
            let body: Record<string, unknown>
            if (decision === "approve") {
                const check = buildApprovePayload(form.state.values)
                if (!check.ok) {
                    setConfirmApprove(false)
                    setError(new Error(check.message))
                    focusInvalidField(check.fieldId)
                    return
                }
                body = check.body
            } else {
                if (!returnReasonValid) return
                body = {
                    availability_reported_at_confirmed: false,
                    work_item_version: Number(task.task_version),
                    comment: returnReason,
                    maintainer_user_id: null,
                    normalized_product: null,
                    existing_product: null,
                    existing_offerings: [],
                }
            }
            try {
                identity = ledger.acquire<ReviewCommand>(
                    storageKey,
                    "supplier-portal:review",
                    {
                        ...body,
                        decision,
                        expected_version: application.version,
                        work_item_id: task.id,
                    },
                )
                savePortalReviewAttempt(storageKey, identity)
            } catch (cause) {
                ledger.settle(storageKey, "failed")
                setConfirmApprove(false)
                setError(cause)
                return
            }
        }

        running.current = true
        setAttempt(identity)
        setConfirmApprove(false)
        setReturnOpen(false)
        let result: PortalApplication
        try {
            result = await mutation.mutateAsync({
                ...identity.payload,
                idempotency_key: identity.idempotencyKey,
            })
        } catch (cause) {
            const canDetermineOutcome = !(
                decision === undefined &&
                isApiError(cause) &&
                (cause.status === 401 ||
                    cause.status === 403 ||
                    cause.status === 404)
            )
            const disposition = canDetermineOutcome
                ? commandFailureDisposition(cause)
                : "unknown"
            if (disposition !== "unknown") {
                clearPortalReviewAttempt(
                    storageKey,
                    task.id,
                    identity.idempotencyKey,
                )
                ledger.settle(storageKey, "failed")
                setAttempt(undefined)
            }
            setNeedsReload(disposition === "conflict")
            setError(cause)
            return
        } finally {
            running.current = false
        }

        clearPortalReviewAttempt(storageKey, task.id, identity.idempotencyKey)
        ledger.settle(storageKey, "succeeded")
        clearPortalReviewDraft(storageKey, identity.payload.expected_version)
        setCompleted(true)
        setAttempt(undefined)
        client.setQueryData<PortalApplication>(
            portalAdminApplicationKey(application.id),
            (current) =>
                !current || current.version < result.version ? result : current,
        )
        setHighlightedFieldId(null)
        toast.add({
            title:
                identity.payload.decision === "approve"
                    ? "已确认通过并生效"
                    : "已退回供应商修改",
            type: "success",
        })
        onSuccess()
    }

    const disabled =
        mutation.isPending || !canProcess || Boolean(attempt) || conflict
    const clearHighlight = useCallback(() => setHighlightedFieldId(null), [])

    const applicationLabel =
        application.title ??
        (application.input.name as string | undefined) ??
        kindLabels[application.kind]

    const approveSummary = [
        `申请类型：${kindLabels[application.kind]}`,
        `申请内容：${applicationLabel}${application.application_no ? `（${application.application_no}）` : ""}`,
        ...(application.supplier_name
            ? [`供应商：${application.supplier_name}`]
            : []),
        ...(rawProduct
            ? [
                  `商品建档方式：${
                      formValues.productTarget === "new"
                          ? "新建公司商品"
                          : `复用已有商品（${
                                duplicates.data?.duplicates.find(
                                    (item) =>
                                        item.product_id ===
                                        formValues.productTarget,
                                )?.name ?? "已选候选"
                            }）`
                  }`,
              ]
            : []),
        ...(formValues.maintainerUserId ? ["内部维护人：已指定"] : []),
    ]

    if (!workspace && application.status !== "pending" && !canRecover)
        return null

    if (!workspace && !canReview) {
        return (
            <DetailWorkspaceSection
                id="supplier-portal-review-section"
                title="采购确认"
                width="form"
            >
                <p className="text-xs text-muted-foreground">
                    当前处理人：{task.owner_user?.display_name ?? "待核对"}。
                </p>
                <PortalError error={error} />
            </DetailWorkspaceSection>
        )
    }

    const basicSection = sectionMap.get("basic")
    const brandCategorySection = sectionMap.get("brand-category")
    const skuMappingSection = sectionMap.get("sku-mapping")
    const decisionSection = sectionMap.get("decision")

    const overview = portalReviewOverview(application)
    const firstIssue = allIssues[0]
    const validationCount = canProcess ? allIssues.length : 0
    const firstEvidenceIssue = overview.outstanding[0]
    const issueCount = validationCount + overview.outstanding.length
    const checkProgress = canProcess
        ? sections.reduce(
              (totals, section) => ({
                  completed: totals.completed + section.completedCount,
                  total: totals.total + section.totalCount,
              }),
              {
                  completed: overview.qualifiedCount,
                  total: overview.qualificationChecks.length,
              },
          )
        : undefined
    const phrases =
        application.kind === "new_product"
            ? ["请补充商品资料或规格说明", "请核对计量单位和包装换算"]
            : application.kind === "stop"
              ? ["请补充停止供应原因及在途订单安排"]
              : application.kind === "cooperation"
                ? ["请补充结算及付款条件"]
                : ["请核对含税报价及生效日期", "请补充可供数量或供货说明"]

    const detailsAnchorId = `${prefix}-details`
    const locateNextIssue = () => {
        if (canProcess && firstIssue) {
            focusInvalidField(firstIssue.fieldId)
            return
        }
        setHighlightedFieldId(detailsAnchorId)
        if (highlightTimerRef.current != null) {
            window.clearTimeout(highlightTimerRef.current)
        }
        highlightTimerRef.current = window.setTimeout(() => {
            setHighlightedFieldId(null)
        }, 2200)
        window.requestAnimationFrame(() => {
            const target = document.getElementById(detailsAnchorId)
            if (!target) return
            const reducedMotion = window.matchMedia(
                "(prefers-reduced-motion: reduce)",
            ).matches
            target.focus({ preventScroll: true })
            target.scrollIntoView({
                behavior: reducedMotion ? "instant" : "smooth",
                block: "center",
            })
        })
    }

    return (
        <>
            {workspace ? (
                <div className="space-y-3.5">
                    {issueCount > 0 ? (
                        <div
                            role="status"
                            className="flex flex-wrap items-center gap-x-3 gap-y-1 rounded-r-md border-l-2 border-warning-border bg-warning-soft/45 px-3 py-2 text-xs text-warning-soft-foreground"
                        >
                            <TriangleAlertIcon
                                aria-hidden="true"
                                className="size-4 shrink-0"
                            />
                            <button
                                id={`${prefix}-locate-missing`}
                                type="button"
                                className="cursor-pointer font-medium underline underline-offset-2"
                                onClick={locateNextIssue}
                            >
                                <span className="num">{issueCount}</span>{" "}
                                项待完善 · 定位
                            </button>
                            <span className="min-w-0 wrap-anywhere">
                                {canProcess && firstIssue
                                    ? firstIssue.message
                                    : firstEvidenceIssue?.label}
                            </span>
                        </div>
                    ) : null}
                    <PortalReviewOverview
                        application={application}
                        fallbackSupplier={workspace.fallbackSupplier}
                        checkProgress={checkProgress}
                    />
                    <div
                        id={detailsAnchorId}
                        tabIndex={-1}
                        className={
                            highlightedFieldId === detailsAnchorId
                                ? "min-w-0 rounded-md ring-1 ring-info-border bg-info-soft/25 outline-none motion-safe:animate-pulse"
                                : "min-w-0 outline-none"
                        }
                    >
                        {workspace.details}
                    </div>
                </div>
            ) : null}
            {application.status === "pending" || canRecover ? (
                <DetailWorkspaceSection
                    id="supplier-portal-review-section"
                    title={workspace ? undefined : "采购确认"}
                    divider={!workspace}
                    width="full"
                    className={
                        workspace
                            ? "py-0 first:pt-0 last:pb-0"
                            : "@container/supplier-portal-review"
                    }
                >
                    <div className="max-w-4xl space-y-3 empty:hidden">
                        {!workspace ? (
                            <p className="text-xs text-muted-foreground">
                                当前处理人：
                                {task.owner_user?.display_name ?? "待核对"}。
                            </p>
                        ) : null}
                        {application.status === "pending" && !canProcess ? (
                            <p className="text-xs text-muted-foreground">
                                当前任务仅可查看，请由任务处理人确认。
                            </p>
                        ) : null}
                        {!attempt ? <PortalError error={error} /> : null}
                        {canProcess && conflict ? (
                            <PortalAdminCommandConflict
                                idPrefix="supplier-portal-review-conflict"
                                onReload={async () => {
                                    await onReload()
                                    if (rawProduct && canProcess) {
                                        const results = await Promise.all([
                                            brands.refetch(),
                                            categories.refetch(),
                                            units.refetch(),
                                            duplicates.refetch(),
                                        ])
                                        const failed = results.find(
                                            (result) => result.error,
                                        )
                                        if (failed?.error) throw failed.error
                                    }
                                }}
                                onConfirmed={() => {
                                    setBasis({
                                        version: application.version,
                                        taskVersion: task.task_version,
                                    })
                                    setNeedsReload(false)
                                    setConfirmApprove(false)
                                    setReportedConfirmed(false)
                                    form.setFieldValue("categoryHierarchy", [])
                                    setError(null)
                                }}
                            />
                        ) : null}
                        <PortalError
                            error={
                                brands.error ??
                                categories.error ??
                                units.error ??
                                duplicates.error
                            }
                            retry={() => {
                                void brands.refetch()
                                void categories.refetch()
                                void units.refetch()
                                void duplicates.refetch()
                            }}
                            id="supplier-portal-review-candidates-retry"
                        />
                    </div>

                    {canReview && createDictionary && rawProduct ? (
                        <PortalDictionaryCreateDialog
                            kind={createDictionary.kind}
                            productKind={rawProduct.product_kind}
                            rawName={createDictionary.rawName}
                            onClose={() => setCreateDictionary(null)}
                            onCreated={(created) => {
                                client.setQueryData<PortalDictionary[]>(
                                    [
                                        ...portalAdminKeys.all,
                                        "dictionary",
                                        createDictionary.kind,
                                    ],
                                    (rows) => [
                                        ...(rows ?? []).filter(
                                            (row) => row.id !== created.id,
                                        ),
                                        created,
                                    ],
                                )
                                if (createDictionary.kind === "brand") {
                                    form.setFieldValue("brandId", created.id)
                                    form.setFieldValue(
                                        "brandVersion",
                                        created.version,
                                    )
                                } else if (
                                    createDictionary.kind === "category"
                                ) {
                                    form.setFieldValue("categoryId", created.id)
                                    form.setFieldValue(
                                        "categoryVersion",
                                        created.version,
                                    )
                                    form.setFieldValue("categoryHierarchy", [])
                                    void categories.refetch().then((result) => {
                                        if (result.error) setError(result.error)
                                    })
                                } else {
                                    const index =
                                        form.state.values.mappings.findIndex(
                                            (row) =>
                                                row.rowId ===
                                                createDictionary.rowId,
                                        )
                                    if (index >= 0) {
                                        form.setFieldValue(
                                            `mappings[${index}].unitId`,
                                            created.id,
                                        )
                                        form.setFieldValue(
                                            `mappings[${index}].unitVersion`,
                                            created.version,
                                        )
                                        form.setFieldValue(
                                            `mappings[${index}].unitSynonymConfirmed`,
                                            false,
                                        )
                                        form.setFieldValue(
                                            `mappings[${index}].unitSynonymReason`,
                                            "",
                                        )
                                    }
                                }
                                setCreateDictionary(null)
                            }}
                        />
                    ) : null}

                    {canProcess ||
                    (!workspace && application.status === "pending") ? (
                        <form
                            data-slot="detail-form-grid"
                            className="space-y-6"
                            onSubmit={(event) => {
                                event.preventDefault()
                                requestDecision("approve")
                            }}
                        >
                            <div
                                className={
                                    workspace
                                        ? "min-w-0"
                                        : "grid min-w-0 items-start gap-6 @[960px]/workspace:grid-cols-[224px_minmax(0,1fr)] @[1120px]/workspace:grid-cols-[240px_minmax(0,1fr)]"
                                }
                            >
                                {!workspace ? (
                                    <ReviewProgressRail
                                        sections={sections}
                                        activeSectionId={activeSectionId}
                                        canProcess={canProcess}
                                        allExpanded={allExpanded}
                                        onSelectSection={
                                            expandAndScrollToSection
                                        }
                                        onToggleAll={toggleAllSections}
                                        onJumpFirstIssue={() => {
                                            const firstIssue = allIssues[0]
                                            if (firstIssue) {
                                                focusInvalidField(
                                                    firstIssue.fieldId,
                                                )
                                            }
                                        }}
                                    />
                                ) : null}

                                <div className="min-w-0">
                                    {basicSection ? (
                                        <CollapsibleReviewSection
                                            section={basicSection}
                                            open={expandedSections.basic}
                                            active={activeSectionId === "basic"}
                                            canProcess={canProcess}
                                            onOpenChange={(open) =>
                                                setExpandedSections((prev) => ({
                                                    ...prev,
                                                    basic: open,
                                                }))
                                            }
                                            onFocusIssue={focusInvalidField}
                                        >
                                            <BasicDuplicateSection
                                                form={form}
                                                application={application}
                                                task={task}
                                                rawProduct={rawProduct}
                                                compact={Boolean(workspace)}
                                                search={search}
                                                onSearchChange={setSearch}
                                                onSearchSubmit={() =>
                                                    setQ(search.trim())
                                                }
                                                duplicates={
                                                    duplicates.data
                                                        ?.duplicates ?? []
                                                }
                                                duplicatesFetching={
                                                    duplicates.isFetching
                                                }
                                                onRefreshDuplicates={async () => {
                                                    await duplicates.refetch({
                                                        throwOnError: true,
                                                    })
                                                }}
                                                existingCandidates={
                                                    existingCandidates
                                                }
                                                disabled={disabled}
                                                highlightedFieldId={
                                                    highlightedFieldId
                                                }
                                                onClearHighlight={
                                                    clearHighlight
                                                }
                                                onJumpToSection={
                                                    expandAndScrollToSection
                                                }
                                            />
                                        </CollapsibleReviewSection>
                                    ) : null}

                                    {rawProduct && brandCategorySection ? (
                                        <CollapsibleReviewSection
                                            section={brandCategorySection}
                                            open={
                                                expandedSections[
                                                    "brand-category"
                                                ]
                                            }
                                            active={
                                                activeSectionId ===
                                                "brand-category"
                                            }
                                            canProcess={canProcess}
                                            onOpenChange={(open) =>
                                                setExpandedSections((prev) => ({
                                                    ...prev,
                                                    "brand-category": open,
                                                }))
                                            }
                                            onFocusIssue={focusInvalidField}
                                        >
                                            <BrandCategorySection
                                                form={form}
                                                rawProduct={rawProduct}
                                                brands={brands.data ?? []}
                                                categories={
                                                    categories.data ?? []
                                                }
                                                categorySuggestion={
                                                    duplicates.data
                                                        ?.category_mapping_suggestion ??
                                                    null
                                                }
                                                canCreateBrand={access.can(
                                                    "product_brand:create",
                                                )}
                                                canCreateCategory={access.can(
                                                    "product_category:create",
                                                )}
                                                disabled={disabled}
                                                highlightedFieldId={
                                                    highlightedFieldId
                                                }
                                                onClearHighlight={
                                                    clearHighlight
                                                }
                                                onRefreshBrands={async () => {
                                                    await brands.refetch({
                                                        throwOnError: true,
                                                    })
                                                }}
                                                onRefreshCategories={async () => {
                                                    await categories.refetch({
                                                        throwOnError: true,
                                                    })
                                                }}
                                                onReloadDictionaries={async () => {
                                                    const results =
                                                        await Promise.all([
                                                            brands.refetch(),
                                                            categories.refetch(),
                                                            units.refetch(),
                                                        ])
                                                    const failed = results.find(
                                                        (result) =>
                                                            result.error,
                                                    )
                                                    if (failed?.error)
                                                        setError(failed.error)
                                                }}
                                                onCreateDictionary={
                                                    setCreateDictionary
                                                }
                                            />
                                        </CollapsibleReviewSection>
                                    ) : null}

                                    {rawProduct && skuMappingSection ? (
                                        <CollapsibleReviewSection
                                            section={skuMappingSection}
                                            open={
                                                expandedSections["sku-mapping"]
                                            }
                                            active={
                                                activeSectionId ===
                                                "sku-mapping"
                                            }
                                            canProcess={canProcess}
                                            onOpenChange={(open) =>
                                                setExpandedSections((prev) => ({
                                                    ...prev,
                                                    "sku-mapping": open,
                                                }))
                                            }
                                            onFocusIssue={focusInvalidField}
                                        >
                                            <SkuMappingSection
                                                form={form}
                                                application={application}
                                                rawProduct={rawProduct}
                                                brands={brands.data ?? []}
                                                categories={
                                                    categories.data ?? []
                                                }
                                                units={units.data ?? []}
                                                duplicates={
                                                    duplicates.data
                                                        ?.duplicates ?? []
                                                }
                                                existingCandidates={
                                                    existingCandidates
                                                }
                                                actorId={
                                                    access.profile.data
                                                        ?.userid ?? ""
                                                }
                                                canListApplications={access.can(
                                                    "supplier_portal_request:list",
                                                )}
                                                canReadDetails={access.can(
                                                    "supplier_portal_request:detail",
                                                )}
                                                canCreateUnit={access.can(
                                                    "unit_of_measure:create",
                                                )}
                                                disabled={disabled}
                                                highlightedFieldId={
                                                    highlightedFieldId
                                                }
                                                onClearHighlight={
                                                    clearHighlight
                                                }
                                                onRefreshUnits={async () => {
                                                    await units.refetch({
                                                        throwOnError: true,
                                                    })
                                                }}
                                                onRefreshDuplicates={async () => {
                                                    await duplicates.refetch({
                                                        throwOnError: true,
                                                    })
                                                }}
                                                onCreateDictionary={
                                                    setCreateDictionary
                                                }
                                                onJumpToSection={
                                                    expandAndScrollToSection
                                                }
                                            />
                                        </CollapsibleReviewSection>
                                    ) : null}

                                    {decisionSection ? (
                                        <CollapsibleReviewSection
                                            section={decisionSection}
                                            open={expandedSections.decision}
                                            active={
                                                activeSectionId === "decision"
                                            }
                                            canProcess={canProcess}
                                            onOpenChange={(open) =>
                                                setExpandedSections((prev) => ({
                                                    ...prev,
                                                    decision: open,
                                                }))
                                            }
                                            onFocusIssue={focusInvalidField}
                                        >
                                            <DecisionSubmitSection
                                                form={form}
                                                application={application}
                                                rawProduct={rawProduct}
                                                initialAvailability={
                                                    initialAvailability
                                                }
                                                nowMs={nowMs}
                                                reportedConfirmed={
                                                    reportedConfirmed
                                                }
                                                onReportedConfirmedChange={
                                                    setReportedConfirmed
                                                }
                                                canProcess={canProcess}
                                                canListProcurementPeople={
                                                    canListProcurementPeople
                                                }
                                                actorUserId={
                                                    access.profile.data
                                                        ?.userid ?? ""
                                                }
                                                disabled={disabled}
                                                highlightedFieldId={
                                                    highlightedFieldId
                                                }
                                                onClearHighlight={
                                                    clearHighlight
                                                }
                                            />
                                        </CollapsibleReviewSection>
                                    ) : null}
                                </div>
                            </div>
                        </form>
                    ) : null}

                    {canProcess || canRecover ? (
                        <PortalReviewFooter
                            idPrefix={prefix}
                            disabled={disabled}
                            conflict={conflict}
                            issueCount={issueCount}
                            skuCount={
                                rawProduct ? overview.skuCount : undefined
                            }
                            pending={mutation.isPending}
                            retryDecision={retryDecision}
                            canRecover={canRecover}
                            error={error}
                            returnOpen={returnOpen}
                            returnReason={returnReasonValue}
                            returnReasonError={returnReasonError}
                            returnReasonValid={returnReasonValid}
                            quickPhrases={phrases}
                            onReturnOpenChange={(open) =>
                                open
                                    ? requestDecision("return")
                                    : setReturnOpen(false)
                            }
                            onReturnReasonChange={(value) =>
                                form.setFieldValue("returnReason", value)
                            }
                            onApprove={() => requestDecision("approve")}
                            onReturn={() => void executeDecision("return")}
                            onRecover={() => void executeDecision()}
                            onLocateIssue={
                                issueCount > 0 ? locateNextIssue : undefined
                            }
                        />
                    ) : null}

                    <FormalActionConfirmDialog
                        idPrefix={`${prefix}-approve-confirm`}
                        open={confirmApprove && !disabled}
                        onOpenChange={setConfirmApprove}
                        title="确认通过供应商申请并生效"
                        actionLabel="通过并生效"
                        confirmLabel="确认通过并生效"
                        fromStatus={{ label: "待采购确认", tone: "info" }}
                        toStatus={{ label: "已生效", tone: "success" }}
                        summary={approveSummary}
                        effects={[
                            "确认通过后本次申请内容立即生效，后续采购与供货关系按新口径执行。",
                            ...(rawProduct
                                ? [
                                      "本次新增规格入库后保持未上架状态，复用已有规格保持原上架状态。",
                                  ]
                                : []),
                            "已生效的历史采购单继续沿用原价格、条款与付款条件。",
                        ]}
                        pending={mutation.isPending}
                        onConfirm={() => executeDecision("approve")}
                    />
                </DetailWorkspaceSection>
            ) : null}
        </>
    )
}
