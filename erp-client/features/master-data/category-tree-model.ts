/**
 * 商品分类树形结构：由扁平列表构建、路径与可选父级校验。
 * 组件本身不请求；数据来自 W14 list 行。
 */

import type { MasterDataListItem } from "@/features/master-data/types"

export type CategoryTreeNode = Readonly<{
  item: MasterDataListItem
  depth: number
  pathLabel: string
  children: readonly CategoryTreeNode[]
}>

function categoryCodeOf(item: MasterDataListItem): string {
  return (
    item.dictionaryCode ??
    item.keyFacts.find((f) => f.label === "分类代码")?.value ??
    item.stableNo
  )
}

/** 扁平列表 → 森林（多根）；同级按名称排序。 */
export function buildCategoryForest(
  rows: readonly MasterDataListItem[]
): CategoryTreeNode[] {
  const byId = new Map(rows.map((r) => [r.stableId, r]))
  const childrenMap = new Map<string | null, MasterDataListItem[]>()

  for (const row of rows) {
    const parentId = row.parentStableId
    const key =
      parentId && byId.has(parentId) && parentId !== row.stableId
        ? parentId
        : null
    const list = childrenMap.get(key) ?? []
    list.push(row)
    childrenMap.set(key, list)
  }

  const sortSiblings = (items: MasterDataListItem[]) =>
    [...items].sort((a, b) => a.name.localeCompare(b.name, "zh-CN"))

  function buildNode(
    item: MasterDataListItem,
    depth: number,
    parentPath: string
  ): CategoryTreeNode {
    const pathLabel = parentPath ? `${parentPath} / ${item.name}` : item.name
    const kids = sortSiblings(childrenMap.get(item.stableId) ?? []).map((child) =>
      buildNode(child, depth + 1, pathLabel)
    )
    return {
      item,
      depth,
      pathLabel,
      children: kids,
    }
  }

  return sortSiblings(childrenMap.get(null) ?? []).map((root) =>
    buildNode(root, 0, "")
  )
}

/** 前序展平，供表格树与 Combobox 共用。 */
export function flattenCategoryForest(
  forest: readonly CategoryTreeNode[]
): readonly CategoryTreeNode[] {
  const out: CategoryTreeNode[] = []
  const walk = (nodes: readonly CategoryTreeNode[]) => {
    for (const node of nodes) {
      out.push(node)
      if (node.children.length > 0) walk(node.children)
    }
  }
  walk(forest)
  return out
}

/** 收集某节点及其全部后代的 stableId（含自身）。 */
export function collectDescendantIds(
  forest: readonly CategoryTreeNode[],
  rootId: string
): Set<string> {
  const ids = new Set<string>()
  const find = (
    nodes: readonly CategoryTreeNode[]
  ): CategoryTreeNode | null => {
    for (const node of nodes) {
      if (node.item.stableId === rootId) return node
      const hit = find(node.children)
      if (hit) return hit
    }
    return null
  }
  const root = find(forest)
  if (!root) return ids
  const walk = (node: CategoryTreeNode) => {
    ids.add(node.item.stableId)
    for (const child of node.children) walk(child)
  }
  walk(root)
  return ids
}

/** Combobox 行：启用项优先；可排除子树（选上级时）。 */
export function toCategoryComboboxItems(
  rows: readonly MasterDataListItem[],
  options?: {
    /** 排除的 ID（自身 + 后代），不可选为上级。 */
    excludeIds?: ReadonlySet<string>
    /** 仅启用。默认 true。 */
    enabledOnly?: boolean
  }
) {
  const enabledOnly = options?.enabledOnly ?? true
  const forest = buildCategoryForest(rows)
  const flat = flattenCategoryForest(forest)
  return flat
    .filter((node) =>
      enabledOnly ? node.item.lifecycleStatus === "ENABLED" : true
    )
    .map((node) => ({
      categoryId: node.item.stableId,
      categoryCode: categoryCodeOf(node.item),
      categoryName: node.item.name,
      pathLabel: node.pathLabel,
      depth: node.depth,
      parentId: node.item.parentStableId,
      statusLabel: node.item.lifecycleStatusLabel,
      statusTone: node.item.lifecycleTone,
      description: node.item.productKind
        ? `适用 ${node.item.productKind}`
        : undefined,
      disabled: options?.excludeIds?.has(node.item.stableId) ?? false,
    }))
}

/** 品牌列表 → Combobox 行。 */
export function toBrandComboboxItems(rows: readonly MasterDataListItem[]) {
  return rows
    .filter((row) => row.lifecycleStatus === "ENABLED")
    .map((row) => ({
      brandId: row.stableId,
      brandCode:
        row.dictionaryCode ??
        row.keyFacts.find((f) => f.label === "品牌代码")?.value ??
        row.stableNo,
      brandName: row.name,
      statusLabel: row.lifecycleStatusLabel,
      statusTone: row.lifecycleTone,
    }))
}
