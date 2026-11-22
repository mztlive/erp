"use client"

import * as React from "react"
import {
    AlertCircleIcon,
    ArrowRightIcon,
    CheckIcon,
    InfoIcon,
    RefreshCwIcon,
    SaveIcon,
    TriangleAlertIcon,
} from "lucide-react"
import { useRouter } from "@/lib/navigation"
import {
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspaceDirtyBadge,
    DetailWorkspaceMeta,
    DetailWorkspaceMetricItem,
    DetailWorkspaceMetrics,
    DetailWorkspacePage,
    DetailWorkspaceSection,
    DraftSaveIndicator,
    FormalActionConfirmDialog,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { LoadingButton } from "@/components/ui/loading-button"
import { toast } from "@/components/ui/toast"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { useMasterDataListQuery } from "@/features/master-data/hooks/queries"
import {
    buildCategoryForest,
    flattenCategoryForest,
} from "@/features/master-data/lib/category-tree-model"
import { useUnitOptionsQuery } from "@/hooks/use-options"
import { getErrorMessage, isApiError } from "@/lib/api/errors"
import { hasPermission } from "@/lib/permissions"
import { cn } from "@/lib/utils"
import { divideFixed, multiplyFixed } from "@/lib/fixed-decimal"
import { useWorkspaceDirtyState } from "@/lib/workspace-form-guard"
import {
    usePreviewAdoptionBatchMutation,
    useReconcileAdoptionBatchMutation,
    useQueryAdoptionBatchSubmissionMutation,
    useSaveAdoptionBatchMutation,
    useSubmitAdoptionBatchMutation,
} from "../hooks/batch"
import { useConnectionCenterQuery } from "../hooks/queries"
import { useBatchWorkspace } from "../hooks/use-batch-workspace"
import type {
    BatchFieldEdit,
    BatchItemEdit,
    BatchRow,
    BatchStep,
    BatchView,
} from "../api/batch"
import { newIdempotencyKey } from "../lib/operations"
import { BatchPricingPanel } from "../components/batch/batch-pricing"
import { BatchResultSummary } from "../components/batch/batch-result-summary"
import { BatchReviewSummary } from "../components/batch/batch-review-summary"
import { BatchDetailsPanel } from "../components/batch/batch-details"
import type { CategoryMappingListHandle } from "../components/batch/batch-category-mappings"
import {
    BatchItemDialog,
    type BatchItemDraft,
    type BatchItemEdits,
} from "../components/batch/batch-item-dialog"
import {
    BatchReviewTable,
    isBatchRowSelectable,
} from "../components/batch/batch-review-table"

const STEPS: Array<{ step: BatchStep; label: string }> = [
    { step: "PRICING", label: "设置售价" },
    { step: "DETAILS", label: "补齐资料" },
    { step: "REVIEW", label: "核对入库" },
    { step: "RESULT", label: "入库结果" },
]
const inputValue = (edit: BatchFieldEdit, previous: string): string =>
    edit.action === "KEEP"
        ? previous
        : edit.action === "CLEAR"
          ? ""
          : edit.value

type StoredSubmission = {
    connectionId: string
    batchId: string
    request: {
        expected_version: number
        preview_version: number
        item_ids: string[]
        idempotency_key: string
    }
}
function restoredSubmission(
    key: string,
    connectionId: string,
    batchId: string,
): StoredSubmission | null {
    if (typeof window === "undefined") return null
    try {
        const raw = window.sessionStorage.getItem(key)
        if (!raw) return null
        const stored = JSON.parse(raw) as StoredSubmission
        return stored.connectionId === connectionId &&
            stored.batchId === batchId &&
            typeof stored.request?.expected_version === "number" &&
            typeof stored.request.preview_version === "number" &&
            typeof stored.request.idempotency_key === "string" &&
            Array.isArray(stored.request.item_ids) &&
            stored.request.item_ids.every((id) => typeof id === "string")
            ? stored
            : null
    } catch {
        return null
    }
}

function rowDraft(row: BatchRow): BatchItemDraft {
    return {
        dropship: row.sales_prices.dropship.value ?? "",
        bulk: row.sales_prices.bulk.value ?? "",
        minimumQuantity: row.sales_prices.minimum_quantity.value ?? "",
        marketPrice: row.sales_prices.market_price.value ?? "",
        name: row.mapping.company_name ?? row.source.name,
        description:
            row.mapping.company_description ?? row.source.description ?? "",
        categoryId: row.mapping.category_id,
        unitId: row.mapping.unit_id,
        inputTaxPercentage: row.terms?.input_tax_rate
            ? multiplyFixed(row.terms.input_tax_rate, "100", {
                  leftMaxScale: 6,
                  rightMaxScale: 0,
                  outputScale: 4,
              })
            : "",
        validFrom: row.terms?.valid_from ?? "",
        validTo: row.terms?.valid_to ?? "",
        deliveryTemplateId: row.delivery_template_id ?? "",
    }
}
function itemPatch(row: BatchRow, edits: BatchItemEdits): BatchItemEdit {
    const initial = rowDraft(row)
    const changed = (fields: Array<keyof BatchItemDraft>) =>
        fields.some((field) => edits[field].action !== "KEEP")
    const patch: BatchItemEdit = { item_id: row.source.id }
    if (changed(["dropship", "bulk", "minimumQuantity", "marketPrice"]))
        patch.sales_edit = {
            dropship: edits.dropship,
            bulk: edits.bulk,
            minimum_quantity: edits.minimumQuantity,
            market_price: edits.marketPrice,
        }
    if (changed(["name", "description", "categoryId", "unitId"]))
        patch.mapping = {
            ...row.mapping,
            company_name: inputValue(edits.name, initial.name),
            company_description: inputValue(
                edits.description,
                initial.description,
            ),
            category_id: inputValue(edits.categoryId, initial.categoryId),
            unit_id: inputValue(edits.unitId, initial.unitId),
        }
    if (changed(["inputTaxPercentage", "validFrom", "validTo"])) {
        const tax = inputValue(
            edits.inputTaxPercentage,
            initial.inputTaxPercentage,
        )
        patch.terms = {
            dropship_supply_price_gross: row.source.purchase_price ?? "",
            bulk_supply_price_gross: row.terms?.bulk_supply_price_gross ?? null,
            bulk_minimum_order_quantity:
                row.terms?.bulk_minimum_order_quantity ?? null,
            product_capabilities: row.terms?.product_capabilities ?? [],
            input_tax_rate: tax
                ? divideFixed(tax, "100", {
                      numeratorMaxScale: 4,
                      denominatorMaxScale: 0,
                      outputScale: 6,
                  })
                : "",
            valid_from: inputValue(edits.validFrom, initial.validFrom),
            valid_to: inputValue(edits.validTo, initial.validTo) || null,
        }
    }
    patch.delivery_template_id = edits.deliveryTemplateId
    return patch
}

export function BatchWorkspace({
    view,
    reviewedVersion,
    actorId,
    refreshError,
    refreshing = false,
    onRefresh,
}: {
    view: BatchView
    reviewedVersion: number | null
    actorId: string
    refreshError?: unknown
    refreshing?: boolean
    onRefresh: () => Promise<unknown>
}) {
    const router = useRouter()
    const workspace = useBatchWorkspace(view, reviewedVersion)
    const { form } = workspace
    const identity = {
        connectionId: view.batch.connection_id,
        batchId: view.batch.id,
    }
    const center = useConnectionCenterQuery(identity.connectionId)
    const profile = useAccountProfileQuery()
    const canReadCost = hasPermission(
        profile.data?.permissions,
        "supplier_offering_cost:detail",
    )
    const canWrite =
        canReadCost &&
        hasPermission(
            profile.data?.permissions,
            "supplier_api_connection:catalog_sync",
        )
    const categories = useMasterDataListQuery({
        resource: "categories",
        lifecycleStatus: "enabled",
        revisionTiming: "current",
    })
    const units = useUnitOptionsQuery()
    const categoryOptions = flattenCategoryForest(
        buildCategoryForest(categories.data?.rows ?? []),
    )
        .filter(({ item }) => item.productKind === "实物")
        .map(({ item, pathLabel }) => ({
            value: item.stableId,
            label: pathLabel,
        }))
    const unitOptions = (units.data ?? []).map((unit) => ({
        value: unit.id,
        label: unit.label,
    }))
    const save = useSaveAdoptionBatchMutation()
    const preview = usePreviewAdoptionBatchMutation()
    const submit = useSubmitAdoptionBatchMutation()
    const reconcile = useReconcileAdoptionBatchMutation()
    const querySubmission = useQueryAdoptionBatchSubmissionMutation()
    const [submissionNotice, setSubmissionNotice] = React.useState<
        string | null
    >(null)
    const [editingId, setEditingId] = React.useState<string | null>(null)
    const [selectedIds, setSelectedIds] = React.useState<string[]>([])
    const [leaveOpen, setLeaveOpen] = React.useState(false)
    const [confirmSubmitOpen, setConfirmSubmitOpen] = React.useState(false)
    const [pendingAction, setPendingAction] = React.useState<string | null>(
        null,
    )
    const categoryListRef = React.useRef<CategoryMappingListHandle>(null)
    const missingCategoryCount = workspace.values.categoryMappings.filter(
        (row) => !row.categoryId,
    ).length
    const storageKey = `supplier-catalog-batch-submission-${actorId}-${view.batch.connection_id}-${view.batch.id}`
    const originalSubmission = restoredSubmission(
        storageKey,
        identity.connectionId,
        identity.batchId,
    )
    const [uncertainSubmission, setUncertainSubmission] = React.useState(
        () => typeof window !== "undefined" && Boolean(originalSubmission),
    )
    const requestIdentity = React.useRef<
        Parameters<typeof submit.mutateAsync>[0] | null
    >(originalSubmission)
    const step = view.batch.step
    const tableMode =
        step === "RECOVERY"
            ? "RECOVERY"
            : step === "RESULT"
              ? "RESULT"
              : "REVIEW"
    const busy =
        workspace.busy ||
        save.isPending ||
        preview.isPending ||
        submit.isPending ||
        reconcile.isPending ||
        querySubmission.isPending
    useWorkspaceDirtyState({ dirty: workspace.dirty, busy })
    const processing = view.rows.filter(
        (row) =>
            row.status === "PROCESSING" ||
            (row.status === "READY" && row.operation_id),
    ).length
    const [nowSec, setNowSec] = React.useState(() =>
        Math.floor(Date.now() / 1000),
    )
    const hasDraftOperation = Boolean(view.batch.draft_operation)
    const draftStartedAt = view.batch.draft_started_at
    React.useEffect(() => {
        if (!hasDraftOperation || draftStartedAt == null) return
        const currentSec = Math.floor(Date.now() / 1000)
        setNowSec(currentSec)
        const remainingMs = (draftStartedAt + 300 - currentSec) * 1000
        if (remainingMs <= 0) return
        const timer = window.setTimeout(() => {
            setNowSec(Math.floor(Date.now() / 1000))
        }, remainingMs + 250)
        return () => window.clearTimeout(timer)
    }, [hasDraftOperation, draftStartedAt])
    const draftActive =
        hasDraftOperation &&
        draftStartedAt != null &&
        nowSec - draftStartedAt < 300
    const draftInterrupted = hasDraftOperation && !draftActive
    const preparing = view.batch.confirmations.some(
        (entry) =>
            !view.batch.published_operations.includes(entry.operation_id),
    )
    const editable = canWrite && processing === 0 && !preparing && !draftActive
    const editing = view.rows.find((row) => row.source.id === editingId)
    const unsubmittedReady = view.rows.filter(
        (row) => row.status === "READY" && !row.operation_id,
    ).length
    const readyIds = view.rows
        .filter(
            (row) =>
                isBatchRowSelectable(row, "REVIEW") &&
                row.preview_version === view.batch.preview_version,
        )
        .map((row) => row.source.id)
    const selectedReady = selectedIds.filter((id) => readyIds.includes(id))
    const selectedReadySet = new Set(selectedReady)
    const selectedSummaryCounts = {
        createProduct: view.rows.filter(
            (row) =>
                selectedReadySet.has(row.source.id) &&
                row.resolution?.kind === "CREATE_PRODUCT",
        ).length,
        createSku: view.rows.filter(
            (row) =>
                selectedReadySet.has(row.source.id) &&
                row.resolution?.kind === "CREATE_SKU",
        ).length,
        existing: view.rows.filter(
            (row) =>
                selectedReadySet.has(row.source.id) &&
                row.resolution?.kind === "EXISTING",
        ).length,
    }
    const needsRecheck =
        workspace.dirty ||
        !workspace.hasCurrentReview ||
        view.batch.preview_version == null
    const warningRows = view.rows.filter(
        (row) =>
            row.warnings.length &&
            !row.warnings_confirmed &&
            ["DRAFT", "READY", "FAILED"].includes(row.status) &&
            !row.operation_id,
    )
    const selectionRevision = React.useRef("")
    React.useEffect(() => {
        const revision = `${view.batch.preview_version}:${tableMode}`
        const selectable = view.rows
            .filter(
                (row) =>
                    isBatchRowSelectable(row, tableMode) &&
                    (tableMode !== "REVIEW" ||
                        row.preview_version === view.batch.preview_version),
            )
            .map((row) => row.source.id)
        if (selectionRevision.current !== revision) {
            selectionRevision.current = revision
            setSelectedIds(tableMode === "RECOVERY" ? [] : selectable)
        } else
            setSelectedIds((current) =>
                current.filter((id) => selectable.includes(id)),
            )
    }, [view.batch.preview_version, tableMode, view.rows])
    const runAction = async (action: string, task: () => Promise<unknown>) => {
        setPendingAction(action)
        try {
            return await task()
        } finally {
            setPendingAction((current) => (current === action ? null : current))
        }
    }
    const updateStep = async (next: BatchStep, actionKey = "step") => {
        await runAction(actionKey, async () => {
            try {
                const saved = await save.mutateAsync({
                    ...identity,
                    request: {
                        expected_version: view.batch.version,
                        step: next,
                    },
                })
                workspace.acceptSaved(saved)
            } catch {
                /* 页面显示错误，保留当前内容。 */
            }
        })
    }
    const persistWithAction = async (
        actionKey: string,
        next?: BatchStep,
        check = false,
    ) => {
        let ok = false
        await runAction(actionKey, async () => {
            ok = await workspace.persist(next, check)
        })
        return ok
    }
    const handleStepJump = async (target: BatchStep) => {
        if (!canWrite || !editable || busy || step === "RESULT") return
        if (step === "PRICING" || step === "DETAILS") {
            await persistWithAction(`step-${target}`, target)
        } else {
            await updateStep(target, `step-${target}`)
        }
    }
    const saveItem = async (patch: BatchItemEdit) => {
        const saved = await save.mutateAsync({
            ...identity,
            request: { expected_version: view.batch.version, items: [patch] },
        })
        workspace.acceptSaved(saved)
    }
    const handleExcludeSelected = async (ids: string[]) => {
        if (!ids.length) return
        await runAction("exclude", async () => {
            const nextExcluded = Array.from(
                new Set([...view.batch.excluded_ids, ...ids]),
            )
            const saved = await save.mutateAsync({
                ...identity,
                request: {
                    expected_version: view.batch.version,
                    excluded_ids: nextExcluded,
                },
            })
            workspace.acceptSaved(saved)
            const removed = new Set(ids)
            setSelectedIds((current) =>
                current.filter((id) => !removed.has(id)),
            )
            toast.add({
                type: "success",
                title: `已从本批排除 ${ids.length} 个 SKU`,
            })
        })
    }
    const verifyOriginal = async () => {
        await runAction("reconcile", async () => {
            try {
                const original = requestIdentity.current
                if (original) {
                    const result = await querySubmission.mutateAsync(original)
                    submit.reset()
                    workspace.acceptSaved(result.view)
                    if (result.status === "NOT_ACCEPTED")
                        workspace.requireReview()
                    window.sessionStorage.removeItem(storageKey)
                    setUncertainSubmission(false)
                    requestIdentity.current = null
                    setSubmissionNotice(
                        result.status === "NOT_ACCEPTED"
                            ? "原确认未受理，已停止接收该次请求。请重新核对后确认入库。"
                            : "原确认已受理，请查看本批实际处理结果。",
                    )
                } else {
                    const checked = await reconcile.mutateAsync({
                        ...identity,
                        expected_version: view.batch.version,
                    })
                    workspace.acceptSaved(checked)
                }
                await onRefresh()
            } catch {
                /* 保持原操作待核实，禁止发起新操作。 */
            }
        })
    }
    const confirm = async () => {
        if (
            !editable ||
            workspace.dirty ||
            !workspace.hasCurrentReview ||
            uncertainSubmission ||
            view.batch.preview_version == null ||
            !selectedReady.length
        )
            return
        const request = {
            ...identity,
            request: {
                expected_version: view.batch.version,
                preview_version: view.batch.preview_version,
                item_ids: selectedReady,
                idempotency_key: newIdempotencyKey("catalog-batch"),
            },
        }
        setSubmissionNotice(null)
        requestIdentity.current = request
        window.sessionStorage.setItem(storageKey, JSON.stringify(request))
        setUncertainSubmission(true)
        try {
            const result = await submit.mutateAsync(request)
            workspace.acceptSaved(result)
            requestIdentity.current = null
            window.sessionStorage.removeItem(storageKey)
            setUncertainSubmission(false)
            toast.add({
                type: "success",
                title: `已提交 ${selectedReady.length} 个 SKU 入库`,
            })
        } catch (error) {
            if (
                isApiError(error) &&
                error.status &&
                [400, 401, 403, 404, 409, 422].includes(error.status)
            ) {
                requestIdentity.current = null
                window.sessionStorage.removeItem(storageKey)
                setUncertainSubmission(false)
            }
            /* 网络或提交结果不明时保留原请求，只查询原结果。 */
        }
    }
    const recheck = async (actionKey = "recheck") => {
        await runAction(actionKey, async () => {
            try {
                const checked = await preview.mutateAsync({
                    ...identity,
                    expected_version: view.batch.version,
                    item_ids: step === "RECOVERY" ? selectedIds : undefined,
                })
                workspace.acceptPreview(checked)
            } catch {
                /* 服务端失败原因显示在当前页面。 */
            }
        })
    }
    const returnToCatalog = () =>
        router.push(
            `/supplier-api/catalog?connectionId=${encodeURIComponent(identity.connectionId)}`,
        )
    const errors = [
        workspace.save.error,
        workspace.preview.error,
        save.error,
        preview.error,
        submit.error,
        reconcile.error,
        querySubmission.error,
    ].filter(Boolean)
    const completed = view.counts.succeeded + view.counts.skipped
    const activeStepIndex = STEPS.findIndex(
        (entry) => entry.step === (step === "RECOVERY" ? "DETAILS" : step),
    )
    const primaryStatus =
        processing > 0 || preparing || view.batch.execution_pending
            ? { label: "正在入库", tone: "info" as const }
            : uncertainSubmission || view.counts.unknown > 0
              ? { label: "结果待核实", tone: "warning" as const }
              : step === "RESULT"
                ? view.counts.failed + view.counts.draft + unsubmittedReady > 0
                    ? {
                          label: completed > 0 ? "部分完成" : "入库未完成",
                          tone: "warning" as const,
                      }
                    : { label: "入库完成", tone: "success" as const }
                : step === "RECOVERY"
                  ? { label: "待处理恢复", tone: "warning" as const }
                  : step === "REVIEW"
                    ? needsRecheck
                        ? { label: "待重新核对", tone: "warning" as const }
                        : { label: "已核对可入库", tone: "success" as const }
                    : step === "DETAILS"
                      ? { label: "补齐资料", tone: "info" as const }
                      : { label: "设置售价", tone: "info" as const }
    return (
        <DetailWorkspacePage>
            <DetailWorkspaceChrome
                documentNumber={view.batch.batch_no}
                numberLabel="批次编号"
                primaryStatus={primaryStatus}
                titleExtra={
                    workspace.dirty ? <DetailWorkspaceDirtyBadge /> : undefined
                }
                title={
                    step === "PRICING"
                        ? "设置售价"
                        : step === "DETAILS"
                          ? "补齐资料"
                          : step === "RECOVERY"
                            ? "待处理商品"
                            : step === "RESULT"
                              ? "入库结果"
                              : "批量选品入库"
                }
                back={{
                    id: "supplier-catalog-batch-return-catalog",
                    label: "返回商品库",
                    visibility: "always",
                    onClick: () =>
                        workspace.dirty
                            ? setLeaveOpen(true)
                            : returnToCatalog(),
                }}
                meta={
                    <DetailWorkspaceMeta
                        items={[
                            center.data?.supplier.name ?? "供应商商品库",
                            <span className="num" key="counts">
                                {view.product_count} 个商品 /{" "}
                                {view.batch.selected_count -
                                    view.counts.excluded}{" "}
                                个 SKU
                            </span>,
                        ]}
                    />
                }
            >
                {step !== "RESULT" ? (
                    <ol
                        className="flex items-center overflow-x-auto py-1"
                        aria-label="批量入库步骤"
                    >
                        {STEPS.map((entry, index) => {
                            const isCurrent = index === activeStepIndex
                            const isComplete = index < activeStepIndex
                            const canJump =
                                isComplete &&
                                canWrite &&
                                editable &&
                                !busy &&
                                entry.step !== "RESULT"
                            const stepBadge = (
                                <>
                                    <span
                                        className={cn(
                                            "num flex size-7 shrink-0 items-center justify-center rounded-full text-body-compact",
                                            isCurrent || isComplete
                                                ? "bg-primary text-primary-foreground"
                                                : "bg-muted text-muted-foreground",
                                        )}
                                    >
                                        {isComplete ? (
                                            <>
                                                <CheckIcon
                                                    className="size-4"
                                                    aria-hidden
                                                />
                                                <span className="sr-only">
                                                    已完成
                                                </span>
                                            </>
                                        ) : (
                                            index + 1
                                        )}
                                    </span>
                                    <span
                                        className={cn(
                                            "whitespace-nowrap text-xs sm:text-body-compact",
                                            isCurrent
                                                ? "font-medium text-foreground"
                                                : canJump
                                                  ? "text-foreground group-hover:text-primary"
                                                  : "text-muted-foreground",
                                        )}
                                    >
                                        {entry.label}
                                    </span>
                                </>
                            )
                            return (
                                <li
                                    key={entry.step}
                                    aria-current={
                                        isCurrent ? "step" : undefined
                                    }
                                    className="flex min-w-0 flex-1 items-center last:flex-none"
                                >
                                    {canJump ? (
                                        <button
                                            id={`supplier-catalog-batch-step-${entry.step.toLowerCase()}`}
                                            type="button"
                                            className="group flex shrink-0 cursor-pointer flex-col items-center gap-2 rounded-md px-1 py-0.5 transition-colors hover:bg-muted/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring sm:flex-row"
                                            onClick={() =>
                                                void handleStepJump(entry.step)
                                            }
                                        >
                                            {stepBadge}
                                        </button>
                                    ) : (
                                        <div className="flex shrink-0 flex-col items-center gap-2 sm:flex-row">
                                            {stepBadge}
                                        </div>
                                    )}
                                    {index < STEPS.length - 1 ? (
                                        <span
                                            aria-hidden
                                            className={cn(
                                                "mx-2 mt-3.5 h-0.5 min-w-2 flex-1 self-start sm:mx-4 sm:mt-0 sm:self-auto",
                                                isComplete
                                                    ? "bg-primary"
                                                    : "bg-border",
                                            )}
                                        />
                                    ) : null}
                                </li>
                            )
                        })}
                    </ol>
                ) : null}
            </DetailWorkspaceChrome>
            <DetailWorkspaceBody
                alerts={
                    <>
                        {refreshError ? (
                            <Alert variant="warning">
                                <TriangleAlertIcon aria-hidden="true" />
                                <AlertTitle>批次状态自动刷新失败</AlertTitle>
                                <AlertDescription className="flex flex-wrap items-center justify-between gap-2">
                                    <span>
                                        {getErrorMessage(
                                            refreshError,
                                            "网络波动导致刷新失败，当前草稿与数据已保留。",
                                        )}
                                    </span>
                                    <LoadingButton
                                        id="supplier-catalog-batch-refresh-retry"
                                        type="button"
                                        variant="outline"
                                        size="xs"
                                        loading={refreshing}
                                        onClick={() => void onRefresh()}
                                    >
                                        <RefreshCwIcon aria-hidden="true" />
                                        重新读取
                                    </LoadingButton>
                                </AlertDescription>
                            </Alert>
                        ) : null}
                        {!canWrite ? (
                            <Alert variant="info">
                                <InfoIcon aria-hidden="true" />
                                <AlertDescription>
                                    当前账号没有入库或接口成本权限，可查看已获授权的批次内容。
                                </AlertDescription>
                            </Alert>
                        ) : null}
                        {view.batch.draft_operation ? (
                            <Alert
                                variant={draftInterrupted ? "warning" : "info"}
                            >
                                {draftInterrupted ? (
                                    <TriangleAlertIcon aria-hidden="true" />
                                ) : (
                                    <InfoIcon aria-hidden="true" />
                                )}
                                <AlertTitle>
                                    {draftActive
                                        ? "正在同步草稿与核对"
                                        : "上次保存或核对已中断"}
                                </AlertTitle>
                                <AlertDescription className="flex flex-wrap items-center justify-between gap-3">
                                    <span>
                                        {draftActive
                                            ? "正在保存或核对本批商品，处理进度会自动恢复。"
                                            : "上次保存或核对已超时中断，请重新读取后继续核对。"}
                                    </span>
                                    <div className="flex flex-wrap items-center gap-2">
                                        <LoadingButton
                                            id="supplier-catalog-batch-refresh-draft"
                                            type="button"
                                            variant="outline"
                                            size="sm"
                                            loading={refreshing}
                                            onClick={() => void onRefresh()}
                                        >
                                            重新读取批次
                                        </LoadingButton>
                                        {draftInterrupted && canWrite ? (
                                            <LoadingButton
                                                id="supplier-catalog-batch-resume-draft"
                                                type="button"
                                                size="sm"
                                                loading={
                                                    pendingAction ===
                                                    "resume-draft"
                                                }
                                                disabled={busy || !canWrite}
                                                onClick={() =>
                                                    void recheck("resume-draft")
                                                }
                                            >
                                                继续核对
                                            </LoadingButton>
                                        ) : null}
                                    </div>
                                </AlertDescription>
                            </Alert>
                        ) : null}
                        {errors.map((error, index) => (
                            <Alert
                                key={`${index}-${error?.message}`}
                                variant="destructive"
                            >
                                <AlertCircleIcon aria-hidden="true" />
                                <AlertDescription>
                                    {error?.message}
                                </AlertDescription>
                            </Alert>
                        ))}
                        {submissionNotice ? (
                            <Alert variant="info">
                                <InfoIcon aria-hidden="true" />
                                <AlertDescription>
                                    {submissionNotice}
                                </AlertDescription>
                            </Alert>
                        ) : null}
                        {canWrite && uncertainSubmission ? (
                            <Alert variant="warning">
                                <TriangleAlertIcon aria-hidden="true" />
                                <AlertTitle>本次确认结果待核实</AlertTitle>
                                <AlertDescription className="flex flex-wrap items-center justify-between gap-3">
                                    <span>
                                        请先查询原操作结果，查明前不能直接重新提交。
                                    </span>
                                    <LoadingButton
                                        id="supplier-catalog-batch-query-submission"
                                        type="button"
                                        variant="outline"
                                        size="sm"
                                        loading={pendingAction === "reconcile"}
                                        disabled={busy}
                                        onClick={() => void verifyOriginal()}
                                    >
                                        查询原操作结果
                                    </LoadingButton>
                                </AlertDescription>
                            </Alert>
                        ) : null}
                    </>
                }
            >
                <DetailWorkspaceSection contentClassName="space-y-4">
                    {canWrite && (step === "PRICING" || step === "DETAILS") ? (
                        <form.AppForm>
                            <form
                                id="supplier-catalog-batch-draft-form"
                                onSubmit={(event) => {
                                    event.preventDefault()
                                    event.stopPropagation()
                                    void persistWithAction("save")
                                }}
                            >
                                {step === "PRICING" ? (
                                    <BatchPricingPanel
                                        workspace={workspace}
                                        view={view}
                                        disabled={!editable || busy}
                                    />
                                ) : (
                                    <fieldset
                                        disabled={!editable || busy}
                                        className="min-w-0 disabled:opacity-70"
                                    >
                                        <BatchDetailsPanel
                                            workspace={workspace}
                                            supplierId={view.batch.supplier_id}
                                            categories={categoryOptions}
                                            categoryListRef={categoryListRef}
                                            disabled={!editable || busy}
                                        />
                                    </fieldset>
                                )}
                            </form>
                        </form.AppForm>
                    ) : (
                        <>
                            {step === "REVIEW" ? (
                                <BatchReviewSummary
                                    view={view}
                                    readyIds={readyIds}
                                    needsRecheck={needsRecheck}
                                />
                            ) : step === "RESULT" ? (
                                <BatchResultSummary
                                    view={view}
                                    processing={processing}
                                    preparing={preparing}
                                    unsubmittedReady={unsubmittedReady}
                                    busy={busy}
                                    refreshing={refreshing}
                                    recovering={
                                        pendingAction === "open-recovery"
                                    }
                                    reconciling={pendingAction === "reconcile"}
                                    canWrite={canWrite}
                                    onRefresh={() => void onRefresh()}
                                    onRecovery={() =>
                                        void updateStep(
                                            "RECOVERY",
                                            "open-recovery",
                                        )
                                    }
                                    onReconcile={() => void verifyOriginal()}
                                    onReturn={() =>
                                        workspace.dirty
                                            ? setLeaveOpen(true)
                                            : returnToCatalog()
                                    }
                                />
                            ) : (
                                <DetailWorkspaceMetrics
                                    variant="inline"
                                    columns={4}
                                    ariaLabel="待处理商品统计"
                                    className="xl:grid-cols-5"
                                >
                                    <DetailWorkspaceMetricItem
                                        label="本批 SKU"
                                        value={
                                            view.counts.total -
                                            view.counts.excluded
                                        }
                                    />
                                    <DetailWorkspaceMetricItem
                                        label="待重新核对"
                                        value={unsubmittedReady}
                                    />
                                    <DetailWorkspaceMetricItem
                                        label="结果待核实"
                                        value={view.counts.unknown}
                                    />
                                    <DetailWorkspaceMetricItem
                                        label="待补齐"
                                        value={view.counts.draft}
                                    />
                                    <DetailWorkspaceMetricItem
                                        label="已完成跳过"
                                        value={completed}
                                    />
                                </DetailWorkspaceMetrics>
                            )}
                            {step !== "RESULT" && preparing && !processing ? (
                                <Alert variant="info">
                                    <InfoIcon aria-hidden="true" />
                                    <AlertDescription>
                                        本次确认范围已保存，后台正在准备入库。可以离开页面后继续查看。
                                    </AlertDescription>
                                </Alert>
                            ) : null}
                            {step !== "RESULT" && processing ? (
                                <Alert variant="info">
                                    <InfoIcon aria-hidden="true" />
                                    <AlertDescription>
                                        后台正在处理{" "}
                                        <span className="num font-medium">
                                            {processing}
                                        </span>{" "}
                                        个 SKU；已完成{" "}
                                        <span className="num font-medium">
                                            {completed}
                                        </span>{" "}
                                        个，可离开页面后继续查看。
                                    </AlertDescription>
                                </Alert>
                            ) : null}
                            {canWrite &&
                            step !== "RESULT" &&
                            warningRows.length ? (
                                <Alert variant="warning">
                                    <TriangleAlertIcon aria-hidden="true" />
                                    <AlertDescription className="flex flex-wrap items-center justify-between gap-3">
                                        <span>
                                            <span className="num font-medium">
                                                {warningRows.length}
                                            </span>{" "}
                                            项存在价格提示。请查看逐项说明后确认。
                                        </span>
                                        <LoadingButton
                                            id="supplier-catalog-batch-confirm-price-warnings"
                                            type="button"
                                            variant="outline"
                                            size="sm"
                                            loading={
                                                pendingAction ===
                                                "confirm-warnings"
                                            }
                                            disabled={!editable || busy}
                                            onClick={() =>
                                                void runAction(
                                                    "confirm-warnings",
                                                    async () => {
                                                        try {
                                                            const saved =
                                                                await save.mutateAsync(
                                                                    {
                                                                        ...identity,
                                                                        request:
                                                                            {
                                                                                expected_version:
                                                                                    view
                                                                                        .batch
                                                                                        .version,
                                                                                items: warningRows.map(
                                                                                    (
                                                                                        row,
                                                                                    ) => ({
                                                                                        item_id:
                                                                                            row
                                                                                                .source
                                                                                                .id,
                                                                                        warnings_confirmed: true,
                                                                                    }),
                                                                                ),
                                                                            },
                                                                    },
                                                                )
                                                            workspace.acceptSaved(
                                                                saved,
                                                            )
                                                            toast.add({
                                                                type: "success",
                                                                title: "已确认价格提示",
                                                            })
                                                        } catch {
                                                            /* 保持未确认 */
                                                        }
                                                    },
                                                )
                                            }
                                        >
                                            已核对并确认这些价格提示
                                        </LoadingButton>
                                    </AlertDescription>
                                </Alert>
                            ) : null}
                            <BatchReviewTable
                                key={tableMode}
                                view={view}
                                mode={tableMode}
                                needsRecheck={step === "REVIEW" && needsRecheck}
                                selectedIds={selectedIds}
                                onSelectionChange={setSelectedIds}
                                onEdit={(row) => setEditingId(row.source.id)}
                                onReconcile={() => void verifyOriginal()}
                                onExcludeSelected={
                                    canWrite && editable
                                        ? handleExcludeSelected
                                        : undefined
                                }
                                excluding={pendingAction === "exclude"}
                                reconciling={pendingAction === "reconcile"}
                                busy={busy}
                                canWrite={canWrite}
                                canReadCost={canReadCost}
                            />
                        </>
                    )}
                </DetailWorkspaceSection>
            </DetailWorkspaceBody>
            {canWrite && step !== "RESULT" ? (
                <div className="sticky bottom-0 z-10 -mx-(--scaffold-px) flex flex-wrap items-center justify-between gap-3 border-t border-border/60 bg-card px-(--scaffold-px) py-3">
                    <div className="flex flex-wrap items-center gap-3">
                        <DraftSaveIndicator
                            idPrefix="supplier-catalog-batch-draft-status"
                            state={
                                workspace.save.isPending
                                    ? "saving"
                                    : workspace.save.isError
                                      ? "failed"
                                      : workspace.dirty
                                        ? "dirty"
                                        : "saved"
                            }
                            message={
                                step === "REVIEW"
                                    ? `已选 ${selectedReady.length} 个 SKU${view.counts.draft + view.counts.failed + view.counts.unknown > 0 ? ` · 待处理 ${view.counts.draft + view.counts.failed + view.counts.unknown}` : ""}`
                                    : undefined
                            }
                        />
                        {step === "DETAILS" && missingCategoryCount > 0 ? (
                            <Button
                                id="supplier-catalog-batch-show-missing-categories"
                                type="button"
                                variant="ghost"
                                size="sm"
                                disabled={!editable || busy}
                                onClick={() =>
                                    categoryListRef.current?.showMissing()
                                }
                            >
                                <span className="num">
                                    {missingCategoryCount} 类未设置映射
                                </span>
                            </Button>
                        ) : null}
                    </div>
                    <div className="flex flex-wrap justify-end gap-2">
                        {step === "PRICING" ||
                        step === "DETAILS" ||
                        step === "REVIEW" ? (
                            <LoadingButton
                                id="supplier-catalog-batch-save-draft"
                                type="button"
                                variant="outline"
                                loading={pendingAction === "save"}
                                disabled={!editable || busy}
                                onClick={() =>
                                    step === "REVIEW"
                                        ? void updateStep("REVIEW", "save")
                                        : void persistWithAction("save")
                                }
                            >
                                <SaveIcon aria-hidden />
                                保存草稿
                            </LoadingButton>
                        ) : null}
                        {step === "PRICING" ? (
                            <LoadingButton
                                id="supplier-catalog-batch-next-details"
                                type="button"
                                loading={pendingAction === "next-details"}
                                disabled={!editable || busy}
                                onClick={() =>
                                    void persistWithAction(
                                        "next-details",
                                        "DETAILS",
                                    )
                                }
                            >
                                下一步：补齐资料
                                <ArrowRightIcon
                                    data-icon="inline-end"
                                    aria-hidden
                                />
                            </LoadingButton>
                        ) : step === "DETAILS" ? (
                            <>
                                <LoadingButton
                                    id="supplier-catalog-batch-back-pricing"
                                    type="button"
                                    variant="outline"
                                    loading={pendingAction === "back-pricing"}
                                    disabled={!editable || busy}
                                    onClick={() =>
                                        void persistWithAction(
                                            "back-pricing",
                                            "PRICING",
                                        )
                                    }
                                >
                                    上一步
                                </LoadingButton>
                                <LoadingButton
                                    id="supplier-catalog-batch-next-review"
                                    type="button"
                                    loading={pendingAction === "next-review"}
                                    disabled={!editable || busy}
                                    onClick={() =>
                                        void persistWithAction(
                                            "next-review",
                                            "REVIEW",
                                            true,
                                        )
                                    }
                                >
                                    下一步：核对入库
                                </LoadingButton>
                            </>
                        ) : step === "REVIEW" ? (
                            <>
                                <LoadingButton
                                    id={
                                        view.batch.submitted
                                            ? "supplier-catalog-batch-back-recovery"
                                            : "supplier-catalog-batch-back-details"
                                    }
                                    type="button"
                                    variant="outline"
                                    loading={pendingAction === "back-review"}
                                    disabled={!editable || busy}
                                    onClick={() =>
                                        void updateStep(
                                            view.batch.submitted
                                                ? "RECOVERY"
                                                : "DETAILS",
                                            "back-review",
                                        )
                                    }
                                >
                                    {view.batch.submitted
                                        ? "返回待处理"
                                        : "上一步"}
                                </LoadingButton>
                                <LoadingButton
                                    id="supplier-catalog-batch-recheck"
                                    type="button"
                                    variant={
                                        needsRecheck ? "default" : "outline"
                                    }
                                    loading={pendingAction === "recheck"}
                                    disabled={!editable || busy}
                                    onClick={() => void recheck("recheck")}
                                >
                                    重新核对
                                </LoadingButton>
                                <LoadingButton
                                    id="supplier-catalog-batch-submit"
                                    type="button"
                                    variant={
                                        needsRecheck ? "outline" : "default"
                                    }
                                    loading={submit.isPending}
                                    disabled={
                                        !editable ||
                                        busy ||
                                        workspace.dirty ||
                                        !workspace.hasCurrentReview ||
                                        uncertainSubmission ||
                                        view.batch.preview_version == null ||
                                        !selectedReady.length
                                    }
                                    onClick={() => setConfirmSubmitOpen(true)}
                                >
                                    确认入库{" "}
                                    <span className="num">
                                        {selectedReady.length}
                                    </span>{" "}
                                    个 SKU
                                </LoadingButton>
                            </>
                        ) : step === "RECOVERY" ? (
                            <>
                                <LoadingButton
                                    id="supplier-catalog-batch-back-result"
                                    type="button"
                                    variant="outline"
                                    loading={pendingAction === "back-result"}
                                    disabled={busy}
                                    onClick={() =>
                                        void updateStep("RESULT", "back-result")
                                    }
                                >
                                    返回入库结果
                                </LoadingButton>
                                <LoadingButton
                                    id="supplier-catalog-batch-recovery-preview"
                                    type="button"
                                    loading={
                                        pendingAction === "recovery-recheck"
                                    }
                                    disabled={
                                        !editable || busy || !selectedIds.length
                                    }
                                    onClick={() =>
                                        void recheck("recovery-recheck")
                                    }
                                >
                                    重新核对{" "}
                                    <span className="num">
                                        {selectedIds.length}
                                    </span>{" "}
                                    项
                                </LoadingButton>
                            </>
                        ) : null}
                    </div>
                </div>
            ) : null}
            <FormalActionConfirmDialog
                idPrefix="supplier-catalog-batch-confirm-submit"
                open={confirmSubmitOpen}
                onOpenChange={setConfirmSubmitOpen}
                actionLabel="批量入库"
                title="确认提交批量入库"
                description="请核对本次入库范围与匹配方式，确认后将提交后台创建商品、规格及供应关系。"
                confirmLabel={`确认入库 ${selectedReady.length} 个 SKU`}
                fromStatus={{ label: "已核对可入库", tone: "info" }}
                toStatus={{ label: "提交入库处理", tone: "success" }}
                summary={[
                    <span key="total">
                        本次提交入库：
                        <span className="num font-semibold">
                            {selectedReady.length}
                        </span>{" "}
                        个 SKU
                    </span>,
                    <span key="create-product">
                        新建商品与 SKU：
                        <span className="num font-semibold">
                            {selectedSummaryCounts.createProduct}
                        </span>{" "}
                        个
                    </span>,
                    <span key="create-sku">
                        已有商品新增规格：
                        <span className="num font-semibold">
                            {selectedSummaryCounts.createSku}
                        </span>{" "}
                        个
                    </span>,
                    <span key="existing">
                        关联已有 SKU：
                        <span className="num font-semibold">
                            {selectedSummaryCounts.existing}
                        </span>{" "}
                        个
                    </span>,
                    <span key="excluded">
                        本批已排除：
                        <span className="num font-semibold">
                            {view.counts.excluded}
                        </span>{" "}
                        个
                    </span>,
                ]}
                effects={[
                    "新建商品与新增规格将写入公司商品主数据并建立供应商供给关系。",
                    "关联已有 SKU 仅建立或更新供应商供给，保留原销售价与展示资料。",
                ]}
                pending={submit.isPending}
                onConfirm={confirm}
            />
            {canWrite && editing ? (
                <BatchItemDialog
                    key={`${editing.source.id}-${view.batch.version}`}
                    open
                    item={{
                        ...editing.source,
                        suggested_unit: editing.suggested_unit,
                        unit_inferred: editing.unit_inferred,
                    }}
                    draft={rowDraft(editing)}
                    supplierId={view.batch.supplier_id}
                    resolution={editing.resolution?.kind ?? null}
                    categoryOptions={categoryOptions}
                    unitOptions={unitOptions}
                    bulkLinked={editing.sales_prices.bulk_linked}
                    dropshipOnly={!editing.terms?.bulk_supply_price_gross}
                    manuallyAdjusted={Object.values(editing.sales_prices).some(
                        (field) =>
                            typeof field === "object" &&
                            field != null &&
                            "origin" in field &&
                            (field.origin === "MANUAL" ||
                                field.origin === "CLEARED"),
                    )}
                    readOnly={
                        !editable ||
                        [
                            "SUCCEEDED",
                            "SKIPPED",
                            "UNKNOWN",
                            "PROCESSING",
                        ].includes(editing.status)
                    }
                    onSave={(edits) => saveItem(itemPatch(editing, edits))}
                    onRestore={() =>
                        saveItem({
                            item_id: editing.source.id,
                            restore_pricing: true,
                        })
                    }
                    onOpenChange={(open) => !open && setEditingId(null)}
                />
            ) : null}
            <Dialog
                open={leaveOpen}
                onOpenChange={(next) => {
                    if (!next && busy) return
                    setLeaveOpen(next)
                }}
            >
                <DialogContent
                    closeButtonId="supplier-catalog-batch-leave-close"
                    showCloseButton={!busy}
                >
                    <DialogHeader>
                        <DialogTitle>尚有未保存修改</DialogTitle>
                        <DialogDescription>
                            离开后仅恢复服务端已保存内容。请先保存草稿并离开，或确认放弃当前修改。
                        </DialogDescription>
                    </DialogHeader>
                    <DialogFooter>
                        <Button
                            id="supplier-catalog-batch-leave-cancel"
                            type="button"
                            variant="outline"
                            disabled={busy}
                            onClick={() => setLeaveOpen(false)}
                        >
                            继续编辑
                        </Button>
                        <Button
                            id="supplier-catalog-batch-leave-discard"
                            type="button"
                            variant="destructive"
                            disabled={busy}
                            onClick={returnToCatalog}
                        >
                            放弃修改并离开
                        </Button>
                        <LoadingButton
                            id="supplier-catalog-batch-leave-save"
                            type="button"
                            loading={pendingAction === "leave-save"}
                            disabled={busy}
                            onClick={async () => {
                                if (await persistWithAction("leave-save")) {
                                    setLeaveOpen(false)
                                    returnToCatalog()
                                }
                            }}
                        >
                            <SaveIcon aria-hidden />
                            保存草稿并离开
                        </LoadingButton>
                    </DialogFooter>
                </DialogContent>
            </Dialog>
        </DetailWorkspacePage>
    )
}
