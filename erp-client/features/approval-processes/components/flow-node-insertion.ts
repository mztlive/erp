import type { EditorNode } from "../types"
import {
    arrangeNodes,
    canvasTopology,
    FLOW_END,
    FLOW_START,
    isOrSign,
    orGroupSize,
    type CanvasPositions,
    type FlowDirection,
} from "./flow-canvas-model"

export type NodeInsertion =
    | { position: "before" | "after"; anchorId: string }
    | { position: "start" }
    | { position: "end" }
export type NodeInsertionResult = { node: EditorNode; nodes: EditorNode[] }

/** 以当前步骤身份定位；锚点失效时拒绝插入，不默默追加到错误位置。 */
export function insertionIndex(
    nodes: readonly EditorNode[],
    placement: NodeInsertion,
): number | null {
    if (placement.position === "start") return 0
    if (placement.position === "end") return nodes.length
    const index = nodes.findIndex(
        (node) => node.client_id === placement.anchorId,
    )
    return index < 0 ? null : index + (placement.position === "after" ? 1 : 0)
}

/** 在操作步骤附近摆放新节点，为后续步骤平移让位，保留或签分支内部布局。 */
export function insertionPositions(
    current: CanvasPositions,
    previous: readonly EditorNode[],
    result: NodeInsertionResult,
    placement: NodeInsertion,
    direction: FlowDirection,
    heights: Record<string, number | undefined>,
): CanvasPositions {
    const horizontal = direction === "horizontal"
    const positions = {
        ...arrangeNodes(previous, direction, heights),
        ...current,
    }
    const index = result.nodes.findIndex(
        (node) => node.client_id === result.node.client_id,
    )
    const successor = result.nodes[index + 1]
    const preceding = result.nodes[index - 1]
    let position = arrangeNodes([result.node], direction)[result.node.client_id]
    if (
        (placement.position === "before" || placement.position === "start") &&
        successor
    ) {
        const anchor = positions[successor.client_id]
        position = {
            x: anchor.x,
            y: anchor.y,
        }
    } else if (preceding) {
        const orSign = isOrSign(preceding)
        const anchor = positions[preceding.client_id]
        const size = orSign ? orGroupSize(preceding, direction) : null
        position = horizontal
            ? { x: anchor.x + (size ? size.width + 140 : 540), y: anchor.y }
            : {
                  x: anchor.x,
                  y:
                      anchor.y +
                      (size
                          ? size.height + 140
                          : (heights[preceding.client_id] ?? 176) + 180),
              }
    } else {
        const start = positions[FLOW_START]
        position = horizontal
            ? { x: start.x + 260, y: start.y - 60 }
            : { x: start.x - 130, y: start.y + 160 }
    }
    positions[result.node.client_id] = position
    const downstream = new Set(
        result.nodes.slice(index + 1).map((node) => node.client_id),
    )
    const items = canvasTopology(result.nodes).items
    const following = items.filter(
        (item) =>
            item.id === FLOW_END ||
            (!item.parentId && item.ownerId && downstream.has(item.ownerId)),
    )
    const minimum = Math.min(
        ...following.map((item) =>
            horizontal ? positions[item.id].x : positions[item.id].y,
        ),
    )
    const gap = Math.max(
        0,
        (horizontal ? position.x + 540 : position.y + 356) - minimum,
    )
    for (const item of following) {
        const original = positions[item.id]
        positions[item.id] = {
            x: original.x + (horizontal ? gap : 0),
            y: original.y + (horizontal ? 0 : gap),
        }
    }
    return Object.fromEntries(
        items.map((item) => [item.id, positions[item.id]]),
    )
}
