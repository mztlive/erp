"use client"
import { useRef, useState } from "react"
import Link from "next/link"
import { useQueryClient } from "@tanstack/react-query"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { NativeCheckbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import { PersonDirectoryFilter } from "@/features/entity-selectors/components/person-directory-filter"
import { useWorkItemDetailQuery } from "@/features/work-items/queries"
import type { WorkItemDto } from "@/features/work-items/types"
import {
    PortalApplicationContent,
    PortalTermsContent,
} from "@/features/supplier-portal/components/application-content"
import { PortalError } from "@/features/supplier-portal/components/surface"
import {
    commandKey,
    kindLabels,
    statusLabels,
    timeLabel,
} from "@/features/supplier-portal/lib/presentation"
import type {
    NewProductInput,
    PortalApplication,
    PortalTerms,
    PortalDictionary,
} from "@/features/supplier-portal/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { commandFailureDisposition } from "@/lib/api/command-recovery"
import { reviewPortalApplication } from "../api"
import {
    portalAdminKeys,
    usePortalAdminAccess,
    usePortalAdminApplication,
    usePortalAdminCommand,
    usePortalDuplicates,
    useReviewDictionary,
} from "../hooks"
import { PortalReviewImages } from "../components/review-images"
import { PortalReviewDiff } from "../components/review-diff"
import { PortalOfferingImpacts } from "../components/offering-impacts"
import { PortalDictionaryCreateDialog } from "../components/dictionary-create-dialog"
import { PortalUnitMappingBulk } from "../components/unit-mapping-bulk"
import { PortalCategoryMappingEvidence } from "../components/category-suggestion"
import { PortalAdminFrame } from "../components/admin-frame"
import { PortalCommandConflict } from "../components/command-conflict"
import { PortalCategoryHierarchyConfirmation } from "../components/category-hierarchy-confirmation"
import { PortalReassignForm } from "../components/reassign-form"
import { PortalReviewMappingBatch } from "../components/review-mapping-batch"

export function PortalAdminReviewPage({
    applicationId,
    workItemId,
    embedded = false,
    onTaskCompleted,
}: {
    applicationId: string
    workItemId?: string
    embedded?: boolean
    onTaskCompleted?: (id: string) => void
}) {
    const access = usePortalAdminAccess()
    const query = usePortalAdminApplication(
        applicationId,
        access.can("supplier_portal_request:detail"),
    )
    const id =
        access.can("supplier_portal_request:detail") &&
        access.can("work_item:detail")
            ? (workItemId ?? query.data?.work_item_id ?? "")
            : ""
    const task = useWorkItemDetailQuery(id)
    const application = query.data
    const surface = (
        <div className="space-y-5">
            <PortalError
                error={query.error}
                retry={() => void query.refetch()}
                id="supplier-portal-review-reload"
            />
            {!access.can("supplier_portal_request:detail") &&
                !access.profile.isPending && (
                    <p className="text-sm">
                        当前账号缺少供应商申请资料查看资格，请联系管理员配置或转交处理。
                    </p>
                )}
            {application ? (
                <>
                    <div>
                        <h2 className="text-xl font-semibold">
                            {application.title ??
                                (application.input.name as
                                    | string
                                    | undefined) ??
                                kindLabels[application.kind]}
                        </h2>
                        <p className="mt-1 text-sm text-muted-foreground">
                            {statusLabels[application.status]} ·{" "}
                            {application.supplier_name ?? "供应商申请"}
                        </p>
                    </div>
                    {application.current?.terms && (
                        <PortalTermsContent
                            terms={application.current.terms as PortalTerms}
                            title="当前生效条款"
                        />
                    )}
                    <PortalReviewDiff application={application} />
                    <PortalApplicationContent
                        application={application}
                        showImages={false}
                    />
                    {(application.kind === "terms" ||
                        application.kind === "stop") &&
                        typeof application.input.offering_id === "string" && (
                            <PortalOfferingImpacts
                                offeringId={application.input.offering_id}
                                enabled={access.can(
                                    "supplier_portal_request:detail",
                                )}
                            />
                        )}
                    {application.kind === "new_product" && (
                        <PortalReviewImages application={application} />
                    )}
                    {application.status === "pending" &&
                        !access.can("work_item:detail") && (
                            <p className="text-sm">
                                当前账号缺少审核任务读取资格，请联系管理员配置或转交处理。
                            </p>
                        )}
                    {application.status === "pending" && (
                        <>
                            <PortalError
                                error={task.error}
                                retry={() => void task.refetch()}
                                id="supplier-portal-review-task-reload"
                            />
                            {task.data && (
                                <PortalReviewForm
                                    key={application.id}
                                    application={application}
                                    task={task.data}
                                    onReload={async () => {
                                        const results = await Promise.all([
                                            query.refetch(),
                                            task.refetch(),
                                        ])
                                        const failed = results.find(
                                            (result) => result.error,
                                        )
                                        if (failed?.error) throw failed.error
                                    }}
                                    onSuccess={() => {
                                        void query.refetch()
                                        void task.refetch()
                                        onTaskCompleted?.(task.data?.id ?? "")
                                    }}
                                />
                            )}
                        </>
                    )}
                    {access.can("product:list") && (
                        <Link
                            id="supplier-portal-review-unlisted-link"
                            href="/master-data/products?productListingStatus=unlisted"
                            className="inline-block text-sm text-primary"
                        >
                            进入商品库集中定价及上架
                        </Link>
                    )}
                </>
            ) : query.isPending ? (
                <p className="text-sm">正在读取申请资料…</p>
            ) : null}
        </div>
    )
    return embedded ? (
        <section className="p-5">{surface}</section>
    ) : (
        <PortalAdminFrame title="供应商申请审核">{surface}</PortalAdminFrame>
    )
}

function PortalReviewForm({
    application,
    task,
    onSuccess,
    onReload,
}: {
    application: PortalApplication
    task: WorkItemDto
    onSuccess: () => void
    onReload: () => Promise<void>
}) {
    const access = usePortalAdminAccess()
    const product =
        application.kind === "new_product"
            ? (application.submitted_snapshot ??
              (application.input as unknown as NewProductInput))
            : null
    const rawProduct = product as NewProductInput | null
    const canProcess =
        access.can("supplier_portal_request:review") &&
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
    const client = useQueryClient()
    const [createDictionary, setCreateDictionary] = useState<{
        kind: "brand" | "category" | "unit"
        rawName: string
        rowId?: string
    } | null>(null)
    const [reassign, setReassign] = useState(false)
    const [reportedConfirmed, setReportedConfirmed] = useState(false)
    const existingCandidates =
        duplicates.data?.existing_offerings ??
        application.existing_offerings ??
        []
    const initialAvailability =
        application.kind === "new_product" || application.kind === "quote"
    const decisionRef = useRef<"approve" | "return">("approve")
    const intent = useRef<Record<string, unknown> | null>(null)
    const mutation = usePortalAdminCommand((body: Record<string, unknown>) =>
        reviewPortalApplication(application.id, body),
    )
    const commandFailed = (cause: unknown) => {
        const disposition = commandFailureDisposition(cause)
        if (disposition === "rejected") intent.current = null
        setConflict(disposition === "conflict")
        setError(cause)
    }
    const form = useAppForm({
        defaultValues: {
            comment: "",
            maintainerUserId: "",
            productTarget: "unconfirmed",
            brandId: rawProduct?.brand.selected_id ?? "",
            categoryId: rawProduct?.category.selected_id ?? "",
            brandVersion: rawProduct?.brand.expected_version ?? 0,
            categoryVersion: rawProduct?.category.expected_version ?? 0,
            categoryHierarchy: [] as NonNullable<PortalDictionary["hierarchy"]>,
            existingConfirmed: [] as string[],
            mappings:
                rawProduct?.skus.map((sku) => ({
                    rowId: sku.row_id,
                    unitId: sku.unit.selected_id ?? "",
                    unitVersion: sku.unit.expected_version ?? 0,
                    target: "unconfirmed",
                    unitSynonymConfirmed: false,
                    unitSynonymReason: "",
                })) ?? [],
        },
        validators: {
            onSubmit: z.object({
                comment: z.string().max(500, "处理说明最多500字"),
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
        onSubmit: async ({ value }) => {
            const decision = decisionRef.current
            setError(null)
            if (intent.current) {
                try {
                    await mutation.mutateAsync(intent.current)
                    intent.current = null
                    onSuccess()
                } catch (cause) {
                    commandFailed(cause)
                }
                return
            }
            if (!canProcess) {
                setError(
                    new Error("当前账号或任务不具备处理资格，请重新核对任务"),
                )
                return
            }
            if (decision === "return" && !value.comment.trim()) {
                setError(new Error("退回必须填写供应商可见的原因"))
                return
            }
            if (decision === "approve" && rawProduct && !value.comment.trim()) {
                setError(
                    new Error(
                        "请填写匹配与建档核对说明，保留本次复用或新建理由",
                    ),
                )
                return
            }
            if (
                decision === "approve" &&
                initialAvailability &&
                !reportedConfirmed
            ) {
                setError(new Error("请明确核对供应商实际报送时间后再通过"))
                return
            }
            if (decision === "approve" && !value.maintainerUserId) {
                setError(new Error("请明确选择建档或供给的内部维护人"))
                return
            }
            let normalizedProduct: Record<string, unknown> | null = null
            let existingProduct: Record<string, unknown> | null = null
            if (decision === "approve" && rawProduct) {
                if (
                    value.productTarget === "unconfirmed" ||
                    value.mappings.some((row) => row.target === "unconfirmed")
                ) {
                    setError(new Error("请逐项明确选择新建或复用商品及规格"))
                    return
                }
                const brand = brands.data?.find(
                    (item) => item.id === value.brandId,
                )
                const category = categories.data?.find(
                    (item) => item.id === value.categoryId,
                )
                if (!brand || !category) {
                    setError(new Error("品牌和分类尚未完成有效匹配，不能通过"))
                    return
                }
                if (
                    !value.categoryHierarchy.length ||
                    JSON.stringify(value.categoryHierarchy) !==
                        JSON.stringify(category.hierarchy)
                ) {
                    setError(
                        new Error("请读取并明确确认当前完整分类路径后再通过"),
                    )
                    return
                }
                const selectedProduct = duplicates.data?.duplicates.find(
                    (item) => item.product_id === value.productTarget,
                )
                if (value.productTarget !== "new" && !selectedProduct) {
                    setError(new Error("所选商品候选已不可用，请重新核对"))
                    return
                }
                existingProduct = selectedProduct
                    ? {
                          product_id: selectedProduct.product_id,
                          version: selectedProduct.version,
                          revision_id: selectedProduct.revision_id,
                      }
                    : null
                const mappings = value.mappings.map((row) => {
                    const sku = rawProduct.skus.find(
                        (item) => item.row_id === row.rowId,
                    )
                    const unit = units.data?.find(
                        (item) => item.id === row.unitId,
                    )
                    if (!sku || !unit)
                        throw new Error("每个规格必须完成有效基础单位匹配")
                    if (
                        row.unitSynonymConfirmed &&
                        !row.unitSynonymReason.trim()
                    )
                        throw new Error(
                            "确认单位同义映射时必须填写逐行核对依据",
                        )
                    if (
                        sku.unit.raw_name.trim() !== unit.name.trim() &&
                        !row.unitSynonymConfirmed
                    )
                        throw new Error(
                            "单位原文与正式单位不同，须明确确认同义含义或退回供应商",
                        )
                    const matchedSku = selectedProduct?.skus.find(
                        (item) => item.sku_id === row.target,
                    )
                    if (row.target !== "new" && !matchedSku)
                        throw new Error(
                            "所选规格必须属于明确匹配的商品，请重新核对",
                        )
                    return {
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
                    }
                })
                for (const candidate of existingCandidates) {
                    if (
                        !value.existingConfirmed.includes(candidate.offering_id)
                    )
                        throw new Error(
                            "请核对相同订货编码的既有供给，并明确确认本次修订",
                        )
                    const mapping = value.mappings.find(
                        (row) => row.rowId === candidate.row_id,
                    )
                    if (mapping?.target !== candidate.sku_id)
                        throw new Error(
                            "相同订货编码已关联已有规格，请复用该精确规格；含义变化时退回供应商核对",
                        )
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
            const body = {
                availability_reported_at_confirmed:
                    initialAvailability && reportedConfirmed,
                expected_version: application.version,
                work_item_id: task.id,
                work_item_version: Number(task.task_version),
                decision,
                comment: value.comment.trim() || null,
                maintainer_user_id: value.maintainerUserId || null,
                normalized_product: normalizedProduct,
                existing_product: existingProduct,
                existing_offerings:
                    existingCandidates
                        .filter((item) =>
                            value.existingConfirmed.includes(item.offering_id),
                        )
                        .map((item) => ({
                            row_id: item.row_id,
                            offering_id: item.offering_id,
                            expected_offering_version:
                                item.expected_offering_version,
                            expected_revision_no: item.expected_revision_no,
                        })) ?? [],
            }
            intent.current ??= {
                ...body,
                idempotency_key: commandKey("review"),
            }
            try {
                await mutation.mutateAsync(intent.current)
                intent.current = null
                onSuccess()
            } catch (cause) {
                commandFailed(cause)
            }
        },
    })
    const disabled = mutation.isPending || !canProcess || !!intent.current
    const dictionaryOptions = (
        rows: { id: string; name: string; path?: string }[] | undefined,
    ) =>
        (rows ?? []).map((item) => ({
            value: item.id,
            label: item.path ?? item.name,
        }))
    return (
        <section className="space-y-4 rounded-xl border p-5">
            <div className="flex flex-wrap items-start justify-between gap-3">
                <div>
                    <h2 className="font-semibold">采购确认</h2>
                    <p className="text-sm text-muted-foreground">
                        当前处理人：{task.owner_user?.display_name ?? "待核对"}
                        。申请原稿保持不变，单位、规格含义或报价变化请退回供应商确认。
                    </p>
                </div>
                {task.allowed_actions?.includes("REASSIGN") && (
                    <Button
                        id="supplier-portal-review-reassign-toggle"
                        variant="outline"
                        onClick={() => setReassign(!reassign)}
                        disabled={mutation.isPending}
                    >
                        转交具体处理人
                    </Button>
                )}
            </div>
            {!canProcess && (
                <p className="text-sm text-muted-foreground">
                    当前任务仅可查看，请由任务处理人确认。
                </p>
            )}
            <PortalError error={error} />
            {conflict && (
                <PortalCommandConflict
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
                        intent.current = null
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
            {reassign && (
                <PortalReassignForm
                    task={task}
                    onReload={onReload}
                    onSuccess={() => {
                        setReassign(false)
                        void onReload().catch(setError)
                    }}
                />
            )}
            {createDictionary && rawProduct && (
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
                className="space-y-5"
                onSubmit={(event) => {
                    event.preventDefault()
                    void form.handleSubmit().catch(setError)
                }}
            >
                {rawProduct && (
                    <section className="space-y-4">
                        <h3 className="font-semibold">字典及重复匹配</h3>
                        <div className="grid gap-4 md:grid-cols-2">
                            <form.AppField name="brandId">
                                {(field) => (
                                    <field.SelectField
                                        onValueChange={(id) =>
                                            form.setFieldValue(
                                                "brandVersion",
                                                brands.data?.find(
                                                    (row) => row.id === id,
                                                )?.version ?? 0,
                                            )
                                        }
                                        id="supplier-portal-review-brand"
                                        label={`品牌原稿：${rawProduct.brand.raw_name}`}
                                        options={dictionaryOptions(brands.data)}
                                        disabled={
                                            disabled ||
                                            !!rawProduct.brand.selected_id
                                        }
                                        allowClear={
                                            !rawProduct.brand.selected_id
                                        }
                                    />
                                )}
                            </form.AppField>
                            <form.AppField name="categoryId">
                                {(field) => (
                                    <field.SelectField
                                        onValueChange={(id) => {
                                            form.setFieldValue(
                                                "categoryVersion",
                                                categories.data?.find(
                                                    (row) => row.id === id,
                                                )?.version ?? 0,
                                            )
                                            form.setFieldValue(
                                                "categoryHierarchy",
                                                [],
                                            )
                                        }}
                                        id="supplier-portal-review-category"
                                        label={`分类原稿：${rawProduct.category.raw_name}`}
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
                                            !!rawProduct.category.selected_id
                                        }
                                        allowClear={
                                            !rawProduct.category.selected_id
                                        }
                                    />
                                )}
                            </form.AppField>
                        </div>
                        <form.Subscribe
                            selector={(state) => ({
                                id: state.values.categoryId,
                                version: state.values.categoryVersion,
                                hierarchy: state.values.categoryHierarchy,
                            })}
                        >
                            {(selection) => (
                                <PortalCategoryHierarchyConfirmation
                                    category={categories.data?.find(
                                        (item) => item.id === selection.id,
                                    )}
                                    expectedVersion={selection.version}
                                    value={selection.hierarchy}
                                    disabled={disabled}
                                    onConfirm={(hierarchy) =>
                                        form.setFieldValue(
                                            "categoryHierarchy",
                                            hierarchy,
                                        )
                                    }
                                />
                            )}
                        </form.Subscribe>
                        <p className="text-sm text-muted-foreground">
                            未匹配字典需由具备相应维护权限的内部人员补建；完成后重新核对候选。可转交当前申请，供应商原稿保留。
                        </p>
                        <div className="flex flex-wrap gap-3">
                            <Button
                                id="supplier-portal-review-dictionaries-reload"
                                type="button"
                                variant="outline"
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
                                    if (failed?.error) setError(failed.error)
                                }}
                            >
                                重新读取字典并核对映射
                            </Button>
                            {access.can("product_brand:create") &&
                                !rawProduct.brand.selected_id && (
                                    <Button
                                        id="supplier-portal-review-create-brand"
                                        type="button"
                                        variant="outline"
                                        disabled={disabled}
                                        onClick={() =>
                                            setCreateDictionary({
                                                kind: "brand",
                                                rawName:
                                                    rawProduct.brand.raw_name,
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
                                        variant="outline"
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
                                duplicates.data?.category_mapping_suggestion ??
                                null
                            }
                        />
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
                                        categoryVersion: values.categoryVersion,
                                        categoryHierarchy:
                                            values.categoryHierarchy,
                                        mappings: values.mappings,
                                    }}
                                    actorId={access.profile.data?.userid ?? ""}
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
                                                        ? { ...row, ...update }
                                                        : row
                                                },
                                            ),
                                        )
                                    }}
                                />
                            )}
                        </form.Subscribe>
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
                                variant="outline"
                                disabled={disabled}
                                onClick={() => setQ(search.trim())}
                            >
                                查找重复候选
                            </Button>
                        </div>
                        <form.AppField name="productTarget">
                            {(field) => (
                                <field.SelectField
                                    id="supplier-portal-review-product-target"
                                    label="本次商品建档决定"
                                    allowClear={false}
                                    options={[
                                        {
                                            value: "unconfirmed",
                                            label: "请明确核对结果",
                                            disabled: true,
                                        },
                                        { value: "new", label: "新建公司商品" },
                                        ...(
                                            duplicates.data?.duplicates ?? []
                                        ).map((item) => ({
                                            value: item.product_id,
                                            label: `复用商品：${item.name}`,
                                        })),
                                    ]}
                                    disabled={disabled}
                                />
                            )}
                        </form.AppField>
                        <form.Subscribe
                            selector={(state) => state.values.productTarget}
                        >
                            {(selected) => (
                                <>
                                    {rawProduct.skus.map((sku, index) => {
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
                                                className="grid gap-4 rounded-lg border p-4 md:grid-cols-2"
                                            >
                                                <div className="md:col-span-2">
                                                    <p className="font-medium">
                                                        {sku.name} ·{" "}
                                                        {sku.ordering_code}
                                                    </p>
                                                    <p className="text-xs text-muted-foreground">
                                                        原始单位：
                                                        {sku.unit.raw_name} ·
                                                        原始规格：
                                                        {sku.spec_entries
                                                            .map(
                                                                (entry) =>
                                                                    `${entry.attribute_code}：${entry.attribute_value_code}`,
                                                            )
                                                            .join(" / ")}
                                                    </p>
                                                </div>
                                                <form.AppField
                                                    name={`mappings[${index}].unitId`}
                                                >
                                                    {(field) => (
                                                        <field.SelectField
                                                            onValueChange={(
                                                                id,
                                                            ) => {
                                                                form.setFieldValue(
                                                                    `mappings[${index}].unitVersion`,
                                                                    units.data?.find(
                                                                        (row) =>
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
                                                            options={dictionaryOptions(
                                                                units.data,
                                                            )}
                                                            disabled={
                                                                disabled ||
                                                                !!sku.unit
                                                                    .selected_id
                                                            }
                                                            allowClear={
                                                                !sku.unit
                                                                    .selected_id
                                                            }
                                                        />
                                                    )}
                                                </form.AppField>
                                                {access.can(
                                                    "unit_of_measure:create",
                                                ) &&
                                                    !sku.unit.selected_id && (
                                                        <Button
                                                            id={`${prefix}-create-unit`}
                                                            type="button"
                                                            variant="outline"
                                                            disabled={disabled}
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
                                                    )}
                                                <form.AppField
                                                    name={`mappings[${index}].unitSynonymConfirmed`}
                                                >
                                                    {(field) => (
                                                        <label
                                                            htmlFor={`${prefix}-unit-synonym-confirmed`}
                                                            className="flex items-center gap-2 text-sm md:col-span-2"
                                                        >
                                                            <NativeCheckbox
                                                                id={`${prefix}-unit-synonym-confirmed`}
                                                                checked={
                                                                    field.state
                                                                        .value
                                                                }
                                                                disabled={
                                                                    disabled
                                                                }
                                                                onCheckedChange={(
                                                                    value,
                                                                ) =>
                                                                    field.handleChange(
                                                                        value,
                                                                    )
                                                                }
                                                            />
                                                            已核对原始单位与所选单位含义相同，仅名称不同，不涉及包装或报价换算
                                                        </label>
                                                    )}
                                                </form.AppField>
                                                <form.AppField
                                                    name={`mappings[${index}].unitSynonymReason`}
                                                >
                                                    {(field) => (
                                                        <field.TextField
                                                            id={`${prefix}-unit-synonym-reason`}
                                                            label="单位同义核对依据（勾选同义确认时必填）"
                                                            disabled={disabled}
                                                        />
                                                    )}
                                                </form.AppField>
                                                <form.AppField
                                                    name={`mappings[${index}].target`}
                                                >
                                                    {(field) => (
                                                        <field.SelectField
                                                            id={`${prefix}-target`}
                                                            label="本次规格建档决定"
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
                                                                    (item) => ({
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
                                                            allowClear={false}
                                                        />
                                                    )}
                                                </form.AppField>
                                            </div>
                                        )
                                    })}
                                </>
                            )}
                        </form.Subscribe>
                        {!!existingCandidates.length && (
                            <form.AppField name="existingConfirmed">
                                {(field) => (
                                    <section className="space-y-3 rounded-lg border p-4">
                                        <h3 className="font-medium">
                                            相同订货编码下的既有供给核对
                                        </h3>
                                        {existingCandidates.map((candidate) => (
                                            <label
                                                key={candidate.offering_id}
                                                htmlFor={`supplier-portal-review-existing-${toAutomationIdSegment(candidate.offering_id)}`}
                                                className="flex items-start gap-2 text-sm"
                                            >
                                                <NativeCheckbox
                                                    id={`supplier-portal-review-existing-${toAutomationIdSegment(candidate.offering_id)}`}
                                                    checked={field.state.value.includes(
                                                        candidate.offering_id,
                                                    )}
                                                    disabled={disabled}
                                                    onCheckedChange={(
                                                        checked,
                                                    ) =>
                                                        field.handleChange(
                                                            checked
                                                                ? [
                                                                      ...field
                                                                          .state
                                                                          .value,
                                                                      candidate.offering_id,
                                                                  ]
                                                                : field.state.value.filter(
                                                                      (id) =>
                                                                          id !==
                                                                          candidate.offering_id,
                                                                  ),
                                                        )
                                                    }
                                                />
                                                我已核对订货编码{" "}
                                                {candidate.supplier_sku_code ??
                                                    "本次订货编码"}{" "}
                                                及第{" "}
                                                {candidate.expected_revision_no}{" "}
                                                版，确认修订该既有供给。
                                            </label>
                                        ))}
                                    </section>
                                )}
                            </form.AppField>
                        )}
                        <p className="text-sm text-muted-foreground">
                            仅本次新建SKU保持未上架。复用在售SKU并增加有效供给后，可能立即参与或恢复销售与采购选源。
                        </p>
                    </section>
                )}
                {initialAvailability && (
                    <section className="space-y-3 rounded-lg border p-4">
                        <h3 className="font-medium">初始可供报送时间核对</h3>
                        <p className="text-sm text-muted-foreground">
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
                            const elapsed = Math.floor(
                                (Date.now() -
                                    new Date(
                                        typeof row.at === "number"
                                            ? row.at * 1000
                                            : row.at,
                                    ).getTime()) /
                                    60000,
                            )
                            return (
                                <p key={index} className="text-sm">
                                    {row.name}：{timeLabel(row.at)} ·{" "}
                                    {Number.isFinite(elapsed) && elapsed >= 0
                                        ? elapsed < 60
                                            ? `距今 ${elapsed} 分钟`
                                            : `距今 ${Math.floor(elapsed / 60)} 小时`
                                        : "请核对报送时间"}
                                </p>
                            )
                        })}
                        <label
                            htmlFor="supplier-portal-review-reported-confirmed"
                            className="flex items-start gap-2 text-sm"
                        >
                            <NativeCheckbox
                                id="supplier-portal-review-reported-confirmed"
                                checked={reportedConfirmed}
                                disabled={disabled}
                                onCheckedChange={setReportedConfirmed}
                            />
                            我已逐项核对供应商实际报送时间，并确认本次可供资料可用于采购确认。
                        </label>
                    </section>
                )}
                {canProcess && (
                    <form.AppField name="maintainerUserId">
                        {(field) => (
                            <div className="space-y-2">
                                <p className="text-xs text-muted-foreground">
                                    通过前请选择合格的内部维护人；所选人员的当前资格由服务端重新核对。
                                </p>
                                {access.can("procurement_person:list") ? (
                                    <PersonDirectoryFilter
                                        id="supplier-portal-review-maintainer"
                                        category="procurement"
                                        value={field.state.value}
                                        onChange={(value) => {
                                            if (!disabled)
                                                field.handleChange(value)
                                        }}
                                        label="建档及供给内部维护人"
                                        selectionMode="single"
                                    />
                                ) : (
                                    <>
                                        <p className="text-sm">
                                            当前账号无采购人员目录资格，可明确选择由本人维护或转交处理。
                                        </p>
                                        <Button
                                            id="supplier-portal-review-maintainer-self"
                                            type="button"
                                            variant="outline"
                                            disabled={
                                                disabled ||
                                                !access.profile.data?.userid
                                            }
                                            onClick={() =>
                                                field.handleChange(
                                                    access.profile.data
                                                        ?.userid ?? "",
                                                )
                                            }
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
                )}
                <form.AppField name="comment">
                    {(field) => (
                        <field.TextareaField
                            id="supplier-portal-review-comment"
                            label={
                                rawProduct
                                    ? "匹配与建档核对说明／供应商可见处理意见（必填）"
                                    : "供应商可见处理意见（退回必填）"
                            }
                            disabled={disabled}
                            rows={3}
                        />
                    )}
                </form.AppField>
                <div className="flex flex-wrap gap-2">
                    <Button
                        id="supplier-portal-review-approve"
                        type="button"
                        disabled={
                            mutation.isPending ||
                            conflict ||
                            !canProcess ||
                            (initialAvailability && !reportedConfirmed) ||
                            (intent.current != null &&
                                intent.current.decision !== "approve")
                        }
                        onClick={() => {
                            decisionRef.current = "approve"
                            void form.handleSubmit().catch(setError)
                        }}
                    >
                        {intent.current?.decision === "approve"
                            ? "重试原通过决定"
                            : "确认通过并生效"}
                    </Button>
                    <Button
                        id="supplier-portal-review-return"
                        type="button"
                        variant="outline"
                        disabled={
                            mutation.isPending ||
                            conflict ||
                            !canProcess ||
                            (intent.current != null &&
                                intent.current.decision !== "return")
                        }
                        onClick={() => {
                            decisionRef.current = "return"
                            void form.handleSubmit().catch(setError)
                        }}
                    >
                        {intent.current?.decision === "return"
                            ? "重试原退回决定"
                            : "退回供应商修改"}
                    </Button>
                </div>
            </form>
        </section>
    )
}
