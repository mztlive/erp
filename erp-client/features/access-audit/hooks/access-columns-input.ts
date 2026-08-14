"use client"

import * as React from "react"

import type { AccountDraft } from "@/features/admin/account-form-dialog"
import type {
    AccessChangeCommand,
    AccessGovernancePolicyView,
    AccessListView,
} from "@/features/access-audit/types"

export type AccountFormState = {
    mode: "create" | "edit"
    account: AccountDraft | null
}

export type DeletingAccountState = {
    id: string
    account: string
}

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
    setAccountForm: React.Dispatch<
        React.SetStateAction<AccountFormState | null>
    >
    setDeletingAccount: React.Dispatch<
        React.SetStateAction<DeletingAccountState | null>
    >
    setDeletingRole: React.Dispatch<
        React.SetStateAction<DeletingRoleState | null>
    >
}
