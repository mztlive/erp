"use client"

import { REJECT_RESTART_COPY } from "../labels"
import type { DefinitionDetailView } from "../types"

/** 已发布及历史版本以节点顺序展示，不复用草稿的禁用表单或生成默认节点。 */
export function DefinitionReadView({
    detail,
}: {
    detail: DefinitionDetailView
}) {
    const nodes = [...detail.nodes].sort(
        (left, right) => left.display_order - right.display_order,
    )
    return (
        <section aria-label="审批流程详情" className="py-6">
            <div className="flex flex-wrap items-start justify-between gap-4 border-b border-border pb-5">
                <div className="space-y-1.5">
                    <h2 className="text-base font-semibold">{detail.name}</h2>
                    <p className="text-sm text-muted-foreground">
                        按以下顺序依次审批。{REJECT_RESTART_COPY}
                    </p>
                </div>
                <span className="text-xs text-muted-foreground">
                    共 <span className="num">{nodes.length}</span> 个审批节点
                </span>
            </div>
            {nodes.length ? (
                <ol aria-label="审批顺序" className="divide-y divide-border">
                    {nodes.map((node, index) => (
                        <li
                            key={node.node_id}
                            className="grid items-center gap-3 py-5 sm:grid-cols-[3rem_minmax(0,1fr)_minmax(10rem,1fr)]"
                        >
                            <span
                                className="num text-sm text-muted-foreground"
                                aria-label={`第 ${index + 1} 个节点`}
                            >
                                {String(index + 1).padStart(2, "0")}
                            </span>
                            <div className="space-y-1">
                                <p className="text-sm font-medium">
                                    {node.node_name}
                                </p>
                                <p className="text-xs text-muted-foreground">
                                    审批通过后
                                    {index === nodes.length - 1
                                        ? "完成流程"
                                        : "进入下一节点"}
                                </p>
                            </div>
                            <div className="space-y-1 sm:pl-6">
                                <p className="text-xs text-muted-foreground">
                                    审批人
                                </p>
                                <p className="text-sm">
                                    {node.assignee_name_snapshot ||
                                        "审批人信息待确认"}
                                </p>
                            </div>
                        </li>
                    ))}
                </ol>
            ) : (
                <p className="py-8 text-sm text-muted-foreground">
                    此版本没有审批节点记录。
                </p>
            )}
            <p className="border-t border-border pt-4 text-xs leading-5 text-muted-foreground">
                此版本只读。调整节点或审批人需创建新草稿，发布后用于新建单据。
            </p>
        </section>
    )
}
