import { Fragment, type ReactNode } from "react"

/** 任务身份变化时重建整个嵌入式作业面，防止上一条任务的 URL、表单或草稿状态残留。 */
export const WorkspaceTaskSurfaceBoundary = ({
    workItemId,
    children,
}: {
    workItemId: string
    children: ReactNode
}) => <Fragment key={workItemId}>{children}</Fragment>
