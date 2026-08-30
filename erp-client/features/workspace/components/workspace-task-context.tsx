import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionList,
    DescriptionTerm,
} from "@/components/ui/description-list"
import { StatusBadge } from "@/components/ui/status-badge"

import type { WorkspaceWorkItem } from "../types"
import { isBlockedWorkItem } from "../lib/work-item"
import { WorkspaceDocumentBadge } from "./workspace-document-badge"

const PRIORITY_LABEL: Record<number, string> = {
    1: "紧急",
    2: "高",
    3: "普通",
    4: "低",
}

function responsibilityLabel(item: WorkspaceWorkItem): string {
    return [
        item.ownerRoleLabel,
        item.ownerUserLabel,
        item.ownerOrganizationLabel,
    ]
        .filter((value) => value.trim())
        .join(" · ")
}

/** 每种任务共用的责任上下文；业务作业面只负责展示事实并提交强类型命令。 */
export function WorkspaceTaskContext({ item }: { item: WorkspaceWorkItem }) {
    const blocked = isBlockedWorkItem(item)
    const overdue = item.dueBucket === "overdue"
    const priorityLabel = PRIORITY_LABEL[item.priority] ?? `P${item.priority}`

    return (
        <aside
            className="flex shrink-0 flex-col gap-3 border-b border-grid pb-4"
            aria-label="任务责任与处理要求"
        >
            <div className="flex flex-wrap items-center gap-2">
                <WorkspaceDocumentBadge item={item} />
                <span className="text-sm font-medium">
                    {item.workItemTypeLabel}
                </span>
                <StatusBadge
                    label={blocked ? "处理受阻" : overdue ? "已超期" : "待处理"}
                    tone={
                        blocked ? "warning" : overdue ? "destructive" : "info"
                    }
                />
                <span className="text-xs text-muted-foreground">
                    优先级：{priorityLabel}
                </span>
            </div>

            <DescriptionList columns="three" className="gap-y-3">
                <DescriptionItem>
                    <DescriptionTerm>为什么到你</DescriptionTerm>
                    <DescriptionDetails>{item.reasonLabel}</DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>不处理的影响</DescriptionTerm>
                    <DescriptionDetails>
                        {item.impactSummary}
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>现在应做什么</DescriptionTerm>
                    <DescriptionDetails>
                        {item.nextActionHint}
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>当前责任</DescriptionTerm>
                    <DescriptionDetails>
                        {responsibilityLabel(item) || "责任人待确认"}
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>进入工作台</DescriptionTerm>
                    <DescriptionDetails>
                        {item.enteredAtLabel}
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>处理时限</DescriptionTerm>
                    <DescriptionDetails>
                        {item.dueAt ? item.dueAtLabel : "未设置截止时间"}
                    </DescriptionDetails>
                </DescriptionItem>
            </DescriptionList>

            {item.actionBlockers.length > 0 ? (
                <Alert variant="warning">
                    <AlertTitle>当前处理受阻</AlertTitle>
                    <AlertDescription>
                        <ul className="list-disc space-y-1 pl-4">
                            {item.actionBlockers.map((blocker, index) => (
                                <li
                                    key={`${blocker.action}:${blocker.code}:${index}`}
                                >
                                    {blocker.message}
                                </li>
                            ))}
                        </ul>
                    </AlertDescription>
                </Alert>
            ) : null}
        </aside>
    )
}
