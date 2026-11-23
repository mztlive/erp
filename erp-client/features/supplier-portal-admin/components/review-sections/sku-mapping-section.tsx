"use client"

import { useEffect, useMemo, useState } from "react"
import { useStore } from "@tanstack/react-form"
import {
    ChevronDownIcon,
    LayersIcon,
    PlusIcon,
    Undo2Icon,
    WandSparklesIcon,
} from "lucide-react"
import { detailWorkspaceStyles } from "@/components/business/detail-workspace"
import { OptionCombobox } from "@/components/business/option-combobox"
import { withForm } from "@/components/form"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { NativeCheckbox } from "@/components/ui/checkbox"
import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { StatusBadge } from "@/components/ui/status-badge"
import type {
    NewProductInput,
    PortalApplication,
    PortalDictionary,
} from "@/features/supplier-portal/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import type { PortalDuplicate } from "../../api"
import { readMappingPlan } from "../../lib/review-mapping-plan"
import { PortalReviewMappingBatch } from "../review-mapping-batch"
import { PortalUnitMappingBulk } from "../unit-mapping-bulk"
import {
    dictionaryOptions,
    evaluateSkuRowStatus,
    fieldHighlightClass,
    reviewFormBaseOptions,
    type BatchUnitApplySnapshot,
    type BatchUnitRowRecord,
    type CreateDictionaryRequest,
    type ExistingOfferingCandidate,
    type ReviewSectionId,
    type SkuUnitPreviousState,
} from "./types"

export const SkuMappingSection = withForm({
    ...reviewFormBaseOptions,
    props: {
        application: {} as PortalApplication,
        rawProduct: {} as NewProductInput,
        brands: [] as PortalDictionary[],
        categories: [] as PortalDictionary[],
        units: [] as PortalDictionary[],
        duplicates: [] as PortalDuplicate[],
        existingCandidates: [] as ExistingOfferingCandidate[],
        actorId: "",
        canListApplications: false,
        canReadDetails: false,
        canCreateUnit: false,
        disabled: false,
        highlightedFieldId: null as string | null,
        onClearHighlight: () => {},
        onRefreshUnits: async () => {},
        onRefreshDuplicates: async () => {},
        onCreateDictionary: (_req: CreateDictionaryRequest) => {},
        onJumpToSection: (_sectionId: ReviewSectionId) => {},
    },
    render: function SkuMappingSectionContent({
        form,
        application,
        rawProduct,
        brands,
        categories,
        units,
        duplicates,
        existingCandidates,
        actorId,
        canListApplications,
        canReadDetails,
        canCreateUnit,
        disabled,
        highlightedFieldId,
        onClearHighlight,
        onRefreshUnits,
        onRefreshDuplicates,
        onCreateDictionary,
        onJumpToSection,
    }) {
        const [defaultUnitId, setDefaultUnitId] = useState("")
        const [batchNotice, setBatchNotice] = useState("")
        const [lastBatchApply, setLastBatchApply] =
            useState<BatchUnitApplySnapshot | null>(null)
        const [rowBatchRecords, setRowBatchRecords] = useState<
            Record<string, BatchUnitRowRecord>
        >({})
        const [sourceBulkOpen, setSourceBulkOpen] = useState(false)
        const [batchPlanOpen, setBatchPlanOpen] = useState(false)

        useEffect(() => {
            if (!actorId) return
            const saved = readMappingPlan(actorId, application.id)
            if (saved.plan || saved.issue) {
                setBatchPlanOpen(true)
            }
        }, [actorId, application.id])

        const mappings = useStore(form.store, (state) => state.values.mappings)
        const productTarget = useStore(
            form.store,
            (state) => state.values.productTarget,
        )

        const unitOptions = useMemo(() => dictionaryOptions(units), [units])
        const selectedDefaultUnit = useMemo(
            () => units.find((item) => item.id === defaultUnitId) ?? null,
            [defaultUnitId, units],
        )

        const skuStats = useMemo(() => {
            let lockedCount = 0
            let unmappedUnlockedCount = 0
            let mappedUnlockedCount = 0
            for (const sku of rawProduct.skus) {
                if (sku.unit.selected_id) {
                    lockedCount += 1
                    continue
                }
                const row = mappings.find((item) => item.rowId === sku.row_id)
                if (row?.unitId) {
                    mappedUnlockedCount += 1
                } else {
                    unmappedUnlockedCount += 1
                }
            }
            return {
                total: rawProduct.skus.length,
                lockedCount,
                unlockedCount: unmappedUnlockedCount + mappedUnlockedCount,
                unmappedUnlockedCount,
                mappedUnlockedCount,
            }
        }, [mappings, rawProduct.skus])

        const applyDefaultUnit = (scope: "unmapped" | "all") => {
            if (disabled || !selectedDefaultUnit) return
            onClearHighlight()
            setBatchNotice("")
            const currentMappings = form.state.values.mappings
            const previousRows: Record<string, SkuUnitPreviousState> = {}
            const appliedRowIds: string[] = []
            const overwrittenRowIds: string[] = []
            const synonymRequiredRowIds: string[] = []
            const nextRowRecords: Record<string, BatchUnitRowRecord> = {
                ...rowBatchRecords,
            }
            let skippedLockedCount = 0

            for (let index = 0; index < rawProduct.skus.length; index++) {
                const sku = rawProduct.skus[index]
                if (!sku) continue
                if (sku.unit.selected_id) {
                    skippedLockedCount += 1
                    continue
                }
                const currentRow = currentMappings[index]
                if (!currentRow) continue
                const alreadyMapped = Boolean(currentRow.unitId)
                if (scope === "unmapped" && alreadyMapped) continue
                const sameUnitId = currentRow.unitId === selectedDefaultUnit.id
                if (
                    sameUnitId &&
                    currentRow.unitVersion === selectedDefaultUnit.version
                ) {
                    continue
                }

                const previous: SkuUnitPreviousState = {
                    unitId: currentRow.unitId,
                    unitVersion: currentRow.unitVersion,
                    unitSynonymConfirmed: currentRow.unitSynonymConfirmed,
                    unitSynonymReason: currentRow.unitSynonymReason,
                }
                previousRows[sku.row_id] = previous
                appliedRowIds.push(sku.row_id)
                if (alreadyMapped && !sameUnitId) {
                    overwrittenRowIds.push(sku.row_id)
                }
                const preserveSynonym =
                    sameUnitId && currentRow.unitSynonymConfirmed
                const synonymAlreadyComplete =
                    preserveSynonym &&
                    Boolean(currentRow.unitSynonymReason.trim())
                if (
                    sku.unit.raw_name.trim() !==
                        selectedDefaultUnit.name.trim() &&
                    !synonymAlreadyComplete
                ) {
                    synonymRequiredRowIds.push(sku.row_id)
                }

                form.setFieldValue(
                    `mappings[${index}].unitId`,
                    selectedDefaultUnit.id,
                )
                form.setFieldValue(
                    `mappings[${index}].unitVersion`,
                    selectedDefaultUnit.version,
                )
                if (!sameUnitId) {
                    form.setFieldValue(
                        `mappings[${index}].unitSynonymConfirmed`,
                        false,
                    )
                    form.setFieldValue(
                        `mappings[${index}].unitSynonymReason`,
                        "",
                    )
                }

                nextRowRecords[sku.row_id] = {
                    unitId: selectedDefaultUnit.id,
                    unitName: selectedDefaultUnit.name,
                    overwritten: alreadyMapped && !sameUnitId,
                    previous,
                }
            }

            if (appliedRowIds.length === 0) {
                setBatchNotice(
                    `全部可编辑 SKU 已匹配为「${selectedDefaultUnit.name}」，无需重复应用。`,
                )
                return
            }

            setRowBatchRecords(nextRowRecords)
            setLastBatchApply({
                unitId: selectedDefaultUnit.id,
                unitName: selectedDefaultUnit.name,
                scope,
                appliedRowIds,
                overwrittenRowIds,
                synonymRequiredRowIds,
                skippedLockedCount,
                previousRows,
            })
        }

        const activeBatchRowIds = useMemo(() => {
            if (!lastBatchApply) return []
            return lastBatchApply.appliedRowIds.filter((rowId) => {
                const record = rowBatchRecords[rowId]
                const row = mappings.find((item) => item.rowId === rowId)
                return Boolean(
                    record &&
                    record.unitId === lastBatchApply.unitId &&
                    row &&
                    row.unitId === lastBatchApply.unitId,
                )
            })
        }, [lastBatchApply, mappings, rowBatchRecords])

        const undoLastBatchApply = () => {
            if (disabled || !lastBatchApply) return
            onClearHighlight()
            setBatchNotice("")
            const currentMappings = form.state.values.mappings
            const nextRowRecords = { ...rowBatchRecords }
            for (const rowId of lastBatchApply.appliedRowIds) {
                const record = rowBatchRecords[rowId]
                if (!record || record.unitId !== lastBatchApply.unitId) continue
                const previous = lastBatchApply.previousRows[rowId]
                const index = currentMappings.findIndex(
                    (row) => row.rowId === rowId,
                )
                const currentRow = index >= 0 ? currentMappings[index] : null
                if (
                    !currentRow ||
                    currentRow.unitId !== lastBatchApply.unitId ||
                    !previous
                ) {
                    continue
                }
                form.setFieldValue(`mappings[${index}].unitId`, previous.unitId)
                form.setFieldValue(
                    `mappings[${index}].unitVersion`,
                    previous.unitVersion,
                )
                form.setFieldValue(
                    `mappings[${index}].unitSynonymConfirmed`,
                    previous.unitSynonymConfirmed,
                )
                form.setFieldValue(
                    `mappings[${index}].unitSynonymReason`,
                    previous.unitSynonymReason,
                )
                delete nextRowRecords[rowId]
            }
            setRowBatchRecords(nextRowRecords)
            setLastBatchApply(null)
        }

        const undoSingleRowBatch = (rowId: string, index: number) => {
            if (disabled) return
            const record = rowBatchRecords[rowId]
            if (!record) return
            onClearHighlight()
            form.setFieldValue(
                `mappings[${index}].unitId`,
                record.previous.unitId,
            )
            form.setFieldValue(
                `mappings[${index}].unitVersion`,
                record.previous.unitVersion,
            )
            form.setFieldValue(
                `mappings[${index}].unitSynonymConfirmed`,
                record.previous.unitSynonymConfirmed,
            )
            form.setFieldValue(
                `mappings[${index}].unitSynonymReason`,
                record.previous.unitSynonymReason,
            )
            setRowBatchRecords((prev) => {
                const next = { ...prev }
                delete next[rowId]
                return next
            })
            setLastBatchApply((prev) => {
                if (!prev || !prev.appliedRowIds.includes(rowId)) return prev
                const remainingApplied = prev.appliedRowIds.filter(
                    (id) => id !== rowId,
                )
                if (remainingApplied.length === 0) return null
                const remainingPrevious = { ...prev.previousRows }
                delete remainingPrevious[rowId]
                return {
                    ...prev,
                    appliedRowIds: remainingApplied,
                    overwrittenRowIds: prev.overwrittenRowIds.filter(
                        (id) => id !== rowId,
                    ),
                    synonymRequiredRowIds: prev.synonymRequiredRowIds.filter(
                        (id) => id !== rowId,
                    ),
                    previousRows: remainingPrevious,
                }
            })
        }

        const productCandidate = duplicates.find(
            (item) => item.product_id === productTarget,
        )

        return (
            <div className="space-y-5">
                {/* 快捷工具条：一键应用默认单位到全部 SKU */}
                <div className="space-y-3 border-y border-border/60 bg-muted/25 px-4 py-3.5">
                    <div className="flex flex-wrap items-center justify-between gap-2">
                        <div className="flex items-center gap-2">
                            <WandSparklesIcon
                                aria-hidden="true"
                                className="size-4 shrink-0 text-primary"
                            />
                            <h3 className="text-body-compact font-semibold text-foreground">
                                一键应用默认单位到全部 SKU
                            </h3>
                        </div>
                        <p className="text-xs text-muted-foreground">
                            共{" "}
                            <span className="num font-medium text-foreground">
                                {skuStats.total}
                            </span>{" "}
                            个规格 · 待匹配{" "}
                            <span className="num font-medium text-foreground">
                                {skuStats.unmappedUnlockedCount}
                            </span>{" "}
                            个
                            {skuStats.mappedUnlockedCount > 0 ? (
                                <>
                                    {" "}
                                    · 已选{" "}
                                    <span className="num font-medium text-foreground">
                                        {skuStats.mappedUnlockedCount}
                                    </span>{" "}
                                    个
                                </>
                            ) : null}
                            {skuStats.lockedCount > 0 ? (
                                <>
                                    {" "}
                                    · 供应商已锁定{" "}
                                    <span className="num font-medium text-foreground">
                                        {skuStats.lockedCount}
                                    </span>{" "}
                                    个
                                </>
                            ) : null}
                        </p>
                    </div>

                    <div className="flex flex-col gap-2.5 sm:flex-row sm:items-center">
                        <div className="min-w-0 flex-1 sm:max-w-xs">
                            <OptionCombobox
                                id="supplier-portal-review-default-unit"
                                aria-label="默认标准基础单位"
                                options={unitOptions}
                                value={defaultUnitId || null}
                                onValueChange={(next) => {
                                    setDefaultUnitId(next ?? "")
                                    setBatchNotice("")
                                }}
                                onRefresh={onRefreshUnits}
                                placeholder="选择要批量应用的默认标准单位"
                                disabled={
                                    disabled || skuStats.unlockedCount === 0
                                }
                                size="sm"
                            />
                        </div>
                        <div className="flex flex-wrap items-center gap-2">
                            <Button
                                id="supplier-portal-review-default-unit-apply-unmapped"
                                type="button"
                                size="sm"
                                variant="outline"
                                disabled={
                                    disabled ||
                                    !selectedDefaultUnit ||
                                    skuStats.unmappedUnlockedCount === 0
                                }
                                onClick={() => applyDefaultUnit("unmapped")}
                            >
                                应用到未匹配 SKU（
                                <span className="num">
                                    {skuStats.unmappedUnlockedCount}
                                </span>
                                ）
                            </Button>
                            <Button
                                id="supplier-portal-review-default-unit-apply-all"
                                type="button"
                                size="sm"
                                variant="secondary"
                                disabled={
                                    disabled ||
                                    !selectedDefaultUnit ||
                                    skuStats.unlockedCount === 0
                                }
                                onClick={() => applyDefaultUnit("all")}
                            >
                                一键应用到全部 SKU（
                                <span className="num">
                                    {skuStats.unlockedCount}
                                </span>
                                ）
                            </Button>
                        </div>
                    </div>

                    {selectedDefaultUnit && skuStats.mappedUnlockedCount > 0 ? (
                        <p className="text-xs text-muted-foreground">
                            选择「一键应用到全部 SKU」将覆盖当前已映射的{" "}
                            <span className="num font-medium text-foreground">
                                {skuStats.mappedUnlockedCount}
                            </span>{" "}
                            个可编辑规格（支持一键撤销或逐行还原）；供应商已锁定单位不受影响。提报原文与标准单位不同时仍须逐行勾选同义确认。
                        </p>
                    ) : (
                        <p className="text-xs text-muted-foreground">
                            批量应用后保留每行独立微调与一键撤销能力；提报单位原文与标准单位不同时仍须逐行勾选同义确认并填写核对依据。
                        </p>
                    )}

                    {batchNotice ? (
                        <p
                            role="status"
                            className="text-xs text-muted-foreground"
                        >
                            {batchNotice}
                        </p>
                    ) : null}

                    {lastBatchApply && activeBatchRowIds.length > 0 ? (
                        <div
                            role="status"
                            className="flex flex-wrap items-center justify-between gap-2 border-l-2 border-info-border bg-info-soft/55 px-3 py-2 text-xs text-info-soft-foreground"
                        >
                            <span>
                                已将默认单位「{lastBatchApply.unitName}」应用到{" "}
                                <span className="num font-semibold">
                                    {activeBatchRowIds.length}
                                </span>{" "}
                                个 SKU
                                {lastBatchApply.appliedRowIds.length >
                                activeBatchRowIds.length ? (
                                    <>
                                        （另有{" "}
                                        <span className="num font-semibold">
                                            {lastBatchApply.appliedRowIds
                                                .length -
                                                activeBatchRowIds.length}
                                        </span>{" "}
                                        项已手动修改，撤销时将保留）
                                    </>
                                ) : null}
                                {lastBatchApply.overwrittenRowIds.filter((id) =>
                                    activeBatchRowIds.includes(id),
                                ).length > 0 ? (
                                    <>
                                        （覆盖原有映射{" "}
                                        <span className="num font-semibold">
                                            {
                                                lastBatchApply.overwrittenRowIds.filter(
                                                    (id) =>
                                                        activeBatchRowIds.includes(
                                                            id,
                                                        ),
                                                ).length
                                            }
                                        </span>{" "}
                                        项）
                                    </>
                                ) : null}
                                {lastBatchApply.skippedLockedCount > 0 ? (
                                    <>
                                        ，跳过供应商已锁定{" "}
                                        <span className="num font-semibold">
                                            {lastBatchApply.skippedLockedCount}
                                        </span>{" "}
                                        项
                                    </>
                                ) : null}
                                {lastBatchApply.synonymRequiredRowIds.filter(
                                    (id) => activeBatchRowIds.includes(id),
                                ).length > 0 ? (
                                    <>
                                        ；其中{" "}
                                        <span className="num font-semibold">
                                            {
                                                lastBatchApply.synonymRequiredRowIds.filter(
                                                    (id) =>
                                                        activeBatchRowIds.includes(
                                                            id,
                                                        ),
                                                ).length
                                            }
                                        </span>{" "}
                                        项单位原文与标准单位不同，请在下方逐行确认同义说明
                                    </>
                                ) : null}
                                。
                            </span>
                            <Button
                                id="supplier-portal-review-default-unit-undo"
                                type="button"
                                size="sm"
                                variant="ghost"
                                disabled={disabled}
                                className="h-7 px-2 text-xs text-info-soft-foreground hover:bg-info-soft"
                                onClick={undoLastBatchApply}
                            >
                                <Undo2Icon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                撤销本次批量应用
                            </Button>
                        </div>
                    ) : null}
                </div>

                {/* 逐项 SKU 单位与规格建档映射 */}
                <div className="max-w-4xl divide-y divide-border/60 border-b border-border/60">
                    {rawProduct.skus.map((sku, index) => {
                        const prefix = `supplier-portal-review-sku-${toAutomationIdSegment(sku.row_id)}`
                        const row = mappings[index]
                        const rowStatus = evaluateSkuRowStatus({
                            sku,
                            row,
                            units,
                            selectedProduct: productCandidate,
                            existingCandidates,
                        })
                        const batchRecord = rowBatchRecords[sku.row_id]
                        const isBatchAppliedActive = Boolean(
                            batchRecord &&
                            row &&
                            row.unitId === batchRecord.unitId,
                        )
                        const isManuallyOverridden = Boolean(
                            batchRecord &&
                            row &&
                            row.unitId !== batchRecord.unitId,
                        )

                        return (
                            <div
                                key={sku.row_id}
                                className="grid gap-4 py-4 md:grid-cols-2"
                            >
                                <div className="space-y-1 md:col-span-2">
                                    <div className="flex flex-wrap items-center justify-between gap-2">
                                        <div className="flex flex-wrap items-center gap-2">
                                            <p className="font-medium text-foreground">
                                                {sku.name} ·{" "}
                                                <span className="num">
                                                    {sku.ordering_code}
                                                </span>
                                            </p>
                                            {sku.unit.selected_id ? (
                                                <Badge variant="neutral">
                                                    供应商已选单位
                                                </Badge>
                                            ) : null}
                                            {isBatchAppliedActive &&
                                            batchRecord ? (
                                                <Badge
                                                    variant={
                                                        batchRecord.overwritten
                                                            ? "warning"
                                                            : "info"
                                                    }
                                                >
                                                    {batchRecord.overwritten
                                                        ? `批量覆盖：${batchRecord.unitName}`
                                                        : `批量默认：${batchRecord.unitName}`}
                                                </Badge>
                                            ) : null}
                                            {isManuallyOverridden ? (
                                                <Badge variant="outline">
                                                    已手动覆盖
                                                </Badge>
                                            ) : null}
                                        </div>
                                        <div className="flex items-center gap-2">
                                            {batchRecord &&
                                            !sku.unit.selected_id ? (
                                                <Button
                                                    id={`${prefix}-unit-undo`}
                                                    type="button"
                                                    size="sm"
                                                    variant="ghost"
                                                    disabled={disabled}
                                                    className="h-6 px-1.5 text-tiny text-muted-foreground hover:text-foreground"
                                                    onClick={() =>
                                                        undoSingleRowBatch(
                                                            sku.row_id,
                                                            index,
                                                        )
                                                    }
                                                >
                                                    <Undo2Icon
                                                        data-icon="inline-start"
                                                        aria-hidden="true"
                                                    />
                                                    还原原值
                                                </Button>
                                            ) : null}
                                            {rowStatus.ready ? (
                                                <StatusBadge
                                                    tone="success"
                                                    appearance="quiet"
                                                    label="已就绪"
                                                />
                                            ) : (
                                                <StatusBadge
                                                    tone="destructive"
                                                    label={`${rowStatus.issues.length} 项待补`}
                                                />
                                            )}
                                        </div>
                                    </div>
                                    <p className="text-xs text-muted-foreground">
                                        提报单位：{sku.unit.raw_name} ·
                                        提报规格：
                                        {sku.spec_entries
                                            .map(
                                                (entry) =>
                                                    `${entry.attribute_code}：${entry.attribute_value_code}`,
                                            )
                                            .join(" / ") || "无规格明细"}
                                        {sku.packaging
                                            ? ` · 包装：${sku.packaging.original_unit}含${sku.packaging.units_per_package}${sku.packaging.base_unit}`
                                            : ""}
                                    </p>
                                </div>

                                <div
                                    className={fieldHighlightClass(
                                        highlightedFieldId,
                                        `${prefix}-unit`,
                                    )}
                                >
                                    <form.AppField
                                        name={`mappings[${index}].unitId`}
                                    >
                                        {(field) => (
                                            <field.SelectField
                                                onRefresh={onRefreshUnits}
                                                onValueChange={(id) => {
                                                    onClearHighlight()
                                                    form.setFieldValue(
                                                        `mappings[${index}].unitVersion`,
                                                        units.find(
                                                            (u) => u.id === id,
                                                        )?.version ?? 0,
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
                                                options={unitOptions}
                                                disabled={
                                                    disabled ||
                                                    !!sku.unit.selected_id
                                                }
                                                allowClear={
                                                    !sku.unit.selected_id
                                                }
                                            />
                                        )}
                                    </form.AppField>
                                </div>

                                {canCreateUnit && !sku.unit.selected_id ? (
                                    <div className="flex items-end">
                                        <Button
                                            id={`${prefix}-create-unit`}
                                            type="button"
                                            size="sm"
                                            variant="ghost"
                                            className={
                                                detailWorkspaceStyles.subtleButton
                                            }
                                            disabled={disabled}
                                            onClick={() =>
                                                onCreateDictionary({
                                                    kind: "unit",
                                                    rawName: sku.unit.raw_name,
                                                    rowId: sku.row_id,
                                                })
                                            }
                                        >
                                            <PlusIcon
                                                data-icon="inline-start"
                                                aria-hidden="true"
                                            />
                                            补建此基础单位
                                        </Button>
                                    </div>
                                ) : null}

                                <div
                                    className={cn(
                                        "md:col-span-2",
                                        fieldHighlightClass(
                                            highlightedFieldId,
                                            `${prefix}-unit-synonym-confirmed`,
                                        ),
                                    )}
                                >
                                    <form.AppField
                                        name={`mappings[${index}].unitSynonymConfirmed`}
                                    >
                                        {(field) => (
                                            <label
                                                htmlFor={`${prefix}-unit-synonym-confirmed`}
                                                className={cn(
                                                    "flex items-center gap-2 text-xs",
                                                    rowStatus.needsSynonymConfirmation &&
                                                        !field.state.value &&
                                                        "font-medium text-warning-soft-foreground",
                                                )}
                                            >
                                                <NativeCheckbox
                                                    id={`${prefix}-unit-synonym-confirmed`}
                                                    checked={field.state.value}
                                                    disabled={disabled}
                                                    onCheckedChange={(
                                                        value,
                                                    ) => {
                                                        onClearHighlight()
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
                                    className={fieldHighlightClass(
                                        highlightedFieldId,
                                        `${prefix}-unit-synonym-reason`,
                                    )}
                                >
                                    <form.AppField
                                        name={`mappings[${index}].unitSynonymReason`}
                                    >
                                        {(field) => (
                                            <field.TextField
                                                id={`${prefix}-unit-synonym-reason`}
                                                label="单位一致/换算说明（勾选一致确认时必填）"
                                                required={
                                                    row?.unitSynonymConfirmed
                                                }
                                                disabled={disabled}
                                            />
                                        )}
                                    </form.AppField>
                                </div>

                                <div
                                    className={cn(
                                        "space-y-1.5",
                                        fieldHighlightClass(
                                            highlightedFieldId,
                                            `${prefix}-target`,
                                        ),
                                    )}
                                >
                                    <form.AppField
                                        name={`mappings[${index}].target`}
                                    >
                                        {(field) => (
                                            <field.SelectField
                                                onRefresh={onRefreshDuplicates}
                                                onValueChange={onClearHighlight}
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
                                                    ).map((item) => ({
                                                        value: item.sku_id,
                                                        label: `复用：${item.name ?? productCandidate?.name} · ${item.specification ?? "默认规格"} · ${item.sku_no ?? "已有公司规格"} · ${item.unit_name ?? "请核对单位"}`,
                                                    })),
                                                ]}
                                                disabled={
                                                    disabled ||
                                                    productTarget ===
                                                        "unconfirmed"
                                                }
                                                allowClear={false}
                                            />
                                        )}
                                    </form.AppField>
                                    {productTarget === "unconfirmed" ? (
                                        <p className="text-tiny text-muted-foreground">
                                            请先在第 1 步明确商品建档决定。
                                            <button
                                                id={`${prefix}-jump-product-target`}
                                                type="button"
                                                className="ml-1 cursor-pointer text-primary underline-offset-2 hover:underline"
                                                onClick={() =>
                                                    onJumpToSection("basic")
                                                }
                                            >
                                                前往选择
                                            </button>
                                        </p>
                                    ) : null}
                                </div>
                            </div>
                        )
                    })}
                </div>

                {/* 相同订货编码下的已有供货商品核对 */}
                {existingCandidates.length > 0 ? (
                    <div className="max-w-4xl">
                        <form.AppField name="existingConfirmed">
                            {(field) => (
                                <section className="space-y-3">
                                    <h4 className="text-body-compact font-medium text-foreground">
                                        相同订货编码下的已有供货商品核对
                                    </h4>
                                    {existingCandidates.map((candidate) => {
                                        const candidateId = `supplier-portal-review-existing-${toAutomationIdSegment(candidate.offering_id)}`
                                        return (
                                            <div
                                                key={candidate.offering_id}
                                                className={fieldHighlightClass(
                                                    highlightedFieldId,
                                                    candidateId,
                                                )}
                                            >
                                                <label
                                                    htmlFor={candidateId}
                                                    className="flex items-start gap-2 text-xs"
                                                >
                                                    <NativeCheckbox
                                                        id={candidateId}
                                                        checked={field.state.value.includes(
                                                            candidate.offering_id,
                                                        )}
                                                        disabled={disabled}
                                                        onCheckedChange={(
                                                            checked,
                                                        ) => {
                                                            onClearHighlight()
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
                                    })}
                                </section>
                            )}
                        </form.AppField>
                    </div>
                ) : null}

                <p className="max-w-4xl text-xs text-muted-foreground">
                    仅本次新增规格保持未上架；复用在售商品规格并增加有效供货后，可能立即参与销售及采购供应选择。
                </p>

                {/* 高级批量工具 1：按已确认来源规格批量复制同义与包装映射 */}
                <Collapsible
                    open={sourceBulkOpen}
                    onOpenChange={setSourceBulkOpen}
                >
                    <CollapsibleTrigger
                        id="supplier-portal-review-unit-bulk-toggle"
                        type="button"
                        className="flex w-full cursor-pointer items-center justify-between gap-2 border-t border-border/60 pt-3.5 text-left text-body-compact font-medium text-foreground outline-none hover:text-primary focus-visible:rounded-sm focus-visible:ring-2 focus-visible:ring-ring"
                    >
                        <span className="flex items-center gap-2">
                            <LayersIcon
                                aria-hidden="true"
                                className="size-4 text-muted-foreground"
                            />
                            高级批量工具：按已确认来源规格批量复制同义映射
                        </span>
                        <ChevronDownIcon
                            aria-hidden="true"
                            className={cn(
                                "size-4 shrink-0 text-muted-foreground transition-transform duration-150",
                                !sourceBulkOpen && "-rotate-90",
                            )}
                        />
                    </CollapsibleTrigger>
                    <CollapsibleContent
                        keepMounted
                        className="pt-2 data-closed:hidden"
                    >
                        <PortalUnitMappingBulk
                            skus={rawProduct.skus}
                            units={units}
                            mappings={mappings}
                            disabled={disabled}
                            onApply={(updates) => {
                                setRowBatchRecords((prev) => {
                                    const next = { ...prev }
                                    for (const update of updates) {
                                        delete next[update.rowId]
                                    }
                                    return next
                                })
                                setLastBatchApply(null)
                                setBatchNotice("")
                                for (const update of updates) {
                                    const index =
                                        form.state.values.mappings.findIndex(
                                            (row) => row.rowId === update.rowId,
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
                    </CollapsibleContent>
                </Collapsible>

                {/* 高级批量工具 2：同批申请映射核对（跨申请方案） */}
                <Collapsible
                    open={batchPlanOpen}
                    onOpenChange={setBatchPlanOpen}
                >
                    <CollapsibleTrigger
                        id="supplier-portal-review-mapping-batch-toggle"
                        type="button"
                        className="flex w-full max-w-4xl cursor-pointer items-center justify-between gap-2 border-t border-border/60 pt-3.5 text-left text-body-compact font-medium text-foreground outline-none hover:text-primary focus-visible:rounded-sm focus-visible:ring-2 focus-visible:ring-ring"
                    >
                        <span className="flex items-center gap-2">
                            <LayersIcon
                                aria-hidden="true"
                                className="size-4 text-muted-foreground"
                            />
                            跨申请协作：同批申请映射核对（采用或保存到同供应商其他申请）
                        </span>
                        <ChevronDownIcon
                            aria-hidden="true"
                            className={cn(
                                "size-4 shrink-0 text-muted-foreground transition-transform duration-150",
                                !batchPlanOpen && "-rotate-90",
                            )}
                        />
                    </CollapsibleTrigger>
                    <CollapsibleContent
                        keepMounted
                        className="max-w-4xl pt-2 data-closed:hidden"
                    >
                        <form.Subscribe selector={(state) => state.values}>
                            {(values) => (
                                <PortalReviewMappingBatch
                                    application={application}
                                    rawProduct={rawProduct}
                                    brands={brands}
                                    categories={categories}
                                    units={units}
                                    mapping={{
                                        brandId: values.brandId,
                                        brandVersion: values.brandVersion,
                                        categoryId: values.categoryId,
                                        categoryVersion: values.categoryVersion,
                                        categoryHierarchy:
                                            values.categoryHierarchy,
                                        mappings: values.mappings,
                                    }}
                                    actorId={actorId}
                                    canList={canListApplications}
                                    canReadDetails={canReadDetails}
                                    disabled={disabled}
                                    onAdopt={(adopted) => {
                                        setRowBatchRecords({})
                                        setLastBatchApply(null)
                                        setBatchNotice("")
                                        form.setFieldValue(
                                            "brandId",
                                            adopted.brandId,
                                        )
                                        form.setFieldValue(
                                            "brandVersion",
                                            adopted.brandVersion,
                                        )
                                        form.setFieldValue(
                                            "categoryId",
                                            adopted.categoryId,
                                        )
                                        form.setFieldValue(
                                            "categoryVersion",
                                            adopted.categoryVersion,
                                        )
                                        form.setFieldValue(
                                            "categoryHierarchy",
                                            adopted.categoryHierarchy,
                                        )
                                        form.setFieldValue(
                                            "mappings",
                                            form.state.values.mappings.map(
                                                (row) => {
                                                    const update =
                                                        adopted.mappings.find(
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
                    </CollapsibleContent>
                </Collapsible>
            </div>
        )
    },
})
