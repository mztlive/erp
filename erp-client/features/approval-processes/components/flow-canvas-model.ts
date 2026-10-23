import type { XYPosition } from "@xyflow/react"
import type { EditorNode } from "../types"

export const FLOW_START = "approval-canvas-start"
export const FLOW_END = "approval-canvas-end"
export type CanvasPositions = Record<string, XYPosition>
export type FlowDirection = "horizontal" | "vertical"

export type CanvasKind =
    | "start"
    | "approval"
    | "or"
    | "split"
    | "person"
    | "join"
    | "end"
export type CanvasItem = {
    id: string
    kind: CanvasKind
    ownerId?: string
    personId?: string
    parentId?: string
}
export type CanvasLink = { source: string; target: string; internal: boolean }

export const nodePeople = (node: EditorNode) => [
    ...(node.assignee_user_id
        ? [{ user_id: node.assignee_user_id, name: node.assignee_name }]
        : []),
    ...node.additional_assignees,
]
export const isOrSign = (node: EditorNode) =>
    node.assignee_source === "SPECIFIED" && nodePeople(node).length > 1
export const personNodeId = (ownerId: string, personId: string) =>
    `${ownerId}/person/${personId}`
export const splitNodeId = (ownerId: string) => `${ownerId}/split`
export const joinNodeId = (ownerId: string) => `${ownerId}/join`

/** 分组尺寸包含标题栏、紧凑人员行与内部连线的留白。 */
export function orGroupSize(node: EditorNode, direction: FlowDirection) {
    const count = nodePeople(node).length
    return direction === "horizontal"
        ? { width: 360, height: Math.max(216, 88 + count * 56 + 16) }
        : { width: Math.max(360, count * 196 + 48), height: 250 }
}

/** 生成真实画布节点与连线；或签分支仍归属同一个审批步骤。 */
export function canvasTopology(nodes: readonly EditorNode[]) {
    const items: CanvasItem[] = [{ id: FLOW_START, kind: "start" }]
    const links: CanvasLink[] = []
    let previous = FLOW_START
    for (const node of nodes) {
        const orSign = isOrSign(node)
        items.push({
            id: node.client_id,
            ownerId: node.client_id,
            kind: orSign ? "or" : "approval",
        })
        links.push({
            source: previous,
            target: orSign ? splitNodeId(node.client_id) : node.client_id,
            internal: false,
        })
        if (orSign) {
            const join = joinNodeId(node.client_id)
            const split = splitNodeId(node.client_id)
            items.push({
                id: split,
                ownerId: node.client_id,
                parentId: node.client_id,
                kind: "split",
            })
            for (const person of nodePeople(node)) {
                const id = personNodeId(node.client_id, person.user_id)
                items.push({
                    id,
                    ownerId: node.client_id,
                    kind: "person",
                    parentId: node.client_id,
                    personId: person.user_id,
                })
                links.push(
                    { source: split, target: id, internal: true },
                    { source: id, target: join, internal: true },
                )
            }
            items.push({
                id: join,
                ownerId: node.client_id,
                parentId: node.client_id,
                kind: "join",
            })
            previous = join
        } else previous = node.client_id
    }
    items.push({ id: FLOW_END, kind: "end" })
    links.push({ source: previous, target: FLOW_END, internal: false })
    return { items, links }
}

/** 横向时分支竖排，纵向时分支横排；所有步骤共用一条主干中线。 */
export function arrangeNodes(
    nodes: readonly EditorNode[],
    direction: FlowDirection = "horizontal",
    heights: Record<string, number | undefined> = {},
): CanvasPositions {
    const horizontal = direction === "horizontal"
    const mainY = 200
    const mainX = 280
    const positions: CanvasPositions = {
        [FLOW_START]: horizontal
            ? { x: 0, y: mainY - 22 }
            : { x: mainX - 60, y: 0 },
    }
    let cursor = horizontal ? 160 : 140
    for (const node of nodes) {
        if (horizontal) {
            if (isOrSign(node)) {
                const people = nodePeople(node)
                const { width, height } = orGroupSize(node, direction)
                const centerY = 70 + (height - 70) / 2
                // 子节点坐标相对父分组；拖动分组由 React Flow 统一移动。
                positions[node.client_id] = {
                    x: cursor,
                    y: mainY - centerY,
                }
                positions[splitNodeId(node.client_id)] = {
                    x: 24,
                    y: centerY - 20,
                }
                people.forEach((person, index) => {
                    positions[personNodeId(node.client_id, person.user_id)] = {
                        x: 108,
                        y: 92 + index * 56,
                    }
                })
                positions[joinNodeId(node.client_id)] = {
                    x: width - 12,
                    y: centerY - 4,
                }
                cursor += width + 40
            } else {
                const height = heights[node.client_id] ?? 176
                positions[node.client_id] = {
                    x: cursor,
                    y: mainY - height / 2,
                }
                cursor += 340
            }
        } else if (isOrSign(node)) {
            const people = nodePeople(node)
            const { width, height } = orGroupSize(node, direction)
            const stackWidth = people.length * 196 - 16
            const stackLeft = width / 2 - stackWidth / 2
            positions[node.client_id] = {
                x: mainX - width / 2,
                y: cursor,
            }
            positions[splitNodeId(node.client_id)] = {
                x: width / 2 - 44,
                y: 88,
            }
            people.forEach((person, index) => {
                positions[personNodeId(node.client_id, person.user_id)] = {
                    x: stackLeft + index * 196,
                    y: 150,
                }
            })
            positions[joinNodeId(node.client_id)] = {
                x: width / 2 - 4,
                y: height - 12,
            }
            cursor += height + 140
        } else {
            positions[node.client_id] = {
                x: mainX - 150,
                y: cursor,
            }
            cursor += (heights[node.client_id] ?? 176) + 180
        }
    }
    positions[FLOW_END] = horizontal
        ? { x: cursor, y: mainY - 22 }
        : { x: mainX - 60, y: cursor }
    return positions
}

/** 只允许从步骤出口连接到另一审批步骤入口，不可把人员分支改成串行任务。 */
export function connectCanvasOrder(
    nodes: readonly EditorNode[],
    source: string,
    target: string,
) {
    const { items } = canvasTopology(nodes)
    const from = items.find((item) => item.id === source)
    const to = items.find((item) => item.id === target)
    if (
        !from ||
        !to ||
        !["start", "approval", "join"].includes(from.kind) ||
        !["approval", "split", "end"].includes(to.kind)
    )
        return null
    return connectInOrder(nodes, from.ownerId ?? from.id, to.ownerId ?? to.id)
}

/** 保留自由摆放坐标；保存后的节点身份及更换人员时迁移对应显示位置。 */
export function reconcilePositions(
    current: CanvasPositions,
    nodes: readonly EditorNode[],
    previous: readonly EditorNode[],
    direction: FlowDirection,
): CanvasPositions {
    const defaults = arrangeNodes(nodes, direction)
    const oldDefaults = arrangeNodes(previous, direction)
    const expanded =
        nodes.length > previous.length ||
        nodes.some((node) => {
            const old = previous.find(
                (item) => item.client_id === node.client_id,
            )
            return old && !isOrSign(old) && isOrSign(node)
        })
    const { items } = canvasTopology(nodes)
    const next: CanvasPositions = {}
    for (const item of items) {
        const index = nodes.findIndex((node) => node.client_id === item.ownerId)
        const owner = nodes[index]
        const oldOwner =
            previous.find((node) => node.client_id === item.ownerId) ??
            (previous.length === nodes.length &&
            !previous[index]?.node_id &&
            owner?.node_id
                ? previous[index]
                : undefined)
        let oldId = item.id
        if (oldOwner && owner) {
            if (item.kind === "join") oldId = joinNodeId(oldOwner.client_id)
            else if (item.kind === "split")
                oldId = splitNodeId(oldOwner.client_id)
            else if (item.kind === "person") {
                const oldPeople = nodePeople(oldOwner)
                const people = nodePeople(owner)
                const person =
                    oldPeople.find(
                        (person) => person.user_id === item.personId,
                    ) ??
                    (oldPeople.length === people.length
                        ? oldPeople[
                              people.findIndex(
                                  (person) => person.user_id === item.personId,
                              )
                          ]
                        : undefined)
                if (person)
                    oldId = personNodeId(oldOwner.client_id, person.user_id)
            } else oldId = oldOwner.client_id
        }
        const resizedBranch =
            owner &&
            oldOwner &&
            isOrSign(owner) &&
            nodePeople(owner).length !== nodePeople(oldOwner).length
        const saved =
            resizedBranch && ["person", "join", "split"].includes(item.kind)
                ? undefined
                : current[oldId]
        if (saved && resizedBranch && item.kind === "or") {
            // 或签人数变化会改变分组高度/宽度，保留另一轴的自由位置，
            // 同时在主轴方向重新居中，避免步骤连线错位。
            next[item.id] =
                direction === "horizontal"
                    ? { x: saved.x, y: defaults[item.id].y }
                    : { x: defaults[item.id].x, y: saved.y }
            continue
        }
        if (saved) {
            const before = oldDefaults[oldId]
            const after = defaults[item.id]
            // 扩展成或签分支时给后续步骤让出空间；其余自由摆放偏移保持不变。
            const shift =
                before && expanded && !item.parentId
                    ? Math.max(
                          0,
                          direction === "horizontal"
                              ? after.x - before.x
                              : after.y - before.y,
                      )
                    : 0
            next[item.id] = {
                x: saved.x + (direction === "horizontal" ? shift : 0),
                y: saved.y + (direction === "vertical" ? shift : 0),
            }
        } else next[item.id] = defaults[item.id]
    }
    return next
}

/** 拉线将目标节点移到来源节点之后，自动补齐剩余单一路线。 */
export function connectInOrder(
    nodes: readonly EditorNode[],
    source: string,
    target: string,
): EditorNode[] | null {
    if (source === target || source === FLOW_END || target === FLOW_START)
        return null
    if (source === FLOW_START && target === FLOW_END)
        return nodes.length ? null : []
    if (target === FLOW_END) {
        const node = nodes.find((item) => item.client_id === source)
        return node ? [...nodes.filter((item) => item !== node), node] : null
    }
    const node = nodes.find((item) => item.client_id === target)
    if (!node) return null
    const next = nodes.filter((item) => item !== node)
    if (source === FLOW_START) return [node, ...next]
    const index = next.findIndex((item) => item.client_id === source)
    if (index < 0) return null
    next.splice(index + 1, 0, node)
    return next
}
