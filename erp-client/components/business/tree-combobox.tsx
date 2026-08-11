"use client"

import * as React from "react"
import { ChevronRightIcon } from "lucide-react"

import {
    Combobox,
    ComboboxContent,
    ComboboxEmpty,
    ComboboxInput,
    ComboboxItem,
    ComboboxList,
} from "@/components/ui/combobox"
import { cn } from "@/lib/utils"

// ---------------------------------------------------------------------------
// 树形下拉选择
// ---------------------------------------------------------------------------

export type TreeComboboxNode = Readonly<{
    id: string
    label: string
    /** 稳定编号（如分类代码）；行尾次要展示。 */
    code?: string
    /** 为 true 时不在下拉中展示（例如选上级时排除自身与子树）。 */
    disabled?: boolean
    children: readonly TreeComboboxNode[]
}>

export type TreeComboboxProps = {
    /** 森林（多根）；children 递归形成层级。 */
    nodes: readonly TreeComboboxNode[]
    value?: string
    onValueChange: (id?: string) => void
    onSearchChange?: (query: string) => void
    /** 服务端已完成搜索时关闭本地二次过滤。 */
    filterMode?: "local" | "remote"
    label: string
    placeholder?: string
    emptyLabel?: string
    loading?: boolean
    disabled?: boolean
    required?: boolean
    className?: string
    /** 初始展开的节点 ID；缺省时全部展开。 */
    defaultExpandedIds?: readonly string[]
    onOpenChange?: (open: boolean) => void
}

type FlattenedEntry = Readonly<{
    node: TreeComboboxNode
    depth: number
}>

/** 全量前序展平（忽略展开状态），用于搜索命中与选中回显；disabled 节点及其子树不进入。 */
function flattenAll(nodes: readonly TreeComboboxNode[]): FlattenedEntry[] {
    const out: FlattenedEntry[] = []
    const walk = (list: readonly TreeComboboxNode[], depth: number) => {
        for (const node of list) {
            if (node.disabled) continue
            out.push({ node, depth })
            if (node.children.length > 0) walk(node.children, depth + 1)
        }
    }
    walk(nodes, 0)
    return out
}

/** 按展开状态展平（收起分支不进入列表）；disabled 节点及其子树不进入。 */
function flattenVisible(
    nodes: readonly TreeComboboxNode[],
    expanded: ReadonlySet<string>,
): FlattenedEntry[] {
    const out: FlattenedEntry[] = []
    const walk = (list: readonly TreeComboboxNode[], depth: number) => {
        for (const node of list) {
            if (node.disabled) continue
            out.push({ node, depth })
            if (node.children.length > 0 && expanded.has(node.id)) {
                walk(node.children, depth + 1)
            }
        }
    }
    walk(nodes, 0)
    return out
}

/** 有子节点的节点 ID；缺省展开态 = 全部展开。 */
function collectParentIds(nodes: readonly TreeComboboxNode[]): Set<string> {
    const ids = new Set<string>()
    const walk = (list: readonly TreeComboboxNode[]) => {
        for (const node of list) {
            if (node.children.length > 0) {
                ids.add(node.id)
                walk(node.children)
            }
        }
    }
    walk(nodes)
    return ids
}

/** 某节点到根的祖先 ID 列表（不含自身）；未找到时为空。 */
function collectAncestorIds(
    nodes: readonly TreeComboboxNode[],
    targetId: string,
): string[] {
    const ancestors: string[] = []
    const walk = (
        list: readonly TreeComboboxNode[],
        path: readonly string[],
    ): boolean => {
        for (const node of list) {
            if (node.id === targetId) {
                ancestors.push(...path)
                return true
            }
            if (walk(node.children, [...path, node.id])) return true
        }
        return false
    }
    walk(nodes, [])
    return ancestors
}

/**
 * 树形下拉选择：层级展开/收起；搜索时在整棵树中命中并平铺。
 * 仅渲染可见节点，键盘导航与 Base UI 列表一致。
 */
export function TreeCombobox({
    nodes,
    value,
    onValueChange,
    onSearchChange,
    filterMode = "local",
    label,
    placeholder = "搜索名称或编号",
    emptyLabel = "没有符合条件的对象",
    loading = false,
    disabled = false,
    required = false,
    className,
    defaultExpandedIds,
    onOpenChange,
}: TreeComboboxProps) {
    const [query, setQuery] = React.useState("")
    const [expandedIds, setExpandedIds] = React.useState<ReadonlySet<string> | null>(
        () => (defaultExpandedIds ? new Set(defaultExpandedIds) : null),
    )

    const parentIds = React.useMemo(() => collectParentIds(nodes), [nodes])
    const expanded = expandedIds ?? parentIds

    const allEntries = React.useMemo(() => flattenAll(nodes), [nodes])
    const selected =
        allEntries.find(({ node }) => node.id === value)?.node ?? null
    const selectedLabel = selected?.label

    /**
     * 是否为真实搜索：输入框文本非空，且不是 Base UI 同步的选中项 label。
     * 选中后 Base UI 会把输入框重置为选中项 label 并触发 onInputValueChange，
     * 若把它当搜索词，复显有值时整棵树会被过滤掉。
     */
    const searching = React.useMemo(() => {
        const q = query.trim().toLowerCase()
        if (!q) return false
        if (selectedLabel && q === selectedLabel.trim().toLowerCase()) {
            return false
        }
        return true
    }, [query, selectedLabel])

    const displayEntries = React.useMemo(() => {
        if (filterMode === "local" && searching) {
            const q = query.trim().toLowerCase()
            return flattenAll(nodes).filter(({ node }) =>
                [node.label, node.code]
                    .filter(Boolean)
                    .join(" ")
                    .toLowerCase()
                    .includes(q),
            )
        }
        return flattenVisible(nodes, expanded)
    }, [nodes, searching, query, filterMode, expanded])

    const toggle = React.useCallback(
        (id: string) => {
            setExpandedIds((previous) => {
                const next = new Set(previous ?? parentIds)
                if (next.has(id)) next.delete(id)
                else next.add(id)
                return next
            })
        },
        [parentIds],
    )

    const handleOpenChange = React.useCallback(
        (open: boolean) => {
            if (open && value) {
                // 打开时展开选中项的祖先，保证回显可见。
                setExpandedIds((previous) => {
                    const next = new Set(previous ?? parentIds)
                    for (const id of collectAncestorIds(nodes, value)) {
                        next.add(id)
                    }
                    return next
                })
            } else if (!open) {
                // Base UI 关闭时把输入框重置为选中项文案，同步清空本地过滤。
                setQuery("")
            }
            onOpenChange?.(open)
        },
        [nodes, value, parentIds, onOpenChange],
    )

    return (
        <Combobox
            items={displayEntries.map(({ node }) => node)}
            value={selected}
            onValueChange={(next) => onValueChange(next?.id)}
            onInputValueChange={(next) => {
                setQuery(next)
                onSearchChange?.(next)
            }}
            itemToStringLabel={(item) => item.label}
            itemToStringValue={(item) => item.id}
            isItemEqualToValue={(item, current) => item.id === current.id}
            filter={() => true}
            onOpenChange={handleOpenChange}
            disabled={disabled}
            required={required}
        >
            <div data-slot="tree-combobox" className={cn("min-w-0", className)}>
                <ComboboxInput
                    aria-label={label}
                    aria-busy={loading}
                    placeholder={placeholder}
                    showClear
                    disabled={disabled}
                    className="w-full"
                />
                <ComboboxContent>
                    <ComboboxEmpty>
                        {loading ? "正在加载…" : emptyLabel}
                    </ComboboxEmpty>
                    <ComboboxList>
                        {displayEntries.map(({ node, depth }) => {
                            const hasChildren = node.children.length > 0
                            const isOpen = expanded.has(node.id)
                            return (
                                <ComboboxItem
                                    key={node.id}
                                    value={node}
                                    style={{
                                        paddingLeft: `${0.5 + depth * 1.1}rem`,
                                    }}
                                    onKeyDown={(event) => {
                                        if (
                                            event.key === "ArrowRight" &&
                                            hasChildren &&
                                            !isOpen
                                        ) {
                                            event.preventDefault()
                                            toggle(node.id)
                                        }
                                        if (
                                            event.key === "ArrowLeft" &&
                                            hasChildren &&
                                            isOpen
                                        ) {
                                            event.preventDefault()
                                            toggle(node.id)
                                        }
                                    }}
                                >
                                    <div className="flex min-w-0 flex-1 items-center gap-1.5">
                                        {hasChildren ? (
                                            <button
                                                type="button"
                                                aria-label={
                                                    isOpen ? "收起" : "展开"
                                                }
                                                aria-expanded={isOpen}
                                                className="pointer-events-auto inline-flex size-5 shrink-0 items-center justify-center rounded-sm text-muted-foreground hover:bg-background hover:text-foreground"
                                                onClick={(event) => {
                                                    event.stopPropagation()
                                                    toggle(node.id)
                                                }}
                                            >
                                                <ChevronRightIcon
                                                    className={cn(
                                                        "size-3.5 transition-transform",
                                                        isOpen && "rotate-90",
                                                    )}
                                                />
                                            </button>
                                        ) : (
                                            <span
                                                className="inline-flex size-5 shrink-0"
                                                aria-hidden
                                            />
                                        )}
                                        <span className="truncate font-medium">
                                            {node.label}
                                        </span>
                                        {node.code ? (
                                            <span className="num ml-auto shrink-0 text-xs text-muted-foreground">
                                                {node.code}
                                            </span>
                                        ) : null}
                                    </div>
                                </ComboboxItem>
                            )
                        })}
                    </ComboboxList>
                </ComboboxContent>
            </div>
        </Combobox>
    )
}
