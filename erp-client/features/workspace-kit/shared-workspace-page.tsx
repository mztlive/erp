"use client"

import * as React from "react"

import { Button } from "@/components/ui/button"
import { PageHeader } from "@/components/business"
import { AnalyticsWorkspacePage } from "@/features/workspace-kit/analytics-workspace-page"
import { GovernanceWorkspacePage } from "@/features/workspace-kit/governance-workspace-page"
import {
  ListWorkspaceError,
  ListWorkspaceLoading,
  ListWorkspacePage,
} from "@/features/workspace-kit/list-workspace-page"
import { ObjectWorkspacePage } from "@/features/workspace-kit/object-workspace-page"
import { useWorkspacePageQuery } from "@/features/workspace-kit/queries"
import { QueueWorkspacePage } from "@/features/workspace-kit/queue-workspace-page"
import type { WorkspaceId } from "@/lib/workspace-registry"
import { getWorkspaceById } from "@/lib/workspace-registry"

export function SharedWorkspacePage({
  workspaceId,
}: {
  workspaceId: WorkspaceId
}) {
  const meta = getWorkspaceById(workspaceId)
  const query = useWorkspacePageQuery(workspaceId)

  if (query.isPending) {
    return <ListWorkspaceLoading title={meta.name} />
  }

  if (query.isError || !query.data) {
    return (
      <ListWorkspaceError
        title={meta.name}
        onRetry={() => {
          void query.refetch()
        }}
      />
    )
  }

  const def = query.data
  switch (def.shell.kind) {
    case "list":
      return <ListWorkspacePage def={def} />
    case "queue":
      return (
        <React.Suspense
          fallback={<ListWorkspaceLoading title={meta.name} />}
        >
          <QueueWorkspacePage def={def} />
        </React.Suspense>
      )
    case "object":
      return <ObjectWorkspacePage def={def} />
    case "analytics":
      return <AnalyticsWorkspacePage def={def} />
    case "governance":
      return <GovernanceWorkspacePage def={def} />
    default:
      return (
        <div className="p-5">
          <PageHeader title={meta.name} description="未识别的页面模式。" />
          <Button type="button" onClick={() => void query.refetch()}>
            重试
          </Button>
        </div>
      )
  }
}
