"use client"

import { Button } from "@/components/ui/button"
import { PersonDirectoryFilter } from "@/features/entity-selectors/components/person-directory-filter"
import { OrganizationUnitFilter } from "@/features/organization/components/organization-unit-filter"
import type { useCurrentQualityFilters } from "../../hooks/use-current-quality-filters"
import type { PatchDual } from "../../lib/dual-filter-state"

const CURRENT_SORT_OPTIONS = [
    { value: "orderCount:desc", label: "订单数降序" },
    { value: "orderCount:asc", label: "订单数升序" },
    { value: "grossTotal:desc", label: "含税总额降序" },
    { value: "grossTotal:asc", label: "含税总额升序" },
    { value: "customerNo:asc", label: "客户编号升序" },
    { value: "label:asc", label: "分组名称升序" },
    { value: "customerCount:desc", label: "客户数降序" },
]

export function CurrentCaliberFilters({
    filters,
    patchDual,
}: {
    filters: ReturnType<typeof useCurrentQualityFilters>
    patchDual: PatchDual
}) {
    const {
        ownerIds,
        orgIds,
        includeDescendants,
        ownerGroup,
        dimension,
        sort,
        qDraft,
        setQDraft,
        hasFilters,
        applySearch,
        applyOwners,
        applyOrgs,
        applyDescendants,
    } = filters
    return (
        <>
            <div className="grid min-w-0 grid-cols-1 gap-2 sm:grid-cols-2">
                <div className="flex min-w-0 gap-2">
                    <label
                        htmlFor="customers-quality-dual-search"
                        className="sr-only"
                    >
                        搜索客户或单号
                    </label>
                    <input
                        id="customers-quality-dual-search"
                        type="search"
                        value={qDraft}
                        onChange={(e) => setQDraft(e.target.value)}
                        onKeyDown={(e) => {
                            if (e.key === "Enter") {
                                e.preventDefault()
                                applySearch()
                            }
                        }}
                        placeholder="客户编号 / 名称 / 单号"
                        className="h-9 min-w-0 flex-1 rounded-lg border border-border bg-background px-3 text-sm"
                    />
                    <Button
                        id="customers-quality-dual-apply"
                        type="button"
                        size="sm"
                        onClick={applySearch}
                    >
                        查询
                    </Button>
                </div>
                <PersonDirectoryFilter
                    id="customers-quality-dual-owner"
                    category="sales"
                    label="现任负责人"
                    value={ownerIds.join(",")}
                    onChange={applyOwners}
                    orgUnitIds={orgIds}
                    includeDescendants={
                        orgIds.length > 0 && includeDescendants === true
                    }
                />
            </div>
            <OrganizationUnitFilter
                id="customers-quality-dual-org"
                label="现任组织"
                value={orgIds.join(",")}
                onChange={applyOrgs}
                includeDescendants={includeDescendants === true}
                onDescendantsChange={applyDescendants}
            />

            <div className="flex min-w-0 flex-wrap items-center gap-2 text-[13px]">
                <label
                    htmlFor="customers-quality-dual-dimension"
                    className="text-muted-foreground"
                >
                    分组
                </label>
                <select
                    id="customers-quality-dual-dimension"
                    value={dimension}
                    onChange={(e) =>
                        patchDual({
                            dualDimension:
                                e.target.value === "customer"
                                    ? null
                                    : e.target.value,
                            ownerGroup: null,
                            scopeVersion: null,
                            dualPage: null,
                        })
                    }
                    className="h-9 min-w-0 rounded-lg border border-border bg-background px-2"
                >
                    <option value="customer">按客户</option>
                    <option value="owner_user">按现任负责人</option>
                    <option value="owner_org">按现任组织</option>
                </select>
                <label
                    htmlFor="customers-quality-dual-sort"
                    className="text-muted-foreground"
                >
                    排序
                </label>
                <select
                    id="customers-quality-dual-sort"
                    value={sort}
                    onChange={(e) =>
                        patchDual({
                            dualSort:
                                e.target.value === "orderCount:desc"
                                    ? null
                                    : e.target.value,
                            scopeVersion: null,
                            dualPage: null,
                        })
                    }
                    className="h-9 min-w-0 rounded-lg border border-border bg-background px-2"
                >
                    {CURRENT_SORT_OPTIONS.map((o) => (
                        <option key={o.value} value={o.value}>
                            {o.label}
                        </option>
                    ))}
                </select>
                {hasFilters ? (
                    <Button
                        id="customers-quality-dual-clear"
                        type="button"
                        size="sm"
                        variant="ghost"
                        onClick={() =>
                            patchDual({
                                ownerUserIds: null,
                                orgUnitIds: null,
                                includeDescendants: null,
                                ownerGroup: null,
                                dualCustomerId: null,
                                dualQ: null,
                                scopeVersion: null,
                                dualPage: null,
                            })
                        }
                    >
                        清除筛选
                    </Button>
                ) : null}
            </div>

            {ownerGroup ? (
                <div className="flex min-w-0 flex-wrap items-center gap-2 rounded-xl border border-border p-2 text-[13px]">
                    <span className="min-w-0 truncate">
                        现任分组下钻：{ownerGroup}
                    </span>
                    <Button
                        id="customers-quality-dual-drill-clear"
                        type="button"
                        size="sm"
                        variant="ghost"
                        onClick={() =>
                            patchDual({
                                ownerGroup: null,
                                scopeVersion: null,
                                dualPage: null,
                            })
                        }
                    >
                        清除下钻
                    </Button>
                </div>
            ) : null}
        </>
    )
}
