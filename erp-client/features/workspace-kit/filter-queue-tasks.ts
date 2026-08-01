import type { QueueTask } from "@/features/workspace-kit/types"

/**
 * Queue scope filtering used by QueueWorkspacePage.
 * Default (first) scope shows all tasks.
 * Other scopes: task.scopeTags includes the scope label, or status.label equals
 * the scope label (e.g. 待领取).
 */
export function filterQueueTasks(
  tasks: readonly QueueTask[],
  options: {
    scope?: string
    scopeLabels?: readonly string[]
  }
): QueueTask[] {
  const { scope, scopeLabels = [] } = options
  const defaultScope = scopeLabels[0]
  if (!scope || !defaultScope || scope === defaultScope) {
    return [...tasks]
  }
  return tasks.filter((task) => {
    if ((task.scopeTags ?? []).includes(scope)) return true
    if (task.status.label === scope) return true
    return false
  })
}
