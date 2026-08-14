"use client"

import * as React from "react"

import type { SupplierComboboxItem } from "@/components/business/entity-comboboxes"
import type { ProcurementSupplyOption } from "@/features/procurement-confirmation/api"
import { money } from "@/features/procurement-confirmation/lib/format"
import {
    capabilityCodeForMode,
    supplyCostForQuantity,
} from "@/features/procurement-confirmation/lib/supply-cost"
import {
    FULFILLMENT_MODE_LABEL,
    type ConfirmationLineDraft,
    type CoverageByLine,
    type FulfillmentMode,
    type ProcurementConfirmationTask,
    type ProcurementRecommendation,
} from "@/features/procurement-confirmation/types"

export type ProcurementConfirmationDraftsOptions = {
    task: ProcurementConfirmationTask | undefined
    confirmOpen: boolean
    recommendation: ProcurementRecommendation | undefined
    supplyOptions: readonly ProcurementSupplyOption[]
    supplierOptions: readonly SupplierComboboxItem[] | undefined
    setSaveMessage: React.Dispatch<React.SetStateAction<string | null>>
    setActionError: React.Dispatch<React.SetStateAction<string | null>>
}

/**
 * 确认分行草稿：任务同步、系统方案装载、供应商名称回填，
 * 以及覆盖缺口、合法性校验、当前方案汇总与分行编辑动作。
 */
export function useProcurementConfirmationDrafts({
    task,
    confirmOpen,
    recommendation,
    supplyOptions,
    supplierOptions,
    setSaveMessage,
    setActionError,
}: ProcurementConfirmationDraftsOptions) {
    const [lineDrafts, setLineDrafts] = React.useState<ConfirmationLineDraft[]>(
        [],
    )
    const [dirty, setDirty] = React.useState(false)
    const [loadedPlanKey, setLoadedPlanKey] = React.useState<string | null>(
        null,
    )

    // 同步当前任务分行草稿
    React.useEffect(() => {
        if (!task) {
            setLineDrafts([])
            setDirty(false)
            setLoadedPlanKey(null)
            return
        }
        setLineDrafts(task.confirmation.lines.map((line) => ({ ...line })))
        setDirty(false)
        setLoadedPlanKey(null)
        setActionError(null)
        setSaveMessage(null)
    }, [task, setActionError, setSaveMessage])

    // 只有点击“确认通过”后才把最低成本方案装入可编辑草稿；关闭再打开时保留本次调整。
    React.useEffect(() => {
        if (!confirmOpen || !task || !recommendation?.ready || dirty) return
        const planKey = `${task.confirmation.confirmationId}:${recommendation.calculatedAt}`
        if (loadedPlanKey === planKey) return
        setLineDrafts(recommendation.lines.map((line) => ({ ...line })))
        setLoadedPlanKey(planKey)
        setSaveMessage(null)
    }, [
        confirmOpen,
        dirty,
        loadedPlanKey,
        recommendation,
        task,
        setSaveMessage,
    ])

    // 选项异步到达时仅回填展示名称，不覆盖用户已编辑的草稿字段。
    React.useEffect(() => {
        if (!supplierOptions?.length) return
        setLineDrafts((current) =>
            current.map((line) => {
                const supplier = supplierOptions.find(
                    (option) => option.supplierId === line.supplierId,
                )
                return supplier && supplier.supplierName !== line.supplierName
                    ? { ...line, supplierName: supplier.supplierName }
                    : line
            }),
        )
    }, [supplierOptions])

    const coverage: CoverageByLine[] = React.useMemo(() => {
        if (!task) return []
        return task.salesSubmission.lines.map((line) => {
            const confirmed = lineDrafts
                .filter((c) => c.submissionLineId === line.submissionLineId)
                .reduce((sum, c) => sum + Number(c.confirmedQuantity || 0), 0)
            const required = Number(line.committedQuantity)
            const complete = confirmed + 1e-9 >= required && required > 0
            const gap = Math.max(0, required - confirmed)
            return {
                submissionLineId: line.submissionLineId,
                itemName: line.itemName,
                confirmed: confirmed.toFixed(0),
                required: line.committedQuantity,
                complete,
                gap: gap.toFixed(0),
            }
        })
    }, [task, lineDrafts])

    const linesValid =
        lineDrafts.length > 0 &&
        lineDrafts.every(
            (line) =>
                line.supplierId.trim().length > 0 &&
                line.offeringRevisionId.trim().length > 0 &&
                line.confirmedQuantity.trim().length > 0 &&
                Number(line.confirmedQuantity) > 0 &&
                line.latestCostGross.trim().length > 0 &&
                Number(line.latestCostGross) >= 0 &&
                line.inputTaxRate.trim().length > 0 &&
                Number(line.inputTaxRate) >= 0 &&
                line.expectedDeliveryDate.trim().length > 0 &&
                line.capabilityRevisionId.trim().length > 0,
        )
    const allCovered =
        coverage.length > 0 && coverage.every((c) => c.complete) && linesValid
    const clientBlocking = coverage
        .filter((c) => !c.complete)
        .map((c) => ({
            id: c.submissionLineId,
            label: c.itemName,
            message: `已确认 ${c.confirmed}/${c.required}，缺口 ${c.gap}`,
            targetId: `submission-line-${c.submissionLineId}`,
        }))

    const currentPlanSummary = React.useMemo(() => {
        let purchaseGross = 0
        const chargedGroups = new Set<string>()
        const orderGroups = new Set<string>()
        for (const line of lineDrafts) {
            const quantity = Number(line.confirmedQuantity || 0)
            const unitCost = Number(line.latestCostGross || 0)
            if (Number.isFinite(quantity) && Number.isFinite(unitCost)) {
                purchaseGross += quantity * unitCost
            }
            if (!line.offeringRevisionId || !line.supplierId) continue
            const feeGroup = `${line.offeringRevisionId}:${line.fulfillmentMode}`
            orderGroups.add(`${line.supplierId}:${line.fulfillmentMode}`)
            if (chargedGroups.has(feeGroup)) continue
            chargedGroups.add(feeGroup)
            const offering = supplyOptions.find(
                (option) =>
                    option.offeringRevisionId === line.offeringRevisionId,
            )
            if (!offering) continue
            purchaseGross += Number(offering.serviceFeeAmount || 0)
            if (line.fulfillmentMode === "WAREHOUSE") {
                purchaseGross += Number(offering.freightAmount || 0)
            }
        }
        return {
            purchaseGross,
            grossMargin:
                Number(task?.salesSubmission.grossAmount ?? 0) - purchaseGross,
            orderCount: orderGroups.size,
        }
    }, [lineDrafts, supplyOptions, task?.salesSubmission.grossAmount])

    const updateLine = React.useCallback(
        (lineKey: string, patch: Partial<ConfirmationLineDraft>) => {
            setLineDrafts((prev) =>
                prev.map((l) =>
                    l.lineKey === lineKey ? { ...l, ...patch } : l,
                ),
            )
            setDirty(true)
        },
        [],
    )

    const repriceDrafts = React.useCallback(
        (drafts: ConfirmationLineDraft[]) => {
            const quantityByOffering = new Map<string, number>()
            for (const line of drafts) {
                if (!line.offeringRevisionId) continue
                quantityByOffering.set(
                    line.offeringRevisionId,
                    (quantityByOffering.get(line.offeringRevisionId) ?? 0) +
                        Number(line.confirmedQuantity || 0),
                )
            }
            return drafts.map((line) => {
                const offering = supplyOptions.find(
                    (option) =>
                        option.offeringRevisionId === line.offeringRevisionId,
                )
                if (!offering) return line
                return {
                    ...line,
                    latestCostGross: supplyCostForQuantity(
                        offering,
                        String(
                            quantityByOffering.get(line.offeringRevisionId) ??
                                0,
                        ),
                    ),
                }
            })
        },
        [supplyOptions],
    )

    const updatePlanLine = React.useCallback(
        (lineKey: string, patch: Partial<ConfirmationLineDraft>) => {
            setLineDrafts((current) =>
                repriceDrafts(
                    current.map((line) =>
                        line.lineKey === lineKey ? { ...line, ...patch } : line,
                    ),
                ),
            )
            setDirty(true)
        },
        [repriceDrafts],
    )

    const applyRecommendation = React.useCallback(() => {
        if (!recommendation?.ready || recommendation.lines.length === 0) {
            setActionError("当前没有可执行的系统采购方案，请先处理阻断项")
            return
        }
        setLineDrafts(recommendation.lines.map((line) => ({ ...line })))
        setDirty(true)
        setSaveMessage("已重新载入系统最低成本方案，请核对交期后保存")
        setActionError(null)
    }, [recommendation, setActionError, setSaveMessage])

    const offeringOptionsForSku = React.useCallback(
        (skuId: string) =>
            supplyOptions
                .filter((option) => option.skuId === skuId)
                .map((option) => {
                    const supplier = supplierOptions?.find(
                        (row) => row.supplierId === option.supplierId,
                    )
                    return {
                        value: option.offeringRevisionId,
                        label: `${supplier?.supplierName ?? "供应商"} · 一件代发 ${money.format(Number(option.dropshipCostGross))} / 集采 ${money.format(Number(option.bulkCostGross))}（满 ${option.bulkMinimumOrderQuantity}）`,
                    }
                }),
        [supplierOptions, supplyOptions],
    )

    const capabilityOptionsForOffering = React.useCallback(
        (offeringRevisionId: string, fulfillmentMode: FulfillmentMode) =>
            supplyOptions
                .find(
                    (option) =>
                        option.offeringRevisionId === offeringRevisionId,
                )
                ?.capabilities.filter(
                    (capability) =>
                        capability.capabilityCode ===
                        capabilityCodeForMode(fulfillmentMode),
                )
                .map((capability) => ({
                    value: capability.revisionId,
                    label: capability.label,
                })) ?? [],
        [supplyOptions],
    )

    const fulfillmentOptionsForOffering = React.useCallback(
        (offeringRevisionId: string) => {
            const offering = supplyOptions.find(
                (option) => option.offeringRevisionId === offeringRevisionId,
            )
            const modes = (
                Object.keys(FULFILLMENT_MODE_LABEL) as FulfillmentMode[]
            ).filter(
                (mode) =>
                    !offering ||
                    (supplyCostForQuantity(offering, "1").length > 0 &&
                        offering.capabilities.some(
                            (capability) =>
                                capability.capabilityCode ===
                                capabilityCodeForMode(mode),
                        )),
            )
            return modes.map((mode) => ({
                value: mode,
                label: FULFILLMENT_MODE_LABEL[mode],
            }))
        },
        [supplyOptions],
    )

    const addSplitLine = React.useCallback(
        (submissionLineId: string) => {
            if (!task) return
            const sub = task.salesSubmission.lines.find(
                (l) => l.submissionLineId === submissionLineId,
            )
            if (!sub) return
            const key = `cl_new_${submissionLineId}_${Date.now().toString(36)}`
            setLineDrafts((prev) => [
                ...prev,
                {
                    lineKey: key,
                    submissionLineId,
                    supplierId: "",
                    supplierName: "",
                    offeringRevisionId: "",
                    confirmedQuantity: "0",
                    latestCostGross: "",
                    inputTaxRate: "",
                    expectedDeliveryDate: "",
                    fulfillmentMode: "WAREHOUSE",
                    capabilityRevisionId: "",
                    capabilitySummary: "请选择供应商并核对供给与能力",
                    qualificationStatus: "INVALID",
                },
            ])
            setDirty(true)
        },
        [task],
    )

    const removeLine = React.useCallback((lineKey: string) => {
        setLineDrafts((prev) => {
            const target = prev.find((l) => l.lineKey === lineKey)
            if (!target) return prev
            const same = prev.filter(
                (l) => l.submissionLineId === target.submissionLineId,
            )
            if (same.length <= 1) return prev
            return prev.filter((l) => l.lineKey !== lineKey)
        })
        setDirty(true)
    }, [])

    return {
        lineDrafts,
        dirty,
        setDirty,
        coverage,
        linesValid,
        allCovered,
        clientBlocking,
        currentPlanSummary,
        updateLine,
        updatePlanLine,
        applyRecommendation,
        addSplitLine,
        removeLine,
        offeringOptionsForSku,
        capabilityOptionsForOffering,
        fulfillmentOptionsForOffering,
    }
}
