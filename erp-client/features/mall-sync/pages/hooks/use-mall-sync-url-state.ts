"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"

import type {
    MallSyncViewName,
    MappingTaskBase,
} from "@/features/mall-sync/types"
import {
    ALL_OBJECT_PARAMS,
    parseView,
    VIEW_OBJECT_PARAMS,
} from "@/features/mall-sync/lib/presentation"
import { patchUrl as patchSearchParams } from "@/lib/patch-search-params"

export type PatchUrl = (
    patch: Record<string, string | null | undefined>,
    options?: { replace?: boolean },
) => void

/** 可被单独移除的已生效条件（与 URL 参数一一对应）。 */
export type MallSyncFilterKey =
    | "q"
    | "mappingType"
    | "jobId"
    | "snapshotId"
    | "mappingTaskId"
    | "workItemId"
    | "differenceId"

export type MallSyncAppliedChip = Readonly<{
    key: MallSyncFilterKey
    label: string
}>

export type MallSyncMappingTypeDraft = MappingTaskBase["mappingType"] | "all"

const MAPPING_TYPE_VALUES: readonly MappingTaskBase["mappingType"][] = [
    "CUSTOMER",
    "CONTRACT",
    "SETTLEMENT_PARTY",
    "VOUCHER_CATEGORY",
    "UNIQUE_LINE",
    "AMOUNT_FORMAT",
]

export type MallSyncUrlState = {
    view: MallSyncViewName
    q: string
    mappingType?: MappingTaskBase["mappingType"]
    jobId?: string
    snapshotId?: string
    mappingTaskId?: string
    workItemId?: string
    differenceId?: string
    queueContextId: string
    searchDraft: string
    setSearchDraft: (value: string) => void
    searchInputRef: React.RefObject<HTMLInputElement | null>
    patchUrl: PatchUrl
    clearObjectParamsForView: (next: MallSyncViewName) => Record<string, null>
    hasActiveFilters: boolean
    hasStructuredFilters: boolean
    panelOpen: boolean
    setPanelOpen: React.Dispatch<React.SetStateAction<boolean>>
    mappingTypeDraft: MallSyncMappingTypeDraft
    setMappingTypeDraft: React.Dispatch<
        React.SetStateAction<MallSyncMappingTypeDraft>
    >
    applyFilters: () => void
    resetMoreFilters: () => void
    removeFilter: (key: MallSyncFilterKey) => void
    clearAllFilters: () => void
    searchParams: ReturnType<typeof useSearchParams>
}

/**
 * 商城同步筛选状态：Applied 在 URL（唯一事实源），Draft 与面板展开态在本地。
 * 关键词与结构化条件经同一个 `applyFilters` 一次性提交；Draft 变化不触发请求。
 */
export function useMallSyncUrlState(): MallSyncUrlState {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const view = parseView(searchParams.get("view"))
    const q = searchParams.get("q") ?? ""
    // 非法枚举值在解析时降级为默认（undefined），不继续传给接口
    const mappingType = MAPPING_TYPE_VALUES.find(
        (value) => value === searchParams.get("mappingType"),
    )
    const jobId = searchParams.get("jobId") ?? undefined
    const snapshotId = searchParams.get("snapshotId") ?? undefined
    const mappingTaskId = searchParams.get("mappingTaskId") ?? undefined
    const workItemId =
        searchParams.get("workItemId") ??
        searchParams.get("currentWorkItemId") ??
        undefined
    const differenceId = searchParams.get("differenceId") ?? undefined
    const queueContextId =
        searchParams.get("queueContextId") ?? "queue:W17:mall-sync"
    const hasStructuredFilters = mappingType != null
    const hasActiveFilters = Boolean(
        q ||
        mappingType ||
        jobId ||
        snapshotId ||
        mappingTaskId ||
        workItemId ||
        differenceId,
    )

    const [searchDraft, setSearchDraft] = React.useState(q)
    const [mappingTypeDraft, setMappingTypeDraft] =
        React.useState<MallSyncMappingTypeDraft>(mappingType ?? "all")
    // 有结构化条件的初始深链展开面板；URL 回填不得再次强制展开
    const [panelOpen, setPanelOpen] = React.useState(hasStructuredFilters)
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)

    // URL 回填只同步 Draft；输入框处于编辑状态时保护未提交的关键词
    React.useEffect(() => {
        if (document.activeElement !== searchInputRef.current) {
            setSearchDraft(q)
        }
        setMappingTypeDraft(mappingType ?? "all")
    }, [mappingType, q, searchInputRef])

    // / 聚焦搜索；忽略输入框、文本域、弹层与抽屉场景
    React.useEffect(() => {
        const onKey = (event: KeyboardEvent) => {
            if (
                event.key !== "/" ||
                event.metaKey ||
                event.ctrlKey ||
                event.altKey
            ) {
                return
            }
            const target = event.target as HTMLElement | null
            const tag = target?.tagName
            if (
                tag === "INPUT" ||
                tag === "TEXTAREA" ||
                tag === "SELECT" ||
                target?.isContentEditable
            ) {
                return
            }
            if (
                document.querySelector('[role="dialog"], [data-slot="sheet"]')
            ) {
                return
            }
            event.preventDefault()
            searchInputRef.current?.focus()
        }
        window.addEventListener("keydown", onKey)
        return () => window.removeEventListener("keydown", onKey)
    }, [])

    function patchUrl(
        patch: Record<string, string | null | undefined>,
        options?: { replace?: boolean; scroll?: boolean },
    ) {
        patchSearchParams(
            { router, pathname, searchParams, view },
            patch,
            options,
        )
    }

    const clearObjectParamsForView = React.useCallback(
        (next: MallSyncViewName) => {
            const keep = new Set(VIEW_OBJECT_PARAMS[next])
            const patch: Record<string, null> = {}
            for (const key of ALL_OBJECT_PARAMS) {
                if (!keep.has(key)) patch[key] = null
            }
            return patch
        },
        [],
    )

    /** 收起态 Enter / 提交箭头与展开态「应用全部筛选」共用同一条提交路径。 */
    const applyFilters = React.useCallback(() => {
        patchUrl(
            {
                q: searchDraft.trim() || null,
                mappingType:
                    mappingTypeDraft === "all" ? null : mappingTypeDraft,
            },
            { replace: true, scroll: false },
        )
        setPanelOpen(false)
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [mappingTypeDraft, patchUrl, searchDraft])

    /** 仅清除「更多筛选」结构化条件；保留关键词与对象定位条件，面板保持展开。 */
    const resetMoreFilters = React.useCallback(() => {
        setMappingTypeDraft("all")
        patchUrl({ mappingType: null }, { replace: true, scroll: false })
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [patchUrl])

    /** 移除单个已生效条件；映射任务与待办作为一对一起移除。 */
    const removeFilter = React.useCallback(
        (key: MallSyncFilterKey) => {
            if (key === "q") setSearchDraft("")
            if (key === "mappingType") setMappingTypeDraft("all")
            if (key === "mappingTaskId") {
                patchUrl(
                    {
                        mappingTaskId: null,
                        workItemId: null,
                        currentWorkItemId: null,
                    },
                    { replace: true, scroll: false },
                )
                return
            }
            if (key === "workItemId") {
                patchUrl(
                    {
                        workItemId: null,
                        currentWorkItemId: null,
                    },
                    { replace: true, scroll: false },
                )
                return
            }
            patchUrl({ [key]: null }, { replace: true, scroll: false })
        },
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [patchUrl, setSearchDraft],
    )

    /** 同时重置草稿、面板、URL 筛选参数与对象定位参数；保留视图与队列上下文。 */
    const clearAllFilters = React.useCallback(() => {
        setSearchDraft("")
        setMappingTypeDraft("all")
        setPanelOpen(false)
        patchUrl(
            {
                q: null,
                mappingType: null,
                jobId: null,
                snapshotId: null,
                mappingTaskId: null,
                workItemId: null,
                currentWorkItemId: null,
                differenceId: null,
            },
            { replace: true, scroll: false },
        )
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [patchUrl, setSearchDraft])

    return {
        view,
        q,
        mappingType,
        jobId,
        snapshotId,
        mappingTaskId,
        workItemId,
        differenceId,
        queueContextId,
        searchDraft,
        setSearchDraft,
        searchInputRef,
        patchUrl,
        clearObjectParamsForView,
        hasActiveFilters,
        hasStructuredFilters,
        panelOpen,
        setPanelOpen,
        mappingTypeDraft,
        setMappingTypeDraft,
        applyFilters,
        resetMoreFilters,
        removeFilter,
        clearAllFilters,
        searchParams,
    }
}
