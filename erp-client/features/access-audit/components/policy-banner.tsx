"use client"

import { ChevronDownIcon, ShieldAlertIcon } from "lucide-react"

import { surfaceInsetClassName } from "@/components/business"
import { cn } from "@/lib/utils"
import { Badge } from "@/components/ui/badge"
import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
} from "@/components/ui/collapsible"
import type {
    AccessGovernancePolicyView,
    AccessView,
} from "@/features/access-audit/types"
import { ACCESS_LAYER_HELP } from "@/features/access-audit/types"
import { formatDateTime } from "@/lib/datetime"

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

    const summaryItems: { key: string; label: string; missing: boolean }[] = [
        {
            key: "time",
            label: `角色时间 · ${policyStatusLabel(time.state)}`,
            missing: time.state === "MISSING",
        },
        {
            key: "field",
            label: `字段粒度 · ${policyStatusLabel(field.state)}`,
            missing: field.state === "MISSING",
        },
        {
            key: "audit",
            label: `审计导出 · ${policyStatusLabel(audit.state)}`,
            missing: audit.state === "MISSING",
        },
    ]

    return (
        <Collapsible
            data-slot="policy-banner"
            className={cn(surfaceInsetClassName, "overflow-hidden")}
        >
            <CollapsibleTrigger className="group flex w-full items-center gap-2 px-3 py-2 text-left text-xs hover:bg-muted/40">
                <ShieldAlertIcon
                    className={
                        hasMissing
                            ? "size-3.5 shrink-0 text-warning"
                            : "size-3.5 shrink-0 text-muted-foreground"
                    }
                    aria-hidden="true"
                />
                <span className="font-medium text-foreground">治理策略</span>
                <span className="min-w-0 flex-1">
                    <span className="inline-flex flex-wrap items-center gap-1.5 align-middle">
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
                    className="size-3.5 shrink-0 text-muted-foreground transition-transform group-aria-expanded:rotate-180"
                />
            </CollapsibleTrigger>
            <CollapsibleContent className="border-t border-grid px-3 py-2 text-xs text-muted-foreground">
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

export { PolicyBanner }
