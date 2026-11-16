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
                    title="暂无访问权限"
                    description="当前账号无权查看「组织与范围配置」，请联系系统管理员开通。"
                />
            )
        case "NO_DATA_SCOPE":
            return (
                <BusinessEmptyState
                    kind="no-scope"
                    title="暂无可管理的部门范围"
                    description="当前账号尚未分配可管理的部门范围，请联系系统管理员配置。"
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
                    title="暂无可展示的记录"
                    description="当前还没有可展示的部门或范围配置。"
                />
            )
    }
}
