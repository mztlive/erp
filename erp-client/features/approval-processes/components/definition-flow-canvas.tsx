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
    CircleHelpIcon,
    CopyIcon,
    EllipsisIcon,
    LayoutGridIcon,
    MaximizeIcon,
    MinusIcon,
    PlusIcon,
} from "lucide-react"
import { Button } from "@/components/ui/button"
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuItem,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import {
    Popover,
    PopoverContent,
    PopoverDescription,
    PopoverTitle,
    PopoverTrigger,
} from "@/components/ui/popover"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import type { DocumentType, EditorNode } from "../types"
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
const fitOptions = { padding: 0.06, minZoom: 0.15, maxZoom: 1.1 }
const initialFitOptions = { ...fitOptions, minZoom: 0.65 }
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
    sidebar,
    onSelect,
    onReorder,
    onInsertNode,
    onRemoveNode,
    onCopyFlow,
}: {
    id: string
    definitionId: string
    documentType: DocumentType
    nodes: readonly EditorNode[]
    selectedClientId?: string | null
    disabled: boolean
    sidebar?: ReactNode
    onSelect: (clientId: string) => void
    onReorder: (nodes: EditorNode[]) => void
    onInsertNode: (placement: NodeInsertion) => NodeInsertionResult | null
    onRemoveNode: (clientId: string) => boolean
    onCopyFlow: () => void
}) {
    const layout = useFlowCanvasLayout(definitionId, nodes)
    const [instance, setInstance] = useState<ReactFlowInstance<
        CanvasNode,
        Edge
    > | null>(null)
    const [notice, setNotice] = useState("")
    const [zoom, setZoom] = useState(layout.viewport?.zoom ?? 1)
    const containerRef = useRef<HTMLDivElement>(null)
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
            selected:
                Boolean(node) &&
                node?.client_id === selectedClientId &&
                ["approval", "or"].includes(item.kind),
            position: layout.positions[clientId] ?? defaults[clientId],
            dragHandle: ".approval-node-drag",
            width:
                groupSize?.width ??
                (item.kind === "split"
                    ? 88
                    : item.kind === "person"
                      ? 180
                      : item.kind === "join"
                        ? 8
                        : node
                          ? 300
                          : 120),
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
                onEdit: () => {
                    if (node) onSelect(node.client_id)
                },
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
    useEffect(() => {
        if (!instance) return
        const handleKeyDown = (event: KeyboardEvent) => {
            if (event.metaKey || event.ctrlKey || event.altKey) return
            if (
                event.target instanceof HTMLElement &&
                event.target.closest(
                    "input,textarea,select,[contenteditable='true']",
                )
            )
                return
            if (event.key === "+" || event.key === "=") {
                event.preventDefault()
                void instance.zoomIn({ duration: 150 })
            } else if (event.key === "-") {
                event.preventDefault()
                void instance.zoomOut({ duration: 150 })
            } else if (event.key === "0") {
                event.preventDefault()
                void instance.zoomTo(1, { duration: 150 })
            } else if (event.key.toLowerCase() === "f") {
                event.preventDefault()
                void instance.fitView({ ...fitOptions, duration: 200 })
            }
        }
        window.addEventListener("keydown", handleKeyDown)
        return () => window.removeEventListener("keydown", handleKeyDown)
    }, [instance])
    return (
        <section
            aria-label="审批流程自由画布"
            className="min-w-0 overflow-hidden rounded-lg border border-border bg-card"
        >
            <div className="flex flex-wrap items-center justify-between gap-2 border-b border-border px-4 py-2.5">
                <div className="flex min-w-0 items-center gap-2">
                    <h3 className="text-sm font-medium">审批流程</h3>
                    <span className="rounded-md bg-muted px-2 py-0.5 text-xs text-muted-foreground">
                        {nodes.length} 个审批步骤
                    </span>
                    <Popover>
                        <PopoverTrigger
                            id={`${id}-help`}
                            render={
                                <Button
                                    type="button"
                                    variant="ghost"
                                    size="icon-xs"
                                    aria-label="画布操作说明"
                                    title="画布操作说明"
                                />
                            }
                        >
                            <CircleHelpIcon />
                        </PopoverTrigger>
                        <PopoverContent align="start" className="w-80">
                            <PopoverTitle className="text-sm">
                                画布操作
                            </PopoverTitle>
                            <PopoverDescription className="space-y-2 text-xs leading-5">
                                <span className="block">
                                    拖动节点或分组标题调整显示位置，审批人可在组内单独摆放。
                                </span>
                                <span className="block">
                                    右键节点可在前后插入步骤；拖动出口连接点可调整审批顺序。
                                </span>
                                <span className="block">
                                    画布缩放和位置只保存在当前浏览器，不改变审批规则。
                                </span>
                                <span className="block">
                                    快捷键：+ / − 缩放，0 恢复 100%，F
                                    适应画布。
                                </span>
                            </PopoverDescription>
                        </PopoverContent>
                    </Popover>
                </div>
                <div className="flex w-full flex-wrap items-center gap-1.5 sm:w-auto sm:flex-nowrap">
                    <Button
                        id={`${id}-add-node`}
                        type="button"
                        variant="outline"
                        size="sm"
                        disabled={disabled || nodes.length >= 20}
                        aria-label="添加步骤"
                        title={
                            nodes.length >= 20
                                ? "最多配置 20 个审批步骤"
                                : "添加步骤"
                        }
                        onClick={() => insert({ position: "end" })}
                    >
                        <PlusIcon data-icon="inline-start" />
                        <span className="hidden sm:inline">添加步骤</span>
                    </Button>
                    <span aria-hidden className="mx-0.5 h-5 w-px bg-border" />
                    <div
                        role="group"
                        aria-label="布局方向"
                        className="flex items-center rounded-lg border border-border/80 bg-muted/40 p-0.5"
                    >
                        <Button
                            id={`${id}-horizontal`}
                            type="button"
                            variant={
                                layout.direction === "horizontal"
                                    ? "secondary"
                                    : "ghost"
                            }
                            size="icon-sm"
                            className="rounded-md"
                            aria-pressed={layout.direction === "horizontal"}
                            aria-label="横向布局"
                            title="横向布局"
                            disabled={!instance}
                            onClick={() => arrange("horizontal")}
                        >
                            <ArrowRightIcon />
                        </Button>
                        <Button
                            id={`${id}-vertical`}
                            type="button"
                            variant={
                                layout.direction === "vertical"
                                    ? "secondary"
                                    : "ghost"
                            }
                            size="icon-sm"
                            className="rounded-md"
                            aria-pressed={layout.direction === "vertical"}
                            aria-label="纵向布局"
                            title="纵向布局"
                            disabled={!instance}
                            onClick={() => arrange("vertical")}
                        >
                            <ArrowDownIcon />
                        </Button>
                    </div>
                    <Button
                        id={`${id}-arrange`}
                        type="button"
                        variant="ghost"
                        size="icon-sm"
                        aria-label="自动排列"
                        title="自动排列"
                        disabled={!instance}
                        onClick={() => arrange()}
                    >
                        <LayoutGridIcon />
                    </Button>
                    <Button
                        id={`${id}-fit`}
                        type="button"
                        variant="ghost"
                        size="icon-sm"
                        aria-label="适应画布"
                        title="适应画布"
                        disabled={!instance}
                        onClick={() =>
                            void instance?.fitView({
                                ...fitOptions,
                                duration: 200,
                            })
                        }
                    >
                        <MaximizeIcon />
                    </Button>
                    <DropdownMenu>
                        <DropdownMenuTrigger
                            id={`${id}-more`}
                            render={
                                <Button
                                    type="button"
                                    variant="ghost"
                                    size="icon-sm"
                                    aria-label="更多画布操作"
                                    title="更多"
                                />
                            }
                        >
                            <EllipsisIcon />
                        </DropdownMenuTrigger>
                        <DropdownMenuContent align="end">
                            <DropdownMenuItem
                                id={`${id}-copy-flow`}
                                disabled={disabled}
                                onClick={onCopyFlow}
                            >
                                <CopyIcon aria-hidden />
                                从其它流程复制
                            </DropdownMenuItem>
                        </DropdownMenuContent>
                    </DropdownMenu>
                </div>
            </div>
            <div className="flex flex-col lg:h-[clamp(440px,54vh,620px)] lg:flex-row">
                <div
                    ref={containerRef}
                    className="relative h-[420px] min-h-0 w-full min-w-0 sm:h-[460px] lg:h-full lg:flex-1"
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
                                    ["approval", "or", "person"].includes(
                                        node.data.kind,
                                    )
                                ) {
                                    onSelect(node.data.ownerId)
                                }
                            }}
                            onInit={(flow) => {
                                setInstance(flow)
                                setZoom(flow.getZoom())
                                containerRef.current
                                    ?.querySelector(".react-flow__pane")
                                    ?.setAttribute("id", `${id}-pane`)
                            }}
                            onMoveEnd={(_event, viewport) => {
                                layout.setViewport(viewport)
                                setZoom(viewport.zoom)
                            }}
                            defaultViewport={layout.viewport}
                            fitView={!layout.viewport}
                            fitViewOptions={initialFitOptions}
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
                    <div className="absolute bottom-3 left-3 z-10 flex items-center rounded-full border border-border bg-card/95 p-1 shadow-sm backdrop-blur">
                        <Button
                            id={`${id}-zoom-out`}
                            type="button"
                            variant="ghost"
                            size="icon-xs"
                            aria-label="缩小画布"
                            disabled={!instance}
                            onClick={() =>
                                void instance?.zoomOut({ duration: 150 })
                            }
                        >
                            <MinusIcon />
                        </Button>
                        <Button
                            id={`${id}-zoom-reset`}
                            type="button"
                            variant="ghost"
                            size="xs"
                            className="min-w-12 px-1 font-normal text-muted-foreground"
                            aria-label={`当前缩放 ${Math.round(zoom * 100)}%，点击恢复 100%`}
                            title="恢复 100%"
                            disabled={!instance}
                            onClick={() =>
                                void instance?.zoomTo(1, { duration: 150 })
                            }
                        >
                            {Math.round(zoom * 100)}%
                        </Button>
                        <Button
                            id={`${id}-zoom-in`}
                            type="button"
                            variant="ghost"
                            size="icon-xs"
                            aria-label="放大画布"
                            disabled={!instance}
                            onClick={() =>
                                void instance?.zoomIn({ duration: 150 })
                            }
                        >
                            <PlusIcon />
                        </Button>
                    </div>
                </div>
                {sidebar ? (
                    <aside
                        aria-label="步骤设置"
                        className={cn(
                            "min-w-0 border-t border-border bg-card",
                            "lg:h-full lg:w-[300px] lg:shrink-0 lg:overflow-y-auto lg:border-l lg:border-t-0",
                        )}
                    >
                        {sidebar}
                    </aside>
                ) : null}
            </div>
            <div className="flex flex-wrap items-center justify-between gap-2 border-t border-border px-4 py-2 text-xs text-muted-foreground">
                <p role="status" aria-live="polite" className="min-w-0">
                    {layout.storageWarning
                        ? "浏览器未能保存画布布局，本次仍可编辑；审批配置请保存草稿。"
                        : notice || "拖动节点调整显示位置；右键节点插入步骤。"}
                </p>
                <p className="shrink-0">布局仅保存在当前浏览器</p>
            </div>
        </section>
    )
}
