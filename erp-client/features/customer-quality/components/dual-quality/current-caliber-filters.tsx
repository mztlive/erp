"use client"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { OptionCombobox } from "@/components/business/option-combobox"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { personDirectoryLabel } from "@/features/entity-selectors/api/person-directory"
import { PersonDirectoryFilter } from "@/features/entity-selectors/components/person-directory-filter"
import { usePersonDirectorySelected } from "@/features/entity-selectors/hooks/person-directory"
import { OrganizationUnitFilter } from "@/features/organization/components/organization-unit-filter"
import { useOrganizationStateQuery } from "@/features/organization/hooks/queries"
import { hasPermission } from "@/lib/permissions"
import type { CurrentQualityRow } from "../../dual-types"
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
    items,
}: {
    filters: ReturnType<typeof useCurrentQualityFilters>
    patchDual: PatchDual
    items?: readonly CurrentQualityRow[]
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
    const isUserGroup = ownerGroup?.startsWith("user:") ?? false
    const isOrgGroup = ownerGroup?.startsWith("org:") ?? false
    const strippedGroupId = ownerGroup
        ? ownerGroup.replace(/^(user|org):/, "")
        : ""
    const matchedItem = ownerGroup
        ? items?.find(
              (row) =>
                  (isUserGroup &&
                      (row.groupId === strippedGroupId ||
                          row.ownerUserId === strippedGroupId)) ||
                  (isOrgGroup &&
                      (row.groupId === strippedGroupId ||
                          row.ownerOrgUnitId === strippedGroupId)) ||
                  row.groupId === strippedGroupId,
          )
        : undefined
    const itemLabel =
        matchedItem?.label ??
        (isUserGroup
            ? matchedItem?.ownerUserName
            : matchedItem?.ownerOrgUnitName)
    const account = useAccountProfileQuery()
    const needPersonLookup = Boolean(
        isUserGroup && strippedGroupId && !itemLabel,
    )
    const canReadSales = hasPermission(
        account.data?.permissions,
        "sales_person:list",
    )
    const selectedPeople = usePersonDirectorySelected(
        "sales",
        needPersonLookup ? [strippedGroupId] : [],
        needPersonLookup && canReadSales,
    )
    const needOrgLookup = Boolean(isOrgGroup && strippedGroupId && !itemLabel)
    const canReadOrgs = hasPermission(
        account.data?.permissions,
        "org_unit:list",
    )
    const orgState = useOrganizationStateQuery(needOrgLookup && canReadOrgs)
    const resolvedPerson = selectedPeople.data?.items.find(
        (item) => item.id === strippedGroupId,
    )
    const resolvedOrg = orgState.data?.units.find(
        (unit) => unit.id === strippedGroupId,
    )
    const ownerGroupLabel =
        itemLabel ||
        (resolvedPerson
            ? resolvedPerson.name.trim() || personDirectoryLabel(resolvedPerson)
            : undefined) ||
        resolvedOrg?.name ||
        strippedGroupId
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
                        当前分组明细：{ownerGroupLabel}
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
                        清除明细筛选
                    </Button>
                </div>
            ) : null}
        </>
    )
}
