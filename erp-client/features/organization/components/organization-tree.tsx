"use client"

import { Building2Icon, UsersIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import { KIND_LABEL } from "@/features/organization/lib/labels"
import type { OrgTreeNode } from "@/features/organization/lib/tree"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

function TreeItems({
    nodes,
    depth,
    selectedId,
    onSelect,
}: {
    nodes: OrgTreeNode[]
    depth: number
    selectedId?: string
    onSelect: (id: string) => void
}) {
    return (
        <ul className="min-w-0 space-y-1">
            {nodes.map((node) => {
                const selected = node.unit.id === selectedId
                return (
                    <li key={node.unit.id} className="min-w-0">
                        <Button
                            id={`organization-tree-unit-${toAutomationIdSegment(node.unit.id)}`}
                            type="button"
                            variant={selected ? "secondary" : "ghost"}
                            className={cn(
                                "h-auto min-h-10 w-full min-w-0 justify-start gap-2 whitespace-normal rounded-md px-2 py-2 text-left text-[13px]",
                                selected
                                    ? "bg-primary/10 font-medium text-primary hover:bg-primary/15"
                                    : "text-muted-foreground hover:text-foreground",
                            )}
                            style={{ paddingLeft: 8 + Math.min(depth, 6) * 12 }}
                            aria-current={selected ? "page" : undefined}
                            onClick={() => onSelect(node.unit.id)}
                        >
                            {node.unit.kind === "department" ? (
                                <Building2Icon
                                    className="size-4 shrink-0"
                                    aria-hidden="true"
                                />
                            ) : (
                                <UsersIcon
                                    className="size-4 shrink-0"
                                    aria-hidden="true"
                                />
                            )}
                            <span className="min-w-0 flex-1 wrap-anywhere">
                                {node.unit.name}
                            </span>
                            <span className="shrink-0 text-xs text-muted-foreground">
                                {KIND_LABEL[node.unit.kind]}
                                {node.unit.enabled ? "" : " · 停用"}
                            </span>
                        </Button>
                        {node.children.length > 0 ? (
                            <TreeItems
                                nodes={node.children}
                                depth={depth + 1}
                                selectedId={selectedId}
                                onSelect={onSelect}
                            />
                        ) : null}
                    </li>
                )
            })}
        </ul>
    )
}

export function OrganizationTree({
    nodes,
    selectedId,
    onSelect,
}: {
    nodes: OrgTreeNode[]
    selectedId?: string
    onSelect: (id: string) => void
}) {
    return (
        <nav
            aria-label="组织树"
            className="min-h-0 min-w-0 flex-1 overflow-x-hidden overflow-y-auto overscroll-contain px-4 pb-4 lg:px-5 lg:pb-5"
        >
            <TreeItems
                nodes={nodes}
                depth={0}
                selectedId={selectedId}
                onSelect={onSelect}
            />
        </nav>
    )
}
