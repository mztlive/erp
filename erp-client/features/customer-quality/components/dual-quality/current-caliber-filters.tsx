"use client"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { OptionCombobox } from "@/components/business/option-combobox"
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
                    <Input
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
                        className="min-w-0 flex-1"
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

            <div className="flex min-w-0 flex-wrap items-center gap-2 text-body-compact">
                <label
                    htmlFor="customers-quality-dual-dimension"
                    className="text-muted-foreground"
                >
                    分组
                </label>
                <OptionCombobox
                    id="customers-quality-dual-dimension"
                    value={dimension}
                    aria-label="分组"
                    allowClear={false}
                    onValueChange={(value) => {
                        if (value === null) return
                        patchDual({
                            dualDimension: value === "customer" ? null : value,
                            ownerGroup: null,
                            scopeVersion: null,
                            dualPage: null,
                        })
                    }}
                    options={[
                        { value: "customer", label: "按客户" },
                        { value: "owner_user", label: "按现任负责人" },
                        { value: "owner_org", label: "按现任组织" },
                    ]}
                    className="w-auto min-w-40"
                />
                <label
                    htmlFor="customers-quality-dual-sort"
                    className="text-muted-foreground"
                >
                    排序
                </label>
                <OptionCombobox
                    id="customers-quality-dual-sort"
                    value={sort}
                    aria-label="排序"
                    allowClear={false}
                    onValueChange={(value) => {
                        if (value === null) return
                        patchDual({
                            dualSort:
                                value === "orderCount:desc" ? null : value,
                            scopeVersion: null,
                            dualPage: null,
                        })
                    }}
                    options={CURRENT_SORT_OPTIONS}
                    className="w-auto min-w-40"
                />
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
                <div className="flex min-w-0 flex-wrap items-center gap-2 rounded-xl border border-border p-2 text-body-compact">
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
