"use client"

/**
 * 跨页面复用的选项类 React Query hooks（供应商/负责人/团队/单位）。
 *
 * 基于 optionKeys + lib/options 的 fetch* 函数，staleTime 为 5 分钟，
 * 避免列表/表单频繁触发请求。
 */

import { useQuery } from "@tanstack/react-query"

import {
    fetchOwnerOptions,
    fetchUnitOptions,
} from "@/lib/options"

/** 跨工作面共享选项查询的 queryKey。 */
export const optionKeys = {
    owners: ["options", "owners"] as const,
    units: ["options", "units"] as const,
}

/** 选项类数据缓存 5 分钟，避免列表/表单频繁触发请求。 */
const OPTIONS_STALE_TIME = 5 * 60 * 1000

/**
 * 负责人选项查询（管理后台账号列表），供选择负责人场景使用。
 * @returns useQuery 结果（data 为负责人选项列表）。
 */
export function useOwnerOptionsQuery() {
    return useQuery({
        queryKey: optionKeys.owners,
        queryFn: fetchOwnerOptions,
        staleTime: OPTIONS_STALE_TIME,
    })
}

/**
 * 计量单位选项查询（仅启用状态），供表单单位下拉使用。
 * @returns useQuery 结果（data 为计量单位选项列表）。
 */
export function useUnitOptionsQuery() {
    return useQuery({
        queryKey: optionKeys.units,
        queryFn: fetchUnitOptions,
        staleTime: OPTIONS_STALE_TIME,
    })
}
