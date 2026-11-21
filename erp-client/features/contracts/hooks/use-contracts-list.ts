"use client"

import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { useQuery } from "@tanstack/react-query"
import { selectedObjectDirectory } from "@/lib/object-directory"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "@/lib/navigation"
import type { PaginationState, SortingState } from "@tanstack/react-table"

import {
    contractMetricLabel,
    type ContractMetricFilter,
} from "@/features/contracts/lib/filter-contracts"
import { useContractsQuery } from "./queries"
import {
    contractsUrlCodec,
    type ContractsUrlState,
} from "@/features/contracts/lib/contracts-url-state"
import { useSearchHotkey } from "@/lib/use-search-hotkey"
import { useSearchDraft } from "@/lib/use-search-draft"

/** 可被单独移除的已生效条件。 */
type ContractFilterKey = "customerId" | "orgUnitIds"

type ContractAppliedChip = Readonly<{
    key: ContractFilterKey
    label: string
}>

/** 合同查询使用 URL 中已生效的条件，草稿由「查询」统一提交。 */
export function useContractsList() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)

    const account = useAccountProfileQuery()
    const permissions = account.data?.permissions
    const canReadList = hasPermission(permissions, "contract:list")
    const canFilterOwners =
        canReadList && hasPermission(permissions, "sales_person:list")
    const canFilterOrgs =
        canReadList && hasPermission(permissions, "org_unit:list")
    const allowedFilters = React.useCallback(
        (state: ContractsUrlState): ContractsUrlState => ({
            ...state,
            ownerUserIds: canFilterOwners ? state.ownerUserIds : undefined,
            orgUnitIds: canFilterOrgs ? state.orgUnitIds : undefined,
            includeDescendants:
                canFilterOrgs &&
                Boolean(state.orgUnitIds) &&
                state.includeDescendants,
        }),
        [canFilterOrgs, canFilterOwners],
    )
    const appliedQuery = searchParams.toString()
    const parsed = React.useMemo(
        () => contractsUrlCodec.parse(new URLSearchParams(appliedQuery)),
        [appliedQuery],
    )
    const hasUnavailableFilters =
        (!canFilterOwners && searchParams.has("ownerUserIds")) ||
        (!canFilterOrgs &&
            (searchParams.has("orgUnitIds") ||
                searchParams.has("includeDescendants")))
    const url = React.useMemo(
        () =>
            allowedFilters({
                ...parsed,
                page: hasUnavailableFilters ? 1 : parsed.page,
            }),
        [allowedFilters, hasUnavailableFilters, parsed],
    )
    React.useEffect(() => {
        if (!account.data || !hasUnavailableFilters) return
        const params = new URLSearchParams(appliedQuery)
        if (!canFilterOwners) params.delete("ownerUserIds")
        if (!canFilterOrgs) {
            params.delete("orgUnitIds")
            params.delete("includeDescendants")
        }
        params.delete("page")
        params.delete("scopeVersion")
        const qs = params.toString()
        router.replace(qs ? `${pathname}?${qs}` : pathname, { scroll: false })
    }, [
        account.data,
        appliedQuery,
        canFilterOrgs,
        canFilterOwners,
        hasUnavailableFilters,
        pathname,
        router,
    ])
    const {
        q,
        metric,
        page,
        pageSize,
        sort,
        dir,
        customerId,
        settlementPartyId,
        ownerUserIds,
        orgUnitIds,
        includeDescendants,
        upload,
    } = url

    const contractsQuery = useContractsQuery(url, canReadList)
    const pageRows = React.useMemo(
        () => (canReadList ? (contractsQuery.data?.items ?? []) : []),
        [canReadList, contractsQuery.data],
    )
    const total = canReadList ? (contractsQuery.data?.total ?? 0) : 0

    // Draft：本地受控，变化不请求
    const { searchDraft, setSearchDraft } = useSearchDraft(q ?? "", {
        inputRef: searchInputRef,
    })
    const [metricDraft, setMetricDraft] =
        React.useState<ContractMetricFilter>(metric)
    const [settlementPartyIdDraft, setSettlementPartyIdDraft] = React.useState<
        string | null
    >(settlementPartyId ?? null)
    const [ownerDraftValue, setOwnerDraft] = React.useState<string | null>(
        ownerUserIds ?? null,
    )
    const [orgDraftValue, setOrgDraft] = React.useState(orgUnitIds ?? "")
    const [descendantsDraftValue, setDescendantsDraft] =
        React.useState(includeDescendants)

    const ownerDraft = canFilterOwners ? ownerDraftValue : null
    const orgDraft = canFilterOrgs ? orgDraftValue : ""
    const descendantsDraft = canFilterOrgs && descendantsDraftValue

    // UI 态：条件由标签展示，深链不自动打开浮层。
    const [panelOpen, setPanelOpen] = React.useState(false)

    const pushUrl = React.useCallback(
        (patch: Partial<ContractsUrlState>) => {
            const next = allowedFilters({ ...url, ...patch })
            router.replace(`${pathname}${contractsUrlCodec.build(next)}`, {
                scroll: false,
            })
        },
        [allowedFilters, pathname, router, url],
    )

    /** 唯一提交路径：收起态 Enter 与主行「查询」共用。 */
    const applyFilters = React.useCallback(() => {
        pushUrl({
            q: searchDraft.trim() || undefined,
            metric: metricDraft,
            settlementPartyId: settlementPartyIdDraft ?? undefined,
            ownerUserIds: ownerDraft ?? undefined,
            orgUnitIds: orgDraft.trim() || undefined,
            includeDescendants: Boolean(orgDraft.trim()) && descendantsDraft,
            page: 1,
        })
        setPanelOpen(false)
    }, [
        descendantsDraft,
        metricDraft,
        orgDraft,
        ownerDraft,
        pushUrl,
        searchDraft,
        settlementPartyIdDraft,
    ])

    /** 只清「更多筛选」草稿；保留关键词、快捷筛选、客户锁定和当前结果。 */
    const resetMoreFilters = React.useCallback(() => {
        setOrgDraft("")
        setDescendantsDraft(false)
    }, [])

    /** 关闭面板时仅撤销组织草稿；保留外部常驻条件。 */
    const cancelMoreFilters = React.useCallback(() => {
        setOrgDraft(orgUnitIds ?? "")
        setDescendantsDraft(includeDescendants)
        setPanelOpen(false)
    }, [orgUnitIds, includeDescendants])

    const hasPendingChanges =
        searchDraft.trim() !== (q ?? "").trim() ||
        metricDraft !== metric ||
        settlementPartyIdDraft !== (settlementPartyId ?? null) ||
        ownerDraft !== (ownerUserIds ?? null) ||
        orgDraft.trim() !== (orgUnitIds ?? "").trim() ||
        descendantsDraft !== includeDescendants

    const removeFilter = React.useCallback(
        (key: ContractFilterKey) => {
            if (key === "customerId") {
                pushUrl({ customerId: undefined, page: 1 })
            } else if (key === "orgUnitIds") {
                setOrgDraft("")
                setDescendantsDraft(false)
                pushUrl({
                    orgUnitIds: undefined,
                    includeDescendants: false,
                    page: 1,
                })
            }
        },
        [pushUrl],
    )

    /** 清除全部筛选（含来源锁定与分页）；保留排序等视图/导航参数。 */
    const clearAllFilters = React.useCallback(() => {
        setSearchDraft("")
        setMetricDraft("all")
        setSettlementPartyIdDraft(null)
        setOwnerDraft(null)
        setPanelOpen(false)
        setOrgDraft("")
        setDescendantsDraft(false)
        pushUrl({
            q: undefined,
            metric: "all",
            customerId: undefined,
            settlementPartyId: undefined,
            ownerUserIds: undefined,
            orgUnitIds: undefined,
            includeDescendants: false,
            page: 1,
        })
    }, [pushUrl, setSearchDraft])

    React.useEffect(() => setMetricDraft(metric), [metric])
    React.useEffect(
        () => setSettlementPartyIdDraft(settlementPartyId ?? null),
        [settlementPartyId],
    )
    React.useEffect(() => setOwnerDraft(ownerUserIds ?? null), [ownerUserIds])
    React.useEffect(() => {
        setOrgDraft(orgUnitIds ?? "")
        setDescendantsDraft(includeDescendants)
        if (!canFilterOrgs) setPanelOpen(false)
    }, [canFilterOrgs, includeDescendants, orgUnitIds])

    useSearchHotkey(searchInputRef)

    const sorting = React.useMemo<SortingState>(
        () => (sort ? [{ id: sort, desc: dir === "desc" }] : []),
        [dir, sort],
    )

    const pagination = React.useMemo<PaginationState>(
        () => ({ pageIndex: Math.max(0, page - 1), pageSize }),
        [page, pageSize],
    )

    const customerName = pageRows.find(
        (row) => row.customer.customerId === customerId,
    )?.customer.displayName
    const lockedCustomerLabel =
        customerName && customerName !== customerId
            ? customerName
            : "已指定客户"
    const selectedSettlement = useQuery({
        queryKey: [
            "party-selector",
            "directory-label",
            settlementPartyId ?? "",
        ],
        queryFn: () =>
            selectedObjectDirectory(
                "settlement-parties",
                settlementPartyId ?? "",
            ),
        enabled: canReadList && Boolean(settlementPartyId),
        staleTime: 0,
    })
    const selectedSettlementPartyLabel =
        selectedSettlement.isError || selectedSettlement.isFetching
            ? "不可用"
            : (selectedSettlement.data?.items.find(
                  (item) => item.id === settlementPartyId,
              )?.name ?? "未知")

    const appliedChips: ContractAppliedChip[] = []
    if (customerId)
        appliedChips.push({
            key: "customerId",
            label: `客户：${lockedCustomerLabel}`,
        })
    if (orgUnitIds) {
        appliedChips.push({
            key: "orgUnitIds",
            label: `组织：已选 ${orgUnitIds.split(",").length} 个${includeDescendants ? "（含下级）" : ""}`,
        })
    }

    const filterSnapshotLabel = React.useMemo(() => {
        const parts = [
            `指标=${contractMetricLabel(metric)}`,
            (q ?? "").trim() ? `搜索=${(q ?? "").trim()}` : "搜索=空",
            customerId ? `客户=${lockedCustomerLabel}` : null,
            settlementPartyId
                ? `结算主体=${selectedSettlementPartyLabel}`
                : null,
            ownerUserIds
                ? `当前跟进负责人：已选 ${ownerUserIds.split(",").length} 人`
                : null,
            orgUnitIds
                ? `组织：已选 ${orgUnitIds.split(",").length} 个${includeDescendants ? "（含下级）" : ""}`
                : null,
        ].filter(Boolean)
        return parts.join(" · ")
    }, [
        customerId,
        includeDescendants,
        lockedCustomerLabel,
        metric,
        orgUnitIds,
        ownerUserIds,
        q,
        selectedSettlementPartyLabel,
        settlementPartyId,
    ])

    const handlePaginationChange = React.useCallback(
        (next: PaginationState) => {
            pushUrl({ page: next.pageIndex + 1, pageSize: next.pageSize })
        },
        [pushUrl],
    )

    const handleSortingChange = React.useCallback(
        (next: SortingState) => {
            const head = next[0]
            pushUrl({
                sort: head?.id,
                dir: head ? (head.desc ? "desc" : "asc") : undefined,
                page: 1,
            })
        },
        [pushUrl],
    )

    const isFiltered =
        (q ?? "").trim() !== "" ||
        metric !== "all" ||
        Boolean(customerId) ||
        Boolean(settlementPartyId) ||
        Boolean(ownerUserIds) ||
        Boolean(orgUnitIds)

    return {
        canReadList,
        canFilterOwners,
        canFilterOrgs,
        url,
        contractsQuery,
        total,
        customerId,
        orgDraft,
        setOrgDraft,
        descendantsDraft,
        setDescendantsDraft,
        upload,
        searchDraft,
        setSearchDraft,
        searchInputRef,
        metricDraft,
        setMetricDraft,
        settlementPartyIdDraft,
        setSettlementPartyIdDraft,
        ownerDraft,
        setOwnerDraft,
        panelOpen,
        setPanelOpen,
        sorting,
        pagination,
        pageRows,
        appliedChips,
        filterSnapshotLabel,
        isFiltered,
        applyFilters,
        resetMoreFilters,
        cancelMoreFilters,
        hasPendingChanges,
        removeFilter,
        handleSortingChange,
        handlePaginationChange,
        clearAllFilters,
    }
}
