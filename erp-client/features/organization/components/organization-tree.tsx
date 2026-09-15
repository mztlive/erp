"use client"

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
                                "h-auto min-h-9 w-full min-w-0 justify-start gap-2 whitespace-normal px-2 py-1.5 text-left text-[13px]",
                            )}
                            style={{ paddingLeft: 8 + depth * 16 }}
                            onClick={() => onSelect(node.unit.id)}
                        >
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
        <nav aria-label="组织树" className="min-w-0 overflow-x-hidden">
            <TreeItems
                nodes={nodes}
                depth={0}
                selectedId={selectedId}
                onSelect={onSelect}
            />
        </nav>
    )
}
