"use client"

import { useRef } from "react"
import type { NodeProps } from "@xyflow/react"
import {
    ArrowDownToLineIcon,
    ArrowLeftToLineIcon,
    ArrowRightToLineIcon,
    Trash2Icon,
} from "lucide-react"
import {
    ContextMenu,
    ContextMenuContent,
    ContextMenuGroup,
    ContextMenuItem,
    ContextMenuLabel,
    ContextMenuSeparator,
    ContextMenuTrigger,
} from "@/components/ui/context-menu"
import { FlowCanvasNode, type CanvasNode } from "./flow-canvas-node"
import { FlowBranchNode } from "./flow-branch-node"
import type { NodeInsertion } from "./flow-node-insertion"

/** 每个画布节点使用同一套节点菜单，人员和汇合点定位到所属审批步骤。 */
export function CanvasNodeWithMenu(props: NodeProps<CanvasNode>) {
    const { data } = props
    const ownerId = data.ownerId
    const focusMoved = useRef(false)
    const unavailable = data.disabled || data.nodeLimitReached
    const insert = (placement: NodeInsertion) => {
        if (!unavailable) focusMoved.current = data.onInsert(placement)
    }
    return (
        <ContextMenu
            onOpenChange={(open) => {
                if (open) focusMoved.current = false
            }}
        >
            <ContextMenuTrigger
                id={`${data.domId}-context-trigger`}
                className={data.kind === "or" ? "block h-full" : undefined}
                tabIndex={data.kind === "join" ? -1 : 0}
                role="group"
                aria-label={`${data.node?.node_name || (data.kind === "start" ? "提交申请" : "审批完成")}，右键打开节点菜单`}
                onContextMenuCapture={(event) => {
                    if (
                        event.target instanceof Element &&
                        event.target.closest(
                            "input,textarea,[contenteditable='true']",
                        )
                    )
                        event.stopPropagation()
                }}
                onKeyDown={(event) => {
                    if (
                        event.target instanceof Element &&
                        event.target.closest(
                            "input,textarea,[contenteditable='true']",
                        )
                    )
                        return
                    if (
                        event.key !== "ContextMenu" &&
                        !(event.shiftKey && event.key === "F10")
                    )
                        return
                    event.preventDefault()
                    event.stopPropagation()
                    const bounds = event.currentTarget.getBoundingClientRect()
                    event.currentTarget.dispatchEvent(
                        new MouseEvent("contextmenu", {
                            bubbles: true,
                            cancelable: true,
                            clientX: bounds.left + bounds.width / 2,
                            clientY: bounds.top + bounds.height / 2,
                        }),
                    )
                }}
            >
                {props.type === "branch" ? (
                    <FlowBranchNode {...props} />
                ) : (
                    <FlowCanvasNode {...props} />
                )}
            </ContextMenuTrigger>
            <ContextMenuContent
                id={`${data.domId}-context-menu`}
                className="nodrag nopan nowheel min-w-56"
                finalFocus={() =>
                    focusMoved.current
                        ? false
                        : document.getElementById(
                              `${data.domId}-context-trigger`,
                          )
                }
                onClick={(event) => event.stopPropagation()}
                onKeyDown={(event) => event.stopPropagation()}
            >
                <ContextMenuGroup>
                    <ContextMenuLabel className="max-w-72 truncate">
                        {data.node
                            ? `${data.order}. ${data.node.node_name.trim() || "未命名节点"}`
                            : data.kind === "start"
                              ? "提交申请"
                              : "审批完成"}
                    </ContextMenuLabel>
                    {ownerId ? (
                        <>
                            <ContextMenuItem
                                id={`${data.domId}-insert-before`}
                                disabled={unavailable}
                                onClick={() =>
                                    insert({
                                        position: "before",
                                        anchorId: ownerId,
                                    })
                                }
                            >
                                <ArrowLeftToLineIcon />
                                在此步骤前插入节点
                            </ContextMenuItem>
                            <ContextMenuItem
                                id={`${data.domId}-insert-after`}
                                disabled={unavailable}
                                onClick={() =>
                                    insert({
                                        position: "after",
                                        anchorId: ownerId,
                                    })
                                }
                            >
                                <ArrowRightToLineIcon />
                                在此步骤后插入节点
                            </ContextMenuItem>
                            <ContextMenuSeparator />
                        </>
                    ) : data.kind === "start" ? (
                        <ContextMenuItem
                            id={`${data.domId}-insert-first`}
                            disabled={unavailable}
                            onClick={() => insert({ position: "start" })}
                        >
                            <ArrowRightToLineIcon />
                            在起点后添加节点
                        </ContextMenuItem>
                    ) : null}
                    <ContextMenuItem
                        id={`${data.domId}-append`}
                        disabled={unavailable}
                        onClick={() => insert({ position: "end" })}
                    >
                        <ArrowDownToLineIcon />
                        追加到流程末尾
                    </ContextMenuItem>
                    {ownerId && (
                        <>
                            <ContextMenuSeparator />
                            <ContextMenuItem
                                id={`${data.domId}-delete`}
                                variant="destructive"
                                disabled={data.disabled}
                                onClick={() => {
                                    if (!data.disabled)
                                        focusMoved.current =
                                            data.onRemove(ownerId)
                                }}
                            >
                                <Trash2Icon />
                                {["person", "join", "split"].includes(data.kind)
                                    ? "删除整个审批步骤"
                                    : "删除节点"}
                            </ContextMenuItem>
                        </>
                    )}
                </ContextMenuGroup>
                {(unavailable ||
                    data.kind === "person" ||
                    data.kind === "join") && (
                    <p className="max-w-72 px-2 py-1.5 text-xs leading-5 text-muted-foreground">
                        {data.disabled
                            ? "当前不可编辑审批流程"
                            : data.nodeLimitReached
                              ? "最多配置 20 个审批步骤，可删除后再添加"
                              : "插入和删除均针对所属的整个或签步骤"}
                    </p>
                )}
            </ContextMenuContent>
        </ContextMenu>
    )
}
