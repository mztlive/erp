"use client"

import * as React from "react"
import { WELFARE_SCENARIO_OPTIONS } from "@/lib/business-options"

import type { ComboboxOption } from "@/components/business/option-combobox"
import {
    COST_TYPE_CHIP_PREFIX,
    type ProfitLossAppliedChip,
} from "@/features/actual-profit-loss/hooks/profit-loss-filter-contract"
import {
    COVERAGE_FILTER_LABEL,
    type ProfitLossCoverage,
    type ProfitLossView,
} from "@/features/actual-profit-loss/types"

type Options = Readonly<{
    data: ProfitLossView | undefined
    qParam: string
    coverage: ProfitLossCoverage
    customerId: string | undefined
    salesOrderId: string | undefined
    benefitScenario: string | undefined
    attributionUserIds: readonly string[]
    attributionOrgUnitIds: readonly string[]
    attributionGroup?: string
    costTypes: readonly string[]
}>

/** 将已生效筛选投影为 chip 与可撤销的下拉选项。 */
export function useProfitLossFilterPresentation({
    data,
    qParam,
    coverage,
    customerId,
    salesOrderId,
    benefitScenario,
    costTypes,
    attributionUserIds,
    attributionOrgUnitIds,
    attributionGroup,
}: Options) {
    const selectedCustomerLabel = React.useMemo(
        () =>
            data?.rows.items.find((row) => row.customerId === customerId)
                ?.customerLabel,
        [customerId, data?.rows.items],
    )
    const selectedSalesOrderLabel = React.useMemo(
        () =>
            data?.rows.items.find((row) => row.objectId === salesOrderId)
                ?.identityLabel,
        [data?.rows.items, salesOrderId],
    )
    const costTypeLabelMap = React.useMemo(
        () =>
            new Map(
                (data?.costComposition ?? []).map((item) => [
                    item.costType,
                    item.label,
                ]),
            ),
        [data?.costComposition],
    )

    const appliedChips = React.useMemo<readonly ProfitLossAppliedChip[]>(() => {
        const chips: ProfitLossAppliedChip[] = []
        if (attributionGroup) {
            const [dimension, id] = attributionGroup.split(":")
            const options =
                dimension === "attribution_org"
                    ? data?.attributionOrgOptions
                    : data?.attributionUserOptions
            const label = id
                ? (options?.find((option) => option.value === id)?.label ?? id)
                : "未知归属"
            chips.push({
                key: "attributionGroup",
                label: `历史分组下钻：${label}`,
            })
        }
        for (const [key, ids, options, label] of [
            [
                "attributionUserIds",
                attributionUserIds,
                data?.attributionUserOptions,
                "历史归属销售",
            ],
            [
                "attributionOrgUnitIds",
                attributionOrgUnitIds,
                data?.attributionOrgOptions,
                "历史归属组织",
            ],
        ] as const) {
            if (ids.length)
                chips.push({
                    key,
                    label: `${label}：${ids.map((id) => options?.find((o) => o.value === id)?.label ?? id).join("、")}`,
                })
        }
        const q = qParam.trim()
        if (q) chips.push({ key: "q", label: `搜索：${q}` })
        if (coverage !== "covered") {
            chips.push({
                key: "coverage",
                label: `覆盖：${COVERAGE_FILTER_LABEL[coverage]}`,
            })
        }
        if (customerId) {
            chips.push({
                key: "customerId",
                label: selectedCustomerLabel ?? "客户锁定",
            })
        }
        if (salesOrderId) {
            chips.push({
                key: "salesOrderId",
                label: selectedSalesOrderLabel ?? "销售单锁定",
            })
        }
        if (benefitScenario) {
            chips.push({
                key: "benefitScenario",
                label: `福利场景：${benefitScenario}`,
            })
        }
        for (const value of costTypes) {
            chips.push({
                key: `${COST_TYPE_CHIP_PREFIX}${value}`,
                label: `成本类型：${costTypeLabelMap.get(value) ?? value}`,
            })
        }
        return chips
    }, [
        attributionGroup,
        attributionUserIds,
        attributionOrgUnitIds,
        data?.attributionUserOptions,
        data?.attributionOrgOptions,
        benefitScenario,
        costTypeLabelMap,
        costTypes,
        coverage,
        customerId,
        qParam,
        salesOrderId,
        selectedCustomerLabel,
        selectedSalesOrderLabel,
    ])

    const benefitScenarioOptions = React.useMemo<
        readonly ComboboxOption[]
    >(() => {
        const labels = new Set([
            ...WELFARE_SCENARIO_OPTIONS.map((option) => option.label),
            "未标注",
        ])
        if (benefitScenario) labels.add(benefitScenario)
        return [...labels].map((label) => ({ value: label, label }))
    }, [benefitScenario])
    const costTypeOptions = React.useMemo<readonly ComboboxOption[]>(() => {
        const seen = new Set<string>()
        const options: ComboboxOption[] = []
        for (const item of data?.costComposition ?? []) {
            if (item.costType && !seen.has(item.costType)) {
                seen.add(item.costType)
                options.push({
                    value: item.costType,
                    label: item.label || item.costType,
                })
            }
        }
        for (const value of costTypes) {
            if (!seen.has(value)) {
                options.push({
                    value,
                    label: costTypeLabelMap.get(value) ?? value,
                })
            }
        }
        return options
    }, [costTypeLabelMap, costTypes, data?.costComposition])

    return {
        appliedChips,
        benefitScenarioOptions,
        costTypeOptions,
    }
}

export type { ProfitLossAppliedChip }
