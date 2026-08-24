"use client"

import { LoginForm } from "../components/login-form"
import { LoginStage } from "../components/login-stage"

/**
 * 后台登录页（公开路由）：左品牌舞台 + 右登录表单。
 */
export function LoginPage() {
    return (
        <div className="grid min-h-svh bg-background lg:grid-cols-[minmax(0,1.15fr)_minmax(26rem,0.85fr)]">
            <LoginStage />
            <main className="relative flex items-center justify-center bg-muted/40 p-6 sm:p-10">
                <LoginForm />
            </main>
        </div>
    )
}
