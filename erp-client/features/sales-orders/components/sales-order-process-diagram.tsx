"use client"

import {
    Fragment,
    useCallback,
    useEffect,
    useMemo,
    useRef,
    useState,
} from "react"
import {
    Handle,
    MarkerType,
    Position,
    ReactFlow,
    type Node,
    type NodeProps,
} from "@xyflow/react"
import "@xyflow/react/dist/style.css"
import {
    ArrowRightIcon,
    CheckCircle2Icon,
    CircleIcon,
    CircleHelpIcon,
    CirclePauseIcon,
    InfoIcon,
    ReceiptTextIcon,
    ShoppingCartIcon,
    TicketIcon,
} from "lucide-react"
import { MoneyValue, detailWorkspaceStyles } from "@/components/business"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import type { SalesOrderDetailView } from "../api/contracts"
import {
    salesOrderProcess,
    type ProcessLane,
    type ProcessState,
    type ProcessStep,
} from "../lib/sales-order-current-work"
import { SalesOrderProcessWorkSheet } from "./sales-order-process-work-sheet"
import { remainingReceivableAmount } from "../lib/sales-order-receivable"
import type { NavSectionId } from "../lib/sales-order-detail-model"
import { SalesOrderApprovalDialog } from "./sales-order-approval-dialog"
import type { DeliveryProgressQuery } from "./sales-order-delivery-overview"

const stateStyles: Record<ProcessState, string> = {
    done: "border-success-border bg-success-soft text-success-soft-foreground",
    active: "border-warning-border bg-warning-soft text-warning-soft-foreground",
    waiting: "border-border bg-muted text-muted-foreground",
    unknown: "border-border bg-muted text-muted-foreground",
    stopped: "border-border bg-muted text-muted-foreground",
}
const stateNames: Record<ProcessState, string> = {
    done: "已完成",
    active: "进行中",
    waiting: "待开始",
    unknown: "待确认",
    stopped: "已终止",
}
const laneIcons = {
    delivery: ShoppingCartIcon,
    collection: ReceiptTextIcon,
    invoice: TicketIcon,
}
type DiagramData = {
    step?: ProcessStep
    lane?: ProcessLane
    prefix: string
    selectedId?: string
    laneWidth: number
    effectiveDate?: string
    onMeasure: (id: string, height: number) => void
    onOpen: (step: ProcessStep) => void
    canOpen: (step: ProcessStep) => boolean
}
type DiagramNode = Node<DiagramData, "process">

function StateIcon({ state }: { state: ProcessState }) {
    const Icon =
        state === "done"
            ? CheckCircle2Icon
            : state === "unknown"
              ? CircleHelpIcon
              : state === "stopped"
                ? CirclePauseIcon
                : CircleIcon
    return (
        <Icon
            aria-hidden="true"
            className={cn(
                "size-5 shrink-0",
                state === "done"
                    ? "text-success"
                    : state === "active"
                      ? "text-warning"
                      : "text-muted-foreground",
            )}
        />
    )
}

function Step({ step, data }: { step: ProcessStep; data: DiagramData }) {
    const group = step.groups[0]
    return (
        <div className="grid min-w-0 grid-cols-[20px_minmax(0,1fr)] items-center gap-x-2.5 gap-y-1.5">
            <StateIcon state={step.state} />
            <div
                className={cn(
                    "inline-flex min-h-8 w-fit items-center rounded-md border px-3 py-1 text-body-compact font-semibold",
                    stateStyles[step.state],
                    data.selectedId === step.id &&
                        "ring-2 ring-ring ring-offset-2",
                )}
            >
                {step.title}
            </div>
            <div className="col-start-2 min-w-0 space-y-1.5">
                <p className="sr-only">{stateNames[step.state]}</p>
                <p className="text-xs text-muted-foreground">
                    {group
                        ? `${group.owner} · ${group.count} 项${step.groups.length > 1 ? `，另 ${step.groups.length - 1} 组` : ""}`
                        : step.owner || step.description}
                </p>
                {step.money !== undefined ? (
                    <p className="text-xs text-muted-foreground">
                        <MoneyValue value={step.money} /> 待回款
                    </p>
                ) : null}
                {data.canOpen(step) ? (
                    <Button
                        id={`${data.prefix}-${step.id}-open`}
                        className={cn(
                            "nodrag nopan h-7 text-xs",
                            detailWorkspaceStyles.subtleButton,
                        )}
                        variant="ghost"
                        size="sm"
                        aria-haspopup={
                            step.groups.length ? "dialog" : undefined
                        }
                        aria-expanded={
                            step.groups.length
                                ? data.selectedId === step.id
                                : undefined
                        }
                        onClick={() => data.onOpen(step)}
                    >
                        {step.groups.length
                            ? "查看待办进度"
                            : step.section === "approval"
                              ? "查看审批"
                              : step.section === "receipt"
                                ? "查看回款"
                                : step.section === "invoice"
                                  ? "查看开票"
                                  : step.section === "acceptance"
                                    ? "查看验收"
                                    : "查看交付"}
                    </Button>
                ) : null}
            </div>
        </div>
    )
}

function ProcessNode({ id, data }: NodeProps<DiagramNode>) {
    const nodeRef = useRef<HTMLDivElement>(null)
    const onMeasure = data.onMeasure
    useEffect(() => {
        const element = nodeRef.current
        if (!element) return
        // 文案换行、金额辅助行和字体变化均参与布局，后续泳道按实际高度排列。
        const observer = new ResizeObserver(([entry]) => {
            onMeasure(
                id,
                Math.ceil(
                    entry.borderBoxSize[0]?.blockSize ?? element.offsetHeight,
                ),
            )
        })
        observer.observe(element)
        return () => observer.disconnect()
    }, [id, onMeasure])
    const root = data.step
    if (root)
        return (
            <div
                ref={nodeRef}
                className="flex w-40 flex-col items-center gap-2"
            >
                <div
                    className={cn(
                        "flex w-full items-start gap-2 rounded-lg border px-3 py-4",
                        stateStyles[root.state],
                    )}
                >
                    <StateIcon state={root.state} />
                    <div className="text-body-compact font-semibold leading-6">
                        <p>{root.title}</p>
                        <p>{root.description}</p>
                        {root.state === "active" ? (
                            <div className="mt-2 space-y-1 text-xs font-normal leading-5">
                                {root.currentNode ? (
                                    <p>{root.currentNode}</p>
                                ) : null}
                                <p>{root.owner}</p>
                            </div>
                        ) : null}
                    </div>
                </div>
                {data.effectiveDate ? (
                    <p className="text-center text-xs text-muted-foreground">
                        {new Date(data.effectiveDate).toLocaleDateString(
                            "zh-CN",
                        )}
                    </p>
                ) : null}
                {data.canOpen(root) ? (
                    <Button
                        id={`${data.prefix}-sales-open`}
                        variant="ghost"
                        size="sm"
                        className={detailWorkspaceStyles.subtleButton}
                        onClick={() => data.onOpen(root)}
                    >
                        查看审批
                    </Button>
                ) : null}
                <Handle
                    type="source"
                    position={Position.Right}
                    isConnectable={false}
                    className="invisible"
                    style={{ top: 43 }}
                />
            </div>
        )
    if (!data.lane) return null
    const Icon = laneIcons[data.lane.id]
    return (
        <div
            ref={nodeRef}
            className="grid min-h-[140px] items-start gap-x-4 border-t border-border/60 bg-card py-5"
            style={{
                width: data.laneWidth,
                gridTemplateColumns:
                    "144px minmax(0,1fr) 20px minmax(0,1fr) 20px minmax(0,1fr)",
            }}
        >
            <Handle
                type="target"
                position={Position.Left}
                isConnectable={false}
                className="invisible"
                style={{ top: 43 }}
            />
            <div className="flex w-36 shrink-0 items-center gap-2 rounded-lg bg-muted/45 px-3 py-3 text-body-compact font-semibold text-foreground">
                <Icon className="size-5" aria-hidden="true" />
                {data.lane.title}
            </div>
            {data.lane.steps.map((step, index) => (
                <Fragment key={step.id}>
                    {index > 0 ? (
                        <div className="flex h-8 items-center justify-center">
                            <ArrowRightIcon
                                className="size-5 text-muted-foreground/60"
                                aria-hidden="true"
                            />
                        </div>
                    ) : null}
                    <Step step={step} data={data} />
                </Fragment>
            ))}
        </div>
    )
}
const nodeTypes = { process: ProcessNode }

/** 全流程内的只读并行进度图，节点动作沿用既有业务分区与权限。 */
export function SalesOrderProcessDiagram({
    order,
    deliveryQuery,
    canReadDelivery,
    visibleSections,
    onSelectSection,
}: {
    order: SalesOrderDetailView
    deliveryQuery: DeliveryProgressQuery
    canReadDelivery: boolean
    visibleSections: NavSectionId[]
    onSelectSection: (section: NavSectionId) => void
}) {
    const [expanded, setExpanded] = useState<string>()
    const lastTrigger = useRef<HTMLElement | null>(null)
    const [approvalOpen, setApprovalOpen] = useState(false)
    const [nodeHeights, setNodeHeights] = useState<Record<string, number>>({})
    const onMeasure = useCallback((id: string, height: number) => {
        setNodeHeights((current) =>
            current[id] === height ? current : { ...current, [id]: height },
        )
    }, [])
    const canvasRef = useRef<HTMLDivElement>(null)
    const [canvasWidth, setCanvasWidth] = useState(1210)
    useEffect(() => {
        const element = canvasRef.current
        if (!element) return
        const observer = new ResizeObserver(([entry]) =>
            setCanvasWidth(entry.contentRect.width),
        )
        observer.observe(element)
        return () => observer.disconnect()
    }, [])
    const prefix = `sales-process-${toAutomationIdSegment(order.id)}`
    const process = useMemo(() => {
        const result = salesOrderProcess(
            order,
            canReadDelivery && !deliveryQuery.isError
                ? deliveryQuery.data
                : undefined,
        )
        const collect = result.lanes[1].steps[0]
        if (collect.state === "active")
            collect.money = remainingReceivableAmount(
                order.amountGross,
                order.receivedAmount,
            )
        return result
    }, [order, canReadDelivery, deliveryQuery.isError, deliveryQuery.data])
    const allSteps = [
        process.root,
        ...process.lanes.flatMap((lane) => lane.steps),
    ]
    const selected = allSteps.find((step) => step.id === expanded)
    const data: DiagramData = {
        prefix,
        selectedId: expanded,
        onMeasure,
        laneWidth: Math.max(990, canvasWidth - 220),
        effectiveDate: order.revisions.find(
            (revision) => revision.revisionNo === order.currentRevisionNo,
        )?.effectiveAt,
        canOpen: (step) =>
            step.groups.length > 0 || visibleSections.includes(step.section),
        onOpen: (step) => {
            if (step.section === "approval") {
                setApprovalOpen(true)
            } else if (step.groups.length) {
                lastTrigger.current = document.getElementById(
                    `${prefix}-${step.id}-open`,
                )
                setExpanded(step.id)
            } else {
                onSelectSection(step.section)
            }
        },
    }
    const laneHeights = process.lanes.map((lane) => nodeHeights[lane.id] ?? 140)
    const laneOffsets = laneHeights.map((_, index) =>
        laneHeights.slice(0, index).reduce((sum, height) => sum + height, 0),
    )
    const rootY = laneOffsets[1] ?? 0
    const canvasHeight =
        Math.max(
            laneHeights.reduce((sum, height) => sum + height, 0),
            rootY + (nodeHeights.sales ?? 160),
        ) + 8
    const nodes: DiagramNode[] = [
        {
            id: "sales",
            type: "process",
            position: { x: 0, y: rootY },
            style: { pointerEvents: "all" },
            data: { ...data, step: process.root },
        },
        ...process.lanes.map((lane, index): DiagramNode => ({
            id: lane.id,
            type: "process",
            position: { x: 210, y: laneOffsets[index] },
            style: { pointerEvents: "all" },
            data: { ...data, lane },
        })),
    ]
    const edges = process.lanes.map((lane) => ({
        id: `sales-${lane.id}`,
        source: "sales",
        target: lane.id,
        type: "smoothstep",
        markerEnd: { type: MarkerType.ArrowClosed, width: 14, height: 14 },
        style: { stroke: "var(--muted-foreground)", strokeWidth: 1 },
    }))
    const active = allSteps.filter((step) => step.state === "active")
    return (
        <section aria-label="全流程进度" className="space-y-4">
            <SalesOrderApprovalDialog
                order={order}
                open={approvalOpen}
                onOpenChange={setApprovalOpen}
                idPrefix={`${prefix}-approval`}
                triggerId={`${prefix}-sales-open`}
            />
            <div className="flex flex-wrap items-center justify-between gap-3">
                <div className="flex flex-wrap items-baseline gap-4">
                    <h2 className="text-section-title font-semibold tracking-tight">
                        全流程进度
                    </h2>
                    <p className="text-xs text-muted-foreground">
                        交付、回款与开票分别推进
                    </p>
                </div>
                <div className="flex gap-4 text-xs text-muted-foreground">
                    {(["done", "active", "waiting"] as const).map((state) => (
                        <span key={state} className="flex items-center gap-1.5">
                            <StateIcon state={state} />
                            {stateNames[state]}
                        </span>
                    ))}
                </div>
            </div>
            <div
                className="overflow-x-auto"
                role="region"
                aria-label="销售单并行流程图"
            >
                <div
                    ref={canvasRef}
                    className="min-w-[1210px]"
                    style={{ height: canvasHeight }}
                >
                    <ReactFlow<DiagramNode>
                        id={`${prefix}-canvas`}
                        proOptions={{ hideAttribution: true }}
                        nodes={nodes}
                        edges={edges}
                        nodeTypes={nodeTypes}
                        defaultViewport={{ x: 0, y: 0, zoom: 1 }}
                        nodesDraggable={false}
                        nodesConnectable={false}
                        nodesFocusable={false}
                        edgesFocusable={false}
                        elementsSelectable={false}
                        panOnDrag={false}
                        panOnScroll={false}
                        zoomOnScroll={false}
                        zoomOnPinch={false}
                        zoomOnDoubleClick={false}
                        preventScrolling={false}
                        minZoom={1}
                        maxZoom={1}
                    />
                </div>
            </div>
            {order.nature === "physical_service" &&
            order.currentRevisionNo != null &&
            canReadDelivery &&
            deliveryQuery.isError ? (
                <div
                    role="alert"
                    className="flex items-center gap-3 text-xs text-destructive"
                >
                    交付进度读取失败，交付与验收节点暂不判断完成状态。
                    <Button
                        id={`${prefix}-retry-delivery`}
                        variant="outline"
                        size="sm"
                        onClick={() => void deliveryQuery.refetch()}
                    >
                        重新读取
                    </Button>
                </div>
            ) : null}
            {order.nature === "physical_service" &&
            order.currentRevisionNo != null &&
            !canReadDelivery ? (
                <p className="text-xs text-muted-foreground">
                    当前无权读取交付明细；交付与验收节点仅展示已确认的总体状态。
                </p>
            ) : null}
            <div className="flex items-start gap-2 rounded-lg bg-muted/35 px-4 py-3 text-xs leading-6">
                <InfoIcon
                    className="mt-1 size-4 shrink-0 text-muted-foreground"
                    aria-hidden="true"
                />
                <p>
                    <strong>当前需跟进：</strong>
                    {active.length
                        ? active
                              .map(
                                  (step) =>
                                      `${step.title}（${step.groups.length ? step.groups.map((group) => `${group.owner} · ${group.count} 项`).join("；") : step.owner || step.description}）`,
                              )
                              .join("；")
                        : order.primaryStatus.code === "voided"
                          ? "销售单已作废。"
                          : order.currentRevisionNo == null
                            ? "请先完成销售单审批。"
                            : "暂无明确的当前办理事项，可进入各标签页核对进度。"}
                </p>
            </div>
            <SalesOrderProcessWorkSheet
                key={order.id}
                step={selected}
                prefix={prefix}
                finalFocus={lastTrigger}
                onClose={() => setExpanded(undefined)}
            />
        </section>
    )
}
