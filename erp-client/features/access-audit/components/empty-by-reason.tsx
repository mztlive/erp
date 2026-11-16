"use client"

import { BusinessEmptyState } from "@/components/business"
import { Button } from "@/components/ui/button"
import type { AccessEmptyReason } from "@/features/access-audit/types"

function EmptyByReason({
    reason,
    isAudit = false,
    onClearFilters,
}: {
    reason: AccessEmptyReason
    /** 审计侧的空列表要强调「无记录不等于动作未发生」。 */
    isAudit?: boolean
    onClearFilters?: () => void
}) {
    switch (reason) {
        case "NO_MODULE_PERMISSION":
            return (
                <BusinessEmptyState
                    kind="no-scope"
                    title="暂无访问权限"
                    description="当前账号无权查看「角色与权限」，请联系系统管理员开通。"
                />
            )
        case "NO_DATA_SCOPE":
            return (
                <BusinessEmptyState
                    kind="no-scope"
                    title="暂无可管理的角色或人员"
                    description="当前账号尚未分配可管理的部门或角色范围，请联系系统管理员配置。"
                />
            )
        case "NO_RECORDS_IN_SCOPE":
            return isAudit ? (
                <BusinessEmptyState
                    kind="no-data"
                    title="暂无操作记录"
                    description="当前筛选条件下没有找到操作记录，可尝试放宽时间范围或调整筛选条件。"
                    action={
                        onClearFilters ? (
                            <Button
                                id={
                                    isAudit
                                        ? "operations-audit-empty-clear-filters"
                                        : "operations-access-empty-clear-filters"
                                }
                                type="button"
                                size="sm"
                                variant="secondary"
                                className="rounded-lg shadow-none"
                                onClick={onClearFilters}
                            >
                                清除筛选
                            </Button>
                        ) : null
                    }
                />
            ) : (
                <BusinessEmptyState
                    kind="no-data"
                    title="暂无可展示的记录"
                    description="当前视图下暂无记录，可清除筛选后重试，或新建配置。"
                />
            )
        case "FIELD_MASKED":
            return (
                <BusinessEmptyState
                    kind="no-data"
                    title="部分敏感信息已隐藏"
                    description="为保护业务数据安全，密钥、银行卡号等敏感信息已做脱敏处理。"
                />
            )
        case "FILTER_NO_RESULT":
        default:
            return (
                <BusinessEmptyState
                    kind="filter"
                    title="当前筛选无结果"
                    description={
                        isAudit
                            ? "当前筛选条件下没有找到操作记录，可尝试放宽时间范围或调整筛选条件。"
                            : "没有记录符合当前条件。可清除筛选后重试。"
                    }
                    action={
                        onClearFilters ? (
                            <Button
                                id={
                                    isAudit
                                        ? "operations-audit-filter-empty-clear"
                                        : "operations-access-filter-empty-clear"
                                }
                                type="button"
                                size="sm"
                                variant="secondary"
                                className="rounded-lg shadow-none"
                                onClick={onClearFilters}
                            >
                                清除筛选
                            </Button>
                        ) : null
                    }
                />
            )
    }
}

export { EmptyByReason }
