"use client"

import { useMutation, useQueryClient } from "@tanstack/react-query"

import { login, type LoginInput } from "@/features/auth/api"
import { clearToken } from "@/lib/api/session"

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

/**
 * 主动登出：清 token + 缓存。页面层负责跳转 /login。
 */
export function useLogoutMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async () => {
      clearToken()
    },
    onSuccess: () => {
      queryClient.clear()
    },
  })
}
