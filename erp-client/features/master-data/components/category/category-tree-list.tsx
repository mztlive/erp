"use client"

import {
    ChevronDownIcon,
    ChevronRightIcon,
    FolderTreeIcon,
    PlusIcon,
} from "lucide-react"

import { BusinessStatusBadge } from "@/components/business"
import { Button } from "@/components/ui/button"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import type { CategoryTreeNode } from "@/features/master-data/lib/category-tree-model"
import type { MasterDataListItem } from "@/features/master-data/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

function CategoryTreeRow({
    prefix,
    node,
    expanded,
    selectedId,
    onToggle,
    onSelect,
}: {
    prefix: string
    node: CategoryTreeNode
    expanded: ReadonlySet<string>
    selectedId: string | null
    onToggle: (id: string) => void
    onSelect: (item: MasterDataListItem) => void
}) {
    const hasChildren = node.children.length > 0
    const isOpen = expanded.has(node.item.stableId)
    const selected = selectedId === node.item.stableId
    const code =
        node.item.dictionaryCode ??
        node.item.keyFacts.find((f) => f.label === "分类代码")?.value ??
        "—"

    return (
        <li>
            <div
                id={`${prefix}-row-${toAutomationIdSegment(node.item.stableId)}`}
                role="treeitem"
                aria-expanded={hasChildren ? isOpen : undefined}
                aria-selected={selected}
                tabIndex={0}
                className={cn(
                    "group flex cursor-pointer items-center gap-1 rounded-md border border-transparent px-1.5 py-1.5 text-sm outline-none",
                    "hover:bg-muted/60 focus-visible:ring-2 focus-visible:ring-ring",
                    selected && "border-border bg-muted",
                )}
                style={{ paddingLeft: `${0.375 + node.depth * 1.1}rem` }}
                onClick={() => onSelect(node.item)}
                onKeyDown={(e) => {
                    if (e.key === "Enter" || e.key === " ") {
                        e.preventDefault()
                        onSelect(node.item)
                    }
                    if (e.key === "ArrowRight" && hasChildren && !isOpen) {
                        e.preventDefault()
                        onToggle(node.item.stableId)
                    }
                    if (e.key === "ArrowLeft" && hasChildren && isOpen) {
                        e.preventDefault()
                        onToggle(node.item.stableId)
                    }
                }}
            >
                {hasChildren ? (
                    <button
                        id={`${prefix}-row-${toAutomationIdSegment(node.item.stableId)}-toggle`}
                        type="button"
                        className="inline-flex size-6 shrink-0 items-center justify-center rounded-sm text-muted-foreground hover:bg-background"
                        aria-label={isOpen ? "收起" : "展开"}
                        onClick={(e) => {
                            e.stopPropagation()
                            onToggle(node.item.stableId)
                        }}
                    >
                        {isOpen ? (
                            <ChevronDownIcon className="size-3.5" />
                        ) : (
                            <ChevronRightIcon className="size-3.5" />
                        )}
                    </button>
                ) : (
                    <span className="inline-flex size-6 shrink-0" aria-hidden />
                )}
                <FolderTreeIcon
                    className="size-3.5 shrink-0 text-muted-foreground"
                    aria-hidden
                />
                <span className="min-w-0 flex-1 truncate font-medium">
                    {node.item.name}
                </span>
                <span className="num shrink-0 text-xs text-muted-foreground">
                    {code}
                </span>
                <span className="shrink-0 scale-90">
                    <BusinessStatusBadge
                        context="list"
                        label={node.item.lifecycleStatusLabel}
                        tone={node.item.lifecycleTone}
                    />
                </span>
            </div>
            {hasChildren && isOpen ? (
                <ul role="group" className="m-0 list-none p-0">
                    {node.children.map((child) => (
                        <CategoryTreeRow
                            key={child.item.stableId}
                            prefix={prefix}
                            node={child}
                            expanded={expanded}
                            selectedId={selectedId}
                            onToggle={onToggle}
                            onSelect={onSelect}
                        />
                    ))}
                </ul>
            ) : null}
        </li>
    )
}

/** 左侧树滚动区：树 / 筛选空态 / 首次新建空态。 */
export function CategoryTreeList({
    idPrefix,
    forest,
    expanded,
    selectedId,
    onToggle,
    onSelect,
    filterActive,
    onClearFilters,
    onOpenCreateRoot,
}: {
    idPrefix?: string
    forest: readonly CategoryTreeNode[]
    expanded: ReadonlySet<string>
    selectedId: string | null
    onToggle: (id: string) => void
    onSelect: (item: MasterDataListItem) => void
    filterActive: boolean
    onClearFilters: () => void
    onOpenCreateRoot: () => void
}) {
    const prefix = idPrefix ?? "master-data-category-tree-list"
    return (
        <div className="min-h-0 flex-1 overflow-y-auto p-2">
            {forest.length === 0 ? (
                filterActive ? (
                    <div className="flex flex-col items-center gap-3 py-12 text-center">
                        <p className="text-sm text-muted-foreground">
                            {masterDataCopy.categoryTreeNoMatch}
                        </p>
                        <p className="text-xs text-muted-foreground">
                            {masterDataCopy.categoryTreeNoMatchDesc}
                        </p>
                        <Button
                            id={`${prefix}-clear-filters`}
                            type="button"
                            size="sm"
                            variant="secondary"
                            className="rounded-lg shadow-none"
                            onClick={onClearFilters}
                        >
                            清除筛选
                        </Button>
                    </div>
                ) : (
                    <div className="flex flex-col items-center gap-3 py-12 text-center">
                        <p className="text-sm text-muted-foreground">
                            {masterDataCopy.categoryTreeEmpty}
                        </p>
                        <Button
                            id={`${prefix}-create-root`}
                            type="button"
                            size="sm"
                            onClick={onOpenCreateRoot}
                        >
                            <PlusIcon data-icon="inline-start" aria-hidden />
                            {masterDataCopy.categoryAddRoot}
                        </Button>
                    </div>
                )
            ) : (
                <ul role="tree" className="m-0 list-none p-0">
                    {forest.map((node) => (
                        <CategoryTreeRow
                            key={node.item.stableId}
                            prefix={prefix}
                            node={node}
                            expanded={expanded}
                            selectedId={selectedId}
                            onToggle={onToggle}
                            onSelect={onSelect}
                        />
                    ))}
                </ul>
            )}
        </div>
    )
}
