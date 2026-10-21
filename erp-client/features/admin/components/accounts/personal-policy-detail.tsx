import { SlidersHorizontalIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { actionLabel, resourceLabel } from "@/lib/permission-catalog"
import type { PersonalPolicyView } from "./personal-policy-view"

const summaryClassName =
    "cursor-pointer rounded-sm py-3 font-medium focus-visible:outline-2 focus-visible:outline-ring"

export function PersonalPolicyDetail({
    view,
    canEdit,
    editing,
    onEdit,
}: {
    view: PersonalPolicyView
    canEdit: boolean
    editing: string | null
    onEdit: (resource: string) => void
}) {
    const { business, category, groups, summary } = view
    const segment = toAutomationIdSegment(business.resource)
    const configurable = business.configurable_actions.length > 0
    const otherActions = business.actions.filter(
        (action) => !business.configurable_actions.includes(action),
    )
    const scopeTitle = !configurable
        ? "访问方式"
        : business.authorization_policy === "directory"
          ? "可查询、选择的范围"
          : business.authorization_policy === "governance"
            ? "可管理的范围"
            : "当前数据范围"
    return (
        <section
            id="person-scope-selected-detail"
            aria-labelledby={`person-scope-${segment}-title`}
            className="min-w-0 space-y-6 p-4 sm:p-6"
        >
            <div className="flex flex-wrap items-center justify-between gap-3">
                <div className="space-y-1">
                    <p className="text-xs text-muted-foreground">{category}</p>
                    <h3
                        id={`person-scope-${segment}-title`}
                        className="text-lg font-semibold"
                    >
                        {resourceLabel(business.resource)}
                    </h3>
                </div>
                {configurable && canEdit && (
                    <Button
                        id={`person-scope-${segment}-edit`}
                        size="sm"
                        disabled={Boolean(editing)}
                        onClick={() => onEdit(business.resource)}
                    >
                        <SlidersHorizontalIcon
                            data-icon="inline-start"
                            aria-hidden
                        />
                        配置范围
                    </Button>
                )}
            </div>
            <div className="space-y-3 rounded-lg bg-muted/40 p-5">
                <h4 className="text-xs text-muted-foreground">{scopeTitle}</h4>
                <p className="text-lg leading-relaxed font-medium wrap-anywhere">
                    {summary}
                </p>
                {groups.length > 1 && (
                    <ul className="divide-y divide-grid">
                        {groups.map((group) => (
                            <li
                                key={group.key}
                                className="space-y-1 py-3 first:pt-0 last:pb-0"
                            >
                                <p className="text-xs text-muted-foreground">
                                    {group.actions.map(actionLabel).join("、")}
                                </p>
                                <p className="leading-6 wrap-anywhere">
                                    {group.description}
                                </p>
                            </li>
                        ))}
                    </ul>
                )}
                {!configurable && (
                    <p className="text-sm leading-6 text-muted-foreground">
                        无需单独配置范围。{business.policy_description}
                    </p>
                )}
                {configurable &&
                    business.authorization_policy === "directory" && (
                        <p className="text-xs leading-5 text-muted-foreground">
                            仅控制候选项是否可见，不授予关联业务的数据或操作权限。
                        </p>
                    )}
                {configurable && otherActions.length > 0 && (
                    <p className="text-xs leading-5 text-muted-foreground">
                        此范围适用于 {business.configurable_actions.length}{" "}
                        项操作；另有 {otherActions.length} 项按业务规则校验。
                    </p>
                )}
            </div>
            <div className="divide-y divide-grid border-y border-grid">
                <details>
                    <summary
                        id={`person-scope-${segment}-actions`}
                        className={summaryClassName}
                    >
                        已有操作权限{" "}
                        <span className="ml-1 text-xs font-normal text-muted-foreground">
                            {business.actions.length} 项
                        </span>
                    </summary>
                    <div className="space-y-3 pb-4">
                        <ul className="grid gap-x-6 gap-y-2 sm:grid-cols-2">
                            {business.actions.map((action) => (
                                <li key={action}>{actionLabel(action)}</li>
                            ))}
                        </ul>
                        <p className="text-xs leading-5 text-muted-foreground">
                            操作权限由角色授予，执行时仍须符合数据范围及业务规则。
                        </p>
                    </div>
                </details>
                {configurable && (
                    <details>
                        <summary
                            id={`person-scope-${segment}-details`}
                            className={summaryClassName}
                        >
                            范围说明
                        </summary>
                        <div className="space-y-2 pb-4 text-sm leading-6 text-muted-foreground">
                            <p>{business.policy_description}</p>
                            {!!otherActions.length && (
                                <p>
                                    {otherActions.map(actionLabel).join("、")}
                                    ：按对应业务规则校验，不单独配置范围。
                                </p>
                            )}
                        </div>
                    </details>
                )}
            </div>
        </section>
    )
}
