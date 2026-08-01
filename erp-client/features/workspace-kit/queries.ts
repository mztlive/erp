"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { mockDelay } from "@/features/workspace-kit/delay"
import type {
  QueueTask,
  WorkspacePageDef,
} from "@/features/workspace-kit/types"
import type { WorkspaceId } from "@/lib/workspace-registry"
import {
  getMasterDataPageDef,
  getWorkspacePageDef,
  type MasterDataResource,
  WORKSPACE_PAGE_DEFS,
} from "@/mock/workspace-pages"
import {
  applyQueueTaskOutcome,
  getCompletedQueueTaskIds,
  getHeldQueueTaskIds,
  type QueueTaskOutcome,
} from "@/mock/session-state"

export const workspacePageKeys = {
  all: ["workspace-pages"] as const,
  page: (id: WorkspaceId) => [...workspacePageKeys.all, id] as const,
  masterData: (resource: MasterDataResource) =>
    [...workspacePageKeys.all, "W14", resource] as const,
  queue: (id: WorkspaceId) => [...workspacePageKeys.all, id, "queue"] as const,
}

async function fetchWorkspacePage(id: WorkspaceId): Promise<WorkspacePageDef> {
  await mockDelay()
  return getWorkspacePageDef(id)
}

export function useWorkspacePageQuery(id: WorkspaceId) {
  return useQuery({
    queryKey: workspacePageKeys.page(id),
    queryFn: () => fetchWorkspacePage(id),
  })
}

async function fetchMasterDataPage(
  resource: MasterDataResource
): Promise<WorkspacePageDef> {
  await mockDelay()
  return getMasterDataPageDef(resource)
}

export function useMasterDataPageQuery(resource: MasterDataResource) {
  return useQuery({
    queryKey: workspacePageKeys.masterData(resource),
    queryFn: () => fetchMasterDataPage(resource),
  })
}

/**
 * Queue tasks for a workspace:
 * - terminal completed/rejected tasks are excluded
 * - held tasks remain, with status rewritten to 已暂挂 and scopeTags including 已暂挂
 */
export async function fetchWorkspaceQueueTasks(
  workspaceId: WorkspaceId
): Promise<readonly QueueTask[]> {
  await mockDelay()
  const def = getWorkspacePageDef(workspaceId)
  if (def.shell.kind !== "queue") {
    throw new Error(`${workspaceId} is not a queue workspace`)
  }
  const completed = getCompletedQueueTaskIds(workspaceId)
  const held = getHeldQueueTaskIds(workspaceId)
  return def.shell.payload.tasks
    .filter((task) => !completed.has(task.id))
    .map((task) => {
      if (!held.has(task.id)) return task
      const tags = new Set(task.scopeTags ?? [])
      tags.add("已暂挂")
      return {
        ...task,
        status: { label: "已暂挂", tone: "warning" as const },
        scopeTags: [...tags],
      }
    })
}

export function useWorkspaceQueueQuery(workspaceId: WorkspaceId) {
  return useQuery({
    queryKey: workspacePageKeys.queue(workspaceId),
    queryFn: () => fetchWorkspaceQueueTasks(workspaceId),
  })
}

export async function completeWorkspaceQueueTask(options: {
  workspaceId: WorkspaceId
  taskId: string
  outcome: QueueTaskOutcome
}): Promise<{ reference: string; outcome: QueueTaskOutcome }> {
  await mockDelay(120)
  applyQueueTaskOutcome(options.workspaceId, options.taskId, options.outcome)
  const prefix =
    options.outcome === "succeeded"
      ? "OK"
      : options.outcome === "blocked"
        ? "HOLD"
        : "REJ"
  return {
    outcome: options.outcome,
    reference: `${prefix}-${options.workspaceId}-${options.taskId.toUpperCase()}`,
  }
}

export function useCompleteQueueTaskMutation(workspaceId: WorkspaceId) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input: { taskId: string; outcome: QueueTaskOutcome }) =>
      completeWorkspaceQueueTask({
        workspaceId,
        taskId: input.taskId,
        outcome: input.outcome,
      }),
    onSuccess: async () => {
      await queryClient.invalidateQueries({
        queryKey: workspacePageKeys.queue(workspaceId),
      })
      await queryClient.invalidateQueries({
        queryKey: workspacePageKeys.page(workspaceId),
      })
    },
  })
}

/** Stable list of workspace ids that use shared shells (excludes custom W01/W05/W06/W07). */
export const SHARED_SHELL_WORKSPACE_IDS = Object.keys(
  WORKSPACE_PAGE_DEFS
) as WorkspaceId[]
