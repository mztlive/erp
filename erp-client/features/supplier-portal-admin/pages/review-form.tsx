"use client"

import {
    useCallback,
    useEffect,
    useImperativeHandle,
    useMemo,
    useRef,
    useState,
    type Ref,
} from "react"
import { useQueryClient } from "@tanstack/react-query"
import { useStore } from "@tanstack/react-form"
import { FormalActionConfirmDialog } from "@/components/business"
import { DetailWorkspaceSection } from "@/components/business/detail-workspace"
import { WorkspaceTaskFooter } from "@/components/business/workspace-task-pane"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { RequiredMark } from "@/components/ui/required-mark"
import { Textarea } from "@/components/ui/textarea"
import { toast } from "@/components/ui/toast"
import type { WorkItemDto } from "@/features/work-items/types"
import { PortalAdminCommandConflict } from "@/features/supplier-portal/components/admin-command-conflict"
import { PortalError } from "@/features/supplier-portal/components/surface"
import {
    commandKey,
    kindLabels,
} from "@/features/supplier-portal/lib/presentation"
import type {
    NewProductInput,
    PortalApplication,
    PortalDictionary,
} from "@/features/supplier-portal/types"
import { commandFailureDisposition } from "@/lib/api/command-recovery"
import { useWorkspaceDirtyState } from "@/lib/workspace-form-guard"
import { reviewPortalApplication } from "../api"
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

export type PortalReviewFormHandle = {
    requestDecision: (decision: "approve" | "return") => void
}

export type PortalReviewActionState = {
    canProcess: boolean
    pending: boolean
    conflict: boolean
    retryDecision: "approve" | "return" | null
}

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
    formRef,
    onActionStateChange,
    onRevealReviewTab,
}: {
    application: PortalApplication
    task: WorkItemDto
    onSuccess: () => void
    onReload: () => Promise<void>
    formRef?: Ref<PortalReviewFormHandle>
    onActionStateChange?: (state: PortalReviewActionState) => void
    onRevealReviewTab?: () => void
}) {
    const access = usePortalAdminAccess()
    const product =
        application.kind === "new_product"
            ? (application.submitted_snapshot ??
              (application.input as unknown as NewProductInput))
            : null
    const rawProduct = product as NewProductInput | null
    const canReview = access.can("supplier_portal_request:review")
    const canProcess =
        canReview &&
        task.handler_key === "supplier_portal_review" &&
        task.business_object_type === "supplier_portal_request" &&
        task.business_object_id === application.id &&
        task.status === "OPEN" &&
        !!task.allowed_actions?.includes("PROCESS")

    const brands = useReviewDictionary("brand", canProcess && !!rawProduct)
    const categories = useReviewDictionary(
        "category",
        canProcess && !!rawProduct,
    )
    const units = useReviewDictionary("unit", canProcess && !!rawProduct)
    const [search, setSearch] = useState(rawProduct?.name ?? "")
    const [q, setQ] = useState(rawProduct?.name ?? "")
    const duplicates = usePortalDuplicates(
        application.id,
        q,
        canProcess && !!rawProduct,
    )

    const [error, setError] = useState<unknown>(null)
    const [conflict, setConflict] = useState(false)
    const [retryDecision, setRetryDecision] = useState<
        "approve" | "return" | null
    >(null)
    const [confirmDecision, setConfirmDecision] = useState<
        "approve" | "return" | null
    >(null)
    const [highlightedFieldId, setHighlightedFieldId] = useState<string | null>(
        null,
    )
    const [nowMs, setNowMs] = useState<number | null>(null)
    const client = useQueryClient()
    const [createDictionary, setCreateDictionary] =
        useState<CreateDictionaryRequest | null>(null)
    const [reportedConfirmed, setReportedConfirmed] = useState(false)
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
    const intent = useRef<Record<string, unknown> | null>(null)
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
        onActionStateChange?.({
            canProcess,
            pending: mutation.isPending,
            conflict,
            retryDecision,
        })
    }, [
        canProcess,
        conflict,
        mutation.isPending,
        onActionStateChange,
        retryDecision,
    ])

    const clearIntent = useCallback(() => {
        intent.current = null
        setRetryDecision(null)
    }, [])

    const commandFailed = useCallback(
        (cause: unknown) => {
            const disposition = commandFailureDisposition(cause)
            if (disposition === "rejected") {
                clearIntent()
            }
            setConflict(disposition === "conflict")
            setError(cause)
        },
        [clearIntent],
    )

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

    const focusInvalidField = useCallback(
        (fieldId: string) => {
            onRevealReviewTab?.()
            const targetSection = fieldIdToSectionId(fieldId)
            setExpandedSections((prev) =>
                prev[targetSection] ? prev : { ...prev, [targetSection]: true },
            )
            setActiveSectionId(targetSection)
            setHighlightedFieldId(fieldId)
            window.requestAnimationFrame(() => {
                window.requestAnimationFrame(() => {
                    const target = document.getElementById(fieldId)
                    if (!target) return
                    target.scrollIntoView({
                        behavior: "smooth",
                        block: "center",
                    })
                    if (typeof target.focus === "function") {
                        target.focus({ preventScroll: true })
                    }
                })
            })
        },
        [onRevealReviewTab],
    )

    const defaultValues = createReviewDefaultValues(rawProduct)

    const form = useAppForm({
        ...reviewFormBaseOptions,
        defaultValues,
        onSubmit: async () => {
            // 正式决策统一经由 FormalActionConfirmDialog 确认后执行。
        },
    })

    const isFormDirty = useStore(form.store, (state) => state.isDirty)
    const formValues = useStore(form.store, (state) => state.values)
    const returnReasonValue = formValues.returnReason

    useWorkspaceDirtyState({
        dirty:
            canProcess &&
            (isFormDirty || reportedConfirmed || retryDecision !== null),
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
        (value: typeof formValues) =>
            buildApproveReviewPayload({
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
            }),
        [
            application,
            brands.data,
            canListProcurementPeople,
            canProcess,
            categories.data,
            duplicates.data?.duplicates,
            existingCandidates,
            initialAvailability,
            rawProduct,
            reportedConfirmed,
            task.id,
            task.task_version,
            units.data,
        ],
    )

    const requestDecision = useCallback(
        (decision: "approve" | "return") => {
            if (mutation.isPending || conflict) return
            setError(null)
            setHighlightedFieldId(null)
            if (!canProcess) {
                setError(
                    new Error("当前账号或任务不具备处理资格，请重新核对任务"),
                )
                return
            }
            if (retryDecision) {
                if (retryDecision !== decision) {
                    setError(new Error("请先确认上次操作结果，再执行其他操作"))
                    return
                }
                setConfirmDecision(decision)
                return
            }
            if (decision === "approve") {
                const check = buildApprovePayload(form.state.values)
                if (!check.ok) {
                    setError(new Error(check.message))
                    focusInvalidField(check.fieldId)
                    return
                }
                setConfirmDecision("approve")
                return
            }
            setConfirmDecision("return")
        },
        [
            buildApprovePayload,
            canProcess,
            conflict,
            focusInvalidField,
            form.state.values,
            mutation.isPending,
            retryDecision,
        ],
    )

    useImperativeHandle(
        formRef,
        () => ({
            requestDecision,
        }),
        [requestDecision],
    )

    const executeConfirmedDecision = async () => {
        if (!confirmDecision || mutation.isPending) return
        setError(null)
        if (!intent.current) {
            if (confirmDecision === "approve") {
                const check = buildApprovePayload(form.state.values)
                if (!check.ok) {
                    setConfirmDecision(null)
                    setError(new Error(check.message))
                    focusInvalidField(check.fieldId)
                    return
                }
                intent.current = {
                    ...check.body,
                    idempotency_key: commandKey("review"),
                }
                setRetryDecision("approve")
            } else {
                const reason = form.state.values.returnReason.trim()
                if (!reason) {
                    setError(new Error("退回必须填写供应商可见的原因"))
                    return
                }
                if (reason.length > 500) {
                    setError(new Error("退回原因最多500字"))
                    return
                }
                intent.current = {
                    availability_reported_at_confirmed: false,
                    expected_version: application.version,
                    work_item_id: task.id,
                    work_item_version: Number(task.task_version),
                    decision: "return",
                    comment: reason,
                    maintainer_user_id: null,
                    normalized_product: null,
                    existing_product: null,
                    existing_offerings: [],
                    idempotency_key: commandKey("review"),
                }
                setRetryDecision("return")
            }
        }
        try {
            await mutation.mutateAsync(intent.current)
            const completedDecision = confirmDecision
            clearIntent()
            setConfirmDecision(null)
            setHighlightedFieldId(null)
            toast.add({
                title:
                    completedDecision === "approve"
                        ? "已确认通过并生效"
                        : "已退回供应商修改",
                type: "success",
            })
            onSuccess()
        } catch (cause) {
            commandFailed(cause)
            setConfirmDecision(null)
        }
    }

    const disabled = mutation.isPending || !canProcess || retryDecision !== null
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

    const returnSummary = [
        `申请类型：${kindLabels[application.kind]}`,
        `申请内容：${applicationLabel}${application.application_no ? `（${application.application_no}）` : ""}`,
        ...(application.supplier_name
            ? [`供应商：${application.supplier_name}`]
            : []),
    ]

    if (!canReview) {
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

    return (
        <DetailWorkspaceSection
            id="supplier-portal-review-section"
            title="采购确认"
            width="full"
        >
            <div className="max-w-4xl space-y-3">
                <p className="text-xs text-muted-foreground">
                    当前处理人：{task.owner_user?.display_name ?? "待核对"}
                    。供应商提报资料保持不变，单位、规格含义或报价变化请退回供应商确认。
                </p>
                {!canProcess ? (
                    <p className="text-xs text-muted-foreground">
                        当前任务仅可查看，请由任务处理人确认。
                    </p>
                ) : null}
                <PortalError error={error} />
                {conflict ? (
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
                            clearIntent()
                            setConflict(false)
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
                            form.setFieldValue("brandVersion", created.version)
                        } else if (createDictionary.kind === "category") {
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
                            const index = form.state.values.mappings.findIndex(
                                (row) => row.rowId === createDictionary.rowId,
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

            <form
                data-slot="detail-form-grid"
                className="space-y-6"
                onSubmit={(event) => {
                    event.preventDefault()
                    requestDecision("approve")
                }}
            >
                <div className="grid min-w-0 items-start gap-6 @[960px]/workspace:grid-cols-[224px_minmax(0,1fr)] @[1120px]/workspace:grid-cols-[240px_minmax(0,1fr)]">
                    <ReviewProgressRail
                        sections={sections}
                        activeSectionId={activeSectionId}
                        canProcess={canProcess}
                        allExpanded={allExpanded}
                        onSelectSection={expandAndScrollToSection}
                        onToggleAll={toggleAllSections}
                        onJumpFirstIssue={() => {
                            const firstIssue = allIssues[0]
                            if (firstIssue) {
                                focusInvalidField(firstIssue.fieldId)
                            }
                        }}
                    />

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
                                    search={search}
                                    onSearchChange={setSearch}
                                    onSearchSubmit={() => setQ(search.trim())}
                                    duplicates={
                                        duplicates.data?.duplicates ?? []
                                    }
                                    duplicatesFetching={duplicates.isFetching}
                                    onRefreshDuplicates={async () => {
                                        await duplicates.refetch({
                                            throwOnError: true,
                                        })
                                    }}
                                    existingCandidates={existingCandidates}
                                    disabled={disabled}
                                    highlightedFieldId={highlightedFieldId}
                                    onClearHighlight={clearHighlight}
                                    onJumpToSection={expandAndScrollToSection}
                                />
                            </CollapsibleReviewSection>
                        ) : null}

                        {rawProduct && brandCategorySection ? (
                            <CollapsibleReviewSection
                                section={brandCategorySection}
                                open={expandedSections["brand-category"]}
                                active={activeSectionId === "brand-category"}
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
                                    categories={categories.data ?? []}
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
                                    highlightedFieldId={highlightedFieldId}
                                    onClearHighlight={clearHighlight}
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
                                        const results = await Promise.all([
                                            brands.refetch(),
                                            categories.refetch(),
                                            units.refetch(),
                                        ])
                                        const failed = results.find(
                                            (result) => result.error,
                                        )
                                        if (failed?.error)
                                            setError(failed.error)
                                    }}
                                    onCreateDictionary={setCreateDictionary}
                                />
                            </CollapsibleReviewSection>
                        ) : null}

                        {rawProduct && skuMappingSection ? (
                            <CollapsibleReviewSection
                                section={skuMappingSection}
                                open={expandedSections["sku-mapping"]}
                                active={activeSectionId === "sku-mapping"}
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
                                    categories={categories.data ?? []}
                                    units={units.data ?? []}
                                    duplicates={
                                        duplicates.data?.duplicates ?? []
                                    }
                                    existingCandidates={existingCandidates}
                                    actorId={access.profile.data?.userid ?? ""}
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
                                    highlightedFieldId={highlightedFieldId}
                                    onClearHighlight={clearHighlight}
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
                                    onCreateDictionary={setCreateDictionary}
                                    onJumpToSection={expandAndScrollToSection}
                                />
                            </CollapsibleReviewSection>
                        ) : null}

                        {decisionSection ? (
                            <CollapsibleReviewSection
                                section={decisionSection}
                                open={expandedSections.decision}
                                active={activeSectionId === "decision"}
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
                                    initialAvailability={initialAvailability}
                                    nowMs={nowMs}
                                    reportedConfirmed={reportedConfirmed}
                                    onReportedConfirmedChange={
                                        setReportedConfirmed
                                    }
                                    canProcess={canProcess}
                                    canListProcurementPeople={
                                        canListProcurementPeople
                                    }
                                    actorUserId={
                                        access.profile.data?.userid ?? ""
                                    }
                                    disabled={disabled}
                                    highlightedFieldId={highlightedFieldId}
                                    onClearHighlight={clearHighlight}
                                    allIssues={allIssues}
                                    onFocusIssue={focusInvalidField}
                                />
                            </CollapsibleReviewSection>
                        ) : null}
                    </div>
                </div>

                <WorkspaceTaskFooter>
                    <div className="flex flex-wrap items-center gap-2">
                        <Button
                            id="supplier-portal-review-return"
                            type="button"
                            size="sm"
                            variant="outline"
                            disabled={
                                mutation.isPending ||
                                conflict ||
                                !canProcess ||
                                (retryDecision != null &&
                                    retryDecision !== "return")
                            }
                            onClick={() => requestDecision("return")}
                        >
                            {retryDecision === "return"
                                ? "重试原退回决定"
                                : "退回供应商修改"}
                        </Button>
                        <Button
                            id="supplier-portal-review-approve"
                            type="button"
                            size="sm"
                            className="px-3.5"
                            disabled={
                                mutation.isPending ||
                                conflict ||
                                !canProcess ||
                                (retryDecision != null &&
                                    retryDecision !== "approve")
                            }
                            onClick={() => requestDecision("approve")}
                        >
                            {retryDecision === "approve"
                                ? "重试原通过决定"
                                : "确认通过并生效"}
                        </Button>
                    </div>
                </WorkspaceTaskFooter>
            </form>

            <FormalActionConfirmDialog
                idPrefix="supplier-portal-review-approve-confirm"
                open={confirmDecision === "approve"}
                onOpenChange={(open) => {
                    if (!open && !mutation.isPending) setConfirmDecision(null)
                }}
                title={
                    retryDecision === "approve"
                        ? "重试确认通过并生效"
                        : "确认通过供应商申请并生效"
                }
                actionLabel="通过并生效"
                confirmLabel={
                    retryDecision === "approve"
                        ? "重试原通过决定"
                        : "确认通过并生效"
                }
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
                onConfirm={executeConfirmedDecision}
            />
            <FormalActionConfirmDialog
                idPrefix="supplier-portal-review-return-confirm"
                open={confirmDecision === "return"}
                onOpenChange={(open) => {
                    if (!open && !mutation.isPending) setConfirmDecision(null)
                }}
                title={
                    retryDecision === "return"
                        ? "重试退回供应商修改"
                        : "确认退回供应商修改"
                }
                actionLabel="退回供应商修改"
                confirmLabel={
                    retryDecision === "return"
                        ? "重试原退回决定"
                        : "确认退回供应商修改"
                }
                actionVariant="destructive"
                fromStatus={{ label: "待采购确认", tone: "info" }}
                toStatus={{ label: "已退回", tone: "warning" }}
                summary={returnSummary}
                effects={[
                    "下方退回原因将直接展示给供应商，请勿包含内部核对备注。",
                    "退回后当前生效条款与合作条件保持不变，供应商可修改后再次提交。",
                ]}
                formContent={
                    <div className="space-y-1.5">
                        <label
                            htmlFor="supplier-portal-review-return-dialog-reason"
                            className="block text-xs font-medium text-foreground"
                        >
                            退回原因（供应商可见）
                            <RequiredMark />
                        </label>
                        <Textarea
                            id="supplier-portal-review-return-dialog-reason"
                            value={returnReasonValue}
                            disabled={
                                mutation.isPending || retryDecision === "return"
                            }
                            maxLength={500}
                            rows={3}
                            placeholder="请明确说明需要供应商核对或修改的具体内容"
                            onChange={(event) =>
                                form.setFieldValue(
                                    "returnReason",
                                    event.target.value,
                                )
                            }
                        />
                        <p className="text-xs text-muted-foreground">
                            仅发送此处填写的退回原因，内部核对备注不会展示给供应商。
                        </p>
                    </div>
                }
                pending={mutation.isPending}
                confirmDisabled={
                    !returnReasonValue.trim() ||
                    returnReasonValue.length > 500 ||
                    mutation.isPending ||
                    conflict
                }
                onConfirm={executeConfirmedDecision}
            />
        </DetailWorkspaceSection>
    )
}
