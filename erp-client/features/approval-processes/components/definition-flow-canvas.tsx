"use client"

import {
    useEffect,
    useMemo,
    useRef,
    useState,
    type CSSProperties,
    type ReactNode,
} from "react"
import {
    Background,
    ConnectionLineType,
    MarkerType,
    ReactFlow,
    type Connection,
    type Edge,
    type ReactFlowInstance,
} from "@xyflow/react"
import "@xyflow/react/dist/style.css"
import {
    ArrowDownIcon,
    ArrowRightIcon,
    BellIcon,
    LayoutGridIcon,
    MaximizeIcon,
    MinusIcon,
    PlusIcon,
} from "lucide-react"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { DocumentType, EditorCopyRule, EditorNode } from "../types"
import type { CanvasNode } from "./flow-canvas-node"
import { CanvasNodeWithMenu } from "./flow-node-context-menu"
import type { NodeInsertion, NodeInsertionResult } from "./flow-node-insertion"
import {
    arrangeNodes,
    orGroupSize,
    connectCanvasOrder,
    canvasTopology,
    FLOW_START,
    type FlowDirection,
} from "./flow-canvas-model"
import { useFlowCanvasLayout } from "./use-flow-canvas-layout"

const nodeTypes = { approval: CanvasNodeWithMenu, branch: CanvasNodeWithMenu }
const fitOptions = { padding: 0.2, minZoom: 0.15, maxZoom: 1 }
const canvasStyle = {
    "--xy-edge-stroke-default": "var(--muted-foreground)",
    "--xy-connectionline-stroke-default": "var(--primary)",
    "--xy-background-pattern-dots-color-default": "var(--border)",
    "--xy-background-color-default": "var(--background)",
} as CSSProperties

/** 自由画布负责坐标与连接交互；审批数据继续由父级草稿表单管理。 */
export function DefinitionFlowCanvas({
    id,
    definitionId,
    documentType,
    nodes,
    selectedClientId,
    disabled,
    copyRules,
    onSelect,
    onReorder,
    onInsertNode,
    onRemoveNode,
    onConfigureCopies,
    renderNodeEditor,
}: {
    id: string
    definitionId: string
    documentType: DocumentType
    nodes: readonly EditorNode[]
    selectedClientId?: string | null
    disabled: boolean
    copyRules: readonly EditorCopyRule[]
    onSelect: (clientId: string) => void
    onReorder: (nodes: EditorNode[]) => void
    onInsertNode: (placement: NodeInsertion) => NodeInsertionResult | null
    onRemoveNode: (clientId: string) => boolean
    onConfigureCopies: (clientId: string | null) => void
    renderNodeEditor: (node: EditorNode) => ReactNode
}) {
    const layout = useFlowCanvasLayout(definitionId, nodes)
    const [instance, setInstance] = useState<ReactFlowInstance<
        CanvasNode,
        Edge
    > | null>(null)
    const [notice, setNotice] = useState("")
    const containerRef = useRef<HTMLDivElement>(null)
    const previousSelection = useRef(selectedClientId)
    const defaults = useMemo(
        () => arrangeNodes(nodes, layout.direction),
        [nodes, layout.direction],
    )
    const topology = useMemo(() => canvasTopology(nodes), [nodes])
    const insert = (placement: NodeInsertion) => {
        if (disabled || nodes.length >= 20) return false
        const result = onInsertNode(placement)
        if (!result) return false
        layout.placeInsertion(result, placement)
        setNotice("节点已插入并自动连线，请填写名称和审批人后保存草稿。")
        return true
    }
    const remove = (clientId: string) => {
        if (disabled) return false
        const index = nodes.findIndex((node) => node.client_id === clientId)
        if (index < 0 || !onRemoveNode(clientId)) return false
        const focusId =
            nodes[index + 1]?.client_id ??
            nodes[index - 1]?.client_id ??
            FLOW_START
        requestAnimationFrame(() =>
            document
                .getElementById(
                    `${id}-node-${toAutomationIdSegment(focusId)}-context-trigger`,
                )
                ?.focus({ preventScroll: true }),
        )
        setNotice("节点已删除，前后步骤已自动连线。请保存草稿。")
        return true
    }
    const flowNodes: CanvasNode[] = topology.items.map((item) => {
        const clientId = item.id
        const node = nodes.find((node) => node.client_id === item.ownerId)
        const domId = `${id}-node-${toAutomationIdSegment(clientId)}`
        const domAttributes = {
            id: domId,
            title:
                node?.node_name ??
                (clientId === FLOW_START ? "提交申请" : "审批完成"),
        }
        const groupSize =
            node && item.kind === "or"
                ? orGroupSize(node, layout.direction)
                : undefined
        const parentSize =
            node && item.parentId
                ? orGroupSize(node, layout.direction)
                : undefined
        return {
            id: clientId,
            parentId: item.parentId,
            extent:
                parentSize && item.kind === "person"
                    ? [
                          [16, 72],
                          [parentSize.width - 16, parentSize.height - 16],
                      ]
                    : undefined,
            draggable: !disabled && !["split", "join"].includes(item.kind),
            focusable: item.kind !== "join",
            selectable: item.kind !== "join",
            zIndex: item.parentId ? 2 : 0,
            type: ["or", "split", "person", "join"].includes(item.kind)
                ? "branch"
                : "approval",
            ...layout.nodeState[clientId],
            position: layout.positions[clientId] ?? defaults[clientId],
            dragHandle: ".approval-node-drag",
            width:
                groupSize?.width ??
                (item.kind === "split"
                    ? 88
                    : item.kind === "person"
                      ? 220
                      : item.kind === "join"
                        ? 8
                        : node
                          ? 420
                          : 160),
            height: groupSize?.height,
            style: groupSize
                ? { width: groupSize.width, height: groupSize.height }
                : undefined,
            // React Flow 将 domAttributes 透传到实际可聚焦、可拖动的节点容器。
            domAttributes,
            data: {
                domId,
                canvasId: id,
                onInsert: insert,
                onRemove: remove,
                nodeLimitReached: nodes.length >= 20,
                direction: layout.direction,
                documentType,
                disabled,
                onChange: (patch: Partial<EditorNode>) => {
                    if (!disabled)
                        onReorder(
                            nodes.map((item) =>
                                item.client_id === node?.client_id
                                    ? { ...item, ...patch }
                                    : item,
                            ),
                        )
                },
                kind: item.kind,
                ownerId: item.ownerId,
                personId: item.personId,
                node,
                order: node ? nodes.indexOf(node) + 1 : undefined,
                editor:
                    node &&
                    node.client_id === selectedClientId &&
                    (item.kind === "approval" || item.kind === "or")
                        ? renderNodeEditor(node)
                        : undefined,
                copyCount:
                    copyRules.find(
                        (rule) =>
                            rule.trigger === "NODE_APPROVED" &&
                            rule.nodeClientId === node?.client_id,
                    )?.recipients.length ?? 0,
                onEdit: () => {
                    if (node) onSelect(node.client_id)
                },
                onCopies: () => onConfigureCopies(node?.client_id ?? null),
            },
        }
    })
    const edges: Edge[] = topology.links.map((link) => ({
        id: `${id}-edge-${toAutomationIdSegment(link.source)}-${toAutomationIdSegment(link.target)}`,
        source: link.source,
        target: link.target,
        sourceHandle: "source",
        targetHandle: "target",
        type: "smoothstep",
        zIndex: 1,
        markerEnd:
            link.internal && link.target.endsWith("/join")
                ? undefined
                : {
                      type: MarkerType.ArrowClosed,
                      color: link.internal
                          ? "var(--info)"
                          : "var(--muted-foreground)",
                  },
        style: {
            strokeWidth: 2,
            stroke: link.internal ? "var(--info)" : "var(--muted-foreground)",
        },
        selectable: false,
        focusable: false,
        deletable: false,
    }))

    useEffect(() => {
        if (!instance || previousSelection.current === selectedClientId) return
        previousSelection.current = selectedClientId
        if (!selectedClientId) return
        const frame = requestAnimationFrame(() => {
            void instance.fitView({
                nodes: topology.items
                    .filter((item) => item.ownerId === selectedClientId)
                    .map((item) => ({ id: item.id })),
                padding: 0.4,
                maxZoom: 1,
                duration: 180,
            })
        })
        return () => cancelAnimationFrame(frame)
    }, [instance, selectedClientId, topology])

    const connect = (connection: Connection) => {
        if (disabled) return
        const next = connectCanvasOrder(
            nodes,
            connection.source,
            connection.target,
        )
        if (!next) {
            setNotice(
                "请连接两个不同节点，保持从提交申请到审批完成的单一路线。",
            )
            return
        }
        if (
            next.every(
                (node, index) => node.client_id === nodes[index]?.client_id,
            )
        )
            return
        onReorder(next)
        setNotice("审批顺序已按连线调整，其余连线已自动补齐。请保存草稿。")
    }
    const arrange = (direction: FlowDirection = layout.direction) => {
        layout.setDirection(direction)
        const heights = Object.fromEntries(
            nodes.map((node) => [
                node.client_id,
                layout.nodeState[node.client_id]?.measured?.height,
            ]),
        )
        layout.setPositions(arrangeNodes(nodes, direction, heights))
        requestAnimationFrame(() => {
            void instance?.fitView({ ...fitOptions, duration: 200 })
        })
    }
    return (
        <section
            aria-label="审批流程自由画布"
            className="min-w-0 overflow-hidden rounded-lg border border-border bg-card"
        >
            <div className="flex flex-wrap items-center justify-between gap-3 border-b border-border px-4 py-3">
                <div className="space-y-1">
                    <h3 className="text-sm font-medium">
                        审批流程 · {nodes.length} 个节点
                    </h3>
                    <p className="text-xs text-muted-foreground">
                        拖动或签分组标题可整体移动，审批人可在组内调整位置。右键节点插入步骤，连接步骤出口与下一步入口调整顺序。
                    </p>
                </div>
                <div className="flex flex-wrap items-center gap-1">
                    <Button
                        id={`${id}-horizontal`}
                        type="button"
                        variant={
                            layout.direction === "horizontal"
                                ? "secondary"
                                : "ghost"
                        }
                        size="sm"
                        aria-pressed={layout.direction === "horizontal"}
                        disabled={!instance}
                        onClick={() => arrange("horizontal")}
                    >
                        <ArrowRightIcon />
                        横向
                    </Button>
                    <Button
                        id={`${id}-vertical`}
                        type="button"
                        variant={
                            layout.direction === "vertical"
                                ? "secondary"
                                : "ghost"
                        }
                        size="sm"
                        aria-pressed={layout.direction === "vertical"}
                        disabled={!instance}
                        onClick={() => arrange("vertical")}
                    >
                        <ArrowDownIcon />
                        纵向
                    </Button>
                    <Button
                        id={`${id}-zoom-out`}
                        type="button"
                        variant="ghost"
                        size="icon-sm"
                        aria-label="缩小画布"
                        disabled={!instance}
                        onClick={() =>
                            void instance?.zoomOut({ duration: 150 })
                        }
                    >
                        <MinusIcon />
                    </Button>
                    <Button
                        id={`${id}-zoom-in`}
                        type="button"
                        variant="ghost"
                        size="icon-sm"
                        aria-label="放大画布"
                        disabled={!instance}
                        onClick={() => void instance?.zoomIn({ duration: 150 })}
                    >
                        <PlusIcon />
                    </Button>
                    <Button
                        id={`${id}-fit`}
                        type="button"
                        variant="outline"
                        size="sm"
                        disabled={!instance}
                        onClick={() =>
                            void instance?.fitView({
                                ...fitOptions,
                                duration: 200,
                            })
                        }
                    >
                        <MaximizeIcon />
                        适应画布
                    </Button>
                    <Button
                        id={`${id}-arrange`}
                        type="button"
                        variant="outline"
                        size="sm"
                        disabled={!instance}
                        onClick={() => arrange()}
                    >
                        <LayoutGridIcon />
                        自动排列
                    </Button>
                    <Button
                        id={`${id}-copy-settings`}
                        type="button"
                        variant="outline"
                        size="sm"
                        onClick={() => onConfigureCopies(null)}
                    >
                        <BellIcon />
                        抄送设置
                        {copyRules.length ? ` · ${copyRules.length}` : ""}
                    </Button>
                </div>
            </div>
            <div
                ref={containerRef}
                className="h-[70vh] min-h-[560px] w-full"
                style={canvasStyle}
            >
                {layout.ready && (
                    <ReactFlow<CanvasNode, Edge>
                        id={id}
                        nodes={flowNodes}
                        edges={edges}
                        nodeTypes={nodeTypes}
                        onNodesChange={layout.onNodesChange}
                        onConnect={connect}
                        onConnectStart={() => setNotice("")}
                        onConnectEnd={(_event, connection) => {
                            if (connection.toNode && !connection.isValid) {
                                setNotice(
                                    "请从审批步骤出口或汇合点连接下一步骤入口；或签人员分支由人员配置自动生成。",
                                )
                            }
                        }}
                        isValidConnection={(connection) =>
                            !disabled &&
                            connectCanvasOrder(
                                nodes,
                                connection.source,
                                connection.target,
                            ) !== null
                        }
                        onNodeClick={(event, node) => {
                            if (
                                event.target instanceof Element &&
                                event.target.closest(".nodrag")
                            )
                                return
                            if (
                                node.data.ownerId &&
                                ["approval", "or"].includes(node.data.kind)
                            )
                                onSelect(node.data.ownerId)
                        }}
                        onInit={(flow) => {
                            setInstance(flow)
                            containerRef.current
                                ?.querySelector(".react-flow__pane")
                                ?.setAttribute("id", `${id}-pane`)
                        }}
                        onMoveEnd={(_event, viewport) =>
                            layout.setViewport(viewport)
                        }
                        defaultViewport={layout.viewport}
                        fitView={!layout.viewport}
                        fitViewOptions={fitOptions}
                        minZoom={0.15}
                        maxZoom={1.5}
                        nodesDraggable={!disabled}
                        nodesConnectable={!disabled}
                        edgesReconnectable={false}
                        edgesFocusable={false}
                        deleteKeyCode={null}
                        multiSelectionKeyCode={null}
                        selectionKeyCode={null}
                        panOnScroll
                        zoomOnScroll={false}
                        zoomOnPinch
                        panOnDrag={[0, 1]}
                        connectionLineType={ConnectionLineType.SmoothStep}
                        proOptions={{ hideAttribution: true }}
                        ariaLabelConfig={{
                            "node.a11yDescription.default":
                                "按方向键移动节点，按回车选择节点。",
                            "node.a11yDescription.keyboardDisabled":
                                "拖动节点标题调整位置。",
                        }}
                    >
                        <Background gap={24} size={1} />
                    </ReactFlow>
                )}
            </div>
            <div className="space-y-1 border-t border-border px-4 py-2 text-xs text-muted-foreground">
                <p>
                    拖动空白处或双指滚动平移，双指捏合缩放。摆放位置不改变审批顺序，布局仅保存在当前浏览器。
                </p>
                <p role="status" aria-live="polite">
                    {layout.storageWarning
                        ? "浏览器未能保存画布布局，本次仍可编辑；审批配置请保存草稿。"
                        : notice}
                </p>
            </div>
        </section>
    )
}
