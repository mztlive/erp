"use client"

import * as React from "react"
import { ChevronRightIcon } from "lucide-react"

import { remoteSearchFromInputChange } from "@/components/business/combobox-input-search"
import {
    Combobox,
    ComboboxChip,
    ComboboxChips,
    ComboboxChipsInput,
    ComboboxContent,
    ComboboxEmpty,
    ComboboxInput,
    ComboboxItem,
    ComboboxList,
    ComboboxValue,
    useComboboxAnchor,
} from "@/components/ui/combobox"
import { InputGroupAddon } from "@/components/ui/input-group"
import { toAutomationIdSegment } from "@/lib/automation-id"
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
    /** 查询栏常驻名称，与选中值分开显示，不随选择或输入消失。 */
    filterLabel?: string
    placeholder?: string
    emptyLabel?: string
    loading?: boolean
    disabled?: boolean
    required?: boolean
    id?: string
    "aria-invalid"?: boolean
    "aria-describedby"?: string
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

/** 搜索时保留命中节点；命中父级则保留其子树，仅子级命中则保留祖先路径。 */
function filterMatchingBranches(
    nodes: readonly TreeComboboxNode[],
    query: string,
): TreeComboboxNode[] {
    const q = query.trim().toLowerCase()
    const visit = (node: TreeComboboxNode): TreeComboboxNode | null => {
        const haystack = [node.label, node.code]
            .filter(Boolean)
            .join(" ")
            .toLowerCase()
        if (haystack.includes(q)) return node
        const children = node.children
            .map(visit)
            .filter((child): child is TreeComboboxNode => child != null)
        if (children.length === 0) return null
        return { ...node, children }
    }
    return nodes
        .map(visit)
        .filter((node): node is TreeComboboxNode => node != null)
}

function TreeOption({
    idPrefix,
    node,
    depth,
    expanded,
    onToggle,
}: {
    idPrefix?: string
    node: TreeComboboxNode
    depth: number
    expanded: ReadonlySet<string>
    onToggle: (id: string) => void
}) {
    const hasChildren = node.children.length > 0
    const isOpen = expanded.has(node.id)
    return (
        <ComboboxItem
            id={
                idPrefix
                    ? `${idPrefix}-option-${toAutomationIdSegment(node.id)}`
                    : undefined
            }
            value={node}
            style={{ paddingLeft: `${0.5 + depth * 1.1}rem` }}
            onKeyDown={(event) => {
                if (event.key === "ArrowRight" && hasChildren && !isOpen) {
                    event.preventDefault()
                    onToggle(node.id)
                }
                if (event.key === "ArrowLeft" && hasChildren && isOpen) {
                    event.preventDefault()
                    onToggle(node.id)
                }
            }}
        >
            <div className="flex min-w-0 flex-1 items-center gap-1.5">
                {hasChildren ? (
                    <button
                        id={
                            idPrefix
                                ? `${idPrefix}-node-${toAutomationIdSegment(node.id)}-toggle`
                                : undefined
                        }
                        type="button"
                        aria-label={isOpen ? "收起" : "展开"}
                        aria-expanded={isOpen}
                        className="pointer-events-auto inline-flex size-5 shrink-0 items-center justify-center rounded-sm text-muted-foreground hover:bg-background hover:text-foreground"
                        onClick={(event) => {
                            event.stopPropagation()
                            onToggle(node.id)
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
                    <span className="inline-flex size-5 shrink-0" aria-hidden />
                )}
                <span className="truncate font-medium">{node.label}</span>
                {node.code ? (
                    <span className="num ml-auto shrink-0 text-xs text-muted-foreground">
                        {node.code}
                    </span>
                ) : null}
            </div>
        </ComboboxItem>
    )
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
    filterLabel,
    placeholder = "搜索名称或编号",
    emptyLabel = "没有符合条件的对象",
    loading = false,
    disabled = false,
    required = false,
    id,
    "aria-invalid": ariaInvalid,
    "aria-describedby": ariaDescribedBy,
    className,
    defaultExpandedIds,
    onOpenChange,
}: TreeComboboxProps) {
    const [query, setQuery] = React.useState("")
    const [expandedIds, setExpandedIds] =
        React.useState<ReadonlySet<string> | null>(() =>
            defaultExpandedIds ? new Set(defaultExpandedIds) : null,
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
                    id={id}
                    triggerId={id ? `${id}-trigger` : undefined}
                    clearId={id ? `${id}-clear` : undefined}
                    aria-label={label}
                    aria-invalid={ariaInvalid || undefined}
                    aria-describedby={ariaDescribedBy}
                    aria-busy={loading}
                    placeholder={placeholder}
                    showClear
                    disabled={disabled}
                    className="w-full"
                >
                    {filterLabel ? (
                        <InputGroupAddon className="shrink-0 whitespace-nowrap font-normal">
                            {filterLabel}：
                        </InputGroupAddon>
                    ) : null}
                </ComboboxInput>
                <ComboboxContent>
                    <ComboboxEmpty>
                        {loading ? "正在加载…" : emptyLabel}
                    </ComboboxEmpty>
                    <ComboboxList>
                        {displayEntries.map(({ node, depth }) => (
                            <TreeOption
                                key={node.id}
                                idPrefix={id}
                                node={node}
                                depth={depth}
                                expanded={expanded}
                                onToggle={toggle}
                            />
                        ))}
                    </ComboboxList>
                </ComboboxContent>
            </div>
        </Combobox>
    )
}

export type MultiTreeComboboxProps = {
    nodes: readonly TreeComboboxNode[]
    value: readonly string[]
    onValueChange: (ids: string[]) => void
    label: string
    placeholder?: string
    emptyLabel?: string
    disabled?: boolean
    id?: string
    "aria-describedby"?: string
    className?: string
}

function indexNodes(nodes: readonly TreeComboboxNode[]) {
    const byId = new Map<string, TreeComboboxNode>()
    const pathLabel = new Map<string, string>()
    const walk = (list: readonly TreeComboboxNode[], prefix: string[]) => {
        for (const node of list) {
            const path = [...prefix, node.label]
            byId.set(node.id, node)
            pathLabel.set(node.id, path.join(" / "))
            walk(node.children, path)
        }
    }
    walk(nodes, [])
    return { byId, pathLabel }
}

/**
 * 多选树形下拉：层级展开/收起；搜索保留命中节点和它的祖先路径。
 * 已选项以标签展示，可逐个移除。
 */
export function MultiTreeCombobox({
    nodes,
    value,
    onValueChange,
    label,
    placeholder = "搜索名称",
    emptyLabel = "没有符合条件的对象",
    disabled = false,
    id,
    "aria-describedby": ariaDescribedBy,
    className,
}: MultiTreeComboboxProps) {
    const [query, setQuery] = React.useState("")
    const [expandedIds, setExpandedIds] =
        React.useState<ReadonlySet<string> | null>(null)
    const [searchExpanded, setSearchExpanded] = React.useState<{
        query: string
        ids: ReadonlySet<string>
    } | null>(null)
    const anchorRef = useComboboxAnchor()
    const searching = query.trim().length > 0
    const { byId, pathLabel } = React.useMemo(() => indexNodes(nodes), [nodes])
    const parentIds = React.useMemo(() => collectParentIds(nodes), [nodes])
    const filteredNodes = React.useMemo(
        () => (searching ? filterMatchingBranches(nodes, query) : nodes),
        [nodes, searching, query],
    )
    const filteredParentIds = React.useMemo(
        () => collectParentIds(filteredNodes),
        [filteredNodes],
    )
    const expanded = searching
        ? searchExpanded?.query === query
            ? searchExpanded.ids
            : filteredParentIds
        : (expandedIds ?? parentIds)
    const displayEntries = React.useMemo(
        () => flattenVisible(filteredNodes, expanded),
        [filteredNodes, expanded],
    )
    const selected = React.useMemo(
        () =>
            value.map(
                (itemId) =>
                    byId.get(itemId) ?? {
                        id: itemId,
                        label: "当前不可用",
                        children: [],
                    },
            ),
        [byId, value],
    )

    const toggle = React.useCallback(
        (nodeId: string) => {
            if (searching) {
                setSearchExpanded((previous) => {
                    const base =
                        previous?.query === query
                            ? previous.ids
                            : filteredParentIds
                    const next = new Set(base)
                    if (next.has(nodeId)) next.delete(nodeId)
                    else next.add(nodeId)
                    return { query, ids: next }
                })
                return
            }
            setExpandedIds((previous) => {
                const next = new Set(previous ?? parentIds)
                if (next.has(nodeId)) next.delete(nodeId)
                else next.add(nodeId)
                return next
            })
        },
        [filteredParentIds, parentIds, query, searching],
    )

    const handleOpenChange = React.useCallback(
        (open: boolean) => {
            if (open) {
                setExpandedIds((previous) => {
                    const next = new Set(previous ?? parentIds)
                    for (const itemId of value) {
                        for (const ancestorId of collectAncestorIds(
                            nodes,
                            itemId,
                        )) {
                            next.add(ancestorId)
                        }
                    }
                    return next
                })
            } else {
                setQuery("")
                setSearchExpanded(null)
            }
        },
        [nodes, parentIds, value],
    )

    return (
        <Combobox
            multiple
            items={displayEntries.map(({ node }) => node)}
            value={selected}
            onValueChange={(next) => {
                onValueChange(next.map((item) => item.id))
            }}
            onInputValueChange={(next, details) => {
                const nextQuery = remoteSearchFromInputChange(
                    next,
                    details.reason,
                )
                if (nextQuery !== undefined) setQuery(nextQuery)
            }}
            itemToStringLabel={(item) => pathLabel.get(item.id) ?? item.label}
            itemToStringValue={(item) => item.id}
            isItemEqualToValue={(item, current) => item.id === current.id}
            filter={() => true}
            onOpenChange={handleOpenChange}
            disabled={disabled}
        >
            <div
                ref={anchorRef}
                data-slot="multi-tree-combobox"
                className={cn("min-w-0", className)}
            >
                <ComboboxChips>
                    <ComboboxValue>
                        {(items: TreeComboboxNode[]) =>
                            items.map((item) => {
                                const itemLabel =
                                    pathLabel.get(item.id) ?? item.label
                                return (
                                    <ComboboxChip
                                        key={item.id}
                                        removeId={
                                            id
                                                ? `${id}-chip-${toAutomationIdSegment(item.id)}-remove`
                                                : undefined
                                        }
                                        removeLabel={`移除${itemLabel}`}
                                        aria-label={itemLabel}
                                    >
                                        <span className="min-w-0 truncate">
                                            {itemLabel}
                                        </span>
                                    </ComboboxChip>
                                )
                            })
                        }
                    </ComboboxValue>
                    <ComboboxChipsInput
                        id={id}
                        aria-label={label}
                        aria-describedby={ariaDescribedBy}
                        placeholder={selected.length > 0 ? "" : placeholder}
                        disabled={disabled}
                    />
                </ComboboxChips>
            </div>
            <ComboboxContent anchor={anchorRef}>
                <ComboboxEmpty>{emptyLabel}</ComboboxEmpty>
                <ComboboxList>
                    {displayEntries.map(({ node, depth }) => (
                        <TreeOption
                            key={node.id}
                            idPrefix={id}
                            node={node}
                            depth={depth}
                            expanded={expanded}
                            onToggle={toggle}
                        />
                    ))}
                </ComboboxList>
            </ComboboxContent>
        </Combobox>
    )
}
