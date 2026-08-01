import type { Metadata } from "next"

import { WorkspaceHomePage as WorkspaceHome } from "@/features/workspace/workspace-home-page"

export const metadata: Metadata = {
  title: "今日工作台",
}

export default function WorkspaceHomePage() {
  return <WorkspaceHome />
}
