"use client"

import { useEffect, useRef, useState } from "react"
import type { Node as FlowNode, NodeChange, Viewport } from "@xyflow/react"
import { z } from "zod"
import type { EditorNode } from "../types"
import {
    reconcilePositions,
    type CanvasPositions,
    type FlowDirection,
} from "./flow-canvas-model"

import {
    insertionPositions,
    type NodeInsertion,
    type NodeInsertionResult,
} from "./flow-node-insertion"

const layoutSchema = z.object({
    direction: z.enum(["horizontal", "vertical"]).optional(),
    positions: z.record(
        z.string(),
        z.object({ x: z.number().finite(), y: z.number().finite() }),
    ),
    viewport: z
        .object({
            x: z.number().finite(),
            y: z.number().finite(),
            zoom: z.number().min(0.15).max(1.5),
        })
        .optional(),
})

/** 画布布局是当前浏览器的显示偏好，独立于审批草稿写请求。 */
export function useFlowCanvasLayout(
    definitionId: string,
    nodes: readonly EditorNode[],
) {
    const storageKey = `approval-canvas-layout-v4:${definitionId}`
    const [positions, setPositions] = useState<CanvasPositions>({})
    const [nodeState, setNodeState] = useState<
        Record<string, Pick<FlowNode, "measured" | "dragging" | "selected">>
    >({})
    const [viewport, setViewport] = useState<Viewport>()
    const [direction, setDirection] = useState<FlowDirection>("horizontal")
    const [ready, setReady] = useState(false)
    const [storageWarning, setStorageWarning] = useState(false)
    const previousNodes = useRef(nodes)

    useEffect(() => {
        try {
            const saved = localStorage.getItem(storageKey)
            if (saved) {
                const parsed = layoutSchema.safeParse(JSON.parse(saved))
                if (parsed.success) {
                    setPositions(parsed.data.positions)
                    setViewport(parsed.data.viewport)
                    setDirection(parsed.data.direction ?? "horizontal")
                }
            }
        } catch {
            setStorageWarning(true)
        }
        setReady(true)
    }, [storageKey, definitionId])

    useEffect(() => {
        if (!ready) return
        const previous = previousNodes.current
        previousNodes.current = nodes
        setPositions((current) => {
            const next = reconcilePositions(current, nodes, previous, direction)
            return JSON.stringify(current) === JSON.stringify(next)
                ? current
                : next
        })
    }, [nodes, ready, direction])

    useEffect(() => {
        if (!ready) return
        const timer = window.setTimeout(() => {
            try {
                localStorage.setItem(
                    storageKey,
                    JSON.stringify({ positions, viewport, direction }),
                )
                setStorageWarning(false)
            } catch {
                setStorageWarning(true)
            }
        }, 250)
        return () => window.clearTimeout(timer)
    }, [positions, viewport, direction, ready, storageKey])

    const onNodesChange = (changes: NodeChange[]) => {
        // 受控节点必须回传测量尺寸，否则父级重绘会清空连接点边界。
        setNodeState((current) => {
            const next = { ...current }
            let changed = false
            for (const change of changes) {
                if (
                    change.type !== "dimensions" &&
                    change.type !== "position" &&
                    change.type !== "select"
                )
                    continue
                const previous = current[change.id]
                if (
                    change.type === "select" &&
                    previous?.selected !== change.selected
                ) {
                    next[change.id] = {
                        ...next[change.id],
                        selected: change.selected,
                    }
                    changed = true
                }
                if (
                    change.type === "dimensions" &&
                    change.dimensions &&
                    (previous?.measured?.width !== change.dimensions.width ||
                        previous?.measured?.height !== change.dimensions.height)
                ) {
                    next[change.id] = {
                        ...next[change.id],
                        measured: change.dimensions,
                    }
                    changed = true
                }
                if (
                    change.type === "position" &&
                    change.dragging !== undefined &&
                    previous?.dragging !== change.dragging
                ) {
                    next[change.id] = {
                        ...next[change.id],
                        dragging: change.dragging,
                    }
                    changed = true
                }
            }
            return changed ? next : current
        })
        setPositions((current) => {
            const next = { ...current }
            let changed = false
            for (const change of changes) {
                if (change.type === "position" && change.position) {
                    next[change.id] = change.position
                    changed = true
                }
            }
            return changed ? next : current
        })
    }
    const placeInsertion = (
        result: NodeInsertionResult,
        placement: NodeInsertion,
    ) => {
        // 插入操作已计算后续步骤的平移，下一次表单同步不再重复移动。
        previousNodes.current = result.nodes
        const heights = Object.fromEntries(
            nodes.map((node) => [
                node.client_id,
                nodeState[node.client_id]?.measured?.height,
            ]),
        )
        setPositions((current) =>
            insertionPositions(
                current,
                nodes,
                result,
                placement,
                direction,
                heights,
            ),
        )
    }
    return {
        placeInsertion,
        positions,
        direction,
        setDirection,
        nodeState,
        setPositions,
        viewport,
        setViewport,
        ready,
        storageWarning,
        onNodesChange,
    }
}
