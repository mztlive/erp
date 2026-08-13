"use client"

import * as React from "react"
import { useQuery } from "@tanstack/react-query"

import type { SalesOrderComboboxItem } from "@/components/business/entity-comboboxes"
import {
    fetchContractOption,
    fetchMallOptions,
    fetchCustomerOption,
    fetchPartyOption,
    fetchSupplierOption,
    fetchWarehouseOption,
    searchCompanySkus,
    searchContracts,
    searchCustomers,
    searchParties,
    searchSellableSkus,
    searchSuppliers,
    searchWarehouses,
    type ContractSearch,
    type CustomerSearch,
    type EntitySearch,
    type SellableSkuSearch,
} from "@/features/entity-selectors/api"
import {
    fetchSalesOrderDetail,
    fetchSalesOrders,
} from "@/features/sales-orders/api"
import { fetchMasterDataList } from "@/features/master-data/api"

const STALE_TIME = 5 * 60 * 1000

export const entitySelectorKeys = {
    all: ["entity-selectors"] as const,
    supplier: (input: EntitySearch) =>
        [...entitySelectorKeys.all, "supplier", input] as const,
    supplierDetail: (id: string) =>
        [...entitySelectorKeys.all, "supplier", "detail", id] as const,
    customer: (input: CustomerSearch) =>
        [...entitySelectorKeys.all, "customer", input] as const,
    customerDetail: (id: string) =>
        [...entitySelectorKeys.all, "customer", "detail", id] as const,
    party: (input: EntitySearch) =>
        [...entitySelectorKeys.all, "party", input] as const,
    partyDetail: (id: string) =>
        [...entitySelectorKeys.all, "party", "detail", id] as const,
    warehouse: (input: EntitySearch) =>
        [...entitySelectorKeys.all, "warehouse", input] as const,
    warehouseDetail: (id: string) =>
        [...entitySelectorKeys.all, "warehouse", "detail", id] as const,
    contract: (input: ContractSearch) =>
        [...entitySelectorKeys.all, "contract", input] as const,
    contractDetail: (id: string) =>
        [...entitySelectorKeys.all, "contract", "detail", id] as const,
    salesOrder: (input: EntitySearch) =>
        [...entitySelectorKeys.all, "sales-order", input] as const,
    salesOrderDetail: (id: string) =>
        [...entitySelectorKeys.all, "sales-order", "detail", id] as const,
    sellableSku: (input: SellableSkuSearch) =>
        [...entitySelectorKeys.all, "sellable-sku", input] as const,
    companySku: (input: EntitySearch) =>
        [...entitySelectorKeys.all, "company-sku", input] as const,
    malls: (purpose: string) =>
        [...entitySelectorKeys.all, "mall", { purpose }] as const,
    voucherCategories: (purpose: string) =>
        [...entitySelectorKeys.all, "voucher-category", { purpose }] as const,
}

/** 输入防抖只更新查询条件；HTTP 始终由 TanStack Query 的 queryFn 发起。 */
export function useDebouncedSearch(value: string, delay = 250) {
    const [debounced, setDebounced] = React.useState(value)
    React.useEffect(() => {
        const timer = window.setTimeout(() => setDebounced(value.trim()), delay)
        return () => window.clearTimeout(timer)
    }, [delay, value])
    return debounced
}

function commonQueryOptions() {
    return {
        staleTime: STALE_TIME,
        placeholderData: <T>(previous: T | undefined) => previous,
    }
}

export function useSupplierSelectorQuery(
    input: EntitySearch,
    selectedId?: string,
) {
    const list = useQuery({
        queryKey: entitySelectorKeys.supplier(input),
        queryFn: () => searchSuppliers(input),
        ...commonQueryOptions(),
    })
    const selected = useQuery({
        queryKey: entitySelectorKeys.supplierDetail(selectedId ?? ""),
        queryFn: () => fetchSupplierOption(selectedId ?? ""),
        enabled: Boolean(selectedId),
        staleTime: STALE_TIME,
    })
    return { list, selected }
}

export function useCustomerSelectorQuery(
    input: CustomerSearch,
    selectedId?: string,
) {
    const list = useQuery({
        queryKey: entitySelectorKeys.customer(input),
        queryFn: () => searchCustomers(input),
        ...commonQueryOptions(),
    })
    const selected = useQuery({
        queryKey: entitySelectorKeys.customerDetail(selectedId ?? ""),
        queryFn: () => fetchCustomerOption(selectedId ?? ""),
        enabled: Boolean(selectedId),
        staleTime: STALE_TIME,
    })
    return { list, selected }
}

export function usePartySelectorQuery(
    input: EntitySearch,
    selectedId?: string,
) {
    const list = useQuery({
        queryKey: entitySelectorKeys.party(input),
        queryFn: () => searchParties(input),
        ...commonQueryOptions(),
    })
    const selected = useQuery({
        queryKey: entitySelectorKeys.partyDetail(selectedId ?? ""),
        queryFn: () => fetchPartyOption(selectedId ?? ""),
        enabled: Boolean(selectedId),
        staleTime: STALE_TIME,
    })
    return { list, selected }
}

export function useWarehouseSelectorQuery(
    input: EntitySearch,
    selectedId?: string,
) {
    const list = useQuery({
        queryKey: entitySelectorKeys.warehouse(input),
        queryFn: () => searchWarehouses(input),
        ...commonQueryOptions(),
    })
    const selected = useQuery({
        queryKey: entitySelectorKeys.warehouseDetail(selectedId ?? ""),
        queryFn: () => fetchWarehouseOption(selectedId ?? ""),
        enabled: Boolean(selectedId),
        staleTime: STALE_TIME,
    })
    return { list, selected }
}

export function useContractSelectorQuery(
    input: ContractSearch,
    selectedId?: string,
) {
    const list = useQuery({
        queryKey: entitySelectorKeys.contract(input),
        queryFn: () => searchContracts(input),
        ...commonQueryOptions(),
    })
    const selected = useQuery({
        queryKey: entitySelectorKeys.contractDetail(selectedId ?? ""),
        queryFn: () => fetchContractOption(selectedId ?? ""),
        enabled: Boolean(selectedId),
        staleTime: STALE_TIME,
    })
    return { list, selected }
}

function salesOrderItem(
    row: NonNullable<Awaited<ReturnType<typeof fetchSalesOrderDetail>>>,
): SalesOrderComboboxItem {
    return {
        id: row.id,
        documentNumber: row.documentNumber,
        customerName: row.customerName,
        statusLabel: row.primaryStatus.label,
        statusTone: row.primaryStatus.tone,
        amountGross: row.amountGross,
        natureLabel: row.nature === "card_voucher" ? "卡券" : "实物与服务",
    }
}

export function useSalesOrderSelectorQuery(
    input: EntitySearch,
    selectedId?: string,
) {
    const list = useQuery({
        queryKey: entitySelectorKeys.salesOrder(input),
        queryFn: async () => {
            const result = await fetchSalesOrders({
                page: 1,
                pageSize: 30,
                search: input.query || undefined,
            })
            return result.items.map((row) => ({
                id: row.id,
                documentNumber: row.documentNumber,
                customerName: row.customerName,
                statusLabel: row.primaryStatus.label,
                statusTone: row.primaryStatus.tone,
                amountGross: row.amountGross,
                natureLabel:
                    row.nature === "card_voucher" ? "卡券" : "实物与服务",
            })) satisfies SalesOrderComboboxItem[]
        },
        ...commonQueryOptions(),
    })
    const selected = useQuery({
        queryKey: entitySelectorKeys.salesOrderDetail(selectedId ?? ""),
        queryFn: async () => {
            const row = await fetchSalesOrderDetail(selectedId ?? "")
            return row ? salesOrderItem(row) : null
        },
        enabled: Boolean(selectedId),
        staleTime: STALE_TIME,
    })
    return { list, selected }
}

export function useSellableSkuSelectorQuery(input: SellableSkuSearch) {
    return useQuery({
        queryKey: entitySelectorKeys.sellableSku(input),
        queryFn: () => searchSellableSkus(input),
        ...commonQueryOptions(),
    })
}

export function useCompanySkuSelectorQuery(input: EntitySearch) {
    return useQuery({
        queryKey: entitySelectorKeys.companySku(input),
        queryFn: () => searchCompanySkus(input),
        ...commonQueryOptions(),
    })
}

export function useMallSelectorQuery(purpose: string) {
    return useQuery({
        queryKey: entitySelectorKeys.malls(purpose),
        queryFn: fetchMallOptions,
        staleTime: STALE_TIME,
    })
}

export function useVoucherCategorySelectorQuery(purpose: string) {
    return useQuery({
        queryKey: entitySelectorKeys.voucherCategories(purpose),
        queryFn: async () => {
            const profiles = await fetchMasterDataList({
                resource: "voucher-categories",
                lifecycleStatus: "enabled",
            })
            const source =
                profiles.rows.length > 0
                    ? profiles.rows
                    : (
                          await fetchMasterDataList({
                              resource: "sellable-items",
                              lifecycleStatus: "enabled",
                              productKind: "VOUCHER",
                          })
                      ).rows
            return source.map((item) => ({
                productId: item.stableId,
                revisionId: item.currentRevisionId,
                sku: item.stableNo,
                name: item.name,
                statusLabel: item.lifecycleStatusLabel,
                statusTone: item.lifecycleTone,
                baseUnit: "张",
                description:
                    item.keyFacts.find((fact) => fact.label === "说明")
                        ?.value ??
                    item.keyFacts.find((fact) => fact.label === "商品类型")
                        ?.value,
            }))
        },
        staleTime: STALE_TIME,
    })
}
