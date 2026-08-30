"use client"

import * as React from "react"
import { useQuery } from "@tanstack/react-query"

import type { SalesOrderComboboxItem } from "@/components/business/entity-comboboxes"
import { apiGet, type Page } from "@/lib/api"
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
} from "@/features/entity-selectors/api/index"
import { queryKeyRoots } from "@/lib/query-key-roots"

const STALE_TIME = 5 * 60 * 1000

export const entitySelectorKeys = {
    all: queryKeyRoots.entitySelectors,
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
    warehouseDetail: (id: string, purpose: EntitySearch["purpose"]) =>
        [
            ...entitySelectorKeys.all,
            "warehouse",
            "detail",
            id,
            purpose,
        ] as const,
    contract: (input: ContractSearch) =>
        [...entitySelectorKeys.all, "contract", input] as const,
    contractDetail: (id: string, scope?: ContractSearch["scope"]) =>
        [...entitySelectorKeys.all, "contract", "detail", id, scope] as const,
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
        queryKey: entitySelectorKeys.warehouseDetail(
            selectedId ?? "",
            input.purpose,
        ),
        queryFn: () => fetchWarehouseOption(selectedId ?? "", input.purpose),
        enabled: Boolean(selectedId),
        staleTime: STALE_TIME,
    })
    return { list, selected }
}

export function useContractSelectorQuery(
    input: ContractSearch,
    selectedId?: string,
    options?: { enabled?: boolean },
) {
    const enabled = options?.enabled ?? true
    const list = useQuery({
        queryKey: entitySelectorKeys.contract(input),
        queryFn: () => searchContracts(input),
        ...commonQueryOptions(),
        enabled,
    })
    const selected = useQuery({
        queryKey: entitySelectorKeys.contractDetail(
            selectedId ?? "",
            input.scope,
        ),
        queryFn: () =>
            fetchContractOption(selectedId ?? "", { scope: input.scope }),
        enabled: enabled && Boolean(selectedId),
        staleTime: STALE_TIME,
    })
    return { list, selected }
}

type SalesOrderSelectorDto = Readonly<{
    id: string
    order_no: string
    business_type: string
    customer_id: string
    commercial_status: string
    stage: { label: string; tone: string }
    current_revision_id?: string | null
    revisions?: ReadonlyArray<{
        id: string
        customer_name?: string
        gross_amount: string
    }>
}>

function salesOrderTone(tone: string): SalesOrderComboboxItem["statusTone"] {
    if (
        tone === "success" ||
        tone === "warning" ||
        tone === "destructive" ||
        tone === "info"
    ) {
        return tone
    }
    return "neutral"
}

function salesOrderItem(row: SalesOrderSelectorDto): SalesOrderComboboxItem {
    const revision = row.current_revision_id
        ? row.revisions?.find((item) => item.id === row.current_revision_id)
        : undefined
    return {
        id: row.id,
        documentNumber: row.order_no,
        customerName: revision?.customer_name?.trim() || row.customer_id,
        statusLabel: row.stage.label || row.commercial_status,
        statusTone: salesOrderTone(row.stage.tone),
        amountGross: revision?.gross_amount,
        natureLabel: row.business_type === "VOUCHER" ? "卡券" : "实物与服务",
    }
}

export function useSalesOrderSelectorQuery(
    input: EntitySearch,
    selectedId?: string,
) {
    const list = useQuery({
        queryKey: entitySelectorKeys.salesOrder(input),
        queryFn: async () => {
            const result = await apiGet<Page<SalesOrderSelectorDto>>(
                "/admin/sales-orders",
                {
                    page: 1,
                    page_size: 30,
                    order_no: input.query.trim() || undefined,
                    sort_by: "created_at",
                    sort_dir: "desc",
                },
            )
            return result.items.map(salesOrderItem)
        },
        ...commonQueryOptions(),
    })
    const selected = useQuery({
        queryKey: entitySelectorKeys.salesOrderDetail(selectedId ?? ""),
        queryFn: async () => {
            if (!selectedId) return null
            const row = await apiGet<SalesOrderSelectorDto>(
                `/admin/sales-orders/${encodeURIComponent(selectedId)}`,
            )
            return salesOrderItem(row)
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
