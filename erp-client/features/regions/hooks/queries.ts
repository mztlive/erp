"use client"

import { useQuery } from "@tanstack/react-query"

import { fetchRegions } from "../api"
import { buildRegionIndex } from "../lib/region-index"

export const regionKeys = {
    all: ["regions"] as const,
}

/** 行政区划随版本发布更新，会话内只读取一次。 */
export function useRegionIndex() {
    return useQuery({
        queryKey: regionKeys.all,
        queryFn: fetchRegions,
        staleTime: Number.POSITIVE_INFINITY,
        gcTime: Number.POSITIVE_INFINITY,
        select: buildRegionIndex,
    })
}
