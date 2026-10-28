"use client"

import * as React from "react"
import { useParams, useRouter, useSearchParams } from "@/lib/navigation"
import {
    BusinessFailureState,
    DetailPageHeader,
    DraftSaveIndicator,
    MetricItem,
    MetricStrip,
    PageScaffold,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { useMasterDataListQuery } from "@/features/master-data/hooks/queries"
import { useUnitOptionsQuery } from "@/hooks/use-options"
import { isApiError } from "@/lib/api/errors"
import { hasPermission } from "@/lib/permissions"
import { divideFixed, multiplyFixed } from "@/lib/fixed-decimal"
import {
    useAdoptionBatchQuery,
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
import { BatchDetailsPanel } from "../components/batch/batch-details"
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
export function BatchAdoptionPage() {
    const params = useParams<{ batchId: string }>()
    const search = useSearchParams()
    const identity = {
        connectionId: search.get("connectionId") ?? "",
        batchId: params.batchId,
    }
    const query = useAdoptionBatchQuery(identity)
    const account = useAccountProfileQuery()
    if (!identity.connectionId)
        return (
            <PageScaffold>
                <BusinessFailureState
                    title="缺少供应商连接"
                    description="请从供应商品库重新打开入库批次。"
                />
            </PageScaffold>
        )
    if (query.isPending || account.isPending)
        return (
            <PageScaffold>
                <p role="status" className="text-sm text-muted-foreground">
                    正在恢复已保存的入库批次…
                </p>
            </PageScaffold>
        )
    if (query.isError)
        return (
            <PageScaffold>
                <BusinessFailureState
                    idPrefix="supplier-catalog-batch-load"
                    title="入库批次读取失败"
                    description={query.error.message}
                    onRetry={() => void query.refetch()}
                />
            </PageScaffold>
        )
    return (
        <BatchWorkspace
            key={query.data.batch.id}
            view={query.data}
            actorId={account.data?.userid ?? ""}
            onRefresh={() => query.refetch()}
        />
    )
}
function BatchWorkspace({
    view,
    actorId,
    onRefresh,
}: {
    view: BatchView
    actorId: string
    onRefresh: () => Promise<unknown>
}) {
    const router = useRouter()
    const workspace = useBatchWorkspace(view)
    const { form } = workspace
    const identity = {
        connectionId: view.batch.connection_id,
        batchId: view.batch.id,
    }
    const center = useConnectionCenterQuery(identity.connectionId)
    const profile = useAccountProfileQuery()
    const canWrite =
        hasPermission(
            profile.data?.permissions,
            "supplier_api_connection:catalog_sync",
        ) &&
        hasPermission(
            profile.data?.permissions,
            "supplier_offering_cost:detail",
        )
    const categories = useMasterDataListQuery({
        resource: "categories",
        lifecycleStatus: "enabled",
        revisionTiming: "current",
    })
    const units = useUnitOptionsQuery()
    const categoryOptions = (categories.data?.rows ?? [])
        .filter((row) => row.productKind === "实物")
        .map((row) => ({ value: row.stableId, label: row.name }))
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
    const processing = view.rows.filter(
        (row) =>
            row.status === "PROCESSING" ||
            (row.status === "READY" && row.operation_id),
    ).length
    const draftActive =
        Boolean(view.batch.draft_operation) &&
        view.batch.draft_started_at != null &&
        Math.floor(Date.now() / 1000) - view.batch.draft_started_at < 300
    const draftInterrupted = Boolean(view.batch.draft_operation) && !draftActive
    const preparing = view.batch.confirmations.some(
        (entry) =>
            !view.batch.published_operations.includes(entry.operation_id),
    )
    const editable = canWrite && processing === 0 && !preparing && !draftActive
    const editing = view.rows.find((row) => row.source.id === editingId)
    const unsubmittedReady = view.rows.filter(
        (row) => row.status === "READY" && !row.operation_id,
    ).length
    const pendingCount =
        view.counts.draft +
        view.counts.failed +
        view.counts.unknown +
        unsubmittedReady
    const readyIds = view.rows
        .filter(
            (row) =>
                isBatchRowSelectable(row, "REVIEW") &&
                row.preview_version === view.batch.preview_version,
        )
        .map((row) => row.source.id)
    const selectedReady = selectedIds.filter((id) => readyIds.includes(id))
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
    const updateStep = async (next: BatchStep) => {
        try {
            const saved = await save.mutateAsync({
                ...identity,
                request: { expected_version: view.batch.version, step: next },
            })
            workspace.acceptSaved(saved)
        } catch {
            /* 页面显示错误，保留当前内容。 */
        }
    }
    const saveItem = async (patch: BatchItemEdit) => {
        const saved = await save.mutateAsync({
            ...identity,
            request: { expected_version: view.batch.version, items: [patch] },
        })
        workspace.acceptSaved(saved)
    }
    const verifyOriginal = async () => {
        try {
            const original = requestIdentity.current
            if (original) {
                const result = await querySubmission.mutateAsync(original)
                submit.reset()
                workspace.acceptSaved(result.view)
                if (result.status === "NOT_ACCEPTED") workspace.requireReview()
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
    const recheck = async () => {
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
    return (
        <PageScaffold density="compact" className="min-w-0 gap-6 pb-24">
            <DetailPageHeader
                title={
                    step === "PRICING"
                        ? "设置售价"
                        : step === "DETAILS"
                          ? "补齐资料"
                          : step === "RECOVERY"
                            ? "待处理商品"
                            : step === "RESULT"
                              ? processing || view.batch.execution_pending
                                  ? "本批入库处理中"
                                  : pendingCount
                                    ? "本批入库部分完成"
                                    : "本批入库已完成"
                              : "批量选品入库"
                }
                titleExtra={
                    <Badge variant="secondary">
                        {center.data?.supplier.name ?? "供应商商品库"}
                    </Badge>
                }
                meta={
                    <span>
                        {view.batch.selected_count - view.counts.excluded} 个
                        SKU · {view.product_count} 个来源商品
                    </span>
                }
            />
            <ol
                className="grid grid-cols-2 gap-3 md:grid-cols-4"
                aria-label="批量入库步骤"
            >
                {STEPS.map((entry, index) => (
                    <li key={entry.step} className="flex items-center gap-3">
                        <span
                            className={`num flex size-8 shrink-0 items-center justify-center rounded-full ${entry.step === step || (step === "RECOVERY" && entry.step === "DETAILS") ? "bg-primary text-primary-foreground" : "bg-muted text-muted-foreground"}`}
                        >
                            {index + 1}
                        </span>
                        <span className="text-sm">{entry.label}</span>
                    </li>
                ))}
            </ol>
            <details className="text-xs text-muted-foreground">
                <summary
                    id="supplier-catalog-batch-information"
                    className="cursor-pointer"
                >
                    批次信息
                </summary>
                <p className="num pt-2">入库批次：{view.batch.batch_no}</p>
            </details>
            {!canWrite ? (
                <p
                    role="status"
                    className="rounded-lg border p-3 text-sm text-muted-foreground"
                >
                    当前账号没有入库或接口成本权限，可查看已获授权的批次内容。
                </p>
            ) : null}
            {view.batch.draft_operation ? (
                <div
                    role="status"
                    className="flex flex-wrap items-center justify-between gap-3 rounded-lg border p-3 text-sm"
                >
                    <p>
                        {draftActive
                            ? "正在保存或核对本批商品，处理进度会自动恢复。"
                            : "上次保存或核对已中断，请重新读取后继续核对。"}
                    </p>
                    <Button
                        id="supplier-catalog-batch-refresh-draft"
                        type="button"
                        variant="outline"
                        onClick={() => void onRefresh()}
                    >
                        重新读取批次
                    </Button>
                    {draftInterrupted ? (
                        <Button
                            id="supplier-catalog-batch-resume-draft"
                            type="button"
                            disabled={busy || !canWrite}
                            onClick={() => void recheck()}
                        >
                            继续核对
                        </Button>
                    ) : null}
                </div>
            ) : null}
            {errors.map((error, index) => (
                <p
                    key={`${index}-${error?.message}`}
                    role="alert"
                    className="text-sm text-destructive"
                >
                    {error?.message}
                </p>
            ))}
            {submissionNotice ? (
                <p role="status" className="rounded-lg border p-3 text-sm">
                    {submissionNotice}
                </p>
            ) : null}
            {step === "REVIEW" &&
            view.batch.preview_version != null &&
            !workspace.hasCurrentReview ? (
                <p role="status" className="rounded-lg border p-3 text-sm">
                    已恢复保存的草稿，请点击“重新核对”，查看最新结果后再确认入库。
                </p>
            ) : null}
            {uncertainSubmission ? (
                <div
                    role="alert"
                    className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-warning/30 bg-warning/10 p-4 text-sm"
                >
                    <p>
                        本次确认结果待核实。请先查询原操作，不能直接重新提交。
                    </p>
                    <Button
                        id="supplier-catalog-batch-query-submission"
                        type="button"
                        variant="outline"
                        disabled={busy}
                        onClick={() => void verifyOriginal()}
                    >
                        查询原操作结果
                    </Button>
                </div>
            ) : null}
            {step === "PRICING" || step === "DETAILS" ? (
                <form.AppForm>
                    <form
                        id="supplier-catalog-batch-draft-form"
                        onSubmit={(event) => {
                            event.preventDefault()
                            event.stopPropagation()
                            void workspace.persist()
                        }}
                    >
                        <fieldset
                            disabled={!editable || busy}
                            className="min-w-0 disabled:opacity-70"
                        >
                            {step === "PRICING" ? (
                                <BatchPricingPanel
                                    workspace={workspace}
                                    view={view}
                                />
                            ) : (
                                <BatchDetailsPanel
                                    workspace={workspace}
                                    supplierId={view.batch.supplier_id}
                                    categories={categoryOptions}
                                />
                            )}
                        </fieldset>
                    </form>
                </form.AppForm>
            ) : (
                <>
                    <MetricStrip columns={5}>
                        <MetricItem
                            label={step === "RESULT" ? "已成功" : "本批 SKU"}
                            value={
                                step === "RESULT"
                                    ? view.counts.succeeded
                                    : view.counts.total - view.counts.excluded
                            }
                        />
                        <MetricItem
                            label={
                                step === "RESULT"
                                    ? "明确失败"
                                    : step === "RECOVERY"
                                      ? "待重新核对"
                                      : "可入库"
                            }
                            value={
                                step === "RESULT"
                                    ? view.counts.failed
                                    : step === "RECOVERY"
                                      ? unsubmittedReady
                                      : readyIds.length
                            }
                        />
                        <MetricItem
                            label="结果待核实"
                            value={view.counts.unknown}
                        />
                        <MetricItem
                            label={step === "RESULT" ? "待提交" : "待补齐"}
                            value={
                                view.counts.draft +
                                (step === "RESULT" ? unsubmittedReady : 0)
                            }
                            detail={
                                step === "RESULT"
                                    ? `可入库 ${unsubmittedReady} · 待补齐 ${view.counts.draft}`
                                    : undefined
                            }
                        />
                        <MetricItem
                            label="已完成跳过"
                            value={
                                step === "RESULT"
                                    ? view.counts.skipped
                                    : completed
                            }
                        />
                    </MetricStrip>
                    {step === "RESULT" ? (
                        <p className="text-sm text-muted-foreground">
                            本批 {view.counts.total} 个 SKU · 已完成 {completed}{" "}
                            个 · 处理中 {processing} 个 · 待处理 {pendingCount}{" "}
                            个 · 已排除 {view.counts.excluded} 个
                        </p>
                    ) : null}
                    {step === "REVIEW" ? (
                        <p className="text-sm text-muted-foreground">
                            就绪项：新建 SKU{" "}
                            {
                                view.rows.filter(
                                    (row) =>
                                        readyIds.includes(row.source.id) &&
                                        row.resolution?.kind ===
                                            "CREATE_PRODUCT",
                                ).length
                            }
                            {" · "}新增规格{" "}
                            {
                                view.rows.filter(
                                    (row) =>
                                        readyIds.includes(row.source.id) &&
                                        row.resolution?.kind === "CREATE_SKU",
                                ).length
                            }
                            {" · "}关联已有 SKU{" "}
                            {
                                view.rows.filter(
                                    (row) =>
                                        readyIds.includes(row.source.id) &&
                                        row.resolution?.kind === "EXISTING",
                                ).length
                            }
                        </p>
                    ) : null}
                    {preparing && !processing ? (
                        <p role="status" className="text-sm">
                            本次确认范围已保存，后台正在准备入库。可以离开页面后继续查看。
                        </p>
                    ) : null}
                    {processing ? (
                        <p role="status" className="text-sm">
                            后台正在处理 {processing} 个 SKU；已完成 {completed}{" "}
                            个，可离开页面后继续查看。
                        </p>
                    ) : (
                        <p className="text-sm text-muted-foreground">
                            新建 SKU 保持未上架；关联已有 SKU
                            保留原价格、资料和上架状态。
                        </p>
                    )}
                    {warningRows.length ? (
                        <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-warning/30 bg-warning/10 p-3">
                            <p className="text-sm">
                                {warningRows.length}{" "}
                                项存在价格提示。请查看逐项说明后确认。
                            </p>
                            <Button
                                id="supplier-catalog-batch-confirm-price-warnings"
                                type="button"
                                variant="outline"
                                disabled={!editable || busy}
                                onClick={async () => {
                                    try {
                                        const saved = await save.mutateAsync({
                                            ...identity,
                                            request: {
                                                expected_version:
                                                    view.batch.version,
                                                items: warningRows.map(
                                                    (row) => ({
                                                        item_id: row.source.id,
                                                        warnings_confirmed: true,
                                                    }),
                                                ),
                                            },
                                        })
                                        workspace.acceptSaved(saved)
                                    } catch {
                                        /* 保持未确认 */
                                    }
                                }}
                            >
                                已核对并确认这些价格提示
                            </Button>
                        </div>
                    ) : null}
                    {view.batch.preview_version == null && step === "REVIEW" ? (
                        <p className="text-sm text-warning">
                            草稿已恢复或内容已调整，请重新核对后再确认入库。
                        </p>
                    ) : null}
                    <BatchReviewTable
                        key={tableMode}
                        view={view}
                        mode={tableMode}
                        selectedIds={selectedIds}
                        onSelectionChange={setSelectedIds}
                        onEdit={(row) => setEditingId(row.source.id)}
                        onReconcile={() => void verifyOriginal()}
                        busy={busy || !canWrite}
                    />
                </>
            )}
            <div className="sticky bottom-0 z-10 -mx-1 flex flex-wrap items-center justify-between gap-3 border-t bg-background px-1 py-4">
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
                            ? `将入库 ${selectedReady.length} 个 SKU，${view.counts.draft + view.counts.failed + view.counts.unknown} 个待处理，${view.counts.skipped + view.counts.succeeded} 个已完成跳过`
                            : undefined
                    }
                />
                <div className="flex flex-wrap justify-end gap-2">
                    {step === "PRICING" ||
                    step === "DETAILS" ||
                    step === "REVIEW" ? (
                        <Button
                            id="supplier-catalog-batch-save-draft"
                            type="button"
                            variant="outline"
                            disabled={!editable || busy}
                            onClick={() =>
                                step === "REVIEW"
                                    ? void updateStep("REVIEW")
                                    : void workspace.persist()
                            }
                        >
                            保存草稿
                        </Button>
                    ) : null}
                    <Button
                        id="supplier-catalog-batch-return-catalog"
                        type="button"
                        variant="outline"
                        onClick={() =>
                            workspace.dirty
                                ? setLeaveOpen(true)
                                : returnToCatalog()
                        }
                    >
                        返回商品库
                    </Button>
                    {step === "PRICING" ? (
                        <Button
                            id="supplier-catalog-batch-next-details"
                            type="button"
                            disabled={!editable || busy}
                            onClick={() => void workspace.persist("DETAILS")}
                        >
                            下一步：补齐资料
                        </Button>
                    ) : step === "DETAILS" ? (
                        <>
                            <Button
                                id="supplier-catalog-batch-back-pricing"
                                type="button"
                                variant="outline"
                                disabled={!editable || busy}
                                onClick={() =>
                                    void workspace.persist("PRICING")
                                }
                            >
                                上一步
                            </Button>
                            <Button
                                id="supplier-catalog-batch-next-review"
                                type="button"
                                disabled={!editable || busy}
                                onClick={() =>
                                    void workspace.persist("REVIEW", true)
                                }
                            >
                                下一步：核对入库
                            </Button>
                        </>
                    ) : step === "REVIEW" ? (
                        <>
                            <Button
                                id={
                                    view.batch.submitted
                                        ? "supplier-catalog-batch-back-recovery"
                                        : "supplier-catalog-batch-back-details"
                                }
                                type="button"
                                variant="outline"
                                disabled={!editable || busy}
                                onClick={() =>
                                    void updateStep(
                                        view.batch.submitted
                                            ? "RECOVERY"
                                            : "DETAILS",
                                    )
                                }
                            >
                                {view.batch.submitted ? "返回待处理" : "上一步"}
                            </Button>
                            <Button
                                id="supplier-catalog-batch-recheck"
                                type="button"
                                variant="outline"
                                disabled={!editable || busy}
                                onClick={() => void recheck()}
                            >
                                重新核对
                            </Button>
                            <Button
                                id="supplier-catalog-batch-submit"
                                type="button"
                                disabled={
                                    !editable ||
                                    busy ||
                                    workspace.dirty ||
                                    !workspace.hasCurrentReview ||
                                    uncertainSubmission ||
                                    view.batch.preview_version == null ||
                                    !selectedReady.length
                                }
                                onClick={() => void confirm()}
                            >
                                确认入库 {selectedReady.length} 个 SKU
                            </Button>
                        </>
                    ) : step === "RECOVERY" ? (
                        <>
                            <Button
                                id="supplier-catalog-batch-back-result"
                                type="button"
                                variant="outline"
                                disabled={busy}
                                onClick={() => void updateStep("RESULT")}
                            >
                                返回入库结果
                            </Button>
                            <Button
                                id="supplier-catalog-batch-recovery-preview"
                                type="button"
                                disabled={
                                    !editable || busy || !selectedIds.length
                                }
                                onClick={() => void recheck()}
                            >
                                重新核对 {selectedIds.length} 项
                            </Button>
                        </>
                    ) : pendingCount && !processing ? (
                        <Button
                            id="supplier-catalog-batch-open-recovery"
                            type="button"
                            disabled={busy}
                            onClick={() => void updateStep("RECOVERY")}
                        >
                            处理待办（{pendingCount}）
                        </Button>
                    ) : (
                        <Button
                            id="supplier-catalog-batch-refresh-results"
                            type="button"
                            variant="outline"
                            disabled={busy}
                            onClick={() => void onRefresh()}
                        >
                            刷新结果
                        </Button>
                    )}
                </div>
            </div>
            {editing ? (
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
            <Dialog open={leaveOpen} onOpenChange={setLeaveOpen}>
                <DialogContent closeButtonId="supplier-catalog-batch-leave-close">
                    <DialogHeader>
                        <DialogTitle>尚有未保存修改</DialogTitle>
                        <DialogDescription>
                            离开后仅恢复服务端已保存内容。请先保存草稿，或确认放弃当前修改。
                        </DialogDescription>
                    </DialogHeader>
                    <DialogFooter>
                        <Button
                            id="supplier-catalog-batch-leave-cancel"
                            type="button"
                            variant="outline"
                            onClick={() => setLeaveOpen(false)}
                        >
                            继续编辑
                        </Button>
                        <Button
                            id="supplier-catalog-batch-leave-discard"
                            type="button"
                            variant="outline"
                            onClick={returnToCatalog}
                        >
                            放弃修改并离开
                        </Button>
                        <Button
                            id="supplier-catalog-batch-leave-save"
                            type="button"
                            disabled={busy}
                            onClick={async () => {
                                if (await workspace.persist())
                                    setLeaveOpen(false)
                            }}
                        >
                            保存草稿
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>
        </PageScaffold>
    )
}
