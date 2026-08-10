"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
  fetchAccountProfile,
  login,
  type LoginInput,
} from "@/features/auth/api"
import { isAuthenticated } from "@/lib/api/session"

const accountProfileKeys = {
  all: ["account", "profile"] as const,
  current: () => [...accountProfileKeys.all, "current"] as const,
}

/**
 * 当前登录账号资料（含有效权限）。
 * 侧栏按 permissions 裁剪；登录后全站共用同一缓存。
 */
export function useAccountProfileQuery() {
  return useQuery({
    queryKey: accountProfileKeys.current(),
    queryFn: fetchAccountProfile,
    enabled: typeof window !== "undefined" && isAuthenticated(),
    staleTime: 5 * 60 * 1000,
  })
}

/**
 * 登录 mutation：成功后调用方可跳转工作台。
 */
export function useLoginMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input: LoginInput) => login(input),
    onSuccess: () => {
      // 新会话：丢弃可能残留的匿名/旧用户缓存
      queryClient.clear()
    },
  })
}
