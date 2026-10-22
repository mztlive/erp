"use client"

import { createContext, useContext } from "react"

/** Marks content whose page navigation is provided by the workspace tab bar. */
export const WorkspaceTabContext = createContext(false)

export function useWorkspaceTab(): boolean {
    return useContext(WorkspaceTabContext)
}
