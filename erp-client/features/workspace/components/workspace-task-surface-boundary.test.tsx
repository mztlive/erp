import { render, screen } from "@testing-library/react"
import { useState } from "react"
import { expect, test } from "vitest"

import { WorkspaceTaskSurfaceBoundary } from "./workspace-task-surface-boundary"

const StatefulProbe = ({ taskId }: { taskId: string }) => {
    const [mountedFor] = useState(taskId)
    return <span data-testid="mounted-task">{mountedFor}</span>
}

test("切换到同类型的另一条任务时重建作业面状态", () => {
    const { rerender } = render(
        <WorkspaceTaskSurfaceBoundary workItemId="wi-1">
            <StatefulProbe taskId="wi-1" />
        </WorkspaceTaskSurfaceBoundary>,
    )

    rerender(
        <WorkspaceTaskSurfaceBoundary workItemId="wi-2">
            <StatefulProbe taskId="wi-2" />
        </WorkspaceTaskSurfaceBoundary>,
    )

    expect(screen.getByTestId("mounted-task").textContent).toBe("wi-2")
})
