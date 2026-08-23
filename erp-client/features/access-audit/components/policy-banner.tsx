"use client"

import { ChevronDownIcon, ShieldAlertIcon } from "lucide-react"

import { surfaceInsetClassName } from "@/components/business"
import { cn } from "@/lib/utils"
import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
} from "@/components/ui/collapsible"
import type {
    AccessGovernancePolicyView,
    AccessView,
} from "@/features/access-audit/types"
import { formatDateTime } from "@/lib/datetime"

type MissingPolicy = {
    key: string
    label: string
    consequence: string
}

/**
 * 治理策略提示。
 *
 * 只在有策略缺失、且缺失会影响当前视图能做什么时出现；全部已配置时不占版面，
 * 避免常驻告警造成的警觉疲劳。
 */
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

    const missing: MissingPolicy[] = []
    if (time.state === "MISSING" && (view === "users" || view === "roles")) {
        missing.push({
            key: "time",
            label: "角色生效时间",
            consequence:
                "不能预约生效或到期时间，用户授权只能立即调整或紧急撤权。",
        })
    }
    if (field.state === "MISSING" && view === "roles") {
        missing.push({
            key: "field",
            label: "字段访问粒度",
            consequence: "字段级策略保持只读，不能按字段单独授权。",
        })
    }
    if (audit.state === "MISSING" && view === "audit") {
        missing.push({
            key: "audit",
            label: "审计查询与导出",
            consequence: `只能查询保守窗口（${formatDateTime(audit.fallbackFrom, "full")} ~ ${formatDateTime(audit.fallbackTo, "full")}），导出禁用。`,
        })
    }

    if (missing.length === 0) return null

    return (
        <Collapsible
            data-slot="policy-banner"
            className={cn(surfaceInsetClassName, "overflow-hidden")}
        >
            <CollapsibleTrigger className="group flex w-full items-center gap-2 px-3 py-2 text-left text-xs hover:bg-muted/40">
                <ShieldAlertIcon
                    className="size-3.5 shrink-0 text-warning"
                    aria-hidden="true"
                />
                <span className="min-w-0 flex-1 text-muted-foreground">
                    <span className="font-medium text-foreground">
                        {missing.length} 项治理策略未配置
                    </span>
                    {" · "}
                    {missing.map((item) => item.label).join("、")}
                    ，部分操作因此受限
                </span>
                <span className="shrink-0 text-muted-foreground group-aria-expanded:hidden">
                    详情
                </span>
                <ChevronDownIcon
                    aria-hidden="true"
                    className="size-3.5 shrink-0 text-muted-foreground transition-transform group-aria-expanded:rotate-180"
                />
            </CollapsibleTrigger>
            <CollapsibleContent className="border-t border-grid px-3 py-2 text-xs text-muted-foreground">
                <ul className="flex flex-col gap-1">
                    {missing.map((item) => (
                        <li key={item.key}>
                            <strong className="text-foreground">
                                {item.label}：
                            </strong>
                            {item.consequence}
                        </li>
                    ))}
                </ul>
            </CollapsibleContent>
        </Collapsible>
    )
}

export { PolicyBanner }
