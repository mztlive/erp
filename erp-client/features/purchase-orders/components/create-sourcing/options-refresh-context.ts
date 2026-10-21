"use client"

import { createContext } from "react"

/** 当前工作台的创建依据查询；保留任务范围、表单草稿和选源身份。 */
export const SourcingOptionsRefreshContext = createContext<
    (() => Promise<unknown>) | undefined
>(undefined)
