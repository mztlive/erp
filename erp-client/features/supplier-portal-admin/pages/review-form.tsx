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
import { z } from "zod"
import { FormalActionConfirmDialog } from "@/components/business"
import {
    DetailWorkspaceSection,
    detailWorkspaceStyles,
} from "@/components/business/detail-workspace"
import { WorkspaceTaskFooter } from "@/components/business/workspace-task-pane"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { NativeCheckbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import { RequiredMark } from "@/components/ui/required-mark"
import { Textarea } from "@/components/ui/textarea"
import { toast } from "@/components/ui/toast"
import { PersonDirectoryFilter } from "@/features/entity-selectors/components/person-directory-filter"
import type { WorkItemDto } from "@/features/work-items/types"
import { PortalError } from "@/features/supplier-portal/components/surface"
import {
    commandKey,
    kindLabels,
    timeLabel,
} from "@/features/supplier-portal/lib/presentation"
import type {
    NewProductInput,
    PortalApplication,
    PortalDictionary,
} from "@/features/supplier-portal/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { commandFailureDisposition } from "@/lib/api/command-recovery"
import { cn } from "@/lib/utils"
import { useWorkspaceDirtyState } from "@/lib/workspace-form-guard"
import { reviewPortalApplication } from "../api"
import {
    portalAdminKeys,
    usePortalAdminAccess,
    usePortalAdminCommand,
    usePortalDuplicates,
    useReviewDictionary,
} from "../hooks"
import { PortalDictionaryCreateDialog } from "../components/dictionary-create-dialog"
import { PortalUnitMappingBulk } from "../components/unit-mapping-bulk"
import { PortalCategoryMappingEvidence } from "../components/category-suggestion"
import { PortalAdminCommandConflict } from "@/features/supplier-portal/components/admin-command-conflict"
import { PortalCategoryHierarchyConfirmation } from "../components/category-hierarchy-confirmation"
import { PortalReassignForm } from "../components/reassign-form"
import { PortalReviewMappingBatch } from "../components/review-mapping-batch"

export type PortalReviewFormHandle = {
    requestDecision: (decision: "approve" | "return") => void
}

export type PortalReviewActionState = {
    canProcess: boolean
    pending: boolean
    conflict: boolean
    retryDecision: "approve" | "return" | null
}

type ReviewFormValues = {
    internalNote: string
    returnReason: string
    maintainerUserId: string
    productTarget: string
    brandId: string
    categoryId: string
    brandVersion: number
    categoryVersion: number
    categoryHierarchy: NonNullable<PortalDictionary["hierarchy"]>
    existingConfirmed: string[]
    mappings: {
        rowId: string
        unitId: string
        unitVersion: number
        target: string
        unitSynonymConfirmed: boolean
        unitSynonymReason: string
    }[]
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
    const canReassign =
        access.can("work_item:manage") &&
        !!task.allowed_actions?.includes("REASSIGN")
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
    const [createDictionary, setCreateDictionary] = useState<{
        kind: "brand" | "category" | "unit"
        rawName: string
        rowId?: string
    } | null>(null)
    const [reassign, setReassign] = useState(false)
    const [reportedConfirmed, setReportedConfirmed] = useState(false)
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

    const focusInvalidField = useCallback(
        (fieldId: string) => {
            onRevealReviewTab?.()
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

    const defaultValues: ReviewFormValues = {
        internalNote: "",
        returnReason: "",
        maintainerUserId: "",
        productTarget: "unconfirmed",
        brandId: rawProduct?.brand.selected_id ?? "",
        categoryId: rawProduct?.category.selected_id ?? "",
        brandVersion: rawProduct?.brand.expected_version ?? 0,
        categoryVersion: rawProduct?.category.expected_version ?? 0,
        categoryHierarchy: [],
        existingConfirmed: [],
        mappings:
            rawProduct?.skus.map((sku) => ({
                rowId: sku.row_id,
                unitId: sku.unit.selected_id ?? "",
                unitVersion: sku.unit.expected_version ?? 0,
                target: "unconfirmed",
                unitSynonymConfirmed: false,
                unitSynonymReason: "",
            })) ?? [],
    }

    const form = useAppForm({
        defaultValues,
        validators: {
            onSubmit: z.object({
                internalNote: z.string().max(500, "内部核对备注最多500字"),
                returnReason: z.string().max(500, "退回原因最多500字"),
                maintainerUserId: z.string(),
                productTarget: z.string(),
                brandId: z.string(),
                categoryId: z.string(),
                brandVersion: z.number().int(),
                categoryVersion: z.number().int(),
                categoryHierarchy:
                    z.custom<NonNullable<PortalDictionary["hierarchy"]>>(),
                existingConfirmed: z.array(z.string()),
                mappings: z.array(
                    z.object({
                        rowId: z.string(),
                        unitId: z.string(),
                        unitVersion: z.number().int(),
                        target: z.string(),
                        unitSynonymConfirmed: z.boolean(),
                        unitSynonymReason: z.string(),
                    }),
                ),
            }),
        },
        onSubmit: async () => {
            // 正式决策统一经由 FormalActionConfirmDialog 确认后执行。
        },
    })

    const isFormDirty = useStore(form.store, (state) => state.isDirty)
    const returnReasonValue = useStore(
        form.store,
        (state) => state.values.returnReason,
    )
    const formValuesSnapshot = useStore(form.store, (state) => ({
        productTarget: state.values.productTarget,
        maintainerUserId: state.values.maintainerUserId,
    }))

    useWorkspaceDirtyState({
        dirty:
            canProcess &&
            (isFormDirty || reportedConfirmed || retryDecision !== null),
        busy: mutation.isPending,
    })

    const buildApprovePayload = useCallback(
        (
            value: ReviewFormValues,
        ):
            | { ok: true; body: Record<string, unknown> }
            | { ok: false; message: string; fieldId: string } => {
            if (!canProcess) {
                return {
                    ok: false,
                    message: "当前账号或任务不具备处理资格，请重新核对任务",
                    fieldId: "supplier-portal-review-section",
                }
            }
            let normalizedProduct: Record<string, unknown> | null = null
            let existingProduct: Record<string, unknown> | null = null
            if (rawProduct) {
                const brand = brands.data?.find(
                    (item) => item.id === value.brandId,
                )
                if (!brand) {
                    return {
                        ok: false,
                        message: "品牌尚未完成有效匹配，不能通过",
                        fieldId: "supplier-portal-review-brand",
                    }
                }
                const category = categories.data?.find(
                    (item) => item.id === value.categoryId,
                )
                if (!category) {
                    return {
                        ok: false,
                        message: "分类尚未完成有效匹配，不能通过",
                        fieldId: "supplier-portal-review-category",
                    }
                }
                if (
                    !value.categoryHierarchy.length ||
                    JSON.stringify(value.categoryHierarchy) !==
                        JSON.stringify(category.hierarchy)
                ) {
                    return {
                        ok: false,
                        message: "请读取并明确确认当前完整分类路径后再通过",
                        fieldId: "supplier-portal-review-category-path-confirm",
                    }
                }
                if (value.productTarget === "unconfirmed") {
                    return {
                        ok: false,
                        message: "请明确选择新建或复用公司商品",
                        fieldId: "supplier-portal-review-product-target",
                    }
                }
                const selectedProduct = duplicates.data?.duplicates.find(
                    (item) => item.product_id === value.productTarget,
                )
                if (value.productTarget !== "new" && !selectedProduct) {
                    return {
                        ok: false,
                        message: "所选商品候选已不可用，请重新核对",
                        fieldId: "supplier-portal-review-product-target",
                    }
                }
                existingProduct = selectedProduct
                    ? {
                          product_id: selectedProduct.product_id,
                          version: selectedProduct.version,
                          revision_id: selectedProduct.revision_id,
                      }
                    : null
                const mappings: Record<string, unknown>[] = []
                for (const row of value.mappings) {
                    const prefix = `supplier-portal-review-sku-${toAutomationIdSegment(row.rowId)}`
                    const sku = rawProduct.skus.find(
                        (item) => item.row_id === row.rowId,
                    )
                    const unit = units.data?.find(
                        (item) => item.id === row.unitId,
                    )
                    if (!sku || !unit) {
                        return {
                            ok: false,
                            message: "每个规格必须完成有效基础单位匹配",
                            fieldId: `${prefix}-unit`,
                        }
                    }
                    if (
                        row.unitSynonymConfirmed &&
                        !row.unitSynonymReason.trim()
                    ) {
                        return {
                            ok: false,
                            message: "确认单位同义映射时必须填写逐行核对依据",
                            fieldId: `${prefix}-unit-synonym-reason`,
                        }
                    }
                    if (
                        sku.unit.raw_name.trim() !== unit.name.trim() &&
                        !row.unitSynonymConfirmed
                    ) {
                        return {
                            ok: false,
                            message:
                                "单位原文与标准单位不同，须明确确认同义含义或退回供应商",
                            fieldId: `${prefix}-unit-synonym-confirmed`,
                        }
                    }
                    if (row.target === "unconfirmed") {
                        return {
                            ok: false,
                            message: "请逐项明确选择新建或复用规格",
                            fieldId: `${prefix}-target`,
                        }
                    }
                    const matchedSku = selectedProduct?.skus.find(
                        (item) => item.sku_id === row.target,
                    )
                    if (row.target !== "new" && !matchedSku) {
                        return {
                            ok: false,
                            message:
                                "所选规格必须属于明确匹配的商品，请重新核对",
                            fieldId: `${prefix}-target`,
                        }
                    }
                    mappings.push({
                        row_id: row.rowId,
                        name: sku.name.trim(),
                        unit_id: unit.id,
                        unit_version: sku.unit.selected_id
                            ? sku.unit.expected_version
                            : row.unitVersion,
                        target_sku: matchedSku
                            ? {
                                  sku_id: matchedSku.sku_id,
                                  version: matchedSku.version,
                                  revision_id: matchedSku.revision_id,
                              }
                            : null,
                        unit_synonym_confirmation: row.unitSynonymConfirmed
                            ? {
                                  original_unit: sku.unit.raw_name,
                                  same_unit_meaning_confirmed: true,
                                  reason: row.unitSynonymReason.trim(),
                              }
                            : null,
                    })
                }
                for (const candidate of existingCandidates) {
                    if (
                        !value.existingConfirmed.includes(candidate.offering_id)
                    ) {
                        return {
                            ok: false,
                            message:
                                "请核对相同订货编码的既有供给，并明确确认本次修订",
                            fieldId: `supplier-portal-review-existing-${toAutomationIdSegment(candidate.offering_id)}`,
                        }
                    }
                    const mapping = value.mappings.find(
                        (row) => row.rowId === candidate.row_id,
                    )
                    if (mapping?.target !== candidate.sku_id) {
                        return {
                            ok: false,
                            message:
                                "相同订货编码已关联已有规格，请复用该精确规格；含义变化时退回供应商核对",
                            fieldId: `supplier-portal-review-sku-${toAutomationIdSegment(candidate.row_id)}-target`,
                        }
                    }
                }
                normalizedProduct = {
                    name: rawProduct.name.trim(),
                    brand_id: brand.id,
                    brand_version: rawProduct.brand.selected_id
                        ? rawProduct.brand.expected_version
                        : value.brandVersion,
                    category_id: category.id,
                    category_version: rawProduct.category.selected_id
                        ? rawProduct.category.expected_version
                        : value.categoryVersion,
                    category_hierarchy: value.categoryHierarchy,
                    sku_mappings: mappings,
                }
            }
            if (initialAvailability && !reportedConfirmed) {
                return {
                    ok: false,
                    message: "请明确核对供应商实际报送时间后再通过",
                    fieldId: "supplier-portal-review-reported-confirmed",
                }
            }
            if (!value.maintainerUserId) {
                return {
                    ok: false,
                    message: "请明确选择建档或供给的内部维护人",
                    fieldId: access.can("procurement_person:list")
                        ? "supplier-portal-review-maintainer"
                        : "supplier-portal-review-maintainer-self",
                }
            }
            if (rawProduct && !value.internalNote.trim()) {
                return {
                    ok: false,
                    message: "请填写内部核对备注，保留本次复用或新建理由",
                    fieldId: "supplier-portal-review-comment",
                }
            }
            if (value.internalNote.length > 500) {
                return {
                    ok: false,
                    message: "内部核对备注最多500字",
                    fieldId: "supplier-portal-review-comment",
                }
            }
            return {
                ok: true,
                body: {
                    availability_reported_at_confirmed:
                        initialAvailability && reportedConfirmed,
                    expected_version: application.version,
                    work_item_id: task.id,
                    work_item_version: Number(task.task_version),
                    decision: "approve",
                    comment: value.internalNote.trim() || null,
                    maintainer_user_id: value.maintainerUserId || null,
                    normalized_product: normalizedProduct,
                    existing_product: existingProduct,
                    existing_offerings:
                        existingCandidates
                            .filter((item) =>
                                value.existingConfirmed.includes(
                                    item.offering_id,
                                ),
                            )
                            .map((item) => ({
                                row_id: item.row_id,
                                offering_id: item.offering_id,
                                expected_offering_version:
                                    item.expected_offering_version,
                                expected_revision_no: item.expected_revision_no,
                            })) ?? [],
                },
            }
        },
        [
            access,
            application.version,
            brands.data,
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
    const dictionaryOptions = (
        rows: { id: string; name: string; path?: string }[] | undefined,
    ) =>
        (rows ?? []).map((item) => ({
            value: item.id,
            label: item.path ?? item.name,
        }))

    const fieldHighlight = (fieldId: string) =>
        highlightedFieldId === fieldId
            ? "rounded-md ring-2 ring-destructive/65 ring-offset-2 p-1.5 -m-1.5 transition-shadow"
            : undefined

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
                      formValuesSnapshot.productTarget === "new"
                          ? "新建公司商品"
                          : `复用已有商品（${
                                duplicates.data?.duplicates.find(
                                    (item) =>
                                        item.product_id ===
                                        formValuesSnapshot.productTarget,
                                )?.name ?? "已选候选"
                            }）`
                  }`,
              ]
            : []),
        ...(formValuesSnapshot.maintainerUserId ? ["内部维护人：已指定"] : []),
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
                {canReassign && (
                    <Button
                        id="supplier-portal-review-reassign-toggle"
                        type="button"
                        size="sm"
                        variant="ghost"
                        className={detailWorkspaceStyles.subtleButton}
                        onClick={() => setReassign(!reassign)}
                    >
                        转交具体处理人
                    </Button>
                )}
                {canReassign && reassign && (
                    <PortalReassignForm
                        task={task}
                        onReload={onReload}
                        onSuccess={() => {
                            setReassign(false)
                            void onReload().catch(setError)
                        }}
                    />
                )}
                <PortalError error={error} />
            </DetailWorkspaceSection>
        )
    }

    return (
        <DetailWorkspaceSection
            id="supplier-portal-review-section"
            title="采购确认"
            width={rawProduct ? "full" : "form"}
            actions={
                canReassign ? (
                    <Button
                        id="supplier-portal-review-reassign-toggle"
                        type="button"
                        size="sm"
                        variant="ghost"
                        className={detailWorkspaceStyles.subtleButton}
                        onClick={() => setReassign(!reassign)}
                        disabled={mutation.isPending}
                    >
                        转交具体处理人
                    </Button>
                ) : undefined
            }
        >
            <div className="max-w-4xl space-y-3">
                <p className="text-xs text-muted-foreground">
                    当前处理人：{task.owner_user?.display_name ?? "待核对"}
                    。供应商提报资料保持不变，单位、规格含义或报价变化请退回供应商确认。
                </p>
                {!canProcess && (
                    <p className="text-xs text-muted-foreground">
                        当前任务仅可查看，请由任务处理人确认。
                    </p>
                )}
                <PortalError error={error} />
                {conflict && (
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
                )}
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
                {canReassign && reassign && (
                    <PortalReassignForm
                        task={task}
                        onReload={onReload}
                        onSuccess={() => {
                            setReassign(false)
                            void onReload().catch(setError)
                        }}
                    />
                )}
            </div>
            {canReview && createDictionary && rawProduct && (
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
            )}
            <form
                data-slot="detail-form-grid"
                className="space-y-6"
                onSubmit={(event) => {
                    event.preventDefault()
                    requestDecision("approve")
                }}
            >
                {rawProduct && (
                    <section className="space-y-5">
                        <div className="max-w-4xl space-y-4">
                            <h3 className="font-semibold">
                                品牌、分类匹配与商品查重
                            </h3>
                            <div className="grid gap-4 md:grid-cols-2">
                                <div
                                    className={fieldHighlight(
                                        "supplier-portal-review-brand",
                                    )}
                                >
                                    <form.AppField name="brandId">
                                        {(field) => (
                                            <field.SelectField
                                                onValueChange={(id) => {
                                                    setHighlightedFieldId(null)
                                                    form.setFieldValue(
                                                        "brandVersion",
                                                        brands.data?.find(
                                                            (row) =>
                                                                row.id === id,
                                                        )?.version ?? 0,
                                                    )
                                                }}
                                                id="supplier-portal-review-brand"
                                                label={`提报品牌：${rawProduct.brand.raw_name}`}
                                                required
                                                options={dictionaryOptions(
                                                    brands.data,
                                                )}
                                                onRefresh={() =>
                                                    brands.refetch({
                                                        throwOnError: true,
                                                    })
                                                }
                                                disabled={
                                                    disabled ||
                                                    !!rawProduct.brand
                                                        .selected_id
                                                }
                                                allowClear={
                                                    !rawProduct.brand
                                                        .selected_id
                                                }
                                            />
                                        )}
                                    </form.AppField>
                                </div>
                                <div
                                    className={fieldHighlight(
                                        "supplier-portal-review-category",
                                    )}
                                >
                                    <form.AppField name="categoryId">
                                        {(field) => (
                                            <field.SelectField
                                                onRefresh={() =>
                                                    categories.refetch({
                                                        throwOnError: true,
                                                    })
                                                }
                                                onValueChange={(id) => {
                                                    setHighlightedFieldId(null)
                                                    form.setFieldValue(
                                                        "categoryVersion",
                                                        categories.data?.find(
                                                            (row) =>
                                                                row.id === id,
                                                        )?.version ?? 0,
                                                    )
                                                    form.setFieldValue(
                                                        "categoryHierarchy",
                                                        [],
                                                    )
                                                }}
                                                id="supplier-portal-review-category"
                                                label={`提报分类：${rawProduct.category.raw_name}`}
                                                required
                                                options={dictionaryOptions(
                                                    categories.data?.filter(
                                                        (item) =>
                                                            !item.product_kind ||
                                                            item.product_kind ===
                                                                rawProduct.product_kind,
                                                    ),
                                                )}
                                                disabled={
                                                    disabled ||
                                                    !!rawProduct.category
                                                        .selected_id
                                                }
                                                allowClear={
                                                    !rawProduct.category
                                                        .selected_id
                                                }
                                            />
                                        )}
                                    </form.AppField>
                                </div>
                            </div>
                            <div
                                className={fieldHighlight(
                                    "supplier-portal-review-category-path-confirm",
                                )}
                            >
                                <form.Subscribe
                                    selector={(state) => ({
                                        id: state.values.categoryId,
                                        version: state.values.categoryVersion,
                                        hierarchy:
                                            state.values.categoryHierarchy,
                                    })}
                                >
                                    {(selection) => (
                                        <PortalCategoryHierarchyConfirmation
                                            category={categories.data?.find(
                                                (item) =>
                                                    item.id === selection.id,
                                            )}
                                            expectedVersion={selection.version}
                                            value={selection.hierarchy}
                                            disabled={disabled}
                                            onConfirm={(hierarchy) => {
                                                setHighlightedFieldId(null)
                                                form.setFieldValue(
                                                    "categoryHierarchy",
                                                    hierarchy,
                                                )
                                            }}
                                        />
                                    )}
                                </form.Subscribe>
                            </div>
                            <p className="text-xs text-muted-foreground">
                                未匹配的品牌、分类或单位需由有权限的同事新增；完成后重新核对候选。可转交当前申请，供应商提报资料保留。
                            </p>
                            <div className="flex flex-wrap gap-2">
                                <Button
                                    id="supplier-portal-review-dictionaries-reload"
                                    type="button"
                                    size="sm"
                                    variant="ghost"
                                    className={
                                        detailWorkspaceStyles.subtleButton
                                    }
                                    disabled={disabled}
                                    onClick={async () => {
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
                                >
                                    刷新品牌、分类与单位选项
                                </Button>
                                {access.can("product_brand:create") &&
                                    !rawProduct.brand.selected_id && (
                                        <Button
                                            id="supplier-portal-review-create-brand"
                                            type="button"
                                            size="sm"
                                            variant="ghost"
                                            className={
                                                detailWorkspaceStyles.subtleButton
                                            }
                                            disabled={disabled}
                                            onClick={() =>
                                                setCreateDictionary({
                                                    kind: "brand",
                                                    rawName:
                                                        rawProduct.brand
                                                            .raw_name,
                                                })
                                            }
                                        >
                                            在当前审核补建品牌
                                        </Button>
                                    )}
                                {access.can("product_category:create") &&
                                    !rawProduct.category.selected_id && (
                                        <Button
                                            id="supplier-portal-review-create-category"
                                            type="button"
                                            size="sm"
                                            variant="ghost"
                                            className={
                                                detailWorkspaceStyles.subtleButton
                                            }
                                            disabled={disabled}
                                            onClick={() =>
                                                setCreateDictionary({
                                                    kind: "category",
                                                    rawName:
                                                        rawProduct.category
                                                            .raw_name,
                                                })
                                            }
                                        >
                                            在当前审核补建分类
                                        </Button>
                                    )}
                            </div>
                            <PortalCategoryMappingEvidence
                                suggestion={
                                    duplicates.data
                                        ?.category_mapping_suggestion ?? null
                                }
                            />
                        </div>
                        {/* 宽表子块全宽展示 */}
                        <form.Subscribe
                            selector={(state) => state.values.mappings}
                        >
                            {(mappings) => (
                                <PortalUnitMappingBulk
                                    skus={rawProduct.skus}
                                    units={units.data ?? []}
                                    mappings={mappings}
                                    disabled={disabled}
                                    onApply={(updates) => {
                                        for (const update of updates) {
                                            const index =
                                                form.state.values.mappings.findIndex(
                                                    (row) =>
                                                        row.rowId ===
                                                        update.rowId,
                                                )
                                            if (index >= 0) {
                                                form.setFieldValue(
                                                    `mappings[${index}].unitId`,
                                                    update.unitId,
                                                )
                                                form.setFieldValue(
                                                    `mappings[${index}].unitVersion`,
                                                    update.unitVersion,
                                                )
                                                form.setFieldValue(
                                                    `mappings[${index}].unitSynonymConfirmed`,
                                                    update.unitSynonymConfirmed,
                                                )
                                                form.setFieldValue(
                                                    `mappings[${index}].unitSynonymReason`,
                                                    update.unitSynonymReason,
                                                )
                                            }
                                        }
                                    }}
                                />
                            )}
                        </form.Subscribe>
                        <div className="max-w-4xl space-y-5">
                            <form.Subscribe selector={(state) => state.values}>
                                {(values) => (
                                    <PortalReviewMappingBatch
                                        application={application}
                                        rawProduct={rawProduct}
                                        brands={brands.data ?? []}
                                        categories={categories.data ?? []}
                                        units={units.data ?? []}
                                        mapping={{
                                            brandId: values.brandId,
                                            brandVersion: values.brandVersion,
                                            categoryId: values.categoryId,
                                            categoryVersion:
                                                values.categoryVersion,
                                            categoryHierarchy:
                                                values.categoryHierarchy,
                                            mappings: values.mappings,
                                        }}
                                        actorId={
                                            access.profile.data?.userid ?? ""
                                        }
                                        canList={access.can(
                                            "supplier_portal_request:list",
                                        )}
                                        canReadDetails={access.can(
                                            "supplier_portal_request:detail",
                                        )}
                                        disabled={disabled}
                                        onAdopt={(mapping) => {
                                            form.setFieldValue(
                                                "brandId",
                                                mapping.brandId,
                                            )
                                            form.setFieldValue(
                                                "brandVersion",
                                                mapping.brandVersion,
                                            )
                                            form.setFieldValue(
                                                "categoryId",
                                                mapping.categoryId,
                                            )
                                            form.setFieldValue(
                                                "categoryVersion",
                                                mapping.categoryVersion,
                                            )
                                            form.setFieldValue(
                                                "categoryHierarchy",
                                                mapping.categoryHierarchy,
                                            )
                                            form.setFieldValue(
                                                "mappings",
                                                form.state.values.mappings.map(
                                                    (row) => {
                                                        const update =
                                                            mapping.mappings.find(
                                                                (item) =>
                                                                    item.rowId ===
                                                                    row.rowId,
                                                            )
                                                        return update
                                                            ? {
                                                                  ...row,
                                                                  ...update,
                                                              }
                                                            : row
                                                    },
                                                ),
                                            )
                                        }}
                                    />
                                )}
                            </form.Subscribe>
                            <div className="space-y-4 border-t border-border/60 pt-5">
                                <div className="flex gap-2">
                                    <Input
                                        id="supplier-portal-review-duplicate-query"
                                        aria-label="重复匹配候选搜索"
                                        value={search}
                                        disabled={disabled}
                                        onChange={(event) =>
                                            setSearch(event.target.value)
                                        }
                                    />
                                    <Button
                                        id="supplier-portal-review-duplicate-search"
                                        type="button"
                                        size="sm"
                                        variant="ghost"
                                        className={
                                            detailWorkspaceStyles.subtleButton
                                        }
                                        disabled={disabled}
                                        onClick={() => setQ(search.trim())}
                                    >
                                        查找重复候选
                                    </Button>
                                </div>
                                <div
                                    className={fieldHighlight(
                                        "supplier-portal-review-product-target",
                                    )}
                                >
                                    <form.AppField name="productTarget">
                                        {(field) => (
                                            <field.SelectField
                                                onRefresh={() =>
                                                    duplicates.refetch({
                                                        throwOnError: true,
                                                    })
                                                }
                                                onValueChange={() =>
                                                    setHighlightedFieldId(null)
                                                }
                                                id="supplier-portal-review-product-target"
                                                label="本次商品建档决定"
                                                required
                                                allowClear={false}
                                                options={[
                                                    {
                                                        value: "unconfirmed",
                                                        label: "请明确核对结果",
                                                        disabled: true,
                                                    },
                                                    {
                                                        value: "new",
                                                        label: "新建公司商品",
                                                    },
                                                    ...(
                                                        duplicates.data
                                                            ?.duplicates ?? []
                                                    ).map((item) => ({
                                                        value: item.product_id,
                                                        label: `复用商品：${item.name}`,
                                                    })),
                                                ]}
                                                disabled={disabled}
                                            />
                                        )}
                                    </form.AppField>
                                </div>
                                <form.Subscribe
                                    selector={(state) => ({
                                        productTarget:
                                            state.values.productTarget,
                                        mappings: state.values.mappings,
                                    })}
                                >
                                    {({
                                        productTarget: selected,
                                        mappings,
                                    }) => (
                                        <div className="divide-y divide-border/60 border-t border-border/60">
                                            {rawProduct.skus.map(
                                                (sku, index) => {
                                                    const prefix = `supplier-portal-review-sku-${toAutomationIdSegment(sku.row_id)}`
                                                    const productCandidate =
                                                        duplicates.data?.duplicates.find(
                                                            (item) =>
                                                                item.product_id ===
                                                                selected,
                                                        )
                                                    return (
                                                        <div
                                                            key={sku.row_id}
                                                            className="grid gap-4 py-4 md:grid-cols-2"
                                                        >
                                                            <div className="md:col-span-2">
                                                                <p className="font-medium">
                                                                    {sku.name} ·{" "}
                                                                    <span className="num">
                                                                        {
                                                                            sku.ordering_code
                                                                        }
                                                                    </span>
                                                                </p>
                                                                <p className="text-xs text-muted-foreground">
                                                                    提报单位：
                                                                    {
                                                                        sku.unit
                                                                            .raw_name
                                                                    }{" "}
                                                                    · 提报规格：
                                                                    {sku.spec_entries
                                                                        .map(
                                                                            (
                                                                                entry,
                                                                            ) =>
                                                                                `${entry.attribute_code}：${entry.attribute_value_code}`,
                                                                        )
                                                                        .join(
                                                                            " / ",
                                                                        )}
                                                                </p>
                                                            </div>
                                                            <div
                                                                className={fieldHighlight(
                                                                    `${prefix}-unit`,
                                                                )}
                                                            >
                                                                <form.AppField
                                                                    name={`mappings[${index}].unitId`}
                                                                >
                                                                    {(
                                                                        field,
                                                                    ) => (
                                                                        <field.SelectField
                                                                            onRefresh={() =>
                                                                                units.refetch(
                                                                                    {
                                                                                        throwOnError: true,
                                                                                    },
                                                                                )
                                                                            }
                                                                            onValueChange={(
                                                                                id,
                                                                            ) => {
                                                                                setHighlightedFieldId(
                                                                                    null,
                                                                                )
                                                                                form.setFieldValue(
                                                                                    `mappings[${index}].unitVersion`,
                                                                                    units.data?.find(
                                                                                        (
                                                                                            row,
                                                                                        ) =>
                                                                                            row.id ===
                                                                                            id,
                                                                                    )
                                                                                        ?.version ??
                                                                                        0,
                                                                                )
                                                                                form.setFieldValue(
                                                                                    `mappings[${index}].unitSynonymConfirmed`,
                                                                                    false,
                                                                                )
                                                                                form.setFieldValue(
                                                                                    `mappings[${index}].unitSynonymReason`,
                                                                                    "",
                                                                                )
                                                                            }}
                                                                            id={`${prefix}-unit`}
                                                                            label="确认基础单位"
                                                                            required
                                                                            options={dictionaryOptions(
                                                                                units.data,
                                                                            )}
                                                                            disabled={
                                                                                disabled ||
                                                                                !!sku
                                                                                    .unit
                                                                                    .selected_id
                                                                            }
                                                                            allowClear={
                                                                                !sku
                                                                                    .unit
                                                                                    .selected_id
                                                                            }
                                                                        />
                                                                    )}
                                                                </form.AppField>
                                                            </div>
                                                            {access.can(
                                                                "unit_of_measure:create",
                                                            ) &&
                                                                !sku.unit
                                                                    .selected_id && (
                                                                    <div className="flex items-end">
                                                                        <Button
                                                                            id={`${prefix}-create-unit`}
                                                                            type="button"
                                                                            size="sm"
                                                                            variant="ghost"
                                                                            className={
                                                                                detailWorkspaceStyles.subtleButton
                                                                            }
                                                                            disabled={
                                                                                disabled
                                                                            }
                                                                            onClick={() =>
                                                                                setCreateDictionary(
                                                                                    {
                                                                                        kind: "unit",
                                                                                        rawName:
                                                                                            sku
                                                                                                .unit
                                                                                                .raw_name,
                                                                                        rowId: sku.row_id,
                                                                                    },
                                                                                )
                                                                            }
                                                                        >
                                                                            补建此基础单位
                                                                        </Button>
                                                                    </div>
                                                                )}
                                                            <div
                                                                className={cn(
                                                                    "md:col-span-2",
                                                                    fieldHighlight(
                                                                        `${prefix}-unit-synonym-confirmed`,
                                                                    ),
                                                                )}
                                                            >
                                                                <form.AppField
                                                                    name={`mappings[${index}].unitSynonymConfirmed`}
                                                                >
                                                                    {(
                                                                        field,
                                                                    ) => (
                                                                        <label
                                                                            htmlFor={`${prefix}-unit-synonym-confirmed`}
                                                                            className="flex items-center gap-2 text-xs"
                                                                        >
                                                                            <NativeCheckbox
                                                                                id={`${prefix}-unit-synonym-confirmed`}
                                                                                checked={
                                                                                    field
                                                                                        .state
                                                                                        .value
                                                                                }
                                                                                disabled={
                                                                                    disabled
                                                                                }
                                                                                onCheckedChange={(
                                                                                    value,
                                                                                ) => {
                                                                                    setHighlightedFieldId(
                                                                                        null,
                                                                                    )
                                                                                    field.handleChange(
                                                                                        value,
                                                                                    )
                                                                                }}
                                                                            />
                                                                            已核对提报单位与所选标准单位含义一致，仅名称不同，不涉及包装或报价换算
                                                                        </label>
                                                                    )}
                                                                </form.AppField>
                                                            </div>
                                                            <div
                                                                className={fieldHighlight(
                                                                    `${prefix}-unit-synonym-reason`,
                                                                )}
                                                            >
                                                                <form.AppField
                                                                    name={`mappings[${index}].unitSynonymReason`}
                                                                >
                                                                    {(
                                                                        field,
                                                                    ) => (
                                                                        <field.TextField
                                                                            id={`${prefix}-unit-synonym-reason`}
                                                                            label="单位一致/换算说明（勾选一致确认时必填）"
                                                                            required={
                                                                                mappings[
                                                                                    index
                                                                                ]
                                                                                    ?.unitSynonymConfirmed
                                                                            }
                                                                            disabled={
                                                                                disabled
                                                                            }
                                                                        />
                                                                    )}
                                                                </form.AppField>
                                                            </div>
                                                            <div
                                                                className={fieldHighlight(
                                                                    `${prefix}-target`,
                                                                )}
                                                            >
                                                                <form.AppField
                                                                    name={`mappings[${index}].target`}
                                                                >
                                                                    {(
                                                                        field,
                                                                    ) => (
                                                                        <field.SelectField
                                                                            onRefresh={() =>
                                                                                duplicates.refetch(
                                                                                    {
                                                                                        throwOnError: true,
                                                                                    },
                                                                                )
                                                                            }
                                                                            onValueChange={() =>
                                                                                setHighlightedFieldId(
                                                                                    null,
                                                                                )
                                                                            }
                                                                            id={`${prefix}-target`}
                                                                            label="本次规格建档决定"
                                                                            required
                                                                            options={[
                                                                                {
                                                                                    value: "unconfirmed",
                                                                                    label: "请明确核对结果",
                                                                                    disabled: true,
                                                                                },
                                                                                {
                                                                                    value: "new",
                                                                                    label: "新建此规格，入库后未上架",
                                                                                },
                                                                                ...(
                                                                                    productCandidate?.skus ??
                                                                                    []
                                                                                ).map(
                                                                                    (
                                                                                        item,
                                                                                    ) => ({
                                                                                        value: item.sku_id,
                                                                                        label: `复用：${item.name ?? productCandidate?.name} · ${item.specification ?? "默认规格"} · ${item.sku_no ?? "已有公司规格"} · ${item.unit_name ?? "请核对单位"}`,
                                                                                    }),
                                                                                ),
                                                                            ]}
                                                                            disabled={
                                                                                disabled ||
                                                                                selected ===
                                                                                    "unconfirmed"
                                                                            }
                                                                            allowClear={
                                                                                false
                                                                            }
                                                                        />
                                                                    )}
                                                                </form.AppField>
                                                            </div>
                                                        </div>
                                                    )
                                                },
                                            )}
                                        </div>
                                    )}
                                </form.Subscribe>
                                {!!existingCandidates.length && (
                                    <form.AppField name="existingConfirmed">
                                        {(field) => (
                                            <section className="space-y-3 border-t border-border/60 pt-4">
                                                <h4 className="text-body-compact font-medium">
                                                    相同订货编码下的已有供货商品核对
                                                </h4>
                                                {existingCandidates.map(
                                                    (candidate) => {
                                                        const candidateId = `supplier-portal-review-existing-${toAutomationIdSegment(candidate.offering_id)}`
                                                        return (
                                                            <div
                                                                key={
                                                                    candidate.offering_id
                                                                }
                                                                className={fieldHighlight(
                                                                    candidateId,
                                                                )}
                                                            >
                                                                <label
                                                                    htmlFor={
                                                                        candidateId
                                                                    }
                                                                    className="flex items-start gap-2 text-xs"
                                                                >
                                                                    <NativeCheckbox
                                                                        id={
                                                                            candidateId
                                                                        }
                                                                        checked={field.state.value.includes(
                                                                            candidate.offering_id,
                                                                        )}
                                                                        disabled={
                                                                            disabled
                                                                        }
                                                                        onCheckedChange={(
                                                                            checked,
                                                                        ) => {
                                                                            setHighlightedFieldId(
                                                                                null,
                                                                            )
                                                                            field.handleChange(
                                                                                checked
                                                                                    ? [
                                                                                          ...field
                                                                                              .state
                                                                                              .value,
                                                                                          candidate.offering_id,
                                                                                      ]
                                                                                    : field.state.value.filter(
                                                                                          (
                                                                                              id,
                                                                                          ) =>
                                                                                              id !==
                                                                                              candidate.offering_id,
                                                                                      ),
                                                                            )
                                                                        }}
                                                                    />
                                                                    <span>
                                                                        我已核对订货编码{" "}
                                                                        <span className="num">
                                                                            {candidate.supplier_sku_code ??
                                                                                "本次订货编码"}
                                                                        </span>{" "}
                                                                        及第{" "}
                                                                        <span className="num">
                                                                            {
                                                                                candidate.expected_revision_no
                                                                            }
                                                                        </span>{" "}
                                                                        版，确认修订该既有供给。
                                                                    </span>
                                                                </label>
                                                            </div>
                                                        )
                                                    },
                                                )}
                                            </section>
                                        )}
                                    </form.AppField>
                                )}
                                <p className="text-xs text-muted-foreground">
                                    仅本次新增规格保持未上架；复用在售商品规格并增加有效供货后，可能立即参与销售及采购供应选择。
                                </p>
                            </div>
                        </div>
                    </section>
                )}
                <div className="max-w-4xl space-y-5">
                    {initialAvailability && (
                        <section className="space-y-3 border-t border-border/60 pt-5">
                            <h3 className="font-medium">
                                初始可供报送时间核对
                            </h3>
                            <p className="text-xs text-muted-foreground">
                                当前未配置自动过期时限；请核对供应商实际报送时间，需要新信息时退回确认。通过审核不会重新报送供应商可供情况。
                            </p>
                            {(rawProduct
                                ? rawProduct.skus.map((sku) => ({
                                      name: sku.name,
                                      at: sku.reported_at,
                                  }))
                                : [
                                      {
                                          name: "本次首次报价",
                                          at: (
                                              application.submitted_snapshot ??
                                              application.input
                                          ).availability_reported_at as number,
                                      },
                                  ]
                            ).map((row, index) => {
                                const reportedMs = new Date(
                                    typeof row.at === "number"
                                        ? row.at * 1000
                                        : row.at,
                                ).getTime()
                                const elapsed =
                                    nowMs != null && Number.isFinite(reportedMs)
                                        ? Math.floor(
                                              (nowMs - reportedMs) / 60000,
                                          )
                                        : null
                                return (
                                    <p
                                        key={index}
                                        className="text-body-compact"
                                    >
                                        {row.name}：
                                        <span className="num">
                                            {timeLabel(row.at)}
                                        </span>
                                        {elapsed != null ? (
                                            <span className="num text-muted-foreground">
                                                {" · "}
                                                {elapsed >= 0
                                                    ? elapsed < 60
                                                        ? `距今 ${elapsed} 分钟`
                                                        : `距今 ${Math.floor(elapsed / 60)} 小时`
                                                    : "请核对报送时间"}
                                            </span>
                                        ) : null}
                                    </p>
                                )
                            })}
                            <div
                                className={fieldHighlight(
                                    "supplier-portal-review-reported-confirmed",
                                )}
                            >
                                <label
                                    htmlFor="supplier-portal-review-reported-confirmed"
                                    className="flex items-start gap-2 text-xs"
                                >
                                    <NativeCheckbox
                                        id="supplier-portal-review-reported-confirmed"
                                        checked={reportedConfirmed}
                                        disabled={disabled}
                                        onCheckedChange={(checked) => {
                                            setHighlightedFieldId(null)
                                            setReportedConfirmed(checked)
                                        }}
                                    />
                                    我已逐项核对供应商实际报送时间，并确认本次可供资料可用于采购确认。
                                </label>
                            </div>
                        </section>
                    )}
                    {canProcess && (
                        <div
                            className={cn(
                                "border-t border-border/60 pt-5",
                                fieldHighlight(
                                    access.can("procurement_person:list")
                                        ? "supplier-portal-review-maintainer"
                                        : "supplier-portal-review-maintainer-self",
                                ),
                            )}
                        >
                            <form.AppField name="maintainerUserId">
                                {(field) => (
                                    <div className="space-y-2">
                                        <p className="text-xs text-muted-foreground">
                                            通过前请指定内部跟进负责人。
                                        </p>
                                        {access.can(
                                            "procurement_person:list",
                                        ) ? (
                                            <PersonDirectoryFilter
                                                id="supplier-portal-review-maintainer"
                                                category="procurement"
                                                value={field.state.value}
                                                onChange={(value) => {
                                                    if (!disabled) {
                                                        setHighlightedFieldId(
                                                            null,
                                                        )
                                                        field.handleChange(
                                                            value,
                                                        )
                                                    }
                                                }}
                                                label="建档及供给内部维护人"
                                                selectionMode="single"
                                            />
                                        ) : (
                                            <>
                                                <p className="text-body-compact">
                                                    当前暂无可选采购人员列表，可选择由本人跟进或转交他人处理。
                                                </p>
                                                <Button
                                                    id="supplier-portal-review-maintainer-self"
                                                    type="button"
                                                    size="sm"
                                                    variant="ghost"
                                                    className={
                                                        detailWorkspaceStyles.subtleButton
                                                    }
                                                    disabled={
                                                        disabled ||
                                                        !access.profile.data
                                                            ?.userid
                                                    }
                                                    onClick={() => {
                                                        setHighlightedFieldId(
                                                            null,
                                                        )
                                                        field.handleChange(
                                                            access.profile.data
                                                                ?.userid ?? "",
                                                        )
                                                    }}
                                                >
                                                    {field.state.value ===
                                                    access.profile.data?.userid
                                                        ? "已选择由我维护"
                                                        : "由我维护"}
                                                </Button>
                                            </>
                                        )}
                                    </div>
                                )}
                            </form.AppField>
                        </div>
                    )}
                    <div className="grid gap-5 border-t border-border/60 pt-5 md:grid-cols-2">
                        <div
                            className={fieldHighlight(
                                "supplier-portal-review-comment",
                            )}
                        >
                            <form.AppField name="internalNote">
                                {(field) => (
                                    <field.TextareaField
                                        id="supplier-portal-review-comment"
                                        required={Boolean(rawProduct)}
                                        label={
                                            rawProduct
                                                ? "内部核对备注（通过必填，仅内部留档）"
                                                : "内部核对备注（仅内部留档）"
                                        }
                                        description="记录核对依据与复用/新建理由，仅采购内部留档，退回时不会发送给供应商。"
                                        placeholder="填写匹配依据或内部处理备忘"
                                        disabled={disabled}
                                        rows={3}
                                    />
                                )}
                            </form.AppField>
                        </div>
                        <div
                            className={fieldHighlight(
                                "supplier-portal-review-return-reason",
                            )}
                        >
                            <form.AppField name="returnReason">
                                {(field) => (
                                    <field.TextareaField
                                        id="supplier-portal-review-return-reason"
                                        label="退回原因（供应商可见，退回必填）"
                                        description="仅在退回供应商修改时发送给供应商，请明确指出需调整的字段或材料。"
                                        placeholder="如需退回，请填写清晰可执行的修改说明"
                                        disabled={disabled}
                                        rows={3}
                                    />
                                )}
                            </form.AppField>
                        </div>
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
