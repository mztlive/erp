"use client"

import { useEffect, useState } from "react"
import Link from "next/link"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { NativeCheckbox } from "@/components/ui/checkbox"
import { PortalError } from "@/features/supplier-portal/components/surface"
import type {
    NewProductInput,
    PortalApplication,
    PortalDictionary,
} from "@/features/supplier-portal/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    usePortalAdminApplication,
    usePortalAdminApplications,
    useReviewDictionary,
} from "../hooks"
import {
    applicableMapping,
    confirmedMappingIssue,
    discardMappingPlan,
    mappingConfirmationIdentity,
    mappingPlanIssue,
    readMappingPlan,
    reviewInputIdentity,
    reviewProduct,
    saveMappingPlan,
    type ReviewMappingPlan,
    type ReviewMappingSelection,
} from "../lib/review-mapping-plan"

export type PortalReviewMappingBatchProps = {
    application: PortalApplication
    rawProduct: NewProductInput
    brands: PortalDictionary[]
    categories: PortalDictionary[]
    units: PortalDictionary[]
    mapping: ReviewMappingSelection
    actorId: string
    canList: boolean
    canReadDetails: boolean
    disabled: boolean
    onAdopt: (mapping: ReviewMappingSelection) => void
}

/** 批次仅保存逐申请辅助映射；每张申请必须进入自己的审核页重新确认。 */
export function PortalReviewMappingBatch({
    application,
    rawProduct,
    brands,
    categories,
    units,
    mapping,
    actorId,
    canList,
    canReadDetails,
    disabled,
    onAdopt,
}: PortalReviewMappingBatchProps) {
    const [opened, setOpened] = useState(false)
    const [page, setPage] = useState(1)
    const [feedback, setFeedback] = useState("")
    const [saving, setSaving] = useState(false)
    const [saved, setSaved] = useState<{
        plan: ReviewMappingPlan | null
        issue: string | null
    }>({ plan: null, issue: null })
    const [savedTargets, setSavedTargets] = useState<
        { id: string; name: string }[]
    >([])
    const dictionaries = { brands, categories, units }
    const sourceIssue = confirmedMappingIssue(rawProduct, mapping, dictionaries)
    const confirmation = mappingConfirmationIdentity(application, mapping)
    const canPrepare =
        !disabled &&
        !!actorId &&
        !!application.supplier_id &&
        application.kind === "new_product" &&
        application.status === "pending"
    const applications = usePortalAdminApplications(
        {
            supplier_id: application.supplier_id ?? "",
            status: "pending",
            page,
            page_size: 20,
        },
        opened && canPrepare && canList && canReadDetails,
    )
    const currentApplication = usePortalAdminApplication(application.id, false)
    const currentBrands = useReviewDictionary("brand", false)
    const currentCategories = useReviewDictionary("category", false)
    const currentUnits = useReviewDictionary("unit", false)

    useEffect(() => {
        setSaved(readMappingPlan(actorId, application.id))
    }, [actorId, application.id])

    const form = useAppForm({
        defaultValues: { confirmedSource: "", selectedIds: [] as string[] },
        onSubmit: async ({ value }) => {
            if (
                !canPrepare ||
                !canList ||
                !canReadDetails ||
                sourceIssue ||
                value.confirmedSource !== confirmation
            ) {
                setFeedback(
                    sourceIssue ?? "请先明确核对本次映射并确认处理资格。",
                )
                return
            }
            const selected =
                applications.data?.items.filter((item) =>
                    value.selectedIds.includes(item.id),
                ) ?? []
            if (
                !selected.length ||
                selected.length !== value.selectedIds.length ||
                applications.isFetching ||
                applications.error
            ) {
                setFeedback("请重新读取本页申请并明确勾选适用记录。")
                return
            }
            setSaving(true)
            setFeedback("")
            const results: { id: string; name: string }[] = []
            try {
                for (const target of selected) {
                    const selection = applicableMapping(
                        application,
                        rawProduct,
                        mapping,
                        target,
                    )
                    const input = reviewProduct(target)
                    if (!selection.mapping || !input)
                        throw new Error(
                            selection.issue ?? "所选申请已不适用，请重新核对。",
                        )
                    const targetIssue = confirmedMappingIssue(
                        input,
                        selection.mapping,
                        dictionaries,
                    )
                    if (targetIssue) throw new Error(targetIssue)
                    saveMappingPlan({
                        schemaVersion: 1,
                        actorId,
                        supplierId: application.supplier_id ?? "",
                        sourceRequestId: application.id,
                        sourceVersion: application.version,
                        sourceName: rawProduct.name,
                        targetRequestId: target.id,
                        targetVersion: target.version,
                        targetInputIdentity: await reviewInputIdentity(input),
                        mapping: selection.mapping,
                    })
                    results.push({ id: target.id, name: input.name })
                }
                form.setFieldValue("selectedIds", [])
                setFeedback(
                    `已保存 ${results.length} 张申请的映射方案。请逐张打开、采用并完成本申请审核。`,
                )
            } catch (error) {
                setFeedback(
                    `已保存 ${results.length} 张申请。${error instanceof Error ? error.message : "保存未完成，请重新核对或检查浏览器存储。"}`,
                )
            } finally {
                setSavedTargets(results)
                setSaving(false)
            }
        },
    })
    const adopt = async () => {
        if (!canPrepare || !canReadDetails || !saved.plan || saving) return
        setSaving(true)
        try {
            const [result, brandResult, categoryResult, unitResult] =
                await Promise.all([
                    currentApplication.refetch(),
                    currentBrands.refetch(),
                    currentCategories.refetch(),
                    currentUnits.refetch(),
                ])
            const failed = [
                result,
                brandResult,
                categoryResult,
                unitResult,
            ].find((query) => query.error)
            if (failed?.error) throw failed.error
            if (
                !result.data ||
                !brandResult.data ||
                !categoryResult.data ||
                !unitResult.data
            )
                throw new Error("本申请或字典资料读取未完成，请重新读取。")
            const issue = await mappingPlanIssue(
                saved.plan,
                actorId,
                result.data,
                {
                    brands: brandResult.data,
                    categories: categoryResult.data,
                    units: unitResult.data,
                },
            )
            if (issue) {
                setSaved({ plan: saved.plan, issue })
                return
            }
            onAdopt(saved.plan.mapping)
            setFeedback(
                "已回填本申请确认的品牌、分类和单位。请核对本张申请后单独审核。",
            )
        } catch (error) {
            setFeedback(
                error instanceof Error
                    ? error.message
                    : "本申请读取失败，请重新读取后核对。",
            )
        } finally {
            setSaving(false)
        }
    }
    const candidates =
        applications.data?.items.filter(
            (item) => item.id !== application.id && item.kind === "new_product",
        ) ?? []
    const brand = brands.find((item) => item.id === mapping.brandId)
    const category = categories.find((item) => item.id === mapping.categoryId)
    const prefix = `supplier-portal-review-mapping-batch-${toAutomationIdSegment(application.id)}`
    return (
        <section className="space-y-3 rounded-lg border p-4">
            <h3 className="font-medium">同批申请映射核对</h3>
            <p className="text-sm text-muted-foreground">
                先核对本张申请的映射，再勾选同供应商、同类型及原文口径一致的申请。保存后仍须逐张采用、核对和审核。
            </p>
            {(saved.plan || saved.issue) && (
                <div className="space-y-2 rounded-md border p-3">
                    <p className="text-sm">
                        {saved.plan
                            ? `本申请已有来自“${saved.plan.sourceName}”的映射方案。`
                            : "本申请的映射方案需要重新核对。"}
                    </p>
                    <p className="text-sm text-muted-foreground">
                        {saved.issue ??
                            "采用前将重新读取本申请，核对原稿、申请状态及全部字典版本。"}
                    </p>
                    <div className="flex flex-wrap gap-2">
                        <Button
                            id={`${prefix}-adopt`}
                            type="button"
                            variant="outline"
                            disabled={
                                !canPrepare ||
                                !canReadDetails ||
                                saving ||
                                !saved.plan ||
                                !!saved.issue
                            }
                            onClick={() => void adopt()}
                        >
                            采用为本申请确认的映射
                        </Button>
                        <Button
                            id={`${prefix}-discard`}
                            type="button"
                            variant="ghost"
                            disabled={saving || !actorId}
                            onClick={() => {
                                try {
                                    discardMappingPlan(actorId, application.id)
                                    setSaved({ plan: null, issue: null })
                                    setFeedback(
                                        "已清除本申请的辅助映射方案，请逐项重新核对。",
                                    )
                                } catch {
                                    setFeedback(
                                        "清除未完成，请检查浏览器存储后重试。",
                                    )
                                }
                            }}
                        >
                            清除本申请方案
                        </Button>
                    </div>
                </div>
            )}
            <div className="space-y-1 text-sm">
                <p>
                    品牌原文：{rawProduct.brand.raw_name} →{" "}
                    {brand?.name ?? "待确认"}
                </p>
                <p>
                    分类原始完整路径：{rawProduct.category.raw_name} →{" "}
                    {category?.path ?? category?.name ?? "待确认"}
                </p>
                {rawProduct.skus.map((sku) => {
                    const selected = mapping.mappings.find(
                        (row) => row.rowId === sku.row_id,
                    )
                    const unit = units.find(
                        (item) => item.id === selected?.unitId,
                    )
                    return (
                        <p key={sku.row_id}>
                            {sku.name} · {sku.ordering_code}：
                            {sku.unit.raw_name} → {unit?.name ?? "待确认"}
                            {sku.packaging
                                ? `；包装：${sku.packaging.original_unit}含${sku.packaging.units_per_package}${sku.packaging.base_unit}，原单位价格${sku.packaging.original_unit_price}`
                                : "；无包装换算"}
                            {`；报价口径：${sku.quote_basis || "未标注"}`}
                            {selected?.unitSynonymConfirmed
                                ? `；同义确认：${selected.unitSynonymReason || "待填写说明"}`
                                : ""}
                        </p>
                    )
                })}
            </div>
            {sourceIssue && (
                <p className="text-sm text-muted-foreground">{sourceIssue}</p>
            )}
            <form.Field name="confirmedSource">
                {(field) => (
                    <label
                        htmlFor={`${prefix}-confirm-source`}
                        className="flex items-start gap-2 text-sm"
                    >
                        <NativeCheckbox
                            id={`${prefix}-confirm-source`}
                            checked={
                                !sourceIssue &&
                                field.state.value === confirmation
                            }
                            disabled={!canPrepare || !!sourceIssue || saving}
                            onCheckedChange={(checked) => {
                                field.handleChange(checked ? confirmation : "")
                                form.setFieldValue("selectedIds", [])
                            }}
                        />
                        我已核对以上品牌、完整分类路径和每行单位及包装口径，明确用于下一步所选申请。
                    </label>
                )}
            </form.Field>
            {!canList && (
                <p className="text-sm text-muted-foreground">
                    当前账号没有申请列表资格，可逐张完成当前审核任务。
                </p>
            )}
            <Button
                id={`${prefix}-load`}
                type="button"
                variant="outline"
                disabled={!canPrepare || !canList || !canReadDetails || saving}
                onClick={() => {
                    setOpened(true)
                    if (opened) void applications.refetch()
                }}
            >
                读取同供应商待审核申请
            </Button>
            {opened && canList && canReadDetails && (
                <>
                    <PortalError
                        error={applications.error}
                        retry={() => void applications.refetch()}
                        id={`${prefix}-reload`}
                    />
                    <form.Subscribe selector={(state) => state.values}>
                        {(value) => (
                            <>
                                <form.Field name="selectedIds">
                                    {(field) => (
                                        <div className="space-y-2">
                                            {candidates.map((target) => {
                                                const result =
                                                    applicableMapping(
                                                        application,
                                                        rawProduct,
                                                        mapping,
                                                        target,
                                                    )
                                                const input =
                                                    reviewProduct(target)
                                                const id = `${prefix}-select-${toAutomationIdSegment(target.id)}`
                                                const eligible =
                                                    !!result.mapping &&
                                                    !sourceIssue &&
                                                    value.confirmedSource ===
                                                        confirmation
                                                return (
                                                    <div
                                                        key={target.id}
                                                        className="flex items-start gap-2 rounded-md border p-3 text-sm"
                                                    >
                                                        <NativeCheckbox
                                                            id={id}
                                                            aria-describedby={`${id}-meaning`}
                                                            checked={
                                                                eligible &&
                                                                field.state.value.includes(
                                                                    target.id,
                                                                )
                                                            }
                                                            disabled={
                                                                !canPrepare ||
                                                                !eligible ||
                                                                saving ||
                                                                applications.isFetching
                                                            }
                                                            onCheckedChange={(
                                                                checked,
                                                            ) =>
                                                                field.handleChange(
                                                                    checked
                                                                        ? [
                                                                              ...field.state.value.filter(
                                                                                  (
                                                                                      item,
                                                                                  ) =>
                                                                                      item !==
                                                                                      target.id,
                                                                              ),
                                                                              target.id,
                                                                          ]
                                                                        : field.state.value.filter(
                                                                              (
                                                                                  item,
                                                                              ) =>
                                                                                  item !==
                                                                                  target.id,
                                                                          ),
                                                                )
                                                            }
                                                        />
                                                        <div className="min-w-0">
                                                            <label htmlFor={id}>
                                                                {input?.name ??
                                                                    target.title ??
                                                                    "新品申请"}
                                                                {target.application_no
                                                                    ? ` · ${target.application_no}`
                                                                    : ""}
                                                            </label>
                                                            <p
                                                                id={`${id}-meaning`}
                                                                className="mt-1 text-muted-foreground"
                                                            >
                                                                {result.issue ??
                                                                    (value.confirmedSource !==
                                                                    confirmation
                                                                        ? "请先核对并确认本次来源映射。"
                                                                        : "品牌、完整分类路径及每行单位、包装和报价口径一致。")}
                                                            </p>
                                                        </div>
                                                    </div>
                                                )
                                            })}
                                            {!applications.isFetching &&
                                                !candidates.length && (
                                                    <p className="text-sm text-muted-foreground">
                                                        本页没有其他可核对的新品申请。
                                                    </p>
                                                )}
                                        </div>
                                    )}
                                </form.Field>
                                <Button
                                    id={`${prefix}-save`}
                                    type="button"
                                    variant="outline"
                                    disabled={
                                        !canPrepare ||
                                        saving ||
                                        !!sourceIssue ||
                                        value.confirmedSource !==
                                            confirmation ||
                                        !value.selectedIds.length ||
                                        applications.isFetching ||
                                        !!applications.error
                                    }
                                    onClick={() => void form.handleSubmit()}
                                >
                                    保存所选申请的映射方案（
                                    {value.selectedIds.length}）
                                </Button>
                            </>
                        )}
                    </form.Subscribe>
                    <div className="flex items-center gap-3 text-sm">
                        <Button
                            id={`${prefix}-previous`}
                            type="button"
                            variant="ghost"
                            disabled={
                                saving || page <= 1 || applications.isFetching
                            }
                            onClick={() => {
                                setPage((current) => current - 1)
                                form.setFieldValue("selectedIds", [])
                            }}
                        >
                            上一页
                        </Button>
                        <span>
                            第 {page} 页 · 共 {applications.data?.total ?? 0}{" "}
                            张申请
                        </span>
                        <Button
                            id={`${prefix}-next`}
                            type="button"
                            variant="ghost"
                            disabled={
                                saving ||
                                applications.isFetching ||
                                !applications.data ||
                                page * 20 >= applications.data.total
                            }
                            onClick={() => {
                                setPage((current) => current + 1)
                                form.setFieldValue("selectedIds", [])
                            }}
                        >
                            下一页
                        </Button>
                    </div>
                </>
            )}
            {savedTargets.length > 0 && (
                <div className="flex flex-wrap gap-3">
                    {savedTargets.map((target) => (
                        <Link
                            key={target.id}
                            id={`${prefix}-open-${toAutomationIdSegment(target.id)}`}
                            className="text-sm text-primary"
                            href={`/procurement/supplier-portal/applications/${encodeURIComponent(target.id)}`}
                        >
                            打开 {target.name} 核对
                        </Link>
                    ))}
                </div>
            )}
            {feedback && (
                <p role="status" className="text-sm text-muted-foreground">
                    {feedback}
                </p>
            )}
        </section>
    )
}
