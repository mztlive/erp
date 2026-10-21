import { useState } from "react"
import { ChevronDownIcon, ChevronRightIcon, SearchIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { resourceLabel } from "@/lib/permission-catalog"
import { cn } from "@/lib/utils"
import type { PersonScopeList } from "../../api/person-data-scopes"
import { PersonalPolicyDetail } from "./personal-policy-detail"
import {
    personalPolicyView,
    type PersonalPolicyView,
} from "./personal-policy-view"

export function PersonalPolicyWorkspace({
    data,
    labels,
    canEdit,
    editing,
    onEdit,
}: {
    data: PersonScopeList
    labels: Map<string, string>
    canEdit: boolean
    editing: string | null
    onEdit: (resource: string) => void
}) {
    const [search, setSearch] = useState("")
    const [selected, setSelected] = useState<string | null>(editing)
    const views = data.businesses
        .filter(
            (business) =>
                business.resource === editing ||
                resourceLabel(business.resource).includes(search.trim()),
        )
        .map((business) => personalPolicyView(business, data, labels))
    const configurable = views.filter(
        (view) => view.business.configurable_actions.length > 0,
    )
    const automatic = views.filter(
        (view) => !view.business.configurable_actions.length,
    )
    const active =
        views.find(
            (view) => view.business.resource === (editing ?? selected),
        ) ??
        configurable[0] ??
        automatic[0]
    const activeAutomatic =
        active && !active.business.configurable_actions.length
    const renderChoices = (choices: PersonalPolicyView[]) => (
        <ul className="space-y-1">
            {choices.map((view) => {
                const { business, summary } = view
                const selected = active?.business.resource === business.resource
                return (
                    <li key={business.resource}>
                        <button
                            id={`person-scope-${toAutomationIdSegment(business.resource)}-select`}
                            type="button"
                            aria-pressed={selected}
                            aria-controls="person-scope-selected-detail"
                            disabled={Boolean(editing)}
                            onClick={() => setSelected(business.resource)}
                            className={cn(
                                "flex w-full cursor-pointer items-center gap-3 rounded-lg px-3 py-3 text-left transition-colors focus-visible:outline-2 focus-visible:outline-ring disabled:cursor-default disabled:opacity-60",
                                selected
                                    ? "bg-accent text-accent-foreground"
                                    : "hover:bg-muted/60",
                            )}
                        >
                            <span className="min-w-0 flex-1 space-y-1">
                                <span className="block font-medium">
                                    {resourceLabel(business.resource)}
                                </span>
                                <span className="block truncate text-xs text-muted-foreground">
                                    {summary}
                                </span>
                            </span>
                            {selected && (
                                <ChevronRightIcon
                                    className="size-4 shrink-0"
                                    aria-hidden
                                />
                            )}
                        </button>
                    </li>
                )
            })}
        </ul>
    )
    return (
        <div className="grid min-w-0 rounded-xl border border-grid lg:grid-cols-[16rem_minmax(0,1fr)]">
            <nav
                aria-label="选择业务范围"
                className="min-w-0 border-b border-grid p-3 lg:border-r lg:border-b-0"
            >
                <div className="mb-3 flex items-center justify-between px-1">
                    <h3 className="font-medium">选择业务</h3>
                    <span className="text-xs text-muted-foreground">
                        {search.trim() ? `${views.length} / ` : ""}
                        {data.businesses.length} 项
                    </span>
                </div>
                <div className="relative mb-3">
                    <SearchIcon
                        className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground"
                        aria-hidden
                    />
                    <Input
                        id="person-scope-search"
                        type="search"
                        aria-label="搜索业务"
                        placeholder="搜索业务，如销售单"
                        value={search}
                        disabled={Boolean(editing)}
                        onChange={(event) => setSearch(event.target.value)}
                        className="h-control-sm pl-9"
                    />
                </div>
                <div className="max-h-56 space-y-3 overflow-y-auto p-1 lg:max-h-[36rem]">
                    {renderChoices(configurable)}
                    {!!automatic.length && (
                        <Collapsible
                            key={search.trim()}
                            defaultOpen={
                                Boolean(search.trim()) ||
                                Boolean(activeAutomatic)
                            }
                            className={cn(
                                configurable.length > 0 &&
                                    "border-t border-grid pt-2",
                            )}
                        >
                            <CollapsibleTrigger
                                id="person-scope-group-rules"
                                className="group flex w-full cursor-pointer items-center gap-1 rounded-sm px-2 py-2 text-xs text-muted-foreground focus-visible:outline-2 focus-visible:outline-ring"
                            >
                                按业务规则访问 <span>{automatic.length}</span>
                                <ChevronDownIcon
                                    className="ml-auto size-3.5 transition-transform group-aria-expanded:rotate-180"
                                    aria-hidden
                                />
                            </CollapsibleTrigger>
                            <CollapsibleContent>
                                {renderChoices(automatic)}
                            </CollapsibleContent>
                        </Collapsible>
                    )}
                    {!views.length && (
                        <p className="px-2 py-4 text-xs text-muted-foreground">
                            没有匹配的业务。
                        </p>
                    )}
                </div>
            </nav>
            {active ? (
                <PersonalPolicyDetail
                    key={active.business.resource}
                    view={active}
                    canEdit={canEdit}
                    editing={editing}
                    onEdit={(resource) => {
                        setSelected(resource)
                        onEdit(resource)
                    }}
                />
            ) : (
                <div className="flex min-h-64 flex-col items-center justify-center gap-3 p-6 text-center">
                    <p className="text-muted-foreground">
                        没有找到“{search.trim()}”相关业务
                    </p>
                    <Button
                        id="person-scope-search-clear"
                        variant="outline"
                        size="sm"
                        onClick={() => setSearch("")}
                    >
                        清除搜索
                    </Button>
                </div>
            )}
        </div>
    )
}
