"use client"

import { useQuery } from "@tanstack/react-query"

import { mockDelay } from "@/lib/mock-delay"
import {
  buildTodayWorkspaceView,
  type TodayWorkspaceQuery,
} from "@/mock/workspace"

/** Mock identity / ACL snapshot versions — queryKey must include them (W01 §8.1). */
const MOCK_VIEWER_CONTEXT = {
  userId: "user_wangmin",
  activeRole: "sales_collab",
  permissionVersion: "pv_mock_1",
  dataScopeVersion: "ds_mock_1",
} as const

export const workspaceHomeKeys = {
  all: ["workspace-home"] as const,
  dashboard: (query: TodayWorkspaceQuery) =>
    [
      ...workspaceHomeKeys.all,
      "dashboard",
      MOCK_VIEWER_CONTEXT,
      query,
    ] as const,
}

async function fetchWorkspaceDashboard(query: TodayWorkspaceQuery) {
  await mockDelay()
  return buildTodayWorkspaceView(query)
}

export function useWorkspaceDashboardQuery(query: TodayWorkspaceQuery) {
  return useQuery({
    queryKey: workspaceHomeKeys.dashboard(query),
    queryFn: () => fetchWorkspaceDashboard(query),
    // Keep previous filter results visible while the next filter loads (§6.2).
    placeholderData: (previous) => previous,
  })
}
