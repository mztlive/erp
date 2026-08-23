"use client"

import * as React from "react"

import type { RoleAssignmentTarget } from "@/features/access-audit/components/role-assignment-dialog"
import type {
    AccessChangeCommand,
    AccessGovernancePolicyView,
    AccessListView,
} from "@/features/access-audit/types"

export type DeletingRoleState = {
    id: string
    name: string
}

export type AccessColumnsInput = {
    data?: AccessListView
    policies?: AccessGovernancePolicyView
    router: { push: (href: string) => void }
    rowFocusRef: { current: Map<string, HTMLButtonElement | null> }
    openExplain: (type: "ROLE" | "USER", id: string) => void
    openEvent: (id: string) => void
    startChange: (command: AccessChangeCommand) => Promise<void>
    setRoleAssignment: React.Dispatch<
        React.SetStateAction<RoleAssignmentTarget | null>
    >
    setDeletingRole: React.Dispatch<
        React.SetStateAction<DeletingRoleState | null>
    >
}
