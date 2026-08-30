"use client"

import * as React from "react"

import type { ComboboxOption } from "@/components/business/option-combobox"
import {
    COST_TYPE_CHIP_PREFIX,
    FULFILLMENT_MODE_CHIP_PREFIX,
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
    fulfillmentModes: readonly string[]
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
    fulfillmentModes,
    costTypes,
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
        for (const value of fulfillmentModes) {
            chips.push({
                key: `${FULFILLMENT_MODE_CHIP_PREFIX}${value}`,
                label: `履约方式：${value}`,
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
        benefitScenario,
        costTypeLabelMap,
        costTypes,
        coverage,
        customerId,
        fulfillmentModes,
        qParam,
        salesOrderId,
        selectedCustomerLabel,
        selectedSalesOrderLabel,
    ])

    const benefitScenarioOptions = React.useMemo<
        readonly ComboboxOption[]
    >(() => {
        const seen = new Set<string>()
        const options: ComboboxOption[] = []
        for (const row of data?.rows.items ?? []) {
            for (const value of row.benefitScenarios ?? []) {
                if (value && !seen.has(value)) {
                    seen.add(value)
                    options.push({ value, label: value })
                }
            }
        }
        if (benefitScenario && !seen.has(benefitScenario)) {
            options.push({ value: benefitScenario, label: benefitScenario })
        }
        return options
    }, [benefitScenario, data?.rows.items])
    const fulfillmentModeOptions = React.useMemo<
        readonly ComboboxOption[]
    >(() => {
        const seen = new Set<string>()
        const options: ComboboxOption[] = []
        for (const row of data?.rows.items ?? []) {
            for (const value of row.fulfillmentModes ?? []) {
                if (value && !seen.has(value)) {
                    seen.add(value)
                    options.push({ value, label: value })
                }
            }
        }
        for (const value of fulfillmentModes) {
            if (!seen.has(value)) {
                options.push({ value, label: value })
            }
        }
        return options
    }, [data?.rows.items, fulfillmentModes])
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
        fulfillmentModeOptions,
        costTypeOptions,
    }
}

export type { ProfitLossAppliedChip }
