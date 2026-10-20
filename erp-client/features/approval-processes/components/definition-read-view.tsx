"use client"

import { GitBranchIcon, LockKeyholeIcon } from "lucide-react"
import { rejectionPolicyCopy } from "../labels"
import { toEditorNode } from "../draft-nodes"
import type { DefinitionDetailView } from "../types"
import { CopyRuleSummary } from "./copy-rule-editor"
import { DefinitionFlowchart } from "./definition-flowchart"

/** 流程图完整展示节点与审批人，侧栏集中展示版本说明、抄送和流转规则。 */
export function DefinitionReadView({
    detail,
}: {
    detail: DefinitionDetailView
}) {
    const nodes = [...detail.nodes].sort(
        (a, b) => a.display_order - b.display_order,
    )
    const versionDescription =
        detail.status === "PUBLISHED"
            ? "新建单据使用此版本。调整后需发布新版本，已有单据仍按原版本审批。"
            : detail.status === "RETIRED"
              ? "此版本已退役，不再用于新建单据。已有单据仍按原版本审批。"
              : "此草稿尚未发布，暂不用于新建单据。"

    return (
        <section aria-label="审批流程详情" className="space-y-4 p-4 sm:p-6">
            <div className="flex flex-wrap items-center justify-between gap-2">
                <h2 className="min-w-0 break-words text-base font-semibold">
                    {detail.name}
                </h2>
                <span className="inline-flex items-center gap-1.5 rounded-md bg-muted px-2 py-1 text-xs text-muted-foreground">
                    <LockKeyholeIcon aria-hidden className="size-3.5" />
                    只读
                </span>
            </div>
            <div className="grid items-start gap-5 xl:grid-cols-[minmax(0,1fr)_320px]">
                <DefinitionFlowchart
                    id="governance-approval-processes-detail-read-flow"
                    nodes={nodes.map(toEditorNode)}
                    overview
                />
                <aside
                    aria-label="流程配置与规则"
                    className="min-w-0 divide-y divide-border rounded-lg border border-border bg-card"
                >
                    <div className="space-y-2 rounded-t-lg bg-muted/30 p-4">
                        <h3 className="text-sm font-medium">版本说明</h3>
                        <p className="text-xs leading-5 text-muted-foreground">
                            {versionDescription}
                        </p>
                    </div>
                    <div className="p-4">
                        <CopyRuleSummary detail={detail} embedded />
                    </div>
                    <div className="space-y-3 p-4">
                        <h3 className="flex items-center gap-2 text-sm font-medium">
                            <GitBranchIcon
                                aria-hidden
                                className="size-4 text-muted-foreground"
                            />
                            流转规则
                        </h3>
                        <dl className="space-y-3 text-xs leading-5">
                            <div className="grid grid-cols-[2rem_minmax(0,1fr)] gap-3">
                                <dt className="font-medium">通过</dt>
                                <dd className="text-muted-foreground">
                                    依次进入下一节点，最后一个节点通过后完成审批。
                                </dd>
                            </div>
                            <div className="grid grid-cols-[2rem_minmax(0,1fr)] gap-3">
                                <dt className="font-medium">驳回</dt>
                                <dd className="text-muted-foreground">
                                    {rejectionPolicyCopy(detail.document_type)}
                                </dd>
                            </div>
                        </dl>
                    </div>
                </aside>
            </div>
        </section>
    )
}
