"use client"

import * as React from "react"
import {
    ChevronRightIcon,
    FolderIcon,
    FolderOpenIcon,
    PlusIcon,
} from "lucide-react"

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
            <mark className="rounded-xs bg-highlight px-0.5 text-highlight-foreground">
                {name.slice(index, index + query.length)}
            </mark>
            {name.slice(index + query.length)}
        </>
    )
}

/** 分类树：固定缩进轨道 + 层级引导细线 + 飞书冰蓝选中态 + 完整键盘导航。 */
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
            const isSelected = selectedId === item.stableId
            const isDisabled = item.lifecycleStatus === "DISABLED"
            const contextOnly = filterActive && !matchedIds.has(item.stableId)
            const seg = toAutomationIdSegment(item.stableId)
            const indentPx = 6 + node.depth * 18
            const guideLeftPx = 15 + node.depth * 18

            return (
                <li key={item.stableId} role="none" className="my-0.5">
                    <div
                        id={`${idPrefix}-row-${seg}`}
                        role="treeitem"
                        aria-label={`${item.name}${isDisabled ? "，已停用" : ""}${contextOnly ? "，上级路径" : ""}`}
                        aria-expanded={hasChildren ? isOpen : undefined}
                        aria-selected={isSelected}
                        aria-level={node.depth + 1}
                        tabIndex={activeId === item.stableId ? 0 : -1}
                        title={node.pathLabel}
                        onFocus={() => setFocusedId(item.stableId)}
                        className={cn(
                            "group/tree relative flex min-h-8 cursor-pointer items-center gap-1.5 rounded-md py-1 pr-2 text-body-compact transition-colors duration-100 outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset",
                            isSelected
                                ? "bg-sidebar-accent font-medium text-sidebar-accent-foreground"
                                : "text-foreground/85 hover:bg-muted/80 hover:text-foreground",
                            contextOnly &&
                                !isSelected &&
                                "text-muted-foreground/80",
                            isDisabled &&
                                !isSelected &&
                                "text-muted-foreground",
                        )}
                        style={{ paddingLeft: `${indentPx}px` }}
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
                                id={`${idPrefix}-row-${seg}-toggle`}
                                type="button"
                                tabIndex={-1}
                                className={cn(
                                    "flex size-5 shrink-0 items-center justify-center rounded-sm transition-colors",
                                    isSelected
                                        ? "text-sidebar-accent-foreground/80 hover:bg-primary/10 hover:text-sidebar-accent-foreground"
                                        : "text-muted-foreground/75 hover:bg-foreground/8 hover:text-foreground",
                                )}
                                aria-label={`${isOpen ? "收起" : "展开"}${item.name}`}
                                onClick={(event) => {
                                    event.stopPropagation()
                                    onToggle(item.stableId)
                                }}
                            >
                                <ChevronRightIcon
                                    aria-hidden="true"
                                    className={cn(
                                        "size-3.5 transition-transform duration-150",
                                        isOpen && "rotate-90",
                                    )}
                                />
                            </button>
                        ) : (
                            <span
                                aria-hidden="true"
                                className="flex size-5 shrink-0 items-center justify-center"
                            />
                        )}
                        {hasChildren && isOpen ? (
                            <FolderOpenIcon
                                aria-hidden="true"
                                className={cn(
                                    "size-3.5 shrink-0",
                                    isSelected
                                        ? "text-primary"
                                        : "text-muted-foreground/75",
                                )}
                            />
                        ) : (
                            <FolderIcon
                                aria-hidden="true"
                                className={cn(
                                    "size-3.5 shrink-0",
                                    isSelected
                                        ? "text-primary"
                                        : hasChildren
                                          ? "text-muted-foreground/75"
                                          : "text-muted-foreground/55",
                                )}
                            />
                        )}
                        <span
                            className={cn(
                                "min-w-0 flex-1 truncate",
                                isDisabled &&
                                    "line-through decoration-muted-foreground/50",
                            )}
                        >
                            <CategoryName name={item.name} query={query} />
                        </span>
                        {isDisabled ? (
                            <span className="shrink-0 rounded-sm bg-neutral-soft px-1.5 py-0 text-tiny leading-4 font-medium text-neutral-soft-foreground">
                                停用
                            </span>
                        ) : null}
                        {hasChildren ? (
                            <span
                                className={cn(
                                    "num shrink-0 text-xs",
                                    isSelected
                                        ? "font-medium text-sidebar-accent-foreground/80"
                                        : "text-muted-foreground/65",
                                )}
                            >
                                {node.children.length}
                            </span>
                        ) : null}
                    </div>
                    {hasChildren && isOpen ? (
                        <ul role="group" className="relative list-none p-0">
                            <span
                                aria-hidden="true"
                                className="pointer-events-none absolute top-0 bottom-1.5 w-px bg-border/65"
                                style={{ left: `${guideLeftPx}px` }}
                            />
                            {renderNodes(node.children)}
                        </ul>
                    ) : null}
                </li>
            )
        })

    return (
        <div
            ref={scrollRef}
            className="min-h-0 flex-1 overflow-y-auto overscroll-contain px-2.5 pb-4"
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
                <div className="space-y-3 px-2 py-10 text-center">
                    <p className="text-body-compact text-muted-foreground">
                        {filterActive ? "没有匹配的分类" : "暂无分类"}
                    </p>
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
                            variant="secondary"
                            size="sm"
                            onClick={onOpenCreateRoot}
                        >
                            <PlusIcon aria-hidden="true" className="size-3.5" />
                            新建分类
                        </Button>
                    ) : null}
                </div>
            )}
        </div>
    )
}
