"use client"

import { useMutation, useQuery } from "@tanstack/react-query"

import {
    exportCurrentQuality,
    exportHistoryQuality,
    fetchCurrentQuality,
    fetchHistoryQuality,
} from "../api/dual-caliber"
import type {
    CurrentQualityQuery,
    HistoryQualityQuery,
    QualityCaliber,
} from "../dual-types"

const dualQualityKeys = {
    all: ["customer-quality", "dual"] as const,
    current: (query: CurrentQualityQuery) =>
        [...dualQualityKeys.all, "current", query] as const,
    history: (query: HistoryQualityQuery) =>
        [...dualQualityKeys.all, "history", query] as const,
}

/**
 * 当前负责口径查询：Query key 包含现任人员／组织／业务全部条件与范围版本，
 * 刷新、前进后退与清除条件保持一致；历史条件永不进入本口径 key。
 */
export function useCurrentQualityQuery(query: CurrentQualityQuery | null) {
    return useQuery({
        queryKey: dualQualityKeys.current(
            query ?? {
                from: "",
                to: "",
                dimension: "customer",
                sort: "orderCount:desc",
                page: 1,
                pageSize: 20,
            },
        ),
        queryFn: () => fetchCurrentQuality(query!),
        enabled: Boolean(query?.from && query?.to),
    })
}

/**
 * 历史贡献口径查询：Query key 只含冻结归属条件；现任条件永不进入本口径 key，
 * 两口径缓存天然隔离，不存在合并排名。
 */
export function useHistoryQualityQuery(query: HistoryQualityQuery | null) {
    return useQuery({
        queryKey: dualQualityKeys.history(
            query ?? {
                from: "",
                to: "",
                dimension: "attribution_user",
                sort: "orderCount:desc",
                page: 1,
                pageSize: 20,
            },
        ),
        queryFn: () => fetchHistoryQuality(query!),
        enabled: Boolean(query?.from && query?.to),
    })
}

/** 口径内导出：版本绑定列表首个响应，服务端返回前重验授权。 */
export function useDualQualityExportMutation(caliber: QualityCaliber) {
    return useMutation({
        mutationFn: (input: {
            current?: CurrentQualityQuery
            history?: HistoryQualityQuery
        }) =>
            caliber === "current"
                ? exportCurrentQuality(input.current!)
                : exportHistoryQuality(input.history!),
    })
}
