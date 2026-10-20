"use client"

import * as React from "react"
import { GitBranchIcon, ShieldCheckIcon, UsersRoundIcon } from "lucide-react"
import { rejectionPolicyCopy } from "../labels"
import { toEditorNode } from "../draft-nodes"
import type { DefinitionDetailView } from "../types"
import { CopyRuleSummary } from "./copy-rule-editor"
import { DefinitionFlowchart } from "./definition-flowchart"

/** 只读流程图与选中节点详情，避免重复铺开整份人员名单。 */
export function DefinitionReadView({
    detail,
}: {
    detail: DefinitionDetailView
}) {
    const nodes = [...detail.nodes].sort(
        (a, b) => a.display_order - b.display_order,
    )
    const [selectedId, setSelectedId] = React.useState<string | null>(null)
    const selected =
        nodes.find((node) => node.node_id === selectedId) ?? nodes[0]
    const orSignCount = nodes.filter(
        (node) => node.additional_assignees?.length,
    ).length
    return (
        <section aria-label="审批流程详情" className="space-y-5 p-4 sm:p-6">
            <div className="flex flex-wrap items-center justify-between gap-3">
                <div>
                    <h2 className="text-base font-semibold">{detail.name}</h2>
                    <p className="mt-1 text-xs text-muted-foreground">
                        {nodes.length} 个审批节点
                        {orSignCount ? ` · ${orSignCount} 个或签节点` : ""}
                    </p>
                </div>
                <span className="inline-flex items-center gap-1.5 text-xs text-muted-foreground">
                    <ShieldCheckIcon className="size-3.5" />
                    只读版本
                </span>
            </div>
            <div className="grid items-start gap-5 lg:grid-cols-[minmax(0,1fr)_340px]">
                <DefinitionFlowchart
                    id="governance-approval-processes-detail-read-flow"
                    nodes={nodes.map(toEditorNode)}
                    selectedClientId={selected?.node_id}
                    onSelect={(clientId) => setSelectedId(clientId)}
                />
                <aside className="space-y-5 lg:sticky lg:top-4">
                    {selected && (
                        <div className="overflow-hidden rounded-lg border border-border">
                            <h3 className="border-b border-border px-4 py-3 text-sm font-medium">
                                节点详情
                            </h3>
                            <div className="space-y-4 p-4">
                                <div>
                                    <p className="text-sm font-medium">
                                        {selected.node_name}
                                    </p>
                                    <p className="mt-1 text-xs text-muted-foreground">
                                        {selected.additional_assignees?.length
                                            ? "或签 · 任一人通过即可"
                                            : "单人审批"}
                                    </p>
                                </div>
                                <div>
                                    <p className="mb-2 flex items-center gap-1.5 text-xs text-muted-foreground">
                                        <UsersRoundIcon className="size-3.5" />
                                        审批人
                                    </p>
                                    <div className="flex flex-wrap gap-1.5">
                                        {[
                                            {
                                                participant_id:
                                                    selected.assignee_user_id,
                                                name: selected.assignee_name_snapshot,
                                            },
                                            ...(selected.additional_assignees ??
                                                []),
                                        ].map((person) => (
                                            <span
                                                key={person.participant_id}
                                                className="max-w-full break-words rounded-md bg-muted px-2 py-1 text-xs"
                                            >
                                                {person.name || "待确认"}
                                            </span>
                                        ))}
                                    </div>
                                </div>
                            </div>
                        </div>
                    )}
                    <div className="space-y-3 px-1">
                        <h3 className="flex items-center gap-2 text-sm font-medium">
                            <GitBranchIcon className="size-4 text-muted-foreground" />
                            流转规则
                        </h3>
                        <dl className="space-y-3 text-xs leading-5">
                            <div>
                                <dt className="font-medium">通过</dt>
                                <dd className="mt-1 text-muted-foreground">
                                    依次进入下一节点，最后一个节点通过后完成审批。
                                </dd>
                            </div>
                            <div>
                                <dt className="font-medium">驳回</dt>
                                <dd className="mt-1 text-muted-foreground">
                                    {rejectionPolicyCopy(detail.document_type)}
                                </dd>
                            </div>
                            <div>
                                <dt className="font-medium">调整流程</dt>
                                <dd className="mt-1 text-muted-foreground">
                                    创建新草稿并发布后，用于新建单据。
                                </dd>
                            </div>
                        </dl>
                    </div>
                </aside>
            </div>
            <CopyRuleSummary detail={detail} />
        </section>
    )
}
