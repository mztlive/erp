"use client"

import { ChevronDownIcon, ShieldAlertIcon } from "lucide-react"
import { z } from "zod"

import {
    BusinessEmptyState,
    surfacePanelClassName,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
} from "@/components/ui/collapsible"
import type {
    AccessEmptyReason,
    AccessGovernancePolicyView,
    AccessView,
} from "@/features/access-audit/types"
import { ACCESS_LAYER_HELP } from "@/features/access-audit/types"
import { formatDateTime } from "@/lib/datetime"

function parseView(raw: string | null): AccessView {
    // 字段策略无后端资源（backend_gap），入口隐藏；旧 URL 回退到 roles
    if (
        raw === "roles" ||
        raw === "users" ||
        raw === "scopes" ||
        raw === "audit"
    ) {
        return raw
    }
    return "roles"
}

function riskLabel(flag: string) {
    const map: Record<string, string> = {
        HIGH_PRIVILEGE: "高权限",
        EMPTY_SCOPE: "空数据范围",
        EXPIRING_SOON: "即将过期",
        ACCESS_ADMIN: "权限管理",
        PENDING_DISABLE: "待停用",
        REVOKED: "已撤权",
    }
    return map[flag] ?? flag
}

const changeReasonSchema = z.object({
    reasonCode: z.string().min(1, "请选择变更原因"),
    comment: z.string().trim().max(200),
})

function policyStatusLabel(state: "MISSING" | "CONFIGURED") {
    return state === "MISSING" ? "未配置" : "已配置"
}

function PolicyBanner({
    policies,
    view,
}: {
    policies: AccessGovernancePolicyView
    view: AccessView
}) {
    const time = policies.userRoleTimePolicy
    const field = policies.fieldPolicyGranularity
    const audit = policies.auditAccessPolicy
    const hasMissing =
        time.state === "MISSING" ||
        field.state === "MISSING" ||
        audit.state === "MISSING"

    const summaryItems: { key: string; label: string; missing: boolean }[] = []
    if (view === "users" || view === "roles") {
        summaryItems.push({
            key: "time",
            label: `角色时间 · ${policyStatusLabel(time.state)}`,
            missing: time.state === "MISSING",
        })
    }
    if (view === "fields" || view === "roles") {
        summaryItems.push({
            key: "field",
            label: `字段粒度 · ${policyStatusLabel(field.state)}`,
            missing: field.state === "MISSING",
        })
    }
    if (view === "audit" || view === "roles" || view === "users") {
        summaryItems.push({
            key: "audit",
            label: `审计导出 · ${policyStatusLabel(audit.state)}`,
            missing: audit.state === "MISSING",
        })
    }

    return (
        <Collapsible
            data-slot="policy-banner"
            className={surfacePanelClassName}
        >
            <CollapsibleTrigger className="group flex w-full items-center gap-2 px-3 py-2 text-left text-sm hover:bg-muted/40">
                <ShieldAlertIcon
                    className={
                        hasMissing
                            ? "size-4 shrink-0 text-warning"
                            : "size-4 shrink-0 text-muted-foreground"
                    }
                    aria-hidden="true"
                />
                <span className="min-w-0 flex-1">
                    <span className="font-medium text-foreground">
                        治理策略
                    </span>
                    <span className="ml-2 inline-flex flex-wrap items-center gap-1.5 align-middle">
                        {summaryItems.map((item) => (
                            <Badge
                                key={item.key}
                                variant={item.missing ? "warning" : "outline"}
                            >
                                {item.label}
                            </Badge>
                        ))}
                        <Badge variant="outline">本期无任务流</Badge>
                    </span>
                </span>
                <span className="shrink-0 text-xs text-muted-foreground group-aria-expanded:hidden">
                    详情
                </span>
                <ChevronDownIcon
                    aria-hidden="true"
                    className="size-4 shrink-0 text-muted-foreground transition-transform group-aria-expanded:rotate-180"
                />
            </CollapsibleTrigger>
            <CollapsibleContent className="border-t border-border/30 px-3 py-2 text-xs text-muted-foreground">
                <div className="grid gap-x-4 gap-y-1.5 sm:grid-cols-2">
                    {(view === "users" || view === "roles") && (
                        <p>
                            <strong className="text-foreground">
                                用户角色时间：
                            </strong>
                            {time.state === "MISSING" ? (
                                <>未配置 · 仅允许立即紧急撤权</>
                            ) : (
                                <>
                                    预约{" "}
                                    {time.schedulingAllowed ? "允许" : "禁用"} ·
                                    到期
                                    {time.expirationAllowed ? "允许" : "禁用"}
                                </>
                            )}
                        </p>
                    )}
                    {(view === "fields" || view === "roles") && (
                        <p>
                            <strong className="text-foreground">
                                字段粒度：
                            </strong>
                            {field.state === "MISSING" ? (
                                <>未配置 · 只读，不可自由输入字段名</>
                            ) : (
                                <>
                                    {field.editableTargets
                                        .map((t) => t.label)
                                        .join("、")}
                                </>
                            )}
                        </p>
                    )}
                    {(view === "audit" ||
                        view === "roles" ||
                        view === "users") && (
                        <p>
                            <strong className="text-foreground">
                                审计 / 导出：
                            </strong>
                            {audit.state === "MISSING" ? (
                                <>
                                    未配置 · 保守窗口{" "}
                                    {formatDateTime(audit.fallbackFrom, "full")}{" "}
                                    ~ {formatDateTime(audit.fallbackTo, "full")}
                                    ，导出禁用
                                </>
                            ) : (
                                <>
                                    最长可查{" "}
                                    {Math.round(
                                        audit.maxOnlineWindowSeconds / 3600,
                                    )}{" "}
                                    小时
                                </>
                            )}
                        </p>
                    )}
                    <p className="sm:col-span-2">
                        {ACCESS_LAYER_HELP.map((item) => item.title).join(
                            " · ",
                        )}
                        。命中复核要求的动作，在复核策略确定前将被阻断。
                    </p>
                </div>
            </CollapsibleContent>
        </Collapsible>
    )
}

function EmptyByReason({
    reason,
    onClearFilters,
}: {
    reason: AccessEmptyReason
    onClearFilters?: () => void
}) {
    switch (reason) {
        case "NO_MODULE_PERMISSION":
            return (
                <BusinessEmptyState
                    kind="no-scope"
                    title="无模块权限"
                    description="当前账号不能进入「权限与审计」。正常情况下导航入口应隐藏；这与「无数据范围」或「范围内无记录」不同。"
                />
            )
        case "NO_DATA_SCOPE":
            return (
                <BusinessEmptyState
                    kind="no-scope"
                    title="无数据范围"
                    description="你可以进入本页，但当前管理范围内没有任何可配置主体。请查看管理范围或申请授权——不是筛选过严。"
                />
            )
        case "NO_RECORDS_IN_SCOPE":
            return (
                <BusinessEmptyState
                    kind="no-data"
                    title="范围内无记录"
                    description="管理范围有效，但当前视图下没有可展示的记录。可调整时间范围、清除筛选，或（有权时）创建配置。"
                />
            )
        case "FIELD_MASKED":
            return (
                <BusinessEmptyState
                    kind="no-data"
                    title="字段级打码（非空列表）"
                    description="列表与标签保留，敏感值按字段策略打码显示。权限管理员不会因为能配置权限而自动看到业务敏感正文。"
                />
            )
        case "FILTER_NO_RESULT":
        default:
            return (
                <BusinessEmptyState
                    kind="filter"
                    title="当前筛选无结果"
                    description="没有记录符合当前条件。可清除筛选后重试。"
                    action={
                        onClearFilters ? (
                            <Button
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

export { changeReasonSchema, EmptyByReason, parseView, PolicyBanner, riskLabel }
