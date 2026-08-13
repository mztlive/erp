"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
    createAdmin,
    createRole,
    deleteAdmin,
    deleteRole,
    fetchAssignableRoles,
    fetchRoles,
    updateAdmin,
    updateAdminRole,
    updateRole,
} from "@/features/admin/api/admin"
import type {
    CreateAdminPayload,
    CreateRolePayload,
    UpdateAdminPayload,
    UpdateRolePayload,
} from "@/features/admin/types"

const adminKeys = {
    all: ["admin"] as const,
    admins: () => [...adminKeys.all, "admins"] as const,
    roles: () => [...adminKeys.all, "roles"] as const,
    assignableRoles: () => [...adminKeys.all, "roles", "assignable"] as const,
}

/** 全部角色列表（账号页用于展示角色名、角色页用于列表）。 */
export function useRolesQuery() {
    return useQuery({
        queryKey: adminKeys.roles(),
        queryFn: fetchRoles,
    })
}

/** 可分配角色（账号表单角色选项）；失败时 API 层已回落到全部角色。 */
export function useAssignableRolesQuery() {
    return useQuery({
        queryKey: adminKeys.assignableRoles(),
        queryFn: fetchAssignableRoles,
    })
}

/**
 * 账号写操作集合：创建 / 更新 / 更新角色 / 删除。
 * 成功后失效全部 admin 查询，保证账号与角色页同时刷新。
 */
export function useAdminMutations() {
    const queryClient = useQueryClient()

    const invalidate = async () => {
        await queryClient.invalidateQueries({ queryKey: adminKeys.all })
    }

    const create = useMutation({
        mutationFn: (payload: CreateAdminPayload) => createAdmin(payload),
        onSuccess: invalidate,
    })
    const update = useMutation({
        mutationFn: ({
            id,
            payload,
        }: {
            id: string
            payload: UpdateAdminPayload
        }) => updateAdmin(id, payload),
        onSuccess: invalidate,
    })
    const updateRole = useMutation({
        mutationFn: ({ id, role_ids }: { id: string; role_ids: string[] }) =>
            updateAdminRole(id, { role_ids }),
        onSuccess: invalidate,
    })
    const remove = useMutation({
        mutationFn: (id: string) => deleteAdmin(id),
        onSuccess: invalidate,
    })

    return {
        createAdmin: create.mutateAsync,
        updateAdmin: update.mutateAsync,
        updateAdminRole: updateRole.mutateAsync,
        deleteAdmin: remove.mutateAsync,
        isCreating: create.isPending,
        isUpdating: update.isPending,
        isDeleting: remove.isPending,
    }
}

/**
 * 角色写操作集合：创建 / 更新 / 删除。
 * 成功后失效全部 admin 查询；账号页的角色名映射随之刷新。
 */
export function useRoleMutations() {
    const queryClient = useQueryClient()

    const invalidate = async () => {
        await queryClient.invalidateQueries({ queryKey: adminKeys.all })
    }

    const create = useMutation({
        mutationFn: (payload: CreateRolePayload) => createRole(payload),
        onSuccess: invalidate,
    })
    const update = useMutation({
        mutationFn: ({
            id,
            payload,
        }: {
            id: string
            payload: UpdateRolePayload
        }) => updateRole(id, payload),
        onSuccess: invalidate,
    })
    const remove = useMutation({
        mutationFn: (id: string) => deleteRole(id),
        onSuccess: invalidate,
    })

    return {
        createRole: create.mutateAsync,
        updateRole: update.mutateAsync,
        deleteRole: remove.mutateAsync,
        isCreating: create.isPending,
        isUpdating: update.isPending,
        isDeleting: remove.isPending,
    }
}
