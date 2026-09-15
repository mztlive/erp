"use client"

import { BusinessEmptyState } from "@/components/business"
import { Button } from "@/components/ui/button"
import type { OrganizationEmptyReason } from "@/features/organization/types"

export function OrganizationEmptyByReason({
    reason,
    onClearFilters,
    idPrefix,
}: {
    reason: OrganizationEmptyReason
    onClearFilters?: () => void
    idPrefix: string
}) {
    switch (reason) {
        case "NO_MODULE_PERMISSION":
            return (
                <BusinessEmptyState
                    kind="no-scope"
                    title="无模块权限"
                    description="当前账号没有组织或范围配置的查看权限。这与无数据范围或范围内无记录不同。"
                />
            )
        case "NO_DATA_SCOPE":
            return (
                <BusinessEmptyState
                    kind="no-scope"
                    title="无数据范围"
                    description="你可以进入本页，但当前组织配置范围内没有任何可管理对象。请查看管理范围或申请授权——不是筛选过严。"
                />
            )
        case "FILTER_NO_RESULT":
            return (
                <BusinessEmptyState
                    kind="filter"
                    title="当前筛选无结果"
                    description="没有记录符合当前条件。可清除筛选后重试。"
                    action={
                        onClearFilters ? (
                            <Button
                                id={`${idPrefix}-empty-clear`}
                                type="button"
                                size="sm"
                                variant="secondary"
                                onClick={onClearFilters}
                            >
                                清除筛选
                            </Button>
                        ) : null
                    }
                />
            )
        default:
            return (
                <BusinessEmptyState
                    kind="no-data"
                    title="范围内无记录"
                    description="管理范围有效，但当前还没有可展示的组织或范围配置。"
                />
            )
    }
}
