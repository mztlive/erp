"use client";

import { useState } from "react";
import { QueryClientProvider } from "@tanstack/react-query";
import { ReactQueryDevtools } from "@tanstack/react-query-devtools";
import { makeQueryClient } from "@/lib/query-client";

/**
 * 全局 TanStack Query Provider。
 * 本项目是纯 SPA：所有网络请求必须通过 useQuery / useMutation / useInfiniteQuery 等，
 * 禁止在组件内直接 fetch/axios 后自管 loading/error，禁止 Server Components 拉数。
 */
export function QueryProvider({ children }: { children: React.ReactNode }) {
  // 客户端单例：避免每次 render 新建 QueryClient 导致缓存丢失
  const [queryClient] = useState(() => makeQueryClient());

  return (
    <QueryClientProvider client={queryClient}>
      {children}
      {process.env.NODE_ENV === "development" ? (
        <ReactQueryDevtools initialIsOpen={false} buttonPosition="bottom-left" />
      ) : null}
    </QueryClientProvider>
  );
}
