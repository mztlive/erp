"use client"

import { useMemo } from "react"
import {
    useQueries,
    useQuery,
    type UseQueryResult,
} from "@tanstack/react-query"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { optionKeys } from "@/hooks/use-options"
import { fetchProductUnitDirectory } from "@/lib/options"
import { hasPermission } from "@/lib/permissions"
import { fetchSellableSkuPage } from "@/lib/api/sellable-skus"
import type { SellableSkuDto } from "@/features/master-data/api/contracts"

const SKU_BATCH_SIZE = 50
const STALE_TIME = 5 * 60 * 1000

function combineSkuPresentation(
    queries: UseQueryResult<Awaited<ReturnType<typeof fetchSellableSkuPage>>>[],
) {
    return {
        rows: queries.flatMap((query) => query.data?.items ?? []),
        error: queries.find((query) => query.isError)?.error,
        isPending: queries.some(
            (query) => query.isPending && query.fetchStatus !== "idle",
        ),
        isFetching: queries.some((query) => query.isFetching),
        refetch: () =>
            Promise.all(
                queries
                    .filter((query) => query.isEnabled)
                    .map((query) => query.refetch()),
            ),
    }
}

/** 与公司商品池使用同一权限，每批按稳定 SKU 身份查询。 */
export function useSkuPresentationQueries(
    skuIds: readonly (string | null | undefined)[],
) {
    const account = useAccountProfileQuery()
    const permitted = hasPermission(
        account.data?.permissions,
        "sellable_sku:list",
    )
    const ids = [
        ...new Set(skuIds.filter((id): id is string => Boolean(id))),
    ].sort()
    const batches: string[][] = []
    for (let offset = 0; offset < ids.length; offset += SKU_BATCH_SIZE) {
        batches.push(ids.slice(offset, offset + SKU_BATCH_SIZE))
    }
    return useQueries({
        queries: (permitted ? batches : []).map((batch) => ({
            queryKey: ["master-data", "sku-presentation", batch],
            queryFn: () =>
                fetchSellableSkuPage({
                    sku_ids: batch.join(","),
                    page: 1,
                    page_size: SKU_BATCH_SIZE,
                }),
            staleTime: STALE_TIME,
        })),
        combine: combineSkuPresentation,
    })
}

export function useProductUnitDirectoryQuery(enabled: boolean) {
    const account = useAccountProfileQuery()
    const canReadDirectory = hasPermission(
        account.data?.permissions,
        "unit_of_measure:list",
    )
    const query = useQuery({
        queryKey: optionKeys.unitDirectory,
        queryFn: fetchProductUnitDirectory,
        enabled: enabled && canReadDirectory,
        staleTime: STALE_TIME,
    })
    return enabled && canReadDirectory ? query.data : undefined
}

export function indexProductUnitNames(
    rows: readonly Pick<
        SellableSkuDto,
        "sku_id" | "base_unit_id" | "base_unit_code" | "base_unit_name"
    >[],
    directory: Awaited<ReturnType<typeof fetchProductUnitDirectory>> = [],
) {
    const names = new Map<string, string>()
    const namesByUnitKey = new Map<string, string>()
    const namesBySkuId = new Map<string, string>()
    for (const unit of directory) {
        names.set(unit.name, unit.name)
        namesByUnitKey.set(unit.id, unit.name)
        namesByUnitKey.set(unit.unit_code, unit.name)
    }
    for (const row of rows) {
        const id = row.base_unit_id
        const code = row.base_unit_code?.trim()
        const name =
            (id && namesByUnitKey.get(id)) ||
            (code && namesByUnitKey.get(code)) ||
            row.base_unit_name?.trim()
        if (!name) continue
        namesBySkuId.set(row.sku_id, name)
        if (id) namesByUnitKey.set(id, name)
        if (code) namesByUnitKey.set(code, name)
        names.set(name, name)
    }
    for (const [key, name] of namesByUnitKey) names.set(key, name)
    const nameForUnit = (unit?: string | null, skuId?: string | null) => {
        const key = unit?.trim()
        const skuName = skuId ? namesBySkuId.get(skuId) : undefined
        if (!key || key === skuName) return skuName
        return names.get(key)
    }
    return { names, namesBySkuId, nameForUnit }
}

/** 按原单位匹配，避免用当前 SKU 的单位改写历史单据。 */
export function useProductUnitNames(
    skuIds: readonly (string | null | undefined)[] = [],
    enabled = true,
) {
    const directory = useProductUnitDirectoryQuery(enabled)
    const { rows } = useSkuPresentationQueries(enabled ? skuIds : [])
    return useMemo(
        () => indexProductUnitNames(rows, directory),
        [rows, directory],
    )
}
