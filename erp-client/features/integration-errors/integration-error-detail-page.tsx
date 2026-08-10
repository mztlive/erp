"use client"

import { IntegrationErrorsPage } from "./integration-errors-page"

/** 领域详情：`/errors/:taskId` — 固定聚焦 integration_error_task */
export function IntegrationErrorTaskDetailPage({ taskId }: { taskId: string }) {
    return <IntegrationErrorsPage forcedTaskId={taskId} />
}

/** 领域详情：`/differences/:differenceId` — 固定聚焦 reconciliation_difference */
export function IntegrationDifferenceDetailPage({
    differenceId,
}: {
    differenceId: string
}) {
    return <IntegrationErrorsPage forcedDifferenceId={differenceId} />
}
