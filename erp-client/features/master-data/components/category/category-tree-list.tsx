"use client"

import * as React from "react"
import { ChevronDownIcon, ChevronRightIcon, FolderIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import type { CategoryTreeNode } from "@/features/master-data/lib/category-tree-model"
import type { MasterDataListItem } from "@/features/master-data/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

function collectVisibleNodes(
    forest: readonly CategoryTreeNode[],
    expanded: ReadonlySet<string>,
): CategoryTreeNode[] {
    const visible: CategoryTreeNode[] = []
    const collect = (nodes: readonly CategoryTreeNode[]) => {
        for (const node of nodes) {
            visible.push(node)
            if (expanded.has(node.item.stableId)) collect(node.children)
        }
    }
    collect(forest)
    return visible
}

/** 字面量高亮，不把用户输入解释为正则。 */
export function CategoryName({ name, query }: { name: string; query: string }) {
    const index = query
        ? name.toLocaleLowerCase().indexOf(query.toLocaleLowerCase())
        : -1
    if (index < 0) return <>{name}</>
    return (
        <>
            {name.slice(0, index)}
            <mark className="rounded-sm bg-amber-100 text-inherit dark:bg-amber-900/50">
                {name.slice(index, index + query.length)}
            </mark>
            {name.slice(index + query.length)}
        </>
    )
}

/** 分类树保留完整路径、键盘导航及独立滚动位置。 */
export function CategoryTreeList({
    idPrefix,
    forest,
    expanded,
    selectedId,
    onToggle,
    onSelect,
    matchedIds,
    filterActive,
    query,
    onClearFilters,
    onOpenCreateRoot,
    canCreate,
}: {
    idPrefix: string
    forest: readonly CategoryTreeNode[]
    expanded: ReadonlySet<string>
    selectedId: string | null
    onToggle: (id: string) => void
    onSelect: (item: MasterDataListItem) => void
    matchedIds: ReadonlySet<string>
    filterActive: boolean
    query: string
    onClearFilters: () => void
    onOpenCreateRoot: () => void
    canCreate: boolean
}) {
    const scrollRef = React.useRef<HTMLDivElement>(null)
    const [focusedId, setFocusedId] = React.useState<string | null>(null)
    const visible = collectVisibleNodes(forest, expanded)
    const activeId = visible.some((node) => node.item.stableId === focusedId)
        ? focusedId
        : visible.some((node) => node.item.stableId === selectedId)
          ? selectedId
          : visible[0]?.item.stableId
    const focus = (id: string | undefined) => {
        if (id)
            document
                .getElementById(`${idPrefix}-row-${toAutomationIdSegment(id)}`)
                ?.focus()
    }
    React.useEffect(() => {
        const node = scrollRef.current
        if (!node) return
        try {
            node.scrollTo({
                top: Number(sessionStorage.getItem(`${idPrefix}-scroll`) ?? 0),
            })
        } catch {
            /* 使用默认位置。 */
        }
    }, [idPrefix])
    const renderNodes = (nodes: readonly CategoryTreeNode[]) =>
        nodes.map((node) => {
            const { item } = node
            const hasChildren = node.children.length > 0
            const isOpen = expanded.has(item.stableId)
            const contextOnly = filterActive && !matchedIds.has(item.stableId)
            return (
                <li key={item.stableId} role="none">
                    <div
                        id={`${idPrefix}-row-${toAutomationIdSegment(item.stableId)}`}
                        role="treeitem"
                        aria-label={`${item.name}${item.lifecycleStatus === "DISABLED" ? "，已停用" : ""}${contextOnly ? "，上级路径" : ""}`}
                        aria-expanded={hasChildren ? isOpen : undefined}
                        aria-selected={selectedId === item.stableId}
                        aria-level={node.depth + 1}
                        tabIndex={activeId === item.stableId ? 0 : -1}
                        title={node.pathLabel}
                        onFocus={() => setFocusedId(item.stableId)}
                        className={cn(
                            "group flex min-h-10 cursor-pointer items-center gap-2 rounded-lg py-2 pr-2 text-sm outline-none hover:bg-muted/70 focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset",
                            selectedId === item.stableId &&
                                "bg-muted font-semibold",
                            contextOnly && "text-muted-foreground",
                        )}
                        style={{ paddingLeft: `${8 + node.depth * 16}px` }}
                        onClick={() => onSelect(item)}
                        onKeyDown={(event) => {
                            const index = visible.findIndex(
                                (entry) =>
                                    entry.item.stableId === item.stableId,
                            )
                            switch (event.key) {
                                case "Enter":
                                case " ":
                                    event.preventDefault()
                                    onSelect(item)
                                    break
                                case "ArrowDown":
                                    event.preventDefault()
                                    focus(visible[index + 1]?.item.stableId)
                                    break
                                case "ArrowUp":
                                    event.preventDefault()
                                    focus(visible[index - 1]?.item.stableId)
                                    break
                                case "Home":
                                    event.preventDefault()
                                    focus(visible[0]?.item.stableId)
                                    break
                                case "End":
                                    event.preventDefault()
                                    focus(visible.at(-1)?.item.stableId)
                                    break
                                case "ArrowRight":
                                    event.preventDefault()
                                    if (hasChildren && !isOpen)
                                        onToggle(item.stableId)
                                    else if (hasChildren)
                                        focus(node.children[0]?.item.stableId)
                                    break
                                case "ArrowLeft":
                                    event.preventDefault()
                                    if (hasChildren && isOpen)
                                        onToggle(item.stableId)
                                    else focus(item.parentStableId)
                                    break
                            }
                        }}
                    >
                        {hasChildren ? (
                            <button
                                id={`${idPrefix}-row-${toAutomationIdSegment(item.stableId)}-toggle`}
                                type="button"
                                tabIndex={-1}
                                className="flex size-6 shrink-0 items-center justify-center rounded hover:bg-background"
                                aria-label={`${isOpen ? "收起" : "展开"}${item.name}`}
                                onClick={(event) => {
                                    event.stopPropagation()
                                    onToggle(item.stableId)
                                }}
                            >
                                {isOpen ? (
                                    <ChevronDownIcon className="size-3.5" />
                                ) : (
                                    <ChevronRightIcon className="size-3.5" />
                                )}
                            </button>
                        ) : (
                            <FolderIcon
                                className="mx-1 size-4 shrink-0 text-muted-foreground"
                                aria-hidden
                            />
                        )}
                        <span className="min-w-0 flex-1 truncate">
                            <CategoryName name={item.name} query={query} />
                        </span>
                        {item.lifecycleStatus === "DISABLED" ? (
                            <span className="shrink-0 text-xs text-muted-foreground">
                                停用
                            </span>
                        ) : null}
                        {hasChildren ? (
                            <span className="num text-xs text-muted-foreground">
                                {node.children.length}
                            </span>
                        ) : null}
                    </div>
                    {hasChildren && isOpen ? (
                        <ul role="group" className="list-none p-0">
                            {renderNodes(node.children)}
                        </ul>
                    ) : null}
                </li>
            )
        })
    return (
        <div
            ref={scrollRef}
            className="min-h-0 flex-1 overflow-y-auto overscroll-contain pb-3"
            onScroll={(event) => {
                try {
                    sessionStorage.setItem(
                        `${idPrefix}-scroll`,
                        String(event.currentTarget.scrollTop),
                    )
                } catch {
                    /* 不影响滚动。 */
                }
            }}
        >
            {forest.length ? (
                <ul role="tree" aria-label="商品分类" className="list-none p-0">
                    {renderNodes(forest)}
                </ul>
            ) : (
                <div className="space-y-3 px-2 py-10 text-center text-sm text-muted-foreground">
                    <p>{filterActive ? "没有匹配的分类" : "暂无分类"}</p>
                    {filterActive ? (
                        <Button
                            id={`${idPrefix}-reset`}
                            variant="outline"
                            size="sm"
                            onClick={onClearFilters}
                        >
                            清除筛选
                        </Button>
                    ) : canCreate ? (
                        <Button
                            id={`${idPrefix}-create`}
                            size="sm"
                            onClick={onOpenCreateRoot}
                        >
                            新建分类
                        </Button>
                    ) : null}
                </div>
            )}
        </div>
    )
}
